use super::*;
use crate::transaccion::anclado::journal_temporal::{
    ErrorJournalAnclado, ResultadoPublicacionJournal,
};
use crate::transaccion::journal::{
    ArtifactFingerprint, DirectoryIdentity, Journal, JournalState, RawTreeDigest,
};
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

fn identidad_journal_prueba(id: u64) -> DirectoryIdentity {
    #[cfg(unix)]
    {
        DirectoryIdentity::unix(7, id)
    }
    #[cfg(windows)]
    {
        DirectoryIdentity::windows(7, format!("{id:032x}"))
    }
}

fn fingerprint_journal_prueba(id: u64, digito: char) -> ArtifactFingerprint {
    ArtifactFingerprint::new(
        identidad_journal_prueba(id),
        RawTreeDigest::from_hex(digito.to_string().repeat(64)).unwrap(),
    )
}

fn journal_v1_prueba(parent: DirectoryIdentity) -> Journal {
    Journal::new(
        "0123456789abcdef0123456789abcdef",
        parent,
        fingerprint_journal_prueba(2, 'a'),
        fingerprint_journal_prueba(3, 'b'),
    )
    .unwrap()
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

#[test]
fn leases_secuenciales_guardan_y_leen_journal_v1_con_root_presente_o_ausente() {
    for crear_root in [false, true] {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (ubicacion, root) = abrir_root_y_lock(&parent, crear_root);
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let identidad_parent = owner.identidad_parent.journal_identity().unwrap();
        // Fixture válida para storage v1; los fingerprints Old/New son sintéticos
        // y no representan una admisión ni una decisión de recovery.
        let esperado = journal_v1_prueba(identidad_parent);
        let bytes = esperado.encode().unwrap();
        let temporal = parent.join(ubicacion.nombre_journal_temporal());
        let canonico = ubicacion.journal().as_path().to_path_buf();

        {
            let lease = owner.journal_temporal().unwrap();
            let identidad = lease.crear_nuevo(&bytes).unwrap();
            assert_eq!(fs::read(&temporal).unwrap(), bytes);
            assert!(matches!(
                lease.publicar(identidad, &esperado),
                ResultadoPublicacionJournal::Confirmado { .. }
            ));
        }

        {
            let lease = owner.journal_temporal().unwrap();
            let (_, observado) = lease.cargar_canonico().unwrap().unwrap();
            assert_eq!(observado, esperado);
            assert_eq!(observado.state(), JournalState::Prepared);
        }

        assert_eq!(fs::read(&canonico).unwrap(), bytes);
        assert!(!temporal.exists());
        assert_eq!(root.is_dir(), crear_root);
    }
}

#[test]
fn lease_usa_parent_capturado_tras_moverlo_y_preserva_namespace_decoy() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let (ubicacion, root) = abrir_root_y_lock(&parent, true);
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_bytes: &[u8] = b"owner lease lockfile canary";
    fs::write(&lock_path, lock_bytes).unwrap();
    fs::write(parent.join("origin.txt"), b"captured parent canary").unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    #[cfg(windows)]
    let identidad_lock = lock.identidad_archivo_lock().unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let identidad_parent = owner.identidad_parent;

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32)),
                "{error}"
            );
            assert!(root.is_dir());
            assert!(!moved_parent.exists());
            assert_eq!(
                metadata_directorio(&owner.parent).unwrap(),
                identidad_parent
            );
            assert_eq!(
                owner._lock.identidad_archivo_lock().unwrap(),
                identidad_lock
            );
            eprintln!(
                "Windows impidió mover el parent retenido; el lease contra decoy no se ejercitó."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent de prueba: {error}");
    }

    fs::create_dir(&parent).unwrap();
    fs::create_dir(parent.join("bundle")).unwrap();
    let decoy_lock = parent.join(lock_path.file_name().unwrap());
    let decoy_journal = parent.join(
        ubicacion
            .journal()
            .as_path()
            .file_name()
            .expect("journal sibling tiene basename"),
    );
    let decoy_temporal = parent.join(ubicacion.nombre_journal_temporal());
    fs::write(&decoy_lock, lock_bytes).unwrap();
    fs::write(&decoy_journal, b"decoy canonical journal canary").unwrap();
    fs::write(&decoy_temporal, b"decoy temporary journal canary").unwrap();
    fs::write(parent.join("origin.txt"), b"decoy parent canary").unwrap();

    let journal = journal_v1_prueba(identidad_parent.journal_identity().unwrap());
    let bytes = journal.encode().unwrap();
    let moved_journal = moved_parent.join(
        ubicacion
            .journal()
            .as_path()
            .file_name()
            .expect("journal sibling tiene basename"),
    );
    let moved_temporal = moved_parent.join(ubicacion.nombre_journal_temporal());

    {
        let lease = owner.journal_temporal().unwrap();
        let identidad = lease.crear_nuevo(&bytes).unwrap();
        assert_eq!(fs::read(&moved_temporal).unwrap(), bytes);
        assert_eq!(
            fs::read(&decoy_temporal).unwrap(),
            b"decoy temporary journal canary"
        );
        assert!(matches!(
            lease.publicar(identidad, &journal),
            ResultadoPublicacionJournal::Confirmado { .. }
        ));
    }

    {
        let lease = owner.journal_temporal().unwrap();
        let (_, observado) = lease.cargar_canonico().unwrap().unwrap();
        assert_eq!(observado, journal);
    }

    assert_eq!(fs::read(&moved_journal).unwrap(), bytes);
    assert!(!moved_temporal.exists());
    assert_eq!(fs::read(&decoy_lock).unwrap(), lock_bytes);
    assert_eq!(
        fs::read(&decoy_journal).unwrap(),
        b"decoy canonical journal canary"
    );
    assert_eq!(
        fs::read(&decoy_temporal).unwrap(),
        b"decoy temporary journal canary"
    );
    assert_eq!(
        fs::read(moved_parent.join("origin.txt")).unwrap(),
        b"captured parent canary"
    );
    assert_eq!(
        fs::read(parent.join("origin.txt")).unwrap(),
        b"decoy parent canary"
    );
    assert!(moved_parent.join("bundle").is_dir());
    assert!(root.is_dir());
}

