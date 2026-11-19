use super::*;
use crate::transaccion::anclado::intent_persistido::codificar_nuevo;
use crate::transaccion::anclado::{
    abrir_directorio_absoluto, metadata_directorio, scope_filesystem,
};
use crate::transaccion::journal::Journal;
use sha2::{Digest, Sha256};

#[test]
fn journal_guardado_vinculado_a_otro_parent_falla_y_preserva_intent() {
    let temp = tempdir().unwrap();
    let fixture = super::fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, _) = intent_paths(&fixture);
    let identidad_intent = id_archivo(&canonico);
    let ubicacion = &fixture.publicada.ubicacion;
    let parent_handle =
        abrir_directorio_absoluto(ubicacion.ruta().parent().unwrap(), true).unwrap();
    let scope = scope_filesystem(&parent_handle).unwrap();
    let journal_ubicacion = ubicacion.journal();
    let ruta_journal = journal_ubicacion.as_path();
    let ruta_contexto = contexto_path(ubicacion);
    let bytes_contexto = fs::read(&ruta_contexto).unwrap();
    let digest_contexto: [u8; 32] = Sha256::digest(&bytes_contexto).into();
    let journal_original = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let foreign_path = temp.path().join("foreign-parent");
    fs::create_dir(&foreign_path).unwrap();
    let foreign_handle = abrir_directorio_absoluto(&foreign_path, true).unwrap();
    let foreign_identity = metadata_directorio(&foreign_handle)
        .unwrap()
        .journal_identity()
        .unwrap();
    let mut journal = Journal::new(
        journal_original.transaction_id().to_owned(),
        foreign_identity,
        journal_original.old().clone(),
        journal_original.new_fingerprint().clone(),
    )
    .unwrap();
    for state in [
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ] {
        journal.advance(state).unwrap();
    }
    let bytes_falsificados = codificar_nuevo(
        &journal,
        identidad_intent,
        id_archivo(ruta_journal),
        id_archivo(&ruta_contexto),
        scope.0,
        u64::try_from(bytes_contexto.len()).unwrap(),
        digest_contexto,
    )
    .unwrap();
    fs::write(&canonico, &bytes_falsificados).unwrap();
    assert!(id_archivo(&canonico) == identidad_intent);

    super::assert_no_verificable(&fixture);

    assert_eq!(fs::read(&canonico).unwrap(), bytes_falsificados);
    assert!(id_archivo(&canonico) == identidad_intent);
}

#[test]
fn lector_observa_intent_con_journal_ausente_y_contexto_opcional_sin_reconciliar() {
    for conservar_contexto in [true, false] {
        let temp = tempdir().unwrap();
        let fixture = super::fixture_publicada_con_intent(&temp.path().join("parent"));
        let (intent_canonico, intent_temporal) = intent_paths(&fixture);
        let journal_path = fixture
            .publicada
            .ubicacion
            .journal()
            .as_path()
            .to_path_buf();
        let contexto = contexto_path(&fixture.publicada.ubicacion);
        let ids_storage = capturar_storage(&fixture);
        let bytes_intent = fs::read(&intent_canonico).unwrap();
        let identidad_intent = id_archivo(&intent_canonico);
        let root_identity = identidad_directorio(&fixture.publicada.root);
        fs::remove_file(&journal_path).unwrap();
        if !conservar_contexto {
            fs::remove_file(&contexto).unwrap();
        }

        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let observacion = owner.observar_intent_canonico().unwrap();
        assert_eq!(
            format!("{observacion:?}"),
            "ObservacionIntentCanonico { .. }"
        );
        drop(observacion);

        assert!(!journal_path.exists());
        assert!(!intent_temporal.exists());
        assert_eq!(fs::read(&intent_canonico).unwrap(), bytes_intent);
        assert!(id_archivo(&intent_canonico) == identidad_intent);
        if conservar_contexto {
            assert_eq!(
                fs::read(&contexto).unwrap(),
                fixture.publicada.bytes_contexto
            );
            assert!(id_archivo(&contexto) == ids_storage.0);
        } else {
            assert!(!contexto.exists());
        }
        assert!(identidad_directorio(&fixture.publicada.root) == root_identity);
        verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    }
}

#[test]
fn intent_presente_sigue_bloqueando_prueba_normal_antes_y_despues_de_observar() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ids_storage = capturar_storage(&fixture);
    let (intent_canonico, intent_temporal) = intent_paths(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    drop(owner.probar_replay_admitido_root_coincidente().unwrap());

    let publicado = escribir_intent(&mut owner)
        .verificar_temporal()
        .unwrap()
        .revalidar_completo()
        .unwrap()
        .publicar_intent()
        .unwrap();
    drop(publicado);
    let bytes_intent = fs::read(&intent_canonico).unwrap();
    let identidad_intent = id_archivo(&intent_canonico);

    assert_rechazo(owner.probar_replay_admitido_root_coincidente());
    let observacion = owner.observar_intent_canonico().unwrap();
    drop(observacion);
    assert_rechazo(owner.probar_replay_admitido_root_coincidente());

    assert!(!intent_temporal.exists());
    assert_eq!(fs::read(&intent_canonico).unwrap(), bytes_intent);
    assert!(id_archivo(&intent_canonico) == identidad_intent);
    verificar_storage(&fixture, ids_storage);
}
