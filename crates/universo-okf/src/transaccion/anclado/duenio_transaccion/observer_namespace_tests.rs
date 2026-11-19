#![cfg(all(test, any(target_os = "linux", windows)))]

use super::DuenioTransaccionAnclado;
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    contexto_path, id_archivo, materializar, preparar, verificar_arbol,
};
use crate::transaccion::anclado::{
    abrir_directorio_absoluto, abrir_directorio_hijo_lectura, metadata_directorio,
};
use crate::transaccion::journal::recovery::{ArtifactState, RecoveryInventory};
use crate::transaccion::lock::LockEscritura;
use std::fs;
use tempfile::tempdir;

const TXID: &str = "0123456789abcdef0123456789abcdef";
const OLD_PAYLOAD: &[u8] = b"observer old backup canary";
const DECOY_PAYLOAD: &[u8] = b"observer decoy canary";

fn assert_inventory(
    inventory: RecoveryInventory,
    root: ArtifactState,
    backup: ArtifactState,
    stage: ArtifactState,
) {
    assert_eq!(inventory.root, root);
    assert_eq!(inventory.backup, backup);
    assert_eq!(inventory.stage, stage);
    assert!(!inventory.root_new_validated);
}
#[test]
fn parent_movido_observa_original_y_preserva_siblings_decoy() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let fixture = preparar(&parent);
    let context_path = contexto_path(&fixture.ubicacion);
    let journal_path = fixture.ubicacion.journal().as_path().to_path_buf();
    let lock_path = fixture.ubicacion.lock().as_path().to_path_buf();
    let context_id = id_archivo(&context_path);
    let journal_id = id_archivo(&journal_path);
    let lock_id = id_archivo(&lock_path);
    let lock_bytes = fs::read(&lock_path).unwrap();
    let root_name = fixture.ubicacion.ruta().file_name().unwrap().to_os_string();
    let context_name = context_path.file_name().unwrap().to_os_string();
    let journal_name = journal_path.file_name().unwrap().to_os_string();
    let lock_name = lock_path.file_name().unwrap().to_os_string();
    let backup_name = fixture
        .ubicacion
        .backup()
        .as_path()
        .file_name()
        .unwrap()
        .to_os_string();
    let stage_name = fixture
        .ubicacion
        .stage(TXID)
        .unwrap()
        .as_path()
        .file_name()
        .unwrap()
        .to_os_string();

    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let (_, baseline) = owner.observar_inventario_recovery_anclado().unwrap();
    assert_inventory(
        baseline,
        ArtifactState::New,
        ArtifactState::Absent,
        ArtifactState::Absent,
    );

    if let Err(error) = fs::rename(&parent, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(matches!(
                error.raw_os_error(),
                Some(5) | Some(32) | Some(33)
            ));
            assert!(parent.join(&root_name).is_dir());
            assert!(!moved_parent.exists());
            let (_, still_original) = owner.observar_inventario_recovery_anclado().unwrap();
            assert_inventory(
                still_original,
                ArtifactState::New,
                ArtifactState::Absent,
                ArtifactState::Absent,
            );
            eprintln!("Windows sharing bloqueó mover el parent retenido; el decoy no se ejercitó.");
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent retenido: {error}");
    }

    fs::create_dir(&parent).unwrap();
    let decoy_root = parent.join(&root_name);
    fs::create_dir(&decoy_root).unwrap();
    materializar(&decoy_root, &fixture.entradas);
    fs::write(parent.join(&context_name), b"decoy context").unwrap();
    fs::write(parent.join(&journal_name), &fixture.bytes_journal).unwrap();
    fs::write(parent.join(&lock_name), &lock_bytes).unwrap();

    let decoy_backup = parent.join(&backup_name);
    fs::create_dir(&decoy_backup).unwrap();
    fs::write(decoy_backup.join("marker"), DECOY_PAYLOAD).unwrap();
    let decoy_stage = parent.join(&stage_name);
    fs::create_dir(&decoy_stage).unwrap();
    fs::write(decoy_stage.join("marker"), OLD_PAYLOAD).unwrap();

    let decoy_parent = abrir_directorio_absoluto(&parent, true).unwrap();
    let decoy_root_handle = abrir_directorio_hijo_lectura(&decoy_parent, &root_name).unwrap();
    let decoy_root_id = metadata_directorio(&decoy_root_handle)
        .unwrap()
        .journal_identity()
        .unwrap();
    assert!(fixture.esperado.identity() != &decoy_root_id);
    assert_eq!(fs::read(parent.join(&lock_name)).unwrap(), lock_bytes);
    assert!(id_archivo(&parent.join(&lock_name)) != lock_id);

    let (journal, inventory) = owner.observar_inventario_recovery_anclado().unwrap();

    assert_eq!(journal.transaction_id(), TXID);
    assert_inventory(
        inventory,
        ArtifactState::New,
        ArtifactState::Absent,
        ArtifactState::Absent,
    );
    let moved_context = moved_parent.join(&context_name);
    let moved_journal = moved_parent.join(&journal_name);
    assert_eq!(fs::read(&moved_context).unwrap(), fixture.bytes_contexto);
    assert_eq!(fs::read(&moved_journal).unwrap(), fixture.bytes_journal);
    assert!(id_archivo(&moved_context) == context_id);
    assert!(id_archivo(&moved_journal) == journal_id);
    assert!(owner._lock.identidad_archivo_lock().unwrap() == lock_id);
    assert_eq!(
        fs::read(parent.join(&context_name)).unwrap(),
        b"decoy context"
    );
    assert_eq!(
        fs::read(parent.join(&journal_name)).unwrap(),
        fixture.bytes_journal
    );
    assert_eq!(
        fs::read(decoy_backup.join("marker")).unwrap(),
        DECOY_PAYLOAD
    );
    assert_eq!(fs::read(decoy_stage.join("marker")).unwrap(), OLD_PAYLOAD);
    verificar_arbol(&moved_parent.join(&root_name), &fixture.entradas);
    verificar_arbol(&decoy_root, &fixture.entradas);
}
