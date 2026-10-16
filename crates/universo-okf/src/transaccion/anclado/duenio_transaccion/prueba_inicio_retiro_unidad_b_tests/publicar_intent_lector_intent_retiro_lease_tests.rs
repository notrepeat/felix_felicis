use super::*;

#[test]
fn lease_readonly_revalida_y_retorna_el_mismo_intent() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let bytes = fs::read(&canonico).unwrap();
    let identidad = id_archivo(&canonico);
    let ids_storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let observacion = owner.observar_intent_canonico().unwrap();
    let lease = observacion.abrir_lease_retiro().unwrap().unwrap();

    assert_eq!(format!("{lease:?}"), "LeaseIntentRetiro { .. }");
    assert_eq!(fs::read(&canonico).unwrap(), bytes);
    assert!(id_archivo(&canonico) == identidad);
    assert!(!temporal.exists());
    drop(lease);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn ausencia_real_tras_observacion_no_crea_un_intent() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let retenido = canonico.with_file_name("retained-absent-intent");
    let bytes = fs::read(&canonico).unwrap();
    let identidad = id_archivo(&canonico);
    let ids_storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observacion = owner.observar_intent_canonico().unwrap();

    fs::rename(&canonico, &retenido).unwrap();
    let ausencia = observacion.abrir_lease_retiro().unwrap();

    assert!(ausencia.is_none());
    assert!(!canonico.exists());
    assert!(!temporal.exists());
    assert_eq!(fs::read(&retenido).unwrap(), bytes);
    assert!(id_archivo(&retenido) == identidad);
    drop(ausencia);
    verificar_storage(&fixture, ids_storage);
}
