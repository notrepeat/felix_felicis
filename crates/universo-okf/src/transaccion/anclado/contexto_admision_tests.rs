use super::*;

use crate::transaccion::admision::envelope::{
    EnvelopeContextoAdmision, codec, tests as envelope_fixtures,
};
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, Journal, RawTreeDigest};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::fs::{self, File};
use std::path::Path;
use tempfile::tempdir;

fn frame(transaction_id: &str, parent: DirectoryIdentity) -> Vec<u8> {
    // Storage fixture con parent real; las fingerprints sintéticas no prueban admisión.
    let old = ArtifactFingerprint::new(
        identity(2),
        RawTreeDigest::from_hex("a".repeat(64)).unwrap(),
    );
    let new = ArtifactFingerprint::new(
        identity(3),
        RawTreeDigest::from_hex("b".repeat(64)).unwrap(),
    );
    let journal = Journal::new(transaction_id, parent, old, new).unwrap();
    codec::codificar(&EnvelopeContextoAdmision::ligar(
        envelope_fixtures::contexto(b"\n"),
        &journal,
    ))
    .unwrap()
}

fn identity(seed: u64) -> DirectoryIdentity {
    #[cfg(unix)]
    {
        DirectoryIdentity::unix(seed, seed + 1)
    }
    #[cfg(windows)]
    {
        DirectoryIdentity::windows(seed, format!("{seed:032x}"))
    }
}

fn ubicacion(parent: &Path, crear_root: bool) -> (UbicacionRaiz, std::path::PathBuf) {
    fs::create_dir_all(parent).unwrap();
    let root = parent.join("bundle");
    if crear_root {
        fs::create_dir(&root).unwrap();
    }
    (UbicacionRaiz::nueva(&root).unwrap(), root)
}

fn contexto_path(ubicacion: &UbicacionRaiz) -> std::path::PathBuf {
    let parent = ubicacion.ruta().parent().unwrap();
    parent.join(nombre_canonico(ubicacion.ruta().file_name().unwrap()).unwrap())
}

fn parent_identity(path: &Path) -> DirectoryIdentity {
    let parent = super::super::abrir_directorio_absoluto(path, true).unwrap();
    metadata_directorio(&parent)
        .unwrap()
        .journal_identity()
        .unwrap()
}

#[test]
fn reabre_el_contexto_despues_de_soltar_y_readquirir_el_lock_en_el_proceso() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let esperado = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    fs::write(contexto_path(&ubicacion), &esperado).unwrap();
    {
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner =
            super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
                .unwrap();
        let lease = owner.contexto_admision().unwrap();
        let (envelope, _) = lease.leer_canonico().unwrap().unwrap();
        assert_eq!(codec::codificar(&envelope).unwrap(), esperado);
    }
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (envelope, _) = lease.leer_canonico().unwrap().unwrap();

    assert_eq!(codec::codificar(&envelope).unwrap(), esperado);
}

#[test]
fn solo_devuelve_ausencia_observada_con_root_presente_o_ausente() {
    for crear_root in [false, true] {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (ubicacion, root) = ubicacion(&parent, crear_root);
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner =
            super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
                .unwrap();
        let lease = owner.contexto_admision().unwrap();

        assert!(lease.leer_canonico().unwrap().is_none());
        assert_eq!(root.is_dir(), crear_root);
        assert!(!contexto_path(&ubicacion).exists());
    }
}

#[test]
fn rechaza_directorio_canonico_y_conserva_su_contenido() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let contexto = contexto_path(&ubicacion);
    fs::create_dir(&contexto).unwrap();
    fs::write(contexto.join("canary"), b"preserve directory").unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();

    let error = lease.leer_canonico().unwrap_err();

    assert!(matches!(
        error,
        ErrorContextoAdmisionAnclado::EntryRejected | ErrorContextoAdmisionAnclado::Io
    ));
    assert_eq!(
        fs::read(contexto.join("canary")).unwrap(),
        b"preserve directory"
    );
}

