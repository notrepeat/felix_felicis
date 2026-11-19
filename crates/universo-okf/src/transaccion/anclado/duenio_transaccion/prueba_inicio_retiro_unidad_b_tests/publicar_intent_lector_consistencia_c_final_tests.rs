use super::*;
use crate::transaccion::anclado::duenio_transaccion::intent_retiro::EstadoRetiroIntent;
use std::cell::Cell;
use std::fs;

#[test]
fn retiro_real_seguido_de_observacion_final_fresca_y_redactada() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let root_id = identidad_directorio(&fixture.publicada.root);
    let temporales = temporales(&fixture);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let resultado = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_c_sin_sidecars()
        .unwrap()
        .observar_consistencia_root_c()
        .unwrap()
        .retirar_intent_tras_consistencia_root_c()
        .unwrap();
    assert!(resultado.estado() == EstadoRetiroIntent::Eliminado);

    let observacion = resultado.observar_estado_final_tras_retiro().unwrap();
    assert!(observacion.observacion_coherente_para_test());
    assert_eq!(
        format!("{observacion:?}"),
        "ObservacionFinalRetiroIntent { .. }"
    );
    drop(observacion);
    drop(owner);
    drop(lock);

    assert!(!intent_path.exists());
    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}

#[test]
fn root_reemplazado_tras_retiro_con_mismos_bytes_y_otro_id_falla_fresco() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let root = fixture.publicada.root.clone();
    let retained_root = root.with_file_name("root-original-retained-final-observation");
    let original_root_id = identidad_directorio(&root);
    let temporales = temporales(&fixture);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let resultado = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_c_sin_sidecars()
        .unwrap()
        .observar_consistencia_root_c()
        .unwrap()
        .retirar_intent_tras_consistencia_root_c()
        .unwrap();
    assert!(resultado.estado() == EstadoRetiroIntent::Eliminado);

    fs::rename(&root, &retained_root).unwrap();
    fs::create_dir(&root).unwrap();
    materializar(&root, &fixture.publicada.entradas);
    let replacement_root_id = identidad_directorio(&root);
    assert!(replacement_root_id != original_root_id);
    verificar_arbol(&retained_root, &fixture.publicada.entradas);
    verificar_arbol(&root, &fixture.publicada.entradas);

    assert_error_no_verificable(resultado.observar_estado_final_tras_retiro());
    drop(owner);
    drop(lock);

    assert!(!intent_path.exists());
    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert!(identidad_directorio(&retained_root) == original_root_id);
    assert!(identidad_directorio(&root) == replacement_root_id);
    verificar_arbol(&retained_root, &fixture.publicada.entradas);
    verificar_arbol(&root, &fixture.publicada.entradas);
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}

#[test]
fn sibling_tras_inventario_falla_segunda_sonda_y_se_preserva() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let root_id = identidad_directorio(&fixture.publicada.root);
    let temporales = temporales(&fixture);
    let blocker = fixture
        .publicada
        .root
        .parent()
        .unwrap()
        .join(fixture.publicada.ubicacion.nombre_journal_temporal());
    assert!(!blocker.exists());
    let blocker_bytes = b"reserved sibling created after final inventory".to_vec();
    let blocker_id = Cell::new(None);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let resultado = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_c_sin_sidecars()
        .unwrap()
        .observar_consistencia_root_c()
        .unwrap()
        .retirar_intent_tras_consistencia_root_c()
        .unwrap();
    assert!(resultado.estado() == EstadoRetiroIntent::Eliminado);
    let llamadas_hook = Cell::new(0);

    let final_observacion = resultado.observar_estado_final_tras_retiro_con_prueba(|| {
        llamadas_hook.set(llamadas_hook.get() + 1);
        fs::write(&blocker, &blocker_bytes).unwrap();
        blocker_id.set(Some(id_archivo(&blocker)));
    });

    assert_error_no_verificable(final_observacion);
    assert_eq!(llamadas_hook.get(), 1);
    drop(owner);
    drop(lock);

    assert!(!intent_path.exists());
    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&blocker).unwrap(), blocker_bytes);
    assert!(id_archivo(&blocker) == blocker_id.get().unwrap());
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    for temporal in temporales {
        if temporal != blocker {
            assert!(!temporal.exists());
        }
    }
}

#[test]
fn cada_sibling_reservado_bloquea_final_y_preserva_su_archivo() {
    for indice_bloqueado in 0..6 {
        let temp = tempdir().unwrap();
        let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
        let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
        let (intent_path, _) = intent_paths(&fixture);
        let temporales = temporales(&fixture);
        let siblings = [
            intent_path.clone(),
            journal_path,
            context_path,
            temporales[0].clone(),
            temporales[1].clone(),
            temporales[2].clone(),
        ];
        let blocker = siblings[indice_bloqueado].clone();
        let bytes_blocker = b"reserved sibling preserved by final observation".to_vec();
        let root_id = identidad_directorio(&fixture.publicada.root);

        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let resultado = owner
            .observar_intent_canonico()
            .unwrap()
            .observar_reconciliacion_c_sin_sidecars()
            .unwrap()
            .observar_consistencia_root_c()
            .unwrap()
            .retirar_intent_tras_consistencia_root_c()
            .unwrap();
        assert!(resultado.estado() == EstadoRetiroIntent::Eliminado);

        fs::write(&blocker, &bytes_blocker).unwrap();
        let blocker_id = id_archivo(&blocker);
        assert_error_no_verificable(resultado.observar_estado_final_tras_retiro());
        drop(owner);
        drop(lock);

        for (indice, sibling) in siblings.iter().enumerate() {
            assert_eq!(sibling.exists(), indice == indice_bloqueado);
        }
        assert_eq!(fs::read(&blocker).unwrap(), bytes_blocker);
        assert!(id_archivo(&blocker) == blocker_id);
        assert!(identidad_directorio(&fixture.publicada.root) == root_id);
        verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    }
}

#[test]
fn intent_ausente_despues_del_lease_admite_observacion_final_y_preserva_retenido() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let retenido = intent_path.with_file_name("intent-retained-after-final-observation");
    let bytes_originales = fs::read(&intent_path).unwrap();
    let id_original = id_archivo(&intent_path);
    let root_id = identidad_directorio(&fixture.publicada.root);
    let temporales = temporales(&fixture);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner
        .observar_intent_canonico()
        .unwrap()
        .abrir_lease_retiro()
        .unwrap()
        .unwrap();
    fs::rename(&intent_path, &retenido).unwrap();
    let resultado = lease.retirar();
    assert!(resultado.estado() == EstadoRetiroIntent::Ausente);

    let observacion = resultado.observar_estado_final_tras_retiro().unwrap();
    assert!(observacion.observacion_coherente_para_test());
    drop(observacion);
    drop(owner);
    drop(lock);

    assert!(!intent_path.exists());
    assert_eq!(fs::read(&retenido).unwrap(), bytes_originales);
    assert!(id_archivo(&retenido) == id_original);
    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}
