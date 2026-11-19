use super::*;
use crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado;
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::raiz::UbicacionRaiz;
use std::cell::Cell;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn crear_decoy_lock(ubicacion: &UbicacionRaiz, moved: &Path) -> bool {
    let lock_path_owned = ubicacion.lock();
    let lock_path = lock_path_owned.as_path();
    if let Err(error) = fs::rename(lock_path, moved) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32)),
                "{error}"
            );
            return false;
        }
        #[cfg(not(windows))]
        panic!("no se pudo sustituir el lock de prueba: {error}");
    }
    fs::write(lock_path, b"same path, different lock file").unwrap();
    true
}

#[test]
fn lock_reemplazado_antes_del_lease_falla_cerrado_sin_crear_temporal() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let moved = temp.path().join("lock-original");
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
    let reemplazado = Cell::new(false);

    let resultado = match owner.crear_temporal_journal_v2_con_prueba(
        &journal,
        || reemplazado.set(crear_decoy_lock(&ubicacion, &moved)),
        || {},
        |owner, _| owner.parent_revalidado(),
    ) {
        Ok(_receipt) => Ok(()),
        Err(error) => Err(error),
    };

    if reemplazado.get() {
        assert_eq!(
            resultado.err().unwrap(),
            ErrorJournalAnclado::IdentityMismatch
        );
        assert!(!temporal.exists());
    } else {
        assert!(resultado.is_ok());
        assert!(temporal.is_file());
    }
}

#[test]
fn owner_postcheck_domina_tras_el_readback_y_preserva_el_temporal() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let moved = temp.path().join("lock-original");
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
    let reemplazado = Cell::new(false);

    let resultado = match owner.crear_temporal_journal_v2_con_prueba(
        &journal,
        || {},
        || reemplazado.set(crear_decoy_lock(&ubicacion, &moved)),
        |owner, _| owner.parent_revalidado(),
    ) {
        Ok(_receipt) => Ok(()),
        Err(error) => Err(error),
    };

    if reemplazado.get() {
        assert_eq!(
            resultado.err().unwrap(),
            ErrorJournalAnclado::IdentityMismatch
        );
    } else {
        assert!(resultado.is_ok());
    }
    assert!(temporal.is_file());
    assert!(JournalV2::parse(&fs::read(temporal).unwrap()).is_ok());
}

#[test]
fn owner_postcheck_domina_un_error_del_cuerpo() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let moved = temp.path().join("lock-original");
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let extranjero = journal_v2(identidad_fingerprint(92));
    let reemplazado = Cell::new(false);

    let resultado = match owner.crear_temporal_journal_v2_con_prueba(
        &extranjero,
        || {},
        || reemplazado.set(crear_decoy_lock(&ubicacion, &moved)),
        |owner, _| owner.parent_revalidado(),
    ) {
        Ok(_receipt) => Ok(()),
        Err(error) => Err(error),
    };

    if reemplazado.get() {
        assert_eq!(
            resultado.err().unwrap(),
            ErrorJournalAnclado::IdentityMismatch
        );
    } else {
        assert_eq!(
            resultado.err().unwrap(),
            ErrorJournalAnclado::ParentIdentityMismatch
        );
    }
    assert!(!temporal.exists());
}
