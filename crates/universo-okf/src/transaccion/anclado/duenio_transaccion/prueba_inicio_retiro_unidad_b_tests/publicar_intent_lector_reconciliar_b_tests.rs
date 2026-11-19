use super::*;
use crate::transaccion::anclado::contexto_admision::nombre_temporal_sibling;
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

fn paths_temporales(fixture: &FixtureFilaConservarNew) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = fixture.publicada.ubicacion.ruta();
    let parent = root.parent().unwrap();
    let context = contexto_path(&fixture.publicada.ubicacion);
    let context_temporary =
        context.with_file_name(nombre_temporal_sibling(context.file_name().unwrap()).unwrap());
    let journal_temporary = parent.join(fixture.publicada.ubicacion.nombre_journal_temporal());
    (journal_temporary, context_temporary)
}

#[test]
fn con_journal_ausente_a_rechaza_y_b_observa_sin_recrear_ni_modificar() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let journal_bytes = fs::read(&journal_path).unwrap();
    assert_eq!(journal_bytes, fixture.publicada.bytes_journal);
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let context_bytes = fs::read(&context_path).unwrap();
    let context_id = id_archivo(&context_path);
    let (intent_path, intent_temporary) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let (journal_temporary, context_temporary) = paths_temporales(&fixture);
    fs::remove_file(&journal_path).unwrap();
    assert!(!journal_path.exists());

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert_error_no_verificable(
        owner
            .observar_intent_canonico()
            .unwrap()
            .observar_reconciliacion_a(),
    );
    let observed = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_b_sin_journal()
        .unwrap();

    assert_eq!(
        format!("{observed:?}"),
        "ObservacionIntentConReplayYInventarioSinJournal { .. }"
    );
    drop(observed);
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert!(!intent_temporary.exists());
    assert!(!journal_temporary.exists());
    assert!(!context_temporary.exists());
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

#[test]
fn journal_canonico_presente_bloquea_y_se_preserva() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let journal_bytes = fs::read(&journal_path).unwrap();
    let journal_id = id_archivo(&journal_path);
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let context_bytes = fs::read(&context_path).unwrap();
    let context_id = id_archivo(&context_path);
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

    assert_eq!(fs::read(&journal_path).unwrap(), journal_bytes);
    assert!(id_archivo(&journal_path) == journal_id);
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}
