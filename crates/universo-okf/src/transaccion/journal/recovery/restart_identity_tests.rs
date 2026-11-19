use super::dispatcher::{ErrorRecoverySoportado, ejecutar_recovery_soportado_con};
use super::*;
use crate::transaccion::anclado::DirectorioAnclado;
use crate::transaccion::inventario::inventariar_root_anclado;
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, Journal, RawTreeDigest};
use crate::transaccion::lock::{LockEscritura, serializar_forks_y_reacquisiciones_test};
use crate::transaccion::raiz::UbicacionRaiz;
use crate::transaccion::snapshot::calcular_digest_raw_tree;
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

const TXN: &str = "0123456789abcdef0123456789abcdef";

fn crear_arbol(root: &Path, bytes: &[u8]) {
    fs::create_dir_all(root.join("empty-dir")).unwrap();
    fs::write(root.join("data.bin"), bytes).unwrap();
}

fn identidad_y_digest(root: &Path) -> (DirectoryIdentity, RawTreeDigest) {
    let location = UbicacionRaiz::nueva(root).unwrap();
    let parent = DirectorioAnclado::parent_solo_lectura_desde_ubicacion(&location).unwrap();
    let name = location.ruta().file_name().unwrap();
    let artifact = parent
        .abrir_artifacto_sibling_directorio_lectura(&location, name)
        .unwrap()
        .unwrap();
    let identity = artifact
        .identidad_raiz()
        .unwrap()
        .journal_identity()
        .unwrap();
    let entries = inventariar_root_anclado(artifact).unwrap();
    let digest = calcular_digest_raw_tree(&entries).unwrap();
    (identity, digest)
}

fn identidad_parent(lock: &LockEscritura) -> DirectoryIdentity {
    DirectorioAnclado::parent_solo_lectura_desde_ubicacion(lock.ubicacion())
        .unwrap()
        .identidad_parent()
        .journal_identity()
        .unwrap()
}

fn backup_path(lock: &LockEscritura) -> PathBuf {
    lock.ubicacion().backup().as_path().to_path_buf()
}

#[test]
fn dispatcher_refuses_persisted_new_replacement_with_same_tree_and_other_identity() {
    // Coordina el drop/reacquire con los forks de otros tests.
    let _serializacion = serializar_forks_y_reacquisiciones_test();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    let new_fixture = temp.path().join("new-fixture");
    let original_new = temp.path().join("original-new");
    crear_arbol(&root, b"old-tree");
    crear_arbol(&new_fixture, b"new-tree");

    let lock = LockEscritura::adquirir(&root).unwrap();
    let (old_identity, old_digest) = identidad_y_digest(&root);
    let old = ArtifactFingerprint::new(old_identity, old_digest);
    let (new_identity, new_digest) = identidad_y_digest(&new_fixture);
    let new = ArtifactFingerprint::new(new_identity.clone(), new_digest.clone());
    let mut journal = Journal::new(TXN, identidad_parent(&lock), old, new).unwrap();
    journal.advance(JournalState::BackupReady).unwrap();
    journal.advance(JournalState::Installed).unwrap();
    let canonical = lock.ubicacion().journal().as_path().to_path_buf();
    let journal_bytes = journal.encode().unwrap();
    fs::write(&canonical, &journal_bytes).unwrap();

    let backup = backup_path(&lock);
    // Simulate process exit before changing the persisted filesystem state.
    drop(lock);
    fs::rename(&root, &backup).unwrap();
    fs::rename(&new_fixture, &root).unwrap();
    let installed_new = identidad_y_digest(&root);
    assert!(installed_new.0 == *journal.new_fingerprint().identity());
    assert!(installed_new.1 == new_digest);

    // Keep the original New directory alive so the replacement cannot reuse its identity.
    fs::rename(&root, &original_new).unwrap();
    crear_arbol(&root, b"new-tree");
    let replacement = identidad_y_digest(&root);
    assert!(replacement.1 == new_digest);
    assert!(replacement.0 != *journal.new_fingerprint().identity());
    assert!(identidad_y_digest(&original_new).0 == *journal.new_fingerprint().identity());

    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let observations = Cell::new(0);
    let mutation_callbacks_called = Cell::new(false);
    let result = ejecutar_recovery_soportado_con(
        &mut lock,
        |lock, journal| {
            observations.set(observations.get() + 1);
            observar_inventario_recovery(lock, journal)
        },
        |_| mutation_callbacks_called.set(true),
        |_| mutation_callbacks_called.set(true),
    );

    assert_eq!(result, Err(ErrorRecoverySoportado::FilaNoSoportada));
    assert_eq!(observations.get(), 1);
    assert!(!mutation_callbacks_called.get());
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert!(identidad_y_digest(&root) == replacement);
    assert!(identidad_y_digest(&backup).0 == *journal.old().identity());
    assert!(original_new.is_dir());
    assert!(!lock.ubicacion().stage(TXN).unwrap().as_path().exists());
}
