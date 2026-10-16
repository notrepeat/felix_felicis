use super::*;
use crate::transaccion::anclado::duenio_transaccion::intent_retiro::EstadoRetiroIntent;
use std::cell::Cell;
use std::fs;

#[test]
fn c_root_fresca_retiro_real_y_preserva_storage() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let root_id = identidad_directorio(&fixture.publicada.root);
    let temporales = temporales(&fixture);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observacion = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_c_sin_sidecars()
        .unwrap()
        .observar_consistencia_root_c()
        .unwrap();

    let resultado = observacion
        .retirar_intent_tras_consistencia_root_c()
        .unwrap();
    assert!(resultado.estado() == EstadoRetiroIntent::Eliminado);
    assert!(resultado.owner_binding_coincide_para_test());
    assert!(!intent_path.exists());
    drop(resultado);
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}

#[test]
fn root_reemplazado_con_mismo_arbol_y_otro_id_rechaza_sin_retirar_intent() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let root = fixture.publicada.root.clone();
    let retained_root = root.with_file_name("root-original-retained-c-consumer");
    let original_root_id = identidad_directorio(&root);
    let temporales = temporales(&fixture);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observacion = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_c_sin_sidecars()
        .unwrap()
        .observar_consistencia_root_c()
        .unwrap();

    fs::rename(&root, &retained_root).unwrap();
    fs::create_dir(&root).unwrap();
    materializar(&root, &fixture.publicada.entradas);
    let replacement_root_id = identidad_directorio(&root);
    assert!(replacement_root_id != original_root_id);
    verificar_arbol(&root, &fixture.publicada.entradas);
    let llamadas_hook = Cell::new(0);

    let resultado = observacion.retirar_intent_tras_consistencia_root_c_con_prueba(|| {
        llamadas_hook.set(llamadas_hook.get() + 1);
    });

    assert_error_no_verificable(resultado);
    assert_eq!(llamadas_hook.get(), 0);
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
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}

#[test]
fn ausencia_tras_c_root_fresca_falla_cerrado_y_conserva_intent_retenido() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let retained_intent = intent_path.with_file_name("intent-retained-after-fresh-c");
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let root_id = identidad_directorio(&fixture.publicada.root);
    let temporales = temporales(&fixture);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observacion = owner
        .observar_intent_canonico()
        .unwrap()
        .observar_reconciliacion_c_sin_sidecars()
        .unwrap()
        .observar_consistencia_root_c()
        .unwrap();
    let llamadas_hook = Cell::new(0);

    let resultado = observacion.retirar_intent_tras_consistencia_root_c_con_prueba(|| {
        llamadas_hook.set(llamadas_hook.get() + 1);
        fs::rename(&intent_path, &retained_intent).unwrap();
    });

    assert_error_no_verificable(resultado);
    assert_eq!(llamadas_hook.get(), 1);
    drop(owner);
    drop(lock);

    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert!(!intent_path.exists());
    assert_eq!(fs::read(&retained_intent).unwrap(), intent_bytes);
    assert!(id_archivo(&retained_intent) == intent_id);
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}
