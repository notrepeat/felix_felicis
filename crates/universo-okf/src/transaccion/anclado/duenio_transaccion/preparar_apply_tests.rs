use super::*;
use crate::transaccion::{
    captura::capturar_snapshot_soportado_anclado, lock::LockEscritura, raiz::UbicacionRaiz,
};
use std::fs;
use universo_core::{Atomo, BundlePath, Sorte};
fn config() -> ConfigRawCapturada {
    ConfigRawCapturada::nueva(
        "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n"
            .as_bytes(),
        b"(Type @n/node b)\n".as_slice(),
        None,
        None,
    )
    .unwrap()
}
fn delta() -> Delta {
    Delta::nuevo(
        [Atomo::Type {
            sujeto: BundlePath::nuevo("n/node").unwrap(),
            sorte: Sorte::nuevo("b"),
        }],
        [Atomo::Type {
            sujeto: BundlePath::nuevo("n/node").unwrap(),
            sorte: Sorte::nuevo("a"),
        }],
    )
    .unwrap()
}
fn fixture() -> (tempfile::TempDir, UbicacionRaiz) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::write(
        root.join("n/node.md"),
        "---\ntype: a\ntitle: Private\n---\nBODY\n",
    )
    .unwrap();
    fs::write(root.join("payload.bin"), b"raw auxiliary").unwrap();
    let location = UbicacionRaiz::nueva(&root).unwrap();
    (dir, location)
}
#[test]
fn prepara_type_y_replay_coincidente_sin_publicar_root() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (_dir, location) = fixture();
    let original = fs::read(location.ruta().join("n/node.md")).unwrap();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let snapshot = capturar_snapshot_soportado_anclado(
        &owner.abrir_root_lectura().unwrap().unwrap(),
        config(),
    )
    .unwrap();
    let ResultadoPreparacionApply::Preparada(prepared) = owner
        .preparar_apply(config(), snapshot.token(), &delta())
        .unwrap()
    else {
        panic!("not admitted")
    };
    assert_eq!(
        prepared.journal.state(),
        crate::transaccion::journal::JournalState::Prepared
    );
    assert_eq!(prepared.snapshot.token(), snapshot.token());
    let expected = crate::transaccion::replay::new_esperado_v2::calcular_new_esperado_v2(
        &prepared.contexto,
        &prepared.journal,
    )
    .unwrap();
    assert!(expected.digest() == prepared.stage.recibo().unwrap().raw_tree_digest());
    assert_eq!(
        fs::read(location.ruta().join("n/node.md")).unwrap(),
        original
    );
    assert!(!location.journal().as_path().exists());
    assert!(prepared.owner.parent_revalidado().is_ok());
    prepared.stage.limpiar().unwrap();
}
#[test]
fn stale_precede_parseo_y_rechazo_semantico_no_crea_stage() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (dir, location) = fixture();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let snapshot = capturar_snapshot_soportado_anclado(
        &owner.abrir_root_lectura().unwrap().unwrap(),
        config(),
    )
    .unwrap();
    let bad = ConfigRawCapturada::nueva(b"invalid TOML [".as_slice(), b"(".as_slice(), None, None)
        .unwrap();
    let error = match owner.preparar_apply(bad, snapshot.token(), &delta()) {
        Err(e) => e,
        Ok(_) => panic!("accepted stale"),
    };
    assert_eq!(error.codigo(), "stale");
    assert!(error.stage.is_none() && error.escritura.is_none());
    assert!(matches!(
        owner
            .preparar_apply(config(), snapshot.token(), &Delta::nuevo([], []).unwrap())
            .unwrap(),
        ResultadoPreparacionApply::Rechazada(ResultadoPropuesta::RechazadaInvariantes(_))
    ));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}
#[test]
fn namespace_ocupado_no_se_modifica() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (dir, location) = fixture();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let snapshot = capturar_snapshot_soportado_anclado(
        &owner.abrir_root_lectura().unwrap().unwrap(),
        config(),
    )
    .unwrap();
    fs::write(location.journal().as_path(), b"canary").unwrap();
    assert!(
        owner
            .preparar_apply(config(), snapshot.token(), &delta())
            .is_err()
    );
    assert_eq!(fs::read(location.journal().as_path()).unwrap(), b"canary");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
}

#[test]
fn cambio_root_tras_stage_retiene_capability_sin_publicar() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (_dir, location) = fixture();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let snapshot = capturar_snapshot_soportado_anclado(
        &owner.abrir_root_lectura().unwrap().unwrap(),
        config(),
    )
    .unwrap();
    let error = match owner.preparar_apply_con_hook(config(), snapshot.token(), &delta(), || {
        fs::write(location.ruta().join("payload.bin"), b"changed").unwrap()
    }) {
        Err(e) => e,
        Ok(_) => panic!("accepted changed root"),
    };
    assert_eq!(error.codigo(), "stale");
    assert!(!location.journal().as_path().exists());
    assert_eq!(
        fs::read(location.ruta().join("payload.bin")).unwrap(),
        b"changed"
    );
    assert!(owner.parent_revalidado().is_ok());
    error.stage.unwrap().limpiar().unwrap();
}

#[cfg(unix)]
#[test]
fn owner_sustituido_tras_stage_conserva_capability_y_canario() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (_dir, location) = fixture();
    let lock_path = location.lock().as_path().to_owned();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let snapshot = capturar_snapshot_soportado_anclado(
        &owner.abrir_root_lectura().unwrap().unwrap(),
        config(),
    )
    .unwrap();
    let error = match owner.preparar_apply_con_hook(config(), snapshot.token(), &delta(), || {
        fs::rename(&lock_path, lock_path.with_extension("old")).unwrap();
        fs::write(&lock_path, b"replacement").unwrap();
    }) {
        Err(e) => e,
        Ok(_) => panic!("accepted changed owner"),
    };
    assert_eq!(error.codigo(), "apply_owner_invalid");
    assert!(error.stage.is_some());
    assert!(!location.journal().as_path().exists());
    assert_eq!(fs::read(&lock_path).unwrap(), b"replacement");
    // No cleanup bajo el owner invalidado. La evidencia queda retenida hasta
    // que el test descarte su directorio temporal completo.
}
