use super::DuenioTransaccionAnclado;
use crate::transaccion::anclado::DirectorioAnclado;
use crate::transaccion::captura::{ConfigRawCapturada, Snapshot};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
use crate::transaccion::snapshot::{ErrorSnapshot, SnapshotToken, TipoEntradaSnapshot};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

const MANIFIESTO: &str = r#"version = "snapshot-owner-test/1"

[nodos]
sortes = ["a", "Retirado", "Patrón"]

[nodos.disco]
"Patrón" = "pattern"

[layout]
directorios = ["knowledge"]
sin_iota = []
ignorados = []
"#;

const DOCUMENTO_ORIGINAL: &[u8] = b"---\ntype: a\ntitle: Original\n---\n";
const DOCUMENTO_DECOY: &[u8] = b"---\ntype: a\ntitle: Decoy\n---\n";
const AUXILIAR_ORIGINAL: &[u8] = b"ORIGINAL-AUXILIARY-CANARY";
const AUXILIAR_DECOY: &[u8] = b"DECOY-AUXILIARY-CANARY";

fn config() -> ConfigRawCapturada {
    ConfigRawCapturada::nueva(MANIFIESTO.as_bytes(), b"\n".as_slice(), None, None).unwrap()
}

fn preparar_root(root: &Path, documento: &[u8], auxiliar: &[u8]) {
    fs::create_dir_all(root.join("knowledge")).unwrap();
    fs::write(root.join("knowledge/a.md"), documento).unwrap();
    fs::write(root.join("auxiliar.bin"), auxiliar).unwrap();
    fs::create_dir(root.join("vacio")).unwrap();
}

fn bytes<'a>(snapshot: &'a Snapshot, ruta: &str) -> &'a [u8] {
    snapshot
        .entradas()
        .iter()
        .find(|entrada| entrada.ruta() == ruta)
        .and_then(|entrada| entrada.bytes())
        .expect("la entrada raw debe estar en el snapshot")
}

fn contiene_directorio(snapshot: &Snapshot, ruta: &str) -> bool {
    snapshot
        .entradas()
        .iter()
        .any(|entrada| entrada.ruta() == ruta && entrada.tipo() == TipoEntradaSnapshot::Directorio)
}

fn abrir_owner(root: &Path) -> (UbicacionRaiz, LockEscritura) {
    let ubicacion = UbicacionRaiz::nueva(root).unwrap();
    let lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    (ubicacion, lock)
}

fn assert_perfil_soportado(snapshot: &Snapshot) {
    assert_eq!(snapshot.version_motor(), VERSION_MOTOR_V1);
    assert_eq!(snapshot.policy_id(), POLICY_ID_V1);
    assert!(snapshot.identidad_root().is_some());
    assert_eq!(bytes(snapshot, "knowledge/a.md"), DOCUMENTO_ORIGINAL);
    assert_eq!(bytes(snapshot, "auxiliar.bin"), AUXILIAR_ORIGINAL);
    assert!(contiene_directorio(snapshot, "vacio"));
}

#[test]
fn captura_owner_usa_parent_retenido_y_perfil_fijo() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    fs::create_dir_all(&parent).unwrap();
    let root = parent.join("bundle");
    fs::create_dir(&root).unwrap();
    preparar_root(&root, DOCUMENTO_ORIGINAL, AUXILIAR_ORIGINAL);
    let (_ubicacion, mut lock) = abrir_owner(&root);
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let snapshot = owner.capturar_snapshot_soportado(config()).unwrap();

    assert_perfil_soportado(&snapshot);

    let token = snapshot.token();
    let snapshot_cas = owner
        .capturar_snapshot_soportado_con_cas(config(), token)
        .unwrap();
    assert!(snapshot_cas.token() == token);
    assert_perfil_soportado(&snapshot_cas);
}