#[test]
fn lease_rechaza_lock_visible_sustituido_despues_de_capturar_owner() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, root) = abrir_root_y_lock(&parent, true);
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_bytes: &[u8] = b"same lock bytes, different lock identity";
    fs::write(&lock_path, lock_bytes).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let identidad_guard = lock.identidad_archivo_lock().unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let desplazado = temp.path().join("lock-original-retained");

    if let Err(error) = fs::rename(&lock_path, &desplazado) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33)),
                "{error}"
            );
            assert!(lock_path.is_file());
            assert!(!desplazado.exists());
            let visible = File::open(&lock_path).unwrap();
            assert_eq!(identidad_archivo(&visible).unwrap(), identidad_guard);
            assert_eq!(
                owner._lock.identidad_archivo_lock().unwrap(),
                identidad_guard
            );
            eprintln!(
                "Windows impidió sustituir el lock abierto; el rechazo por identidad distinta no se ejercitó."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo desplazar el lock de prueba: {error}");
    }

    fs::write(&lock_path, lock_bytes).unwrap();
    let visible = File::open(&lock_path).unwrap();
    assert_ne!(identidad_archivo(&visible).unwrap(), identidad_guard);

    let error = match owner.journal_temporal() {
        Ok(_) => panic!("un lock visible con identidad distinta no debe emitir lease"),
        Err(error) => error,
    };
    assert_eq!(error, ErrorJournalAnclado::IdentityMismatch);
    assert_eq!(error.codigo(), "journal_identity_mismatch");
    assert!(!parent.join(ubicacion.nombre_journal_temporal()).exists());
    assert!(!ubicacion.journal().as_path().exists());
    assert!(root.is_dir());
}
