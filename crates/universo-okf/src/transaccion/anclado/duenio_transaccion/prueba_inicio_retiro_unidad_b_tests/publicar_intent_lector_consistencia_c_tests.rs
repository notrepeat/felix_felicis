use super::*;
use crate::transaccion::journal::Journal;
use crate::transaccion::journal::recovery::ArtifactState;
use std::fs;

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

fn quitar_sidecars(
    fixture: &FixtureFilaConservarNew,
) -> (Journal, std::path::PathBuf, std::path::PathBuf) {
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let journal_path = fixture
        .publicada
        .ubicacion
        .journal()
        .as_path()
        .to_path_buf();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    fs::remove_file(&journal_path).unwrap();
    fs::remove_file(&context_path).unwrap();
    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    (journal, journal_path, context_path)
}

fn temporales(fixture: &FixtureFilaConservarNew) -> [std::path::PathBuf; 3] {
    let parent = fixture.publicada.root.parent().unwrap();
    let (intent, intent_temporary) = intent_paths(fixture);
    let journal_temporary = parent.join(fixture.publicada.ubicacion.nombre_journal_temporal());
    let context = contexto_path(&fixture.publicada.ubicacion);
    let context_temporary = parent.join(
        crate::transaccion::anclado::contexto_admision::nombre_temporal_sibling(
            context.file_name().unwrap(),
        )
        .unwrap(),
    );
    assert!(intent.exists());
    [intent_temporary, journal_temporary, context_temporary]
}

fn assert_sin_prueba_normal(owner: &mut DuenioTransaccionAnclado<'_>) {
    let error = owner.probar_replay_admitido_root_coincidente().unwrap_err();
    assert_eq!(error.to_string(), "admission_replay_root_not_proven");
}

#[test]
fn root_new_con_sidecars_ausentes_produce_consistencia_observacional_y_preserva() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let root_id = identidad_directorio(&fixture.publicada.root);
    let temporales = temporales(&fixture);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let inventory = owner
        .observar_inventario_recovery_con_journal_datos(&journal)
        .unwrap();
    assert_eq!(inventory.root, ArtifactState::New);
    assert_eq!(inventory.backup, ArtifactState::Absent);
    assert_eq!(inventory.stage, ArtifactState::Absent);
    assert!(!inventory.root_new_validated);

    assert_sin_prueba_normal(&mut owner);
    let observed = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_c_sin_sidecars()
        .unwrap()
        .observar_consistencia_root_c()
        .unwrap();
    assert_eq!(
        format!("{observed:?}"),
        "ObservacionIntentConsistenciaRootC { .. }"
    );
    drop(observed);
    assert_sin_prueba_normal(&mut owner);
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}

#[test]
fn root_con_mismo_arbol_y_otro_id_rechaza_y_preserva() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let root = fixture.publicada.root.clone();
    let retained_root = root.with_file_name("root-original-retained");
    let original_root_id = identidad_directorio(&root);
    fs::rename(&root, &retained_root).unwrap();
    fs::create_dir(&root).unwrap();
    materializar(&root, &fixture.publicada.entradas);
    let replacement_root_id = identidad_directorio(&root);
    assert!(replacement_root_id != original_root_id);
    verificar_arbol(&root, &fixture.publicada.entradas);

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
            .observar_reconciliacion_c_sin_sidecars()
            .unwrap()
            .observar_consistencia_root_c(),
    );
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert!(identidad_directorio(&retained_root) == original_root_id);
    assert!(identidad_directorio(&root) == replacement_root_id);
    verificar_arbol(&retained_root, &fixture.publicada.entradas);
    verificar_arbol(&root, &fixture.publicada.entradas);
}

#[test]
fn backup_old_bloquea_consistencia_c_y_se_preserva() {
    let temp = tempdir().unwrap();
    let fixture = super::reconciliar_a_inventario_tests::fixture_publicada_con_backup_old(
        &temp.path().join("parent"),
    );
    let (journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
    let backup_id = fixture.identidad_backup_old.as_ref().unwrap().clone();
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
            .observar_reconciliacion_c_sin_sidecars()
            .unwrap()
            .observar_consistencia_root_c(),
    );
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert!(identidad_directorio(&backup) == backup_id);
    verificar_arbol(&backup, fixture.entradas_old.as_ref().unwrap());
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

#[test]
fn stage_unknown_bloquea_consistencia_c_y_se_preserva() {
    let temp = tempdir().unwrap();
    let fixture = super::reconciliar_a_inventario_tests::fixture_publicada_con_backup_old(
        &temp.path().join("parent"),
    );
    let (journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
    let retained_backup = backup.with_file_name("backup-old-retained-for-stage");
    let backup_id = fixture.identidad_backup_old.as_ref().unwrap().clone();
    fs::rename(&backup, &retained_backup).unwrap();
    assert!(identidad_directorio(&retained_backup) == backup_id);

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
    let root_id = identidad_directorio(&fixture.publicada.root);
    assert!(stage_id != root_id);

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
            .observar_reconciliacion_c_sin_sidecars()
            .unwrap()
            .observar_consistencia_root_c(),
    );
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert!(identidad_directorio(&stage) == stage_id);
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    assert!(identidad_directorio(&retained_backup) == backup_id);
    verificar_arbol(&stage, &fixture.publicada.entradas);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    verificar_arbol(&retained_backup, fixture.entradas_old.as_ref().unwrap());
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
}