#[test]
fn lease_lee_el_parent_capturado_y_preserva_el_decoy_del_namespace() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let (ubicacion, root) = ubicacion(&parent, true);
    let actual = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let decoy = b"decoy context canary";
    fs::write(contexto_path(&ubicacion), &actual).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32)),
                "{error}"
            );
            assert!(root.is_dir());
            assert!(!moved_parent.exists());
            assert_eq!(fs::read(contexto_path(&ubicacion)).unwrap(), actual);
            eprintln!("Windows bloqueó el rename con el lock abierto; no se ejercitó el decoy.");
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent de prueba: {error}");
    }

    fs::create_dir(&parent).unwrap();
    fs::create_dir(parent.join("bundle")).unwrap();
    let decoy_context_path = contexto_path(&ubicacion);
    fs::write(&decoy_context_path, decoy).unwrap();
    let ruta_lock = ubicacion.lock();
    let lock_name = ruta_lock.as_path().file_name().unwrap().to_os_string();
    fs::write(parent.join(lock_name), b"decoy lock").unwrap();

    let lease = owner.contexto_admision().unwrap();
    let (envelope, identity) = lease.leer_canonico().unwrap().unwrap();

    assert_eq!(codec::codificar(&envelope).unwrap(), actual);
    assert_eq!(
        identity,
        identidad_archivo(
            &File::open(
                moved_parent.join(nombre_canonico(ubicacion.ruta().file_name().unwrap()).unwrap(),)
            )
            .unwrap()
        )
        .unwrap()
    );
    assert_eq!(fs::read(&decoy_context_path).unwrap().as_slice(), decoy);
    assert!(moved_parent.join("bundle").is_dir());
    assert!(root.exists());
}

#[test]
fn error_de_codec_no_se_convierte_en_ausencia_y_preserva_bytes_redactados() {
    let transaction_id = "0123456789abcdef0123456789abcdef";
    for truncar in [false, true] {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (ubicacion, _) = ubicacion(&parent, true);
        let mut bytes = frame(transaction_id, parent_identity(&parent));
        if truncar {
            bytes.pop();
        } else {
            bytes[0] ^= 1;
        }
        let canonico = contexto_path(&ubicacion);
        fs::write(&canonico, &bytes).unwrap();
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner =
            super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
                .unwrap();
        let lease = owner.contexto_admision().unwrap();
        let lease_debug = format!("{lease:?}");
        assert_eq!(lease_debug, "ContextoAdmisionAnclado { .. }");
        assert!(!lease_debug.contains(canonico.to_string_lossy().as_ref()));
        assert!(!lease_debug.contains(transaction_id));

        let error = lease.leer_canonico().unwrap_err();

        assert!(matches!(error, ErrorContextoAdmisionAnclado::Codec(_)));
        assert_eq!(error.codigo(), "admission_envelope_framing_invalid");
        assert_eq!(error.to_string(), error.codigo());
        let debug = format!("{error:?}");
        assert!(!debug.contains(transaction_id));
        assert!(!debug.contains(canonico.to_string_lossy().as_ref()));
        assert_eq!(fs::read(&canonico).unwrap(), bytes);
    }
}

#[cfg(unix)]
#[test]
fn symlink_canonico_no_se_sigue_y_preserva_el_target_y_el_enlace() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let target = temp.path().join("target-canary");
    let link = contexto_path(&ubicacion);
    fs::write(&target, b"outside target canary").unwrap();
    symlink(&target, &link).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.contexto_admision().unwrap();

    let error = lease.leer_canonico().unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::EntryRejected);
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(&link).unwrap(), target);
    assert_eq!(fs::read(&target).unwrap(), b"outside target canary");
}

#[test]
fn lease_de_contexto_rechaza_lock_sustituido_con_bytes_iguales() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let bytes_contexto = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonico = contexto_path(&ubicacion);
    fs::write(&canonico, &bytes_contexto).unwrap();
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_bytes = b"same context lock bytes, different identity";
    fs::write(&lock_path, lock_bytes).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let identidad_guard = lock.identidad_archivo_lock().unwrap();
    let mut owner =
        super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let desplazado = temp.path().join("original-lock-canary");

    if let Err(error) = fs::rename(&lock_path, &desplazado) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33)),
                "{error}"
            );
            assert!(lock_path.is_file());
            assert!(!desplazado.exists());
            drop(owner);
            drop(lock);
            let visible = File::open(&lock_path).unwrap();
            assert_eq!(identidad_archivo(&visible).unwrap(), identidad_guard);
            assert_eq!(
                fs::read(&lock_path).unwrap().as_slice(),
                lock_bytes.as_slice()
            );
            assert_eq!(fs::read(&canonico).unwrap(), bytes_contexto);
            eprintln!(
                "Windows bloqueó la sustitución del lock; no se ejercitó el rechazo por identidad."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo desplazar el lock de prueba: {error}");
    }

    fs::write(&lock_path, lock_bytes).unwrap();
    let visible = File::open(&lock_path).unwrap();
    assert_ne!(identidad_archivo(&visible).unwrap(), identidad_guard);

    let error = owner.contexto_admision().unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::OwnerUnverified);
    assert_eq!(error.codigo(), "admission_context_owner_unverified");
    drop(visible);
    drop(owner);
    drop(lock);
    assert_eq!(
        fs::read(&lock_path).unwrap().as_slice(),
        lock_bytes.as_slice()
    );
    assert_eq!(
        fs::read(&desplazado).unwrap().as_slice(),
        lock_bytes.as_slice()
    );
    assert_eq!(fs::read(&canonico).unwrap(), bytes_contexto);
}
