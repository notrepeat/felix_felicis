use super::*;
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

fn paths_temporales(fixture: &FixtureFilaConservarNew) -> [std::path::PathBuf; 3] {
    let ubicacion = &fixture.publicada.ubicacion;
    let root = ubicacion.ruta();
    let parent = root.parent().unwrap();
    let (intent, intent_temporal) = intent_paths(fixture);
    let journal_temporal = parent.join(ubicacion.nombre_journal_temporal());
    let context = contexto_path(ubicacion);
    let context_temporal =
        parent.join(nombre_temporal_sibling(context.file_name().unwrap()).unwrap());
    assert!(intent.exists());
    [intent_temporal, journal_temporal, context_temporal]
}

fn observar_a_falla(fixture: &FixtureFilaConservarNew) {
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert_error_no_verificable(
        owner
            .observar_intent_canonico()
            .unwrap()
            .observar_reconciliacion_a(),
    );
}

fn observar_b_falla(fixture: &FixtureFilaConservarNew) {
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert_error_no_verificable(
        owner
            .observar_intent_canonico()
            .unwrap()
            .observar_reconciliacion_b_sin_journal(),
    );
}

#[test]
fn ambos_sidecars_ausentes_a_y_b_rechazan_y_c_observa_sin_recrear() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let temporales = paths_temporales(&fixture);
    fs::remove_file(&journal_path).unwrap();
    fs::remove_file(&context_path).unwrap();
    assert!(!journal_path.exists());
    assert!(!context_path.exists());

    observar_a_falla(&fixture);
    observar_b_falla(&fixture);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observed = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_c_sin_sidecars()
        .unwrap();
    assert_eq!(
        format!("{observed:?}"),
        "ObservacionIntentConJournalYContextoAusentes { .. }"
    );
    drop(observed);
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    for temporal in temporales {
        assert!(!temporal.exists());
    }
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

#[test]
fn sidecar_presente_bloquea_c_y_preserva_el_estado() {
    for quitar_journal in [true, false] {
        let temp = tempdir().unwrap();
        let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
        let journal_location = fixture.publicada.ubicacion.journal();
        let journal_path = journal_location.as_path().to_path_buf();
        let context_path = contexto_path(&fixture.publicada.ubicacion);
        let missing_path = if quitar_journal {
            &journal_path
        } else {
            &context_path
        };
        let remaining_path = if quitar_journal {
            &context_path
        } else {
            &journal_path
        };
        fs::remove_file(missing_path).unwrap();
        let remaining_bytes = fs::read(remaining_path).unwrap();
        let remaining_id = id_archivo(remaining_path);
        let (intent_path, _) = intent_paths(&fixture);
        let intent_bytes = fs::read(&intent_path).unwrap();
        let intent_id = id_archivo(&intent_path);

        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        assert_error_no_verificable(
            owner
                .observar_intent_canonico()
                .unwrap()
                .observar_reconciliacion_c_sin_sidecars(),
        );
        drop(owner);
        drop(lock);

        assert!(!missing_path.exists());
        assert_eq!(fs::read(remaining_path).unwrap(), remaining_bytes);
        assert!(id_archivo(remaining_path) == remaining_id);
        assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
        assert!(id_archivo(&intent_path) == intent_id);
        verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    }
}
