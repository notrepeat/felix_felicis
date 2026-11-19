use super::*;
use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, codec, tests as fixtures};
use crate::transaccion::anclado::intent_persistido::codificar_nuevo;
use crate::transaccion::anclado::scope_filesystem;
use crate::transaccion::journal::Journal;
use crate::transaccion::journal::recovery::ArtifactState;
use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
use sha2::{Digest, Sha256};
use std::fs::{self, File};

#[test]
fn fila_a_con_replay_soportado_observa_y_preserva_todos_los_datos() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (intent_path, intent_temporary) = intent_paths(&fixture);
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let journal_path = {
        let location = fixture.publicada.ubicacion.journal();
        location.as_path().to_path_buf()
    };
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let context_bytes = fs::read(&context_path).unwrap();
    let context_id = id_archivo(&context_path);
    let journal_bytes = fs::read(&journal_path).unwrap();
    let journal_id = id_archivo(&journal_path);
    let storage = capturar_storage(&fixture);
    let expected_journal = Journal::parse(&journal_bytes).unwrap();

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let inventory = owner
        .observar_inventario_recovery_con_journal_datos(&expected_journal)
        .unwrap();
    assert_eq!(inventory.root, ArtifactState::New);
    assert_eq!(inventory.backup, ArtifactState::Absent);
    assert_eq!(inventory.stage, ArtifactState::Absent);
    assert!(!inventory.root_new_validated);
    assert_rechazo(owner.probar_replay_admitido_root_coincidente());
    let observed = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap();

    assert_eq!(
        format!("{observed:?}"),
        "ObservacionIntentConReplayYInventario { .. }"
    );
    drop(observed);
    assert_rechazo(owner.probar_replay_admitido_root_coincidente());
    assert!(id_archivo(&journal_path) == journal_id);
    assert!(id_archivo(&context_path) == context_id);
    assert!(!intent_temporary.exists());
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert_eq!(fs::read(&journal_path).unwrap(), journal_bytes);
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    verificar_storage(&fixture, storage);
}

#[test]
fn journal_canonico_con_mismos_bytes_y_otro_id_rechaza_la_fila_a() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let retained = journal_path.with_file_name("journal-original-retained");
    let original_bytes = fs::read(&journal_path).unwrap();
    let original_id = id_archivo(&journal_path);
    fs::rename(&journal_path, &retained).unwrap();
    fs::write(&journal_path, &original_bytes).unwrap();
    let replacement_id = id_archivo(&journal_path);
    assert!(replacement_id != original_id);

    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let context_bytes = fs::read(&context_path).unwrap();
    let context_id = id_archivo(&context_path);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let error = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap_err();

    assert_eq!(error.to_string(), "transaction_owner_unverified");
    assert_eq!(
        format!("{error:?}"),
        "ErrorDuenioTransaccionAnclado(\"transaction_owner_unverified\")"
    );
    assert_eq!(fs::read(&journal_path).unwrap(), original_bytes);
    assert_eq!(fs::read(&retained).unwrap(), original_bytes);
    assert!(id_archivo(&journal_path) == replacement_id);
    assert!(id_archivo(&retained) == original_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
}

#[test]
fn contexto_canonico_con_mismos_bytes_y_otro_id_rechaza_la_fila_a() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let retained = context_path.with_file_name("context-original-retained");
    let original_bytes = fs::read(&context_path).unwrap();
    let original_id = id_archivo(&context_path);
    fs::rename(&context_path, &retained).unwrap();
    fs::write(&context_path, &original_bytes).unwrap();
    let replacement_id = id_archivo(&context_path);
    assert!(replacement_id != original_id);

    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let journal_bytes = fs::read(&journal_path).unwrap();
    let journal_id = id_archivo(&journal_path);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let error = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap_err();

    assert_eq!(error.to_string(), "transaction_owner_unverified");
    assert_eq!(fs::read(&context_path).unwrap(), original_bytes);
    assert_eq!(fs::read(&retained).unwrap(), original_bytes);
    assert!(id_archivo(&context_path) == replacement_id);
    assert!(id_archivo(&retained) == original_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert_eq!(fs::read(&journal_path).unwrap(), journal_bytes);
    assert!(id_archivo(&journal_path) == journal_id);
}

#[test]
fn contexto_valido_distinto_rechaza_binding_antiguo_y_acepta_el_actualizado() {
    let temp = tempdir().unwrap();
    let mut fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let intent_path = intent_paths(&fixture).0;
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let contexto = fixtures::contexto_con_tags(b" ", VERSION_MOTOR_V1, POLICY_ID_V1);
    let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);
    let replay = envelope.validar_replay_soportado(&journal).unwrap();
    assert!(replay.new_fingerprint() == journal.new_fingerprint());
    let new_context_bytes = codec::codificar(&envelope).unwrap();
    assert_ne!(new_context_bytes, fixture.publicada.bytes_contexto);
    assert_eq!(
        new_context_bytes.len(),
        fixture.publicada.bytes_contexto.len()
    );
    let original_context_id = id_archivo(&context_path);
    fs::write(&context_path, &new_context_bytes).unwrap();
    assert!(id_archivo(&context_path) == original_context_id);
    fixture.publicada.bytes_contexto = new_context_bytes.clone();

    let context_handle = File::open(&context_path).unwrap();
    let scope = scope_filesystem(&context_handle).unwrap();
    let context_id = id_archivo(&context_path);
    let journal_id = id_archivo(&journal_path);
    let intent_id = id_archivo(&intent_path);
    let intent_bytes_original = fs::read(&intent_path).unwrap();
    let storage = capturar_storage(&fixture);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let error = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap_err();

    assert_eq!(error.to_string(), "transaction_owner_unverified");
    assert_eq!(
        format!("{error:?}"),
        "ErrorDuenioTransaccionAnclado(\"transaction_owner_unverified\")"
    );
    drop(owner);
    drop(lock);
    assert_eq!(fs::read(&context_path).unwrap(), new_context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes_original);
    assert!(id_archivo(&intent_path) == intent_id);
    verificar_storage(&fixture, storage);

    let digest_contexto: [u8; 32] = Sha256::digest(&new_context_bytes).into();
    let new_intent_bytes = codificar_nuevo(
        &journal,
        intent_id,
        journal_id,
        context_id,
        scope.0,
        u64::try_from(new_context_bytes.len()).unwrap(),
        digest_contexto,
    )
    .unwrap();
    fs::write(&intent_path, &new_intent_bytes).unwrap();
    let context_id_after = id_archivo(&context_path);
    let intent_id_after = id_archivo(&intent_path);
    assert!(context_id_after == context_id);
    assert!(intent_id_after == intent_id);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observed = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap();

    assert_eq!(
        format!("{observed:?}"),
        "ObservacionIntentConReplayYInventario { .. }"
    );
    assert_eq!(fs::read(&intent_path).unwrap(), new_intent_bytes);
    assert_eq!(fs::read(&context_path).unwrap(), new_context_bytes);
    assert_eq!(
        fs::read(&journal_path).unwrap(),
        fixture.publicada.bytes_journal
    );
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}
