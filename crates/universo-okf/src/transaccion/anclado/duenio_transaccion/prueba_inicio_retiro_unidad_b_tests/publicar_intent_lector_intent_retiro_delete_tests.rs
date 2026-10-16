use super::*;
use crate::transaccion::anclado::duenio_transaccion::intent_retiro::EstadoRetiroIntent;
use std::cell::Cell;

#[test]
fn retirada_real_confirma_ausencia_y_conserva_resultado_owner_bound() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let ids_storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner
        .observar_intent_canonico()
        .unwrap()
        .abrir_lease_retiro()
        .unwrap()
        .unwrap();

    let resultado = lease.retirar();

    assert!(resultado.estado() == EstadoRetiroIntent::Eliminado);
    assert!(resultado.owner_binding_coincide_para_test());
    assert_eq!(format!("{resultado:?}"), "ResultadoRetiroIntent { .. }");
    assert!(!canonico.exists());
    assert!(!temporal.exists());
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn ausencia_previa_confirma_ausente_y_preserva_archivo_retenido() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let retenido = canonico.with_file_name("retained-absent-intent");
    let bytes = fs::read(&canonico).unwrap();
    let identidad = id_archivo(&canonico);
    let ids_storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner
        .observar_intent_canonico()
        .unwrap()
        .abrir_lease_retiro()
        .unwrap()
        .unwrap();
    fs::rename(&canonico, &retenido).unwrap();

    let resultado = lease.retirar();

    assert!(resultado.estado() == EstadoRetiroIntent::Ausente);
    assert!(resultado.owner_binding_coincide_para_test());
    assert!(!canonico.exists());
    assert!(!temporal.exists());
    assert_eq!(fs::read(&retenido).unwrap(), bytes);
    assert!(id_archivo(&retenido) == identidad);
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn error_de_callback_con_source_presente_es_indeterminado() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let ids_storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner
        .observar_intent_canonico()
        .unwrap()
        .abrir_lease_retiro()
        .unwrap()
        .unwrap();
    let llamadas_callback = Cell::new(0);

    let resultado = lease.retirar_con_intento_para_test(|_parent, _nombre, archivo| {
        llamadas_callback.set(llamadas_callback.get() + 1);
        drop(archivo);
        Err(())
    });

    assert!(resultado.estado() == EstadoRetiroIntent::Indeterminado);
    assert_eq!(llamadas_callback.get(), 1);
    assert!(canonico.exists());
    assert!(!temporal.exists());
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn error_de_callback_con_nombre_ausente_es_ausente_sin_causalidad() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let retenido = canonico.with_file_name("retained-error-absent-intent");
    let bytes = fs::read(&canonico).unwrap();
    let identidad = id_archivo(&canonico);
    let ids_storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner
        .observar_intent_canonico()
        .unwrap()
        .abrir_lease_retiro()
        .unwrap()
        .unwrap();
    let llamadas_callback = Cell::new(0);
    let mutaciones_fixture = Cell::new(0);

    let resultado = lease.retirar_con_intento_para_test(|_parent, _nombre, archivo| {
        llamadas_callback.set(llamadas_callback.get() + 1);
        drop(archivo);
        fs::rename(&canonico, &retenido).unwrap();
        mutaciones_fixture.set(mutaciones_fixture.get() + 1);
        Err(())
    });

    assert!(resultado.estado() == EstadoRetiroIntent::Ausente);
    assert_eq!(llamadas_callback.get(), 1);
    assert_eq!(mutaciones_fixture.get(), 1);
    assert!(!canonico.exists());
    assert!(!temporal.exists());
    assert_eq!(fs::read(&retenido).unwrap(), bytes);
    assert!(id_archivo(&retenido) == identidad);
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn fallo_de_owner_despues_de_error_prioriza_indeterminado() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let retenido = canonico.with_file_name("retained-owner-failure-intent");
    let bytes = fs::read(&canonico).unwrap();
    let identidad = id_archivo(&canonico);
    let ids_storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner
        .observar_intent_canonico()
        .unwrap()
        .abrir_lease_retiro()
        .unwrap()
        .unwrap();
    let llamadas_callback = Cell::new(0);

    let resultado = lease.retirar_con_intento_para_test(|_parent, _nombre, archivo| {
        llamadas_callback.set(llamadas_callback.get() + 1);
        drop(archivo);
        fs::rename(&canonico, &retenido).unwrap();
        fs::write(&temporal, b"blocks owner revalidation").unwrap();
        Err(())
    });

    assert!(resultado.estado() == EstadoRetiroIntent::Indeterminado);
    assert_eq!(llamadas_callback.get(), 1);
    assert!(!canonico.exists());
    assert!(temporal.exists());
    assert_eq!(fs::read(&retenido).unwrap(), bytes);
    assert!(id_archivo(&retenido) == identidad);
    fs::remove_file(&temporal).unwrap();
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn rechazo_de_precondicion_no_invoca_callback() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let ids_storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner
        .observar_intent_canonico()
        .unwrap()
        .abrir_lease_retiro()
        .unwrap()
        .unwrap();
    fs::write(&temporal, b"blocks the precondition").unwrap();
    let llamadas_callback = Cell::new(0);

    let resultado = lease.retirar_con_intento_para_test(|_parent, _nombre, archivo| {
        llamadas_callback.set(llamadas_callback.get() + 1);
        drop(archivo);
        Err(())
    });

    assert!(resultado.estado() == EstadoRetiroIntent::Conflicto);
    assert_eq!(llamadas_callback.get(), 0);
    assert!(canonico.exists());
    assert!(temporal.exists());
    fs::remove_file(&temporal).unwrap();
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}