#[test]
fn captura_owner_falla_cerrado_si_root_esta_ausente_y_no_lo_crea() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    fs::create_dir_all(&parent).unwrap();
    let root = parent.join("bundle");
    let (_ubicacion, mut lock) = abrir_owner(&root);
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let error = owner.capturar_snapshot_soportado(config()).unwrap_err();

    assert_eq!(error, ErrorSnapshot::RaizInvalida);
    assert_eq!(error.to_string(), "snapshot_root_invalid");
    assert!(!root.exists());
}

#[test]
fn captura_owner_cas_stale_precede_parseo_y_preserva_root() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    fs::create_dir_all(&parent).unwrap();
    let root = parent.join("bundle");
    fs::create_dir(&root).unwrap();
    preparar_root(&root, DOCUMENTO_ORIGINAL, AUXILIAR_ORIGINAL);
    let (_ubicacion, mut lock) = abrir_owner(&root);
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let esperado: SnapshotToken = owner.capturar_snapshot_soportado(config()).unwrap().token();
    let invalid_config = ConfigRawCapturada::nueva(
        b"PRIVATE-CANARY invalid manifest".as_slice(),
        b"PRIVATE-CANARY invalid invariants".as_slice(),
        None,
        None,
    )
    .unwrap();

    let error = owner
        .capturar_snapshot_soportado_con_cas(invalid_config, esperado)
        .unwrap_err();

    assert_eq!(error, ErrorSnapshot::Stale);
    assert_eq!(error.to_string(), "stale");
    assert!(!format!("{error:?} {error}").contains("PRIVATE-CANARY"));
    assert_eq!(
        fs::read(root.join("knowledge/a.md")).unwrap(),
        DOCUMENTO_ORIGINAL
    );
    assert_eq!(
        fs::read(root.join("auxiliar.bin")).unwrap(),
        AUXILIAR_ORIGINAL
    );
}

#[test]
fn captura_owner_tras_mover_parent_no_sigue_root_decoy() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    fs::create_dir_all(&parent).unwrap();
    let root = parent.join("bundle");
    fs::create_dir(&root).unwrap();
    preparar_root(&root, DOCUMENTO_ORIGINAL, AUXILIAR_ORIGINAL);
    let (_ubicacion, mut lock) = abrir_owner(&root);
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let identidad_original = owner
        .observar_root_raw()
        .unwrap()
        .unwrap()
        .identity()
        .clone();

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(matches!(
                error.raw_os_error(),
                Some(5) | Some(32) | Some(33)
            ));
            assert!(root.is_dir());
            assert!(!moved_parent.exists());
            let snapshot = owner.capturar_snapshot_soportado(config()).unwrap();
            assert_perfil_soportado(&snapshot);
            assert!(
                snapshot
                    .identidad_root()
                    .unwrap()
                    .journal_identity()
                    .as_ref()
                    == Some(&identidad_original)
            );
            eprintln!(
                "Windows bloqueó el rename del parent retenido; esta rama no ejercitó un decoy."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent de prueba: {error}");
    }

    fs::create_dir(&parent).unwrap();
    let decoy_root = parent.join("bundle");
    fs::create_dir(&decoy_root).unwrap();
    preparar_root(&decoy_root, DOCUMENTO_DECOY, AUXILIAR_DECOY);
    let decoy_location = UbicacionRaiz::nueva(&decoy_root).unwrap();
    let decoy_root_handle = DirectorioAnclado::desde_ubicacion(&decoy_location).unwrap();
    let decoy_identity = decoy_root_handle.identidad_raiz().unwrap();

    let snapshot = owner.capturar_snapshot_soportado(config()).unwrap();

    assert_perfil_soportado(&snapshot);
    assert!(
        snapshot
            .identidad_root()
            .unwrap()
            .journal_identity()
            .as_ref()
            == Some(&identidad_original)
    );
    assert!(snapshot.identidad_root().unwrap() != decoy_identity);
    assert_eq!(
        fs::read(decoy_root.join("auxiliar.bin")).unwrap(),
        AUXILIAR_DECOY
    );
    assert_eq!(
        fs::read(moved_parent.join("bundle/auxiliar.bin")).unwrap(),
        AUXILIAR_ORIGINAL
    );
}
