use super::*;
use std::fs;

fn assert_no_verificable<T, E>(resultado: Result<T, E>)
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

fn mutar_archivo_root(fixture: &FixtureFilaConservarNew) -> (std::path::PathBuf, Vec<u8>) {
    let entrada = fixture
        .publicada
        .entradas
        .iter()
        .find(|entrada| {
            entrada.tipo() == crate::transaccion::snapshot::TipoEntradaSnapshot::Archivo
        })
        .unwrap();
    let path = fixture.publicada.root.join(entrada.ruta());
    let mut bytes = fs::read(&path).unwrap();
    bytes.push(0xa5);
    fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

#[test]
fn retira_journal_real_y_devuelve_b_fresca_preservando_contexto_e_intent() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let (intent_path, intent_temporary) = intent_paths(&fixture);
    let context_bytes = fs::read(&context_path).unwrap();
    let intent_bytes = fs::read(&intent_path).unwrap();
    let context_id = id_archivo(&context_path);
    let intent_id = id_archivo(&intent_path);
    let root_id = identidad_directorio(&fixture.publicada.root);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let b = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap()
        .retirar_journal_y_observar_b_sin_journal()
        .unwrap();

    assert_eq!(
        format!("{b:?}"),
        "ObservacionIntentConReplayYInventarioSinJournal { .. }"
    );
    drop(b);
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert!(!intent_temporary.exists());
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

#[test]
fn journal_sustituido_despues_de_a_bloquea_antes_del_seam_y_se_preserva() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let journal_retained = journal_path.with_file_name("journal-original-retained");
    let journal_bytes = fs::read(&journal_path).unwrap();
    let journal_id = id_archivo(&journal_path);
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let (intent_path, _) = intent_paths(&fixture);
    let context_bytes = fs::read(&context_path).unwrap();
    let intent_bytes = fs::read(&intent_path).unwrap();
    let context_id = id_archivo(&context_path);
    let intent_id = id_archivo(&intent_path);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let a = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap();
    fs::rename(&journal_path, &journal_retained).unwrap();
    fs::write(&journal_path, &journal_bytes).unwrap();
    let replacement_id = id_archivo(&journal_path);
    assert!(replacement_id != journal_id);
    let mut seam_calls = 0;

    assert_no_verificable(a.retirar_journal_y_observar_b_sin_journal_con_prueba(|| {
        seam_calls += 1;
    }));

    assert_eq!(seam_calls, 0);
    assert_eq!(fs::read(&journal_path).unwrap(), journal_bytes);
    assert!(id_archivo(&journal_path) == replacement_id);
    assert_eq!(fs::read(&journal_retained).unwrap(), journal_bytes);
    assert!(id_archivo(&journal_retained) == journal_id);
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

#[test]
fn root_mutado_despues_de_a_bloquea_antes_del_seam_y_preserva_journal() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let journal_bytes = fs::read(&journal_path).unwrap();
    let journal_id = id_archivo(&journal_path);
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let (intent_path, _) = intent_paths(&fixture);
    let context_bytes = fs::read(&context_path).unwrap();
    let intent_bytes = fs::read(&intent_path).unwrap();
    let context_id = id_archivo(&context_path);
    let intent_id = id_archivo(&intent_path);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let a = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap();
    let (root_path, root_bytes) = mutar_archivo_root(&fixture);
    let mut seam_calls = 0;

    assert_no_verificable(a.retirar_journal_y_observar_b_sin_journal_con_prueba(|| {
        seam_calls += 1;
    }));

    assert_eq!(seam_calls, 0);
    assert_eq!(fs::read(&root_path).unwrap(), root_bytes);
    assert_eq!(fs::read(&journal_path).unwrap(), journal_bytes);
    assert!(id_archivo(&journal_path) == journal_id);
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
}

#[test]
fn ausencia_inyectada_de_journal_permite_observar_b_y_preserva_contexto_intent_root() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let (intent_path, intent_temporary) = intent_paths(&fixture);
    let context_bytes = fs::read(&context_path).unwrap();
    let intent_bytes = fs::read(&intent_path).unwrap();
    let context_id = id_archivo(&context_path);
    let intent_id = id_archivo(&intent_path);
    let root_id = identidad_directorio(&fixture.publicada.root);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let a = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap();
    let b = a
        .retirar_journal_y_observar_b_sin_journal_con_prueba(|| {
            fs::remove_file(&journal_path).unwrap();
        })
        .unwrap();

    assert_eq!(
        format!("{b:?}"),
        "ObservacionIntentConReplayYInventarioSinJournal { .. }"
    );
    drop(b);
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert!(!intent_temporary.exists());
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

#[test]
fn ausencia_inyectada_con_root_mutado_no_basta_para_observar_b() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let (intent_path, _) = intent_paths(&fixture);
    let context_bytes = fs::read(&context_path).unwrap();
    let intent_bytes = fs::read(&intent_path).unwrap();
    let context_id = id_archivo(&context_path);
    let intent_id = id_archivo(&intent_path);
    let root_id = identidad_directorio(&fixture.publicada.root);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let a = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap();
    let mut root_mutado = None;

    assert_no_verificable(a.retirar_journal_y_observar_b_sin_journal_con_prueba(|| {
        fs::remove_file(&journal_path).unwrap();
        root_mutado = Some(mutar_archivo_root(&fixture));
    }));

    let (root_path, root_bytes) = root_mutado.unwrap();
    assert!(!journal_path.exists());
    assert_eq!(fs::read(&root_path).unwrap(), root_bytes);
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
}
