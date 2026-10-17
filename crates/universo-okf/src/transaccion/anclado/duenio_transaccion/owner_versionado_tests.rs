use super::{abrir_root_y_lock, journal_v1_prueba};
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::identidad_archivo;
use crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado;
use crate::transaccion::anclado::journal_temporal::versionado::ErrorLecturaJournalVersionada;
use crate::transaccion::journal::selector::JournalPersistido;
use crate::transaccion::lock::LockEscritura;
use std::cell::Cell;
use std::fs::{self, File};
use tempfile::tempdir;

#[test]
fn observacion_owner_conserva_registro_identidad_y_archivo() {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let (ubicacion, root) = abrir_root_y_lock(&parent_path, true);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let expected = journal_v1_prueba(owner.identidad_parent.journal_identity().unwrap());
    let bytes = expected.encode().unwrap();
    let canonical = ubicacion.journal().as_path().to_path_buf();
    fs::write(&canonical, &bytes).unwrap();

    let observation = owner.observar_journal_versionado().unwrap().unwrap();

    assert_eq!(observation.journal(), &JournalPersistido::V1(expected));
    assert_eq!(
        observation.identidad().identidad_archivo(),
        identidad_archivo(&File::open(&canonical).unwrap()).unwrap()
    );
    let debug = format!("{observation:?}");
    assert!(!debug.contains(canonical.to_string_lossy().as_ref()));
    assert!(!debug.contains("0123456789abcdef0123456789abcdef"));
    assert_eq!(fs::read(canonical).unwrap(), bytes);
    assert!(root.is_dir());
}

fn falla_postcheck_sin_publicar_ninguna_observacion(canonico_presente: bool) {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let (ubicacion, root) = abrir_root_y_lock(&parent_path, true);
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_canary = b"retained lock canary";
    fs::write(&lock_path, lock_canary).unwrap();
    let retained_bytes = fs::read(&lock_path).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let canonical = ubicacion.journal().as_path().to_path_buf();
    let journal_bytes = if canonico_presente {
        let journal = journal_v1_prueba(owner.identidad_parent.journal_identity().unwrap());
        let bytes = journal.encode().unwrap();
        fs::write(&canonical, &bytes).unwrap();
        Some(bytes)
    } else {
        None
    };
    let retained_lock = parent_path.join("lock-retained-after-reader");
    let replacement = b"replacement lock identity";
    let substituted = Cell::new(false);
    let rename_error = Cell::new(None);

    let result = owner.observar_journal_versionado_con_prueba(|| {
        match fs::rename(&lock_path, &retained_lock) {
            Ok(()) => {
                fs::write(&lock_path, replacement).unwrap();
                substituted.set(true);
            }
            Err(error) => rename_error.set(error.raw_os_error()),
        }
    });
    drop(owner);
    drop(lock);

    if !substituted.get() {
        #[cfg(windows)]
        {
            assert!(matches!(rename_error.get(), Some(5) | Some(32) | Some(33)));
            if let Some(bytes) = &journal_bytes {
                let observation = result.unwrap().unwrap();
                assert_eq!(fs::read(&canonical).unwrap().as_slice(), bytes.as_slice());
                drop(observation);
            } else {
                assert!(result.unwrap().is_none());
                assert!(!canonical.exists());
            }
            assert_eq!(fs::read(&lock_path).unwrap(), retained_bytes);
            assert!(root.is_dir());
            return;
        }
        #[cfg(not(windows))]
        panic!("el lock visible debía poder sustituirse durante la prueba");
    }

    assert_eq!(
        result.unwrap_err(),
        ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::IdentityMismatch)
    );
    assert_eq!(fs::read(&retained_lock).unwrap(), retained_bytes);
    assert_eq!(fs::read(&lock_path).unwrap().as_slice(), replacement);
    if let Some(bytes) = journal_bytes {
        assert_eq!(fs::read(canonical).unwrap(), bytes);
    } else {
        assert!(!canonical.exists());
    }
    assert!(root.is_dir());
}

#[test]
fn reemplazo_lock_despues_de_leer_descarta_observacion() {
    falla_postcheck_sin_publicar_ninguna_observacion(true);
}

#[test]
fn reemplazo_lock_despues_de_ausencia_no_devuelve_none() {
    falla_postcheck_sin_publicar_ninguna_observacion(false);
}

fn error_postcheck_sintetico_descarta_observacion(canonico_presente: bool) {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let (ubicacion, root) = abrir_root_y_lock(&parent_path, true);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let canonical = ubicacion.journal().as_path().to_path_buf();
    let journal_bytes = if canonico_presente {
        let journal = journal_v1_prueba(owner.identidad_parent.journal_identity().unwrap());
        let bytes = journal.encode().unwrap();
        fs::write(&canonical, &bytes).unwrap();
        Some(bytes)
    } else {
        None
    };

    let result = owner.observar_journal_versionado_con_posterror_prueba(
        || {},
        ErrorJournalAnclado::IdentityMismatch,
    );

    assert_eq!(
        result.unwrap_err(),
        ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::IdentityMismatch)
    );
    if let Some(bytes) = journal_bytes {
        assert_eq!(fs::read(&canonical).unwrap(), bytes);
    } else {
        assert!(!canonical.exists());
    }
    assert!(root.is_dir());
}

#[test]
fn posterror_sintetico_descarta_observacion_presente_y_preserva_archivo() {
    error_postcheck_sintetico_descarta_observacion(true);
}

#[test]
fn posterror_sintetico_descarta_none_y_preserva_ausencia() {
    error_postcheck_sintetico_descarta_observacion(false);
}
