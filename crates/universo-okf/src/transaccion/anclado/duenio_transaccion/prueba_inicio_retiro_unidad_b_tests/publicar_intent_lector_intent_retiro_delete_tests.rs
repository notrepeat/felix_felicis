use super::*;
use crate::transaccion::anclado::duenio_transaccion::intent_retiro::EstadoRetiroIntent;

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
