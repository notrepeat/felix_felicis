#![cfg(all(test, any(target_os = "linux", windows)))]

use super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado};
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    FixturePublicada, assert_storage_unchanged, contexto_path, id_archivo, materializar, preparar,
    verificar_arbol,
};
use crate::transaccion::anclado::{
    abrir_directorio_absoluto, abrir_directorio_hijo_lectura, metadata_directorio,
};
use crate::transaccion::journal::recovery::{ArtifactState, RecoveryInventory};
use crate::transaccion::journal::{ArtifactFingerprint, Journal};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::snapshot::{EntradaSnapshot, calcular_digest_raw_tree};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

pub(super) const TXID: &str = "0123456789abcdef0123456789abcdef";
const WRONG_TXID: &str = "ffffffffffffffffffffffffffffffff";
pub(super) const OLD_PAYLOAD: &[u8] = b"observer old backup canary";
const DECOY_PAYLOAD: &[u8] = b"observer decoy canary";

pub(super) fn observar(fixture: &FixturePublicada) -> (Journal, RecoveryInventory) {
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    owner.observar_inventario_recovery_anclado().unwrap()
}

pub(super) fn assert_inventory(
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

pub(super) fn crear_journal_con_backup_old(
    fixture: &FixturePublicada,
    backup_entries: &[EntradaSnapshot],
) -> (PathBuf, Vec<u8>) {
    let backup = fixture.ubicacion.backup().as_path().to_path_buf();
    fs::create_dir(&backup).unwrap();
    materializar(&backup, backup_entries);

    let parent_path = fixture.ubicacion.ruta().parent().unwrap().to_path_buf();
    let parent = abrir_directorio_absoluto(&parent_path, true).unwrap();
    let parent_identity = metadata_directorio(&parent)
        .unwrap()
        .journal_identity()
        .unwrap();
    let backup_name = backup.file_name().unwrap();
    let backup_handle = abrir_directorio_hijo_lectura(&parent, backup_name).unwrap();
    let backup_identity = metadata_directorio(&backup_handle)
        .unwrap()
        .journal_identity()
        .unwrap();
    let old = ArtifactFingerprint::new(
        backup_identity,
        calcular_digest_raw_tree(backup_entries).unwrap(),
    );
    let journal = Journal::new(TXID, parent_identity, old, fixture.esperado.clone()).unwrap();
    let bytes = journal.encode().unwrap();
    let path_journal = fixture.ubicacion.journal().as_path().to_path_buf();

    // Observer-only fixture: this journal binds a real Old backup, but the
    // existing context was not rebound and no replay/admission is exercised.
    fs::write(&path_journal, &bytes).unwrap();
    (path_journal, bytes)
}

#[test]
fn root_new_y_ausencia_de_backup_stage_son_datos_no_validados() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let context_path = contexto_path(&fixture.ubicacion);
    let journal_path = fixture.ubicacion.journal().as_path().to_path_buf();
    let context_id = id_archivo(&context_path);
    let journal_id = id_archivo(&journal_path);
    let backup = fixture.ubicacion.backup().as_path().to_path_buf();
    let stage = fixture
        .ubicacion
        .stage(TXID)
        .unwrap()
        .as_path()
        .to_path_buf();

    let (journal, inventory) = observar(&fixture);

    assert_eq!(journal.transaction_id(), TXID);
    assert_inventory(
        inventory,
        ArtifactState::New,
        ArtifactState::Absent,
        ArtifactState::Absent,
    );
    assert!(!backup.exists());
    assert!(!stage.exists());
    assert_storage_unchanged(
        &fixture,
        &fixture.bytes_contexto,
        &fixture.bytes_journal,
        context_id,
        journal_id,
    );
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn backup_old_se_clasifica_con_id_y_digest_reales_sin_validar_contexto() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let old_entries = [
        EntradaSnapshot::directorio("old"),
        EntradaSnapshot::archivo("old/value.bin", OLD_PAYLOAD),
    ];
    let (journal_path, journal_bytes) = crear_journal_con_backup_old(&fixture, &old_entries);
    let context_path = contexto_path(&fixture.ubicacion);
    let context_id = id_archivo(&context_path);
    let journal_id = id_archivo(&journal_path);
    let backup = fixture.ubicacion.backup().as_path().to_path_buf();
    let stage = fixture
        .ubicacion
        .stage(TXID)
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("marker"), DECOY_PAYLOAD).unwrap();

    let (journal, inventory) = observar(&fixture);

    assert_eq!(journal.transaction_id(), TXID);
    assert_inventory(
        inventory,
        ArtifactState::New,
        ArtifactState::Old,
        ArtifactState::Unknown,
    );
    assert_eq!(fs::read(&journal_path).unwrap(), journal_bytes);
    assert_eq!(fs::read(backup.join("old/value.bin")).unwrap(), OLD_PAYLOAD);
    assert_eq!(fs::read(stage.join("marker")).unwrap(), DECOY_PAYLOAD);
    assert!(id_archivo(&journal_path) == journal_id);
    assert!(id_archivo(&context_path) == context_id);
    assert_eq!(fs::read(context_path).unwrap(), fixture.bytes_contexto);
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn observa_solo_stage_derivado_del_txid_del_journal() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let wrong_stage = fixture
        .ubicacion
        .stage(WRONG_TXID)
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&wrong_stage).unwrap();
    fs::write(wrong_stage.join("marker"), DECOY_PAYLOAD).unwrap();

    let (_, without_exact) = observar(&fixture);
    assert_inventory(
        without_exact,
        ArtifactState::New,
        ArtifactState::Absent,
        ArtifactState::Absent,
    );

    let exact_stage = fixture
        .ubicacion
        .stage(TXID)
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&exact_stage).unwrap();
    fs::write(exact_stage.join("marker"), OLD_PAYLOAD).unwrap();

    let (_, with_exact) = observar(&fixture);
    assert_inventory(
        with_exact,
        ArtifactState::New,
        ArtifactState::Absent,
        ArtifactState::Unknown,
    );
    assert_eq!(fs::read(wrong_stage.join("marker")).unwrap(), DECOY_PAYLOAD);
    assert_eq!(fs::read(exact_stage.join("marker")).unwrap(), OLD_PAYLOAD);
}

#[test]
fn backup_que_es_archivo_se_rechaza_y_conserva_el_canary() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let backup = fixture.ubicacion.backup().as_path().to_path_buf();
    fs::write(&backup, DECOY_PAYLOAD).unwrap();
    let context_path = contexto_path(&fixture.ubicacion);
    let journal_path = fixture.ubicacion.journal().as_path().to_path_buf();
    let context_id = id_archivo(&context_path);
    let journal_id = id_archivo(&journal_path);

    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert!(matches!(
        owner.observar_inventario_recovery_anclado(),
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));

    assert_eq!(fs::read(&backup).unwrap(), DECOY_PAYLOAD);
    assert_storage_unchanged(
        &fixture,
        &fixture.bytes_contexto,
        &fixture.bytes_journal,
        context_id,
        journal_id,
    );
}

#[cfg(unix)]
#[test]
fn backup_symlink_se_rechaza_y_conserva_target_y_enlace() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let backup = fixture.ubicacion.backup().as_path().to_path_buf();
    let target = temp.path().join("backup-target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("marker"), DECOY_PAYLOAD).unwrap();
    symlink(&target, &backup).unwrap();

    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert!(matches!(
        owner.observar_inventario_recovery_anclado(),
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));

    assert!(
        fs::symlink_metadata(&backup)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(target.join("marker")).unwrap(), DECOY_PAYLOAD);
}
