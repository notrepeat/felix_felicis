use super::*;
use crate::transaccion::anclado::intent_persistido::{
    IntentRetiroPersistido, MAX_INTENT_BYTES, codificar_nuevo,
};
use crate::transaccion::anclado::{
    ErrorDirectorioAnclado, ScopeFilesystem, abrir_directorio_absoluto, identidad_archivo,
    metadata_directorio, scope_filesystem,
};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::{self, File};

use crate::transaccion::anclado::duenio_transaccion::lector_intent_canonico::{
    ErrorLecturaIntent, abrir_intent_canonico, leer_intent_canonico, mapear_error_lectura_intent,
};

fn assert_error_externo_redactado(error: ErrorLecturaIntent) {
    let error = mapear_error_lectura_intent(error);
    assert_eq!(error.to_string(), "transaction_owner_unverified");
    assert_eq!(
        format!("{error:?}"),
        "ErrorDuenioTransaccionAnclado(\"transaction_owner_unverified\")"
    );
}

fn abrir_parent(fixture: &FixtureFilaConservarNew) -> File {
    let (canonico, _) = super::intent_paths(fixture);
    abrir_directorio_absoluto(canonico.parent().unwrap(), true).unwrap()
}

#[test]
fn lector_tipado_acepta_frame_vinculado_y_rechaza_oversize() {
    let temp = tempdir().unwrap();
    let fixture = super::fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, _) = super::intent_paths(&fixture);
    let parent = abrir_parent(&fixture);
    let parent_id = metadata_directorio(&parent).unwrap();
    let scope = scope_filesystem(&parent).unwrap();
    let archivo = abrir_intent_canonico(&parent, canonico.file_name().unwrap())
        .unwrap()
        .unwrap();
    let bytes = fs::read(&canonico).unwrap();
    let registro_esperado = IntentRetiroPersistido::decode(&bytes).unwrap();

    let leido = leer_intent_canonico(archivo, scope, parent_id, None, scope_filesystem).unwrap();

    assert!(leido.scope == scope);
    assert_eq!(leido.registro, registro_esperado);
    assert_eq!(leido.identidad, identidad_archivo(&leido.archivo).unwrap());

    drop(leido);
    let bytes_oversize = vec![0xA5; MAX_INTENT_BYTES + 1];
    fs::write(&canonico, &bytes_oversize).unwrap();
    let archivo_oversize = File::open(&canonico).unwrap();
    let identidad_oversize = identidad_archivo(&archivo_oversize).unwrap();
    let retenido = archivo_oversize.try_clone().unwrap();

    let error = leer_intent_canonico(archivo_oversize, scope, parent_id, None, scope_filesystem)
        .err()
        .unwrap();

    assert_eq!(error, ErrorLecturaIntent::Discrepancia);
    assert_error_externo_redactado(error);
    assert_eq!(fs::read(&canonico).unwrap(), bytes_oversize);
    assert_eq!(identidad_archivo(&retenido).unwrap(), identidad_oversize);
}

#[test]
fn delegador_test_permite_lecturas_repetidas_de_la_observacion() {
    let temp = tempdir().unwrap();
    let fixture = super::fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, _) = super::intent_paths(&fixture);
    let bytes = fs::read(&canonico).unwrap();
    let identidad = id_archivo(&canonico);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observacion = owner.observar_intent_canonico().unwrap();

    let primero = observacion.leer_nucleo_tipado_para_test().unwrap();
    let segundo = observacion.leer_nucleo_tipado_para_test().unwrap();

    assert_eq!(primero.identidad, identidad);
    assert_eq!(segundo.identidad, identidad);
    assert_eq!(primero.registro, segundo.registro);
    assert_eq!(fs::read(&canonico).unwrap(), bytes);
    assert!(id_archivo(&canonico) == identidad);
}

#[test]
fn la_apertura_tipada_distingue_solo_la_ausencia_exacta() {
    let temp = tempdir().unwrap();
    let fixture = super::fixture_publicada_con_intent(&temp.path().join("parent"));
    let parent = abrir_parent(&fixture);

    assert!(matches!(
        abrir_intent_canonico(&parent, OsStr::new("missing-canonical-intent")),
        Ok(None)
    ));

    let (canonico, _) = super::intent_paths(&fixture);
    assert!(matches!(
        abrir_intent_canonico(&parent, canonico.file_name().unwrap()),
        Ok(Some(_))
    ));
}

#[test]
fn el_probe_inyectado_clasifica_io_y_el_wrapper_conserva_el_error_redactado() {
    let temp = tempdir().unwrap();
    let fixture = super::fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, _) = super::intent_paths(&fixture);
    let parent = abrir_parent(&fixture);
    let parent_id = metadata_directorio(&parent).unwrap();
    let scope = scope_filesystem(&parent).unwrap();
    let archivo = abrir_intent_canonico(&parent, canonico.file_name().unwrap())
        .unwrap()
        .unwrap();

    let error = leer_intent_canonico(archivo, scope, parent_id, None, |_| {
        Err(ErrorDirectorioAnclado::Io)
    })
    .err()
    .unwrap();

    assert_eq!(error, ErrorLecturaIntent::Io);
    assert_error_externo_redactado(error);
}

