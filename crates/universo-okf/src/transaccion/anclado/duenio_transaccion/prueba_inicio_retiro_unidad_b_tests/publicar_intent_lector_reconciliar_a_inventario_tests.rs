use super::*;
use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, codec, tests as fixtures};
use crate::transaccion::anclado::intent_persistido::codificar_nuevo;
use crate::transaccion::anclado::scope_filesystem;
use crate::transaccion::journal::Journal;
use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::recovery::ArtifactState;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::Path;

pub(super) fn fixture_publicada_con_backup_old(parent: &Path) -> FixtureFilaConservarNew {
    let mut fixture = preparar_fixture(parent, true);
    avanzar_a(&mut fixture, JournalState::Committed);
    let backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
    let retained = parent.with_file_name("backup-old-retained");
    let original_id = fixture.identidad_backup_old.as_ref().unwrap().clone();
    fs::rename(&backup, &retained).unwrap();
    assert!(identidad_directorio(&retained) == original_id);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let published = escribir_intent(&mut owner)
        .verificar_temporal()
        .unwrap()
        .revalidar_completo()
        .unwrap()
        .publicar_intent()
        .unwrap();
    drop(published);
    drop(owner);
    drop(lock);

    fs::rename(&retained, &backup).unwrap();
    assert!(identidad_directorio(&backup) == original_id);
    fixture
}

#[test]
fn lector_canonico_observa_journal_o_contexto_ausente_pero_a_rechaza_y_preserva() {
    for journal_ausente in [true, false] {
        let temp = tempdir().unwrap();
        let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
        let journal_location = fixture.publicada.ubicacion.journal();
        let journal_path = journal_location.as_path().to_path_buf();
        let context_path = contexto_path(&fixture.publicada.ubicacion);
        let (intent_path, intent_temporary) = intent_paths(&fixture);
        let (missing_path, remaining_path) = if journal_ausente {
            (&journal_path, &context_path)
        } else {
            (&context_path, &journal_path)
        };
        fs::remove_file(missing_path).unwrap();
        let remaining_bytes = fs::read(remaining_path).unwrap();
        let remaining_id = id_archivo(remaining_path);
        let intent_bytes = fs::read(&intent_path).unwrap();
        let intent_id = id_archivo(&intent_path);

        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let intent = owner.observar_intent_canonico().unwrap();
        assert_eq!(format!("{intent:?}"), "ObservacionIntentCanonico { .. }");
        let error = intent.observar_reconciliacion_a().unwrap_err();

        assert_eq!(error.to_string(), "transaction_owner_unverified");
        assert_eq!(
            format!("{error:?}"),
            "ErrorDuenioTransaccionAnclado(\"transaction_owner_unverified\")"
        );
        drop(owner);
        drop(lock);
        assert!(!missing_path.exists());
        assert_eq!(fs::read(remaining_path).unwrap(), remaining_bytes);
        assert!(id_archivo(remaining_path) == remaining_id);
        assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
        assert!(id_archivo(&intent_path) == intent_id);
        assert!(!intent_temporary.exists());
        verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    }
}

#[test]
fn root_recreado_con_arbol_igual_y_otro_id_se_clasifica_unknown_y_bloquea() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let root = &fixture.publicada.root;
    let retained_root = root.with_file_name("original-root-retained");
    let storage = capturar_storage(&fixture);
    let original_id = identidad_directorio(root);
    fs::rename(root, &retained_root).unwrap();
    fs::create_dir(root).unwrap();
    materializar(root, &fixture.publicada.entradas);
    let replacement_id = identidad_directorio(root);
    assert!(replacement_id != original_id);
    verificar_arbol(root, &fixture.publicada.entradas);

    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let inventory = owner
        .observar_inventario_recovery_con_journal_datos(&journal)
        .unwrap();
    assert_eq!(inventory.root, ArtifactState::Unknown);
    assert!(!inventory.root_new_validated);

    let error = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap_err();

    assert_eq!(error.to_string(), "transaction_owner_unverified");
    assert!(identidad_directorio(&retained_root) == original_id);
    assert!(identidad_directorio(root) == replacement_id);
    verificar_arbol(&retained_root, &fixture.publicada.entradas);
    verificar_arbol(root, &fixture.publicada.entradas);
    verificar_storage(&fixture, storage);
}

