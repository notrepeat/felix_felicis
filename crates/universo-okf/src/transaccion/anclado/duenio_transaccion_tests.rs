use super::*;
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use tempfile::tempdir;

fn abrir_root_y_lock(parent: &Path, crear_root: bool) -> (UbicacionRaiz, std::path::PathBuf) {
    fs::create_dir_all(parent).unwrap();
    let root = parent.join("bundle");
    if crear_root {
        fs::create_dir(&root).unwrap();
    }
    let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
    (ubicacion, root)
}

#[test]
fn desde_lock_retiene_parent_y_funciona_con_root_presente_o_ausente() {
    for crear_root in [false, true] {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (ubicacion, root) = abrir_root_y_lock(&parent, crear_root);
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let identidad_lock = lock.identidad_archivo_lock().unwrap();

        let owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        assert_eq!(
            owner.identidad_parent,
            metadata_directorio(&owner.parent).unwrap()
        );
        assert!(owner.scope_parent == scope_filesystem(&owner.parent).unwrap());
        assert_eq!(
            identidad_lock,
            owner._lock.identidad_archivo_lock().unwrap()
        );
        assert!(std::sync::Arc::ptr_eq(
            &owner._autoridad,
            &owner._lock.autoridad()
        ));
        assert_eq!(root.is_dir(), crear_root);
        assert_eq!(format!("{owner:?}"), "DuenioTransaccionAnclado { .. }");
        assert!(!format!("{owner:?}").contains(parent.to_string_lossy().as_ref()));
    }
}

#[test]
fn desde_lock_rechaza_parent_decoy_con_lockfile_de_mismos_bytes() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let (ubicacion, root) = abrir_root_y_lock(&parent, true);
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_bytes: &[u8] = b"same lock bytes do not mean same file";
    fs::write(&lock_path, lock_bytes).unwrap();
    assert_eq!(fs::read(&lock_path).unwrap().as_slice(), lock_bytes);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let identidad_guard = lock.identidad_archivo_lock().unwrap();
    let nombre_lock = lock_path.file_name().unwrap();

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32)),
                "{error}"
            );
            assert!(parent.join("bundle").is_dir());
            assert!(!moved_parent.exists());
            assert!(lock_path.is_file());
            assert_eq!(lock.identidad_archivo_lock().unwrap(), identidad_guard);
            drop(lock);
            assert_eq!(fs::read(&lock_path).unwrap().as_slice(), lock_bytes);
            eprintln!(
                "Windows impidió mover el parent con el lock abierto; el rechazo por decoy no se ejercitó."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent de prueba: {error}");
    }
    fs::create_dir(&parent).unwrap();
    fs::create_dir(parent.join("bundle")).unwrap();
    let lock_decoy = parent.join(nombre_lock);
    fs::write(&lock_decoy, lock_bytes).unwrap();
    let archivo_decoy = File::open(&lock_decoy).unwrap();

    assert_eq!(fs::read(&lock_decoy).unwrap().as_slice(), lock_bytes);
    assert_ne!(identidad_archivo(&archivo_decoy).unwrap(), identidad_guard);
    let error = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap_err();
    assert_eq!(error, ErrorDuenioTransaccionAnclado::NoVerificable);
    assert_eq!(error.codigo(), "transaction_owner_unverified");
    assert_eq!(
        format!("{error:?}"),
        "ErrorDuenioTransaccionAnclado(\"transaction_owner_unverified\")"
    );
    assert!(!format!("{error:?}").contains(parent.to_string_lossy().as_ref()));
    assert!(moved_parent.join("bundle").is_dir());
    assert!(root.exists());
}

#[test]
fn desde_lock_falla_cuando_el_parent_ya_no_existe() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let (ubicacion, _) = abrir_root_y_lock(&parent, false);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    #[cfg(windows)]
    let identidad_guard = lock.identidad_archivo_lock().unwrap();

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32)),
                "{error}"
            );
            assert!(parent.is_dir());
            assert!(!moved_parent.exists());
            assert_eq!(lock.identidad_archivo_lock().unwrap(), identidad_guard);
            eprintln!(
                "Windows impidió mover el parent con el lock abierto; la ausencia no se ejercitó."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent de prueba: {error}");
    }

    let error = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap_err();
    assert_eq!(error, ErrorDuenioTransaccionAnclado::NoVerificable);
    assert!(!parent.exists());
    assert!(moved_parent.is_dir());
}

#[test]
fn parent_capturado_no_se_redirige_a_un_decoy() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let (ubicacion, root) = abrir_root_y_lock(&parent, true);
    fs::write(parent.join("origin.txt"), b"captured parent").unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let identidad_parent = owner.identidad_parent;

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32)),
                "{error}"
            );
            assert!(parent.join("bundle").is_dir());
            assert!(!moved_parent.exists());
            assert_eq!(
                fs::read(parent.join("origin.txt")).unwrap(),
                b"captured parent"
            );
            assert_eq!(
                metadata_directorio(&owner.parent).unwrap(),
                identidad_parent
            );
            eprintln!(
                "Windows impidió mover el parent abierto; este caso no ejercitó el reemplazo por decoy."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent de prueba: {error}");
    }

    fs::create_dir(&parent).unwrap();
    fs::create_dir(parent.join("bundle")).unwrap();
    fs::write(parent.join("origin.txt"), b"decoy parent").unwrap();

    let mut origin = abrir_entrada_relativa(
        &owner.parent,
        std::ffi::OsStr::new("origin.txt"),
        TipoEntradaAnclada::Archivo,
    )
    .unwrap();
    let mut contenido = Vec::new();
    origin.read_to_end(&mut contenido).unwrap();

    assert_eq!(contenido, b"captured parent");
    assert_eq!(
        metadata_directorio(&owner.parent).unwrap(),
        identidad_parent
    );
    assert_eq!(
        fs::read(moved_parent.join("origin.txt")).unwrap(),
        b"captured parent"
    );
    assert_eq!(
        fs::read(parent.join("origin.txt")).unwrap(),
        b"decoy parent"
    );
    assert!(moved_parent.join("bundle").is_dir());
    assert!(root.exists());
}
