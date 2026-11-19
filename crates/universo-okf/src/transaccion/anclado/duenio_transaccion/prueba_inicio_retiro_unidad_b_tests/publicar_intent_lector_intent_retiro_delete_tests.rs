use super::*;
use crate::transaccion::anclado::duenio_transaccion::intent_retiro::EstadoRetiroIntent;
use crate::transaccion::anclado::intent_persistido::{IntentRetiroPersistido, codificar_nuevo};
use crate::transaccion::anclado::{abrir_directorio_absoluto, scope_filesystem};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::path::Path;

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

fn registro_valido_distinto_para_misma_identidad(
    fixture: &FixtureFilaConservarNew,
    canonico: &Path,
) -> Vec<u8> {
    let bytes = fs::read(canonico).unwrap();
    let registro_original = IntentRetiroPersistido::decode(&bytes).unwrap();
    let identidad_intent = id_archivo(canonico);
    let parent = abrir_directorio_absoluto(canonico.parent().unwrap(), true).unwrap();
    let scope = scope_filesystem(&parent).unwrap();
    let ubicacion_journal = fixture.publicada.ubicacion.journal();
    let ruta_journal = ubicacion_journal.as_path();
    let ruta_contexto = contexto_path(&fixture.publicada.ubicacion);
    let bytes_contexto = fs::read(&ruta_contexto).unwrap();
    let mut digest_contexto: [u8; 32] = Sha256::digest(&bytes_contexto).into();
    digest_contexto[0] ^= 1;
    let frame_distinto = codificar_nuevo(
        registro_original.journal(),
        identidad_intent,
        id_archivo(ruta_journal),
        id_archivo(&ruta_contexto),
        scope.0,
        u64::try_from(bytes_contexto.len()).unwrap(),
        digest_contexto,
    )
    .unwrap();
    assert_eq!(frame_distinto.len(), bytes.len());
    let registro_distinto = IntentRetiroPersistido::decode(&frame_distinto).unwrap();
    assert_ne!(registro_original, registro_distinto);
    assert!(
        registro_distinto
            .validar_vinculo_archivo_intent(identidad_intent, scope)
            .is_ok()
    );
    frame_distinto
}

#[test]
fn exito_de_callback_con_source_presente_es_indeterminado() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
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
        Ok(())
    });

    assert!(resultado.estado() == EstadoRetiroIntent::Indeterminado);
    assert!(resultado.owner_binding_coincide_para_test());
    assert_eq!(llamadas_callback.get(), 1);
    assert!(canonico.exists());
    assert!(!temporal.exists());
    assert_eq!(fs::read(&canonico).unwrap(), bytes);
    assert!(id_archivo(&canonico) == identidad);
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn reemplazo_con_mismos_bytes_y_otro_id_es_conflicto() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let retenido = canonico.with_file_name("retained-original-same-bytes");
    let bytes = fs::read(&canonico).unwrap();
    let identidad_original = id_archivo(&canonico);
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
        fs::write(&canonico, &bytes).unwrap();
        Err(())
    });

    assert!(resultado.estado() == EstadoRetiroIntent::Conflicto);
    assert!(resultado.owner_binding_coincide_para_test());
    assert_eq!(llamadas_callback.get(), 1);
    assert!(!temporal.exists());
    assert_eq!(fs::read(&retenido).unwrap(), bytes);
    assert_eq!(fs::read(&canonico).unwrap(), bytes);
    assert!(id_archivo(&retenido) == identidad_original);
    assert!(id_archivo(&canonico) != identidad_original);
    fs::remove_file(&retenido).unwrap();
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn registro_valido_distinto_con_mismo_id_es_conflicto() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let identidad = id_archivo(&canonico);
    let frame_distinto = registro_valido_distinto_para_misma_identidad(&fixture, &canonico);
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
        fs::write(&canonico, &frame_distinto).unwrap();
        Err(())
    });

    assert!(resultado.estado() == EstadoRetiroIntent::Conflicto);
    assert!(resultado.owner_binding_coincide_para_test());
    assert_eq!(llamadas_callback.get(), 1);
    assert!(canonico.exists());
    assert!(!temporal.exists());
    assert_eq!(fs::read(&canonico).unwrap(), frame_distinto);
    assert!(id_archivo(&canonico) == identidad);
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn discrepancia_de_fuente_con_owner_invalido_es_indeterminado() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let identidad = id_archivo(&canonico);
    let frame_distinto = registro_valido_distinto_para_misma_identidad(&fixture, &canonico);
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
        fs::write(&canonico, &frame_distinto).unwrap();
        fs::write(&temporal, b"blocks owner revalidation").unwrap();
        Err(())
    });

    assert!(resultado.estado() == EstadoRetiroIntent::Indeterminado);
    assert!(resultado.owner_binding_coincide_para_test());
    assert_eq!(llamadas_callback.get(), 1);
    assert!(canonico.exists());
    assert!(temporal.exists());
    assert_eq!(fs::read(&canonico).unwrap(), frame_distinto);
    assert!(id_archivo(&canonico) == identidad);
    fs::remove_file(&temporal).unwrap();
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn ausencia_preaccion_no_invoca_callback_y_conserva_el_objeto_retenido() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let retenido = canonico.with_file_name("retained-preaction-absent");
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
    let llamadas_callback = Cell::new(0);

    let resultado = lease.retirar_con_intento_para_test(|_parent, _nombre, archivo| {
        llamadas_callback.set(llamadas_callback.get() + 1);
        drop(archivo);
        Err(())
    });

    assert!(resultado.estado() == EstadoRetiroIntent::Ausente);
    assert!(resultado.owner_binding_coincide_para_test());
    assert_eq!(llamadas_callback.get(), 0);
    assert!(!canonico.exists());
    assert!(!temporal.exists());
    assert_eq!(fs::read(&retenido).unwrap(), bytes);
    assert!(id_archivo(&retenido) == identidad);
    fs::remove_file(&retenido).unwrap();
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn exito_de_callback_con_nombre_ausente_se_clasifica_eliminado_observacionalmente() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, temporal) = intent_paths(&fixture);
    let retenido = canonico.with_file_name("retained-success-absent");
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
        Ok(())
    });

    assert!(resultado.estado() == EstadoRetiroIntent::Eliminado);
    assert!(resultado.owner_binding_coincide_para_test());
    assert_eq!(llamadas_callback.get(), 1);
    assert!(!canonico.exists());
    assert!(!temporal.exists());
    assert_eq!(fs::read(&retenido).unwrap(), bytes);
    assert!(id_archivo(&retenido) == identidad);
    fs::remove_file(&retenido).unwrap();
    drop(resultado);
    drop(owner);
    drop(lock);
    verificar_storage(&fixture, ids_storage);
}
