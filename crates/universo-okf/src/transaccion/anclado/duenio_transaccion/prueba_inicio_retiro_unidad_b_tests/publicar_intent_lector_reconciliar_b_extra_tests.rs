use super::*;
use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, codec, tests as fixtures};
use crate::transaccion::journal::Journal;
use crate::transaccion::journal::recovery::ArtifactState;
use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
use sha2::{Digest, Sha256};
use std::fs;

fn quitar_journal_canonico(fixture: &FixtureFilaConservarNew) -> (Journal, std::path::PathBuf) {
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let location = fixture.publicada.ubicacion.journal();
    let path = location.as_path().to_path_buf();
    fs::remove_file(&path).unwrap();
    assert!(!path.exists());
    (journal, path)
}

fn assert_error_no_verificable<T, E>(resultado: Result<T, E>)
where
    T: std::fmt::Debug,
    E: std::fmt::Debug + std::fmt::Display,
{
    let error = resultado.unwrap_err();
    assert_eq!(error.to_string(), "transaction_owner_unverified");
    assert_eq!(
        format!("{error:?}"),
        "ErrorDuenioTransaccionAnclado(\"transaction_owner_unverified\")"
    );
}

#[test]
fn contexto_mismo_id_y_frame_valido_con_sha_distinta_rechaza_y_preserva() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (journal, journal_path) = quitar_journal_canonico(&fixture);
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let original_context = fs::read(&context_path).unwrap();
    let original_context_id = id_archivo(&context_path);
    let new_envelope = EnvelopeContextoAdmision::ligar(
        fixtures::contexto_con_tags(b" ", VERSION_MOTOR_V1, POLICY_ID_V1),
        &journal,
    );
    let new_context = codec::codificar(&new_envelope).unwrap();
    assert_ne!(new_context, original_context);
    assert_eq!(new_context.len(), original_context.len());
    assert_ne!(
        Sha256::digest(&new_context),
        Sha256::digest(&original_context)
    );
    fs::write(&context_path, &new_context).unwrap();
    assert!(id_archivo(&context_path) == original_context_id);

    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    assert_error_no_verificable(
        owner
            .observar_intent_canonico()
            .unwrap()
            .observar_reconciliacion_b_sin_journal(),
    );
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert_eq!(fs::read(&context_path).unwrap(), new_context);
    assert!(id_archivo(&context_path) == original_context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

#[test]
fn root_recreado_con_otro_id_se_clasifica_unknown_y_bloquea_sin_journal() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (journal, journal_path) = quitar_journal_canonico(&fixture);
    let root = &fixture.publicada.root;
    let retained_root = root.with_file_name("original-root-retained");
    let original_root_id = identidad_directorio(root);
    fs::rename(root, &retained_root).unwrap();
    fs::create_dir(root).unwrap();
    materializar(root, &fixture.publicada.entradas);
    let replacement_root_id = identidad_directorio(root);
    assert!(replacement_root_id != original_root_id);
    verificar_arbol(root, &fixture.publicada.entradas);

    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let context_bytes = fs::read(&context_path).unwrap();
    let context_id = id_archivo(&context_path);
    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let inventory = owner
        .observar_inventario_recovery_con_journal_datos(&journal)
        .unwrap();
    assert_eq!(inventory.root, ArtifactState::Unknown);
    assert_eq!(inventory.backup, ArtifactState::Absent);
    assert_eq!(inventory.stage, ArtifactState::Absent);
    assert!(!inventory.root_new_validated);

    assert_error_no_verificable(
        owner
            .observar_intent_canonico()
            .unwrap()
            .observar_reconciliacion_b_sin_journal(),
    );
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert!(identidad_directorio(&retained_root) == original_root_id);
    assert!(identidad_directorio(root) == replacement_root_id);
    verificar_arbol(&retained_root, &fixture.publicada.entradas);
    verificar_arbol(root, &fixture.publicada.entradas);
}

#[test]
fn backup_old_con_journal_ausente_se_clasifica_y_preserva_al_rechazar() {
    let temp = tempdir().unwrap();
    let fixture = super::reconciliar_a_inventario_tests::fixture_publicada_con_backup_old(
        &temp.path().join("parent"),
    );
    let (journal, journal_path) = quitar_journal_canonico(&fixture);
    let backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
    let backup_id = fixture.identidad_backup_old.as_ref().unwrap().clone();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let context_bytes = fs::read(&context_path).unwrap();
    let context_id = id_archivo(&context_path);
    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let inventory = owner
        .observar_inventario_recovery_con_journal_datos(&journal)
        .unwrap();
    assert_eq!(inventory.root, ArtifactState::New);
    assert_eq!(inventory.backup, ArtifactState::Old);
    assert_eq!(inventory.stage, ArtifactState::Absent);
    assert!(!inventory.root_new_validated);

    assert_error_no_verificable(
        owner
            .observar_intent_canonico()
            .unwrap()
            .observar_reconciliacion_b_sin_journal(),
    );
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(identidad_directorio(&backup) == backup_id);
    verificar_arbol(&backup, fixture.entradas_old.as_ref().unwrap());
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

#[test]
fn stage_unknown_con_journal_ausente_se_clasifica_y_preserva_al_rechazar() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (journal, journal_path) = quitar_journal_canonico(&fixture);
    let stage = fixture
        .publicada
        .ubicacion
        .stage(journal.transaction_id())
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&stage).unwrap();
    materializar(&stage, &fixture.publicada.entradas);
    let stage_id = identidad_directorio(&stage);
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let context_bytes = fs::read(&context_path).unwrap();
    let context_id = id_archivo(&context_path);
    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let inventory = owner
        .observar_inventario_recovery_con_journal_datos(&journal)
        .unwrap();
    assert_eq!(inventory.root, ArtifactState::New);
    assert_eq!(inventory.backup, ArtifactState::Absent);
    assert_eq!(inventory.stage, ArtifactState::Unknown);
    assert!(!inventory.root_new_validated);

    assert_error_no_verificable(
        owner
            .observar_intent_canonico()
            .unwrap()
            .observar_reconciliacion_b_sin_journal(),
    );
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(identidad_directorio(&stage) == stage_id);
    verificar_arbol(&stage, &fixture.publicada.entradas);
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}
