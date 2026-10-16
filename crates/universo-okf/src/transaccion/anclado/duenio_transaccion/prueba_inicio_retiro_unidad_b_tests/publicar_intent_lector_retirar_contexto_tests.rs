use super::*;
use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, codec, tests as fixtures};
use crate::transaccion::journal::Journal;
use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
use sha2::{Digest, Sha256};
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
fn retira_contexto_y_devuelve_c_fresca_preservando_intent_y_root() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let (intent_path, intent_temporary) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let root_id = identidad_directorio(&fixture.publicada.root);

    fs::remove_file(&journal_path).unwrap();
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let c = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_b_sin_journal()
        .unwrap()
        .retirar_contexto_y_observar_c_sin_sidecars()
        .unwrap();

    assert_eq!(
        format!("{c:?}"),
        "ObservacionIntentConJournalYContextoAusentes { .. }"
    );
    drop(c);
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert!(!intent_temporary.exists());
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

#[test]
fn contexto_cambiado_o_reemplazado_despues_de_b_bloquea_antes_del_hook() {
    for reemplazar in [false, true] {
        let temp = tempdir().unwrap();
        let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
        let journal_location = fixture.publicada.ubicacion.journal();
        let journal_path = journal_location.as_path().to_path_buf();
        let context_path = contexto_path(&fixture.publicada.ubicacion);
        let context_retained = context_path.with_file_name("context-original-retained");
        let context_bytes = fs::read(&context_path).unwrap();
        let context_id = id_archivo(&context_path);
        let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
        let changed_context = codec::codificar(&EnvelopeContextoAdmision::ligar(
            fixtures::contexto_con_tags(b" ", VERSION_MOTOR_V1, POLICY_ID_V1),
            &journal,
        ))
        .unwrap();
        assert_ne!(changed_context, context_bytes);
        assert_eq!(changed_context.len(), context_bytes.len());
        assert_ne!(
            Sha256::digest(&changed_context),
            Sha256::digest(&context_bytes)
        );
        let (intent_path, _) = intent_paths(&fixture);
        let intent_bytes = fs::read(&intent_path).unwrap();
        let intent_id = id_archivo(&intent_path);
        fs::remove_file(&journal_path).unwrap();

        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let b = owner
            .observar_intent_canonico()
            .unwrap()
            .observar_reconciliacion_b_sin_journal()
            .unwrap();

        if reemplazar {
            fs::rename(&context_path, &context_retained).unwrap();
            fs::write(&context_path, &context_bytes).unwrap();
            assert_ne!(id_archivo(&context_path), context_id);
        } else {
            fs::write(&context_path, &changed_context).unwrap();
            assert!(id_archivo(&context_path) == context_id);
        }
        let mut hook_calls = 0;

        assert_no_verificable(b.retirar_contexto_y_observar_c_sin_sidecars_con_prueba(|| {
            hook_calls += 1;
        }));

        assert_eq!(hook_calls, 0);
        assert!(!journal_path.exists());
        assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
        assert!(id_archivo(&intent_path) == intent_id);
        if reemplazar {
            assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
            assert!(id_archivo(&context_retained) == context_id);
            assert_eq!(fs::read(&context_retained).unwrap(), context_bytes);
        } else {
            assert_eq!(fs::read(&context_path).unwrap(), changed_context);
            assert!(id_archivo(&context_path) == context_id);
        }
    }
}

#[test]
fn root_mutado_despues_de_b_bloquea_antes_del_hook_y_preserva_contexto() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let context_bytes = fs::read(&context_path).unwrap();
    let context_id = id_archivo(&context_path);
    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    fs::remove_file(&journal_path).unwrap();

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let b = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_b_sin_journal()
        .unwrap();
    let (root_path, root_bytes) = mutar_archivo_root(&fixture);
    let mut hook_calls = 0;

    assert_no_verificable(b.retirar_contexto_y_observar_c_sin_sidecars_con_prueba(|| {
        hook_calls += 1;
    }));

    assert_eq!(hook_calls, 0);
    assert_eq!(fs::read(&root_path).unwrap(), root_bytes);
    assert!(!journal_path.exists());
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
}

#[test]
fn ausencia_al_hook_permite_observar_c_sin_recrear_contexto() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    fs::remove_file(&journal_path).unwrap();

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let b = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_b_sin_journal()
        .unwrap();
    let c = b
        .retirar_contexto_y_observar_c_sin_sidecars_con_prueba(|| {
            fs::remove_file(&context_path).unwrap();
        })
        .unwrap();

    assert_eq!(
        format!("{c:?}"),
        "ObservacionIntentConJournalYContextoAusentes { .. }"
    );
    drop(c);
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
}

#[test]
fn root_cambiado_despues_de_b_y_contexto_ausente_solo_alcanza_c_base() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_path = journal_location.as_path().to_path_buf();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    fs::remove_file(&journal_path).unwrap();

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let b = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_b_sin_journal()
        .unwrap();
    let mut root_mutado = None;
    let c = b
        .retirar_contexto_y_observar_c_sin_sidecars_con_prueba(|| {
            root_mutado = Some(mutar_archivo_root(&fixture));
            fs::remove_file(&context_path).unwrap();
        })
        .unwrap();

    assert_eq!(
        format!("{c:?}"),
        "ObservacionIntentConJournalYContextoAusentes { .. }"
    );
    assert_no_verificable(c.observar_consistencia_root_c());
    let (root_path, root_bytes) = root_mutado.unwrap();
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&root_path).unwrap(), root_bytes);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
}