#[test]
fn un_scope_medido_distinto_es_discrepancia_conservando_el_handle() {
    let temp = tempdir().unwrap();
    let fixture = super::fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, _) = super::intent_paths(&fixture);
    let bytes = fs::read(&canonico).unwrap();
    let parent = abrir_parent(&fixture);
    let parent_id = metadata_directorio(&parent).unwrap();
    let scope = scope_filesystem(&parent).unwrap();
    let scope_distinto = ScopeFilesystem(scope.0.wrapping_add(1));
    let archivo = abrir_intent_canonico(&parent, canonico.file_name().unwrap())
        .unwrap()
        .unwrap();
    let identidad = identidad_archivo(&archivo).unwrap();

    let error = leer_intent_canonico(archivo, scope, parent_id, None, |_| Ok(scope_distinto))
        .err()
        .unwrap();

    assert_eq!(error, ErrorLecturaIntent::Discrepancia);
    assert_error_externo_redactado(error);
    assert_eq!(fs::read(&canonico).unwrap(), bytes);
    assert_eq!(
        identidad_archivo(&File::open(&canonico).unwrap()).unwrap(),
        identidad
    );
}

#[test]
fn mismo_frame_con_otro_id_nativo_se_clasifica_como_discrepancia() {
    let temp = tempdir().unwrap();
    let fixture = super::fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, _) = super::intent_paths(&fixture);
    let bytes = fs::read(&canonico).unwrap();
    let parent = abrir_parent(&fixture);
    let parent_id = metadata_directorio(&parent).unwrap();
    let scope = scope_filesystem(&parent).unwrap();
    let archivo_original = abrir_intent_canonico(&parent, canonico.file_name().unwrap())
        .unwrap()
        .unwrap();
    let identidad_original = identidad_archivo(&archivo_original).unwrap();
    let registro = IntentRetiroPersistido::decode(&bytes).unwrap();

    let decoy_path = canonico.with_file_name("intent-retiro-id-decoy");
    fs::write(&decoy_path, &bytes).unwrap();
    let archivo_decoy = File::open(&decoy_path).unwrap();
    let identidad_decoy = identidad_archivo(&archivo_decoy).unwrap();
    assert_ne!(identidad_original, identidad_decoy);
    let _original_retenido = archivo_original.try_clone().unwrap();
    let _decoy_retenido = archivo_decoy.try_clone().unwrap();

    let error = leer_intent_canonico(
        archivo_original,
        scope,
        parent_id,
        Some((identidad_decoy, &registro)),
        scope_filesystem,
    )
    .err()
    .unwrap();

    assert_eq!(error, ErrorLecturaIntent::Discrepancia);
    assert_error_externo_redactado(error);
    assert_eq!(fs::read(&canonico).unwrap(), bytes);
    assert_eq!(fs::read(&decoy_path).unwrap(), bytes);
    assert_eq!(
        identidad_archivo(&_original_retenido).unwrap(),
        identidad_original
    );
    assert_eq!(
        identidad_archivo(&_decoy_retenido).unwrap(),
        identidad_decoy
    );
}

#[test]
fn registro_valido_distinto_con_misma_identidad_y_longitud_es_discrepancia() {
    let temp = tempdir().unwrap();
    let fixture = super::fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, _) = super::intent_paths(&fixture);
    let bytes = fs::read(&canonico).unwrap();
    let parent = abrir_parent(&fixture);
    let parent_id = metadata_directorio(&parent).unwrap();
    let scope = scope_filesystem(&parent).unwrap();
    let archivo = abrir_intent_canonico(&parent, canonico.file_name().unwrap())
        .unwrap()
        .unwrap();
    let identidad_intent = identidad_archivo(&archivo).unwrap();
    let registro_original = IntentRetiroPersistido::decode(&bytes).unwrap();
    let journal = registro_original.journal().clone();
    let ubicacion_journal = fixture.publicada.ubicacion.journal();
    let ruta_journal = ubicacion_journal.as_path();
    let ruta_contexto = contexto_path(&fixture.publicada.ubicacion);
    let bytes_contexto = fs::read(&ruta_contexto).unwrap();
    let mut digest_contexto: [u8; 32] = Sha256::digest(&bytes_contexto).into();
    digest_contexto[0] ^= 1;
    let frame_distinto = codificar_nuevo(
        &journal,
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

    let error = leer_intent_canonico(
        archivo,
        scope,
        parent_id,
        Some((identidad_intent, &registro_distinto)),
        scope_filesystem,
    )
    .err()
    .unwrap();

    assert_eq!(error, ErrorLecturaIntent::Discrepancia);
    assert_error_externo_redactado(error);
    assert_eq!(fs::read(&canonico).unwrap(), bytes);
    assert_eq!(
        identidad_archivo(&File::open(&canonico).unwrap()).unwrap(),
        identidad_intent
    );
}
