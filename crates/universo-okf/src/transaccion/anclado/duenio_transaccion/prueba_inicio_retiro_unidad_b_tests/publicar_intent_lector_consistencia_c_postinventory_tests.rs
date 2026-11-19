use super::*;
use crate::transaccion::anclado::duenio_transaccion::intent_retiro::EstadoRetiroIntent;
use std::cell::Cell;
use std::fs;
use std::path::Path;

fn reemplazar_lock_visible(lock: &Path, retenido: &Path, bytes_reemplazo: &[u8]) {
    fs::rename(lock, retenido).unwrap();
    fs::write(lock, bytes_reemplazo).unwrap();
}

#[test]
fn root_mismo_id_con_digest_modificado_rechaza_antes_del_hook() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let temporal = temporales(&fixture);
    let root_id = identidad_directorio(&fixture.publicada.root);
    let archivo_snapshot = fixture
        .publicada
        .entradas
        .iter()
        .find(|entrada| {
            entrada.tipo() == crate::transaccion::snapshot::TipoEntradaSnapshot::Archivo
        })
        .unwrap();
    let archivo_root = fixture.publicada.root.join(archivo_snapshot.ruta());
    let bytes_originales = fs::read(&archivo_root).unwrap();
    let id_archivo_original = id_archivo(&archivo_root);

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

    let mut bytes_modificados = bytes_originales.clone();
    bytes_modificados.push(0xa5);
    fs::write(&archivo_root, &bytes_modificados).unwrap();
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    assert!(id_archivo(&archivo_root) == id_archivo_original);
    let llamadas_hook = Cell::new(0);
    assert_error_no_verificable(resultado.observar_estado_final_tras_retiro_con_prueba(|| {
        llamadas_hook.set(llamadas_hook.get() + 1);
    }));
    assert_eq!(llamadas_hook.get(), 0);
    drop(owner);
    drop(lock);

    assert!(!intent_path.exists());
    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&archivo_root).unwrap(), bytes_modificados);
    assert!(id_archivo(&archivo_root) == id_archivo_original);
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    for path in temporal {
        assert!(!path.exists());
    }
}

#[test]
fn backup_y_stage_rechazan_en_fixtures_independientes_y_se_preservan() {
    for (nombre, es_backup) in [("backup", true), ("stage", false)] {
        let temp = tempdir().unwrap();
        let fixture = fixture_publicada_con_intent(&temp.path().join(nombre));
        let (journal, journal_path, context_path) = quitar_sidecars(&fixture);
        let (intent_path, _) = intent_paths(&fixture);
        let temporales = temporales(&fixture);
        let root_id = identidad_directorio(&fixture.publicada.root);
        let backup = fixture.publicada.ubicacion.backup().as_path().to_path_buf();
        let stage = fixture
            .publicada
            .ubicacion
            .stage(journal.transaction_id())
            .unwrap()
            .as_path()
            .to_path_buf();
        let (bloqueador, libre, marker_bytes): (&Path, &Path, &[u8]) = if es_backup {
            (&backup, &stage, b"backup inventory blocker")
        } else {
            (&stage, &backup, b"stage inventory blocker")
        };
        assert!(!backup.exists());
        assert!(!stage.exists());

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

        fs::create_dir(bloqueador).unwrap();
        let marker = bloqueador.join("marker");
        fs::write(&marker, marker_bytes).unwrap();
        let bloqueador_id = identidad_directorio(bloqueador);
        let marker_id = id_archivo(&marker);
        let llamadas_hook = Cell::new(0);

        assert_error_no_verificable(resultado.observar_estado_final_tras_retiro_con_prueba(|| {
            llamadas_hook.set(llamadas_hook.get() + 1);
        }));
        assert_eq!(llamadas_hook.get(), 0);
        drop(owner);
        drop(lock);

        assert!(!intent_path.exists());
        assert!(!journal_path.exists());
        assert!(!context_path.exists());
        assert!(identidad_directorio(bloqueador) == bloqueador_id);
        assert_eq!(fs::read(&marker).unwrap(), marker_bytes);
        assert!(id_archivo(&marker) == marker_id);
        assert!(!libre.exists());
        assert!(identidad_directorio(&fixture.publicada.root) == root_id);
        verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
        for temporal in temporales {
            assert!(!temporal.exists());
        }
    }
}

#[test]
fn lock_visible_reemplazado_antes_de_final_rechaza_sin_hook() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let temporales = temporales(&fixture);
    let lock_path = fixture.publicada.ubicacion.lock().as_path().to_path_buf();
    let retained_lock = lock_path.with_file_name("lock-retained-before-final-observation");
    let replacement_bytes = b"replacement visible lock before final";
    let lock_bytes = fs::read(&lock_path).unwrap();
    let lock_id = id_archivo(&lock_path);
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

    reemplazar_lock_visible(&lock_path, &retained_lock, replacement_bytes);
    let replacement_id = id_archivo(&lock_path);
    assert!(replacement_id != lock_id);
    let llamadas_hook = Cell::new(0);
    assert_error_no_verificable(resultado.observar_estado_final_tras_retiro_con_prueba(|| {
        llamadas_hook.set(llamadas_hook.get() + 1);
    }));
    assert_eq!(llamadas_hook.get(), 0);
    drop(owner);
    drop(lock);

    assert!(!intent_path.exists());
    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&retained_lock).unwrap(), lock_bytes);
    assert!(id_archivo(&retained_lock) == lock_id);
    assert_eq!(fs::read(&lock_path).unwrap(), replacement_bytes);
    assert!(id_archivo(&lock_path) == replacement_id);
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}

