use super::*;
use crate::transaccion::anclado::intent_persistido::codificar_nuevo;
use crate::transaccion::anclado::{abrir_directorio_absoluto, scope_filesystem};
use crate::transaccion::journal::Journal;
use sha2::{Digest, Sha256};
use std::path::Path;

fn assert_revalidation_error(error: impl std::fmt::Debug + std::fmt::Display) {
    assert_eq!(error.to_string(), "transaction_owner_unverified");
    assert_eq!(
        format!("{error:?}"),
        "ErrorDuenioTransaccionAnclado(\"transaction_owner_unverified\")"
    );
}

fn frame_with_changed_context_digest(
    fixture: &super::FixtureFilaConservarNew,
    intent_path: &Path,
) -> Vec<u8> {
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let location = &fixture.publicada.ubicacion;
    let parent_path = location.ruta().parent().unwrap();
    let parent = abrir_directorio_absoluto(parent_path, true).unwrap();
    let scope = scope_filesystem(&parent).unwrap();
    let journal_location = location.journal();
    let journal_path = journal_location.as_path();
    let context_path = contexto_path(location);
    let context_bytes = fs::read(&context_path).unwrap();
    let mut context_digest: [u8; 32] = Sha256::digest(&context_bytes).into();
    context_digest[0] ^= 1;

    codificar_nuevo(
        &journal,
        id_archivo(intent_path),
        id_archivo(journal_path),
        id_archivo(&context_path),
        scope.0,
        u64::try_from(context_bytes.len()).unwrap(),
        context_digest,
    )
    .unwrap()
}

#[test]
fn revalidar_observacion_sin_cambios_la_reemite_y_preserva_almacenamiento() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonical, _) = intent_paths(&fixture);
    let bytes = fs::read(&canonical).unwrap();
    let identity = id_archivo(&canonical);
    let storage_ids = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let observation = owner.observar_intent_canonico().unwrap();
    let fresh = observation.revalidar().unwrap();
    assert_eq!(format!("{fresh:?}"), "ObservacionIntentCanonico { .. }");
    drop(fresh);

    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert!(id_archivo(&canonical) == identity);
    verificar_storage(&fixture, storage_ids);
}

#[test]
fn mismo_id_con_frame_valido_distinto_falla_y_preserva_el_nuevo_frame() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonical, _) = intent_paths(&fixture);
    let original_id = id_archivo(&canonical);
    let storage_ids = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observation = owner.observar_intent_canonico().unwrap();

    let changed_frame = frame_with_changed_context_digest(&fixture, &canonical);
    let original_frame = fs::read(&canonical).unwrap();
    assert_ne!(changed_frame, original_frame);
    fs::write(&canonical, &changed_frame).unwrap();
    assert!(id_archivo(&canonical) == original_id);

    assert_revalidation_error(observation.revalidar().unwrap_err());

    assert_eq!(fs::read(&canonical).unwrap(), changed_frame);
    assert!(id_archivo(&canonical) == original_id);
    verificar_storage(&fixture, storage_ids);
}

#[test]
fn mismo_frame_con_id_canonico_nuevo_falla_y_preserva_ambos_archivos() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonical, _) = intent_paths(&fixture);
    let original = canonical.with_file_name("retained-original-intent");
    let bytes = fs::read(&canonical).unwrap();
    let original_id = id_archivo(&canonical);
    let storage_ids = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observation = owner.observar_intent_canonico().unwrap();

    fs::rename(&canonical, &original).unwrap();
    fs::write(&canonical, &bytes).unwrap();
    let replacement_id = id_archivo(&canonical);
    assert!(original_id != replacement_id);

    assert_revalidation_error(observation.revalidar().unwrap_err());

    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert!(id_archivo(&original) == original_id);
    assert!(id_archivo(&canonical) == replacement_id);
    verificar_storage(&fixture, storage_ids);
}