#[test]
fn stage_mismo_arbol_con_id_distinto_es_unknown_y_bloquea() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let stage = fixture
        .publicada
        .ubicacion
        .stage(journal.transaction_id())
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&stage).unwrap();
    materializar(&stage, &fixture.publicada.entradas);
    let id_stage = identidad_directorio(&stage);
    assert!(id_stage != journal.new_fingerprint().identity().clone());
    verificar_arbol(&stage, &fixture.publicada.entradas);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let inventory = owner
        .observar_inventario_recovery_con_journal_datos(&journal)
        .unwrap();
    assert_eq!(inventory.root, ArtifactState::New);
    assert_eq!(inventory.backup, ArtifactState::Absent);
    assert_eq!(inventory.stage, ArtifactState::Unknown);
    assert!(!inventory.root_new_validated);
    let storage = capturar_storage(&fixture);

    let error = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap_err();

    assert_eq!(error.to_string(), "transaction_owner_unverified");
    verificar_storage(&fixture, storage);
    assert!(identidad_directorio(&stage) == id_stage);
    verificar_arbol(&stage, &fixture.publicada.entradas);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}

#[test]
fn backup_old_preservado_se_clasifica_y_bloquea_la_fila_a() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_backup_old(&temp.path().join("parent"));
    let backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
    let backup_id = fixture.identidad_backup_old.as_ref().unwrap().clone();
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let storage = capturar_storage(&fixture);
    let intent_path = intent_paths(&fixture).0;
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

    let error = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap_err();

    assert_eq!(error.to_string(), "transaction_owner_unverified");
    assert!(identidad_directorio(&backup) == backup_id);
    verificar_arbol(&backup, fixture.entradas_old.as_ref().unwrap());
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    verificar_storage(&fixture, storage);
}

#[test]
fn perfil_no_soportado_con_binding_actualizado_se_rechaza_tras_comparadores() {
    let temp = tempdir().unwrap();
    let mut fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let envelope = EnvelopeContextoAdmision::ligar(
        fixtures::contexto_con_tags(b"\n", "engine-unsupported-test", "policy-unsupported-test"),
        &journal,
    );
    let context_bytes = codec::codificar(&envelope).unwrap();
    let context_path = contexto_path(&fixture.publicada.ubicacion);
    fs::write(&context_path, &context_bytes).unwrap();
    let context_handle = File::open(&context_path).unwrap();
    let scope = scope_filesystem(&context_handle).unwrap();
    let context_id = id_archivo(&context_path);
    let journal_location = fixture.publicada.ubicacion.journal();
    let journal_id = id_archivo(journal_location.as_path());
    let intent_path = intent_paths(&fixture).0;
    let intent_id = id_archivo(&intent_path);
    let digest: [u8; 32] = Sha256::digest(&context_bytes).into();
    let intent_bytes = codificar_nuevo(
        &journal,
        intent_id,
        journal_id,
        context_id,
        scope.0,
        u64::try_from(context_bytes.len()).unwrap(),
        digest,
    )
    .unwrap();
    fs::write(&intent_path, &intent_bytes).unwrap();
    fixture.publicada.bytes_contexto = context_bytes.clone();
    let intent_id_after = id_archivo(&intent_path);
    let context_id_after = id_archivo(&context_path);
    assert!(intent_id_after == intent_id);
    assert!(context_id_after == context_id);
    let storage = capturar_storage(&fixture);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let error = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap_err();

    assert_eq!(error.to_string(), "transaction_owner_unverified");
    assert_eq!(fs::read(&context_path).unwrap(), context_bytes);
    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert!(id_archivo(&context_path) == context_id);
    verificar_storage(&fixture, storage);
}