#[test]
fn lock_visible_reemplazado_despues_del_inventario_rechaza_y_preserva() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let temporales = temporales(&fixture);
    let lock_path = fixture.publicada.ubicacion.lock().as_path().to_path_buf();
    let retained_lock = lock_path.with_file_name("lock-retained-after-final-inventory");
    let replacement_bytes = b"replacement visible lock after inventory";
    let lock_bytes = fs::read(&lock_path).unwrap();
    let lock_id = id_archivo(&lock_path);
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
    let llamadas_hook = Cell::new(0);

    assert_error_no_verificable(resultado.observar_estado_final_tras_retiro_con_prueba(|| {
        llamadas_hook.set(llamadas_hook.get() + 1);
        reemplazar_lock_visible(&lock_path, &retained_lock, replacement_bytes);
    }));
    assert_eq!(llamadas_hook.get(), 1);
    let replacement_id = id_archivo(&lock_path);
    assert!(replacement_id != lock_id);
    drop(owner);
    drop(lock);

    assert!(!intent_path.exists());
    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&retained_lock).unwrap(), lock_bytes);
    assert!(id_archivo(&retained_lock) == lock_id);
    assert_eq!(fs::read(&lock_path).unwrap(), replacement_bytes);
    assert!(id_archivo(&lock_path) == replacement_id);
    assert!(identidad_directorio(&fixture.publicada.root) == root_id);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}

#[test]
fn lease_conflictivo_se_rechaza_antes_del_hook_y_de_la_callback() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let temporales = temporales(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let lock_path = fixture.publicada.ubicacion.lock().as_path().to_path_buf();
    let retained_lock = lock_path.with_file_name("lock-retained-before-conflict-attempt");
    let replacement_bytes = b"replacement visible lock before attempt";
    let lock_bytes = fs::read(&lock_path).unwrap();
    let lock_id = id_archivo(&lock_path);
    let llamadas_callback = Cell::new(0);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner
        .observar_intent_canonico()
        .unwrap()
        .abrir_lease_retiro()
        .unwrap()
        .unwrap();
    reemplazar_lock_visible(&lock_path, &retained_lock, replacement_bytes);

    let resultado = lease.retirar_con_intento_para_test(|_, _, _archivo| {
        llamadas_callback.set(llamadas_callback.get() + 1);
        Ok(())
    });
    assert!(resultado.estado() == EstadoRetiroIntent::Conflicto);
    let llamadas_hook = Cell::new(0);
    assert_error_no_verificable(resultado.observar_estado_final_tras_retiro_con_prueba(|| {
        llamadas_hook.set(llamadas_hook.get() + 1);
    }));
    assert_eq!(llamadas_callback.get(), 0);
    assert_eq!(llamadas_hook.get(), 0);
    drop(owner);
    drop(lock);

    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    assert_eq!(fs::read(&retained_lock).unwrap(), lock_bytes);
    assert!(id_archivo(&retained_lock) == lock_id);
    assert_eq!(fs::read(&lock_path).unwrap(), replacement_bytes);
    assert!(id_archivo(&lock_path) != lock_id);
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}

#[test]
fn lease_indeterminado_se_rechaza_antes_del_hook_con_callback_inyectado() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_journal, journal_path, context_path) = quitar_sidecars(&fixture);
    let (intent_path, _) = intent_paths(&fixture);
    let temporales = temporales(&fixture);
    let intent_bytes = fs::read(&intent_path).unwrap();
    let intent_id = id_archivo(&intent_path);
    let llamadas_callback = Cell::new(0);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner
        .observar_intent_canonico()
        .unwrap()
        .abrir_lease_retiro()
        .unwrap()
        .unwrap();
    let resultado = lease.retirar_con_intento_para_test(|_, _, _archivo| {
        llamadas_callback.set(llamadas_callback.get() + 1);
        Err(())
    });
    assert!(resultado.estado() == EstadoRetiroIntent::Indeterminado);
    let llamadas_hook = Cell::new(0);
    assert_error_no_verificable(resultado.observar_estado_final_tras_retiro_con_prueba(|| {
        llamadas_hook.set(llamadas_hook.get() + 1);
    }));
    assert_eq!(llamadas_callback.get(), 1);
    assert_eq!(llamadas_hook.get(), 0);
    drop(owner);
    drop(lock);

    assert_eq!(fs::read(&intent_path).unwrap(), intent_bytes);
    assert!(id_archivo(&intent_path) == intent_id);
    assert!(!journal_path.exists());
    assert!(!context_path.exists());
    for temporal in temporales {
        assert!(!temporal.exists());
    }
}
