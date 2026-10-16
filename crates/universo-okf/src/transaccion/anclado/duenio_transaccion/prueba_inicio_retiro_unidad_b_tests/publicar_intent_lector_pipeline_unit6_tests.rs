use super::*;
use crate::transaccion::anclado::duenio_transaccion::intent_retiro::EstadoRetiroIntent;

#[test]
fn pipeline_privado_completo_observa_final_y_rechaza_replay() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let ubicacion = &fixture.publicada.ubicacion;
    let (intent_path, intent_temporal) = intent_paths(&fixture);
    let journal_path = ubicacion.journal().as_path().to_path_buf();
    let contexto = contexto_path(ubicacion);
    let parent = ubicacion.ruta().parent().unwrap();
    let journal_temporal = parent.join(ubicacion.nombre_journal_temporal());
    let contexto_temporal =
        parent.join(nombre_temporal_sibling(contexto.file_name().unwrap()).unwrap());
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let backup = ubicacion.backup().as_path().to_path_buf();
    let stage = ubicacion
        .stage(journal.transaction_id())
        .unwrap()
        .as_path()
        .to_path_buf();
    assert!(journal_path.is_file());
    assert!(!journal_temporal.exists());
    assert!(contexto.is_file());
    assert!(!contexto_temporal.exists());
    assert!(intent_path.is_file());
    assert!(!intent_temporal.exists());
    let siblings = [
        journal_path,
        journal_temporal,
        contexto,
        contexto_temporal,
        intent_path,
        intent_temporal,
    ];
    let root = &fixture.publicada.root;
    let root_id = identidad_directorio(root);

    assert!(!backup.exists());
    assert!(!stage.exists());

    let mut lock = LockEscritura::adquirir_en(ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let retiro = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_a()
        .unwrap()
        .retirar_journal_y_observar_b_sin_journal()
        .unwrap()
        .retirar_contexto_y_observar_c_sin_sidecars()
        .unwrap()
        .observar_consistencia_root_c()
        .unwrap()
        .retirar_intent_tras_consistencia_root_c()
        .unwrap();
    assert!(retiro.estado() == EstadoRetiroIntent::Eliminado);

    let final_observacion = retiro.observar_estado_final_tras_retiro().unwrap();
    assert_eq!(
        format!("{final_observacion:?}"),
        "ObservacionFinalRetiroIntent { .. }"
    );
    assert!(final_observacion.observacion_coherente_para_test());
    assert!(identidad_directorio(root) == root_id);
    verificar_arbol(root, &fixture.publicada.entradas);
    drop(final_observacion);

    let error = owner.probar_replay_admitido_root_coincidente().unwrap_err();
    assert_eq!(error.to_string(), "admission_replay_root_not_proven");
    assert_eq!(format!("{error:?}"), "ErrorPruebaReplayRootCoincidente");
    drop(owner);
    drop(lock);

    for sibling in &siblings {
        assert!(!sibling.exists());
    }
    assert!(!backup.exists());
    assert!(!stage.exists());
    assert!(identidad_directorio(root) == root_id);
    verificar_arbol(root, &fixture.publicada.entradas);
}
