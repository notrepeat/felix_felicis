#![cfg(all(test, any(target_os = "linux", windows)))]

use super::tests::{OLD_PAYLOAD, TXID, assert_inventory, crear_journal_con_backup_old};
use super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado};
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    contexto_path, id_archivo, preparar, verificar_arbol,
};
use crate::transaccion::anclado::{
    IdentidadDirectorio, abrir_directorio_absoluto, abrir_directorio_hijo_lectura,
    metadata_directorio,
};
use crate::transaccion::journal::Journal;
use crate::transaccion::journal::recovery::ArtifactState;
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::snapshot::EntradaSnapshot;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

const STAGE_CANARY: &[u8] = b"journal data observer stage canary";

fn identidad_directorio(path: &Path) -> IdentidadDirectorio {
    let parent = abrir_directorio_absoluto(path.parent().unwrap(), true).unwrap();
    let directory = abrir_directorio_hijo_lectura(&parent, path.file_name().unwrap()).unwrap();
    metadata_directorio(&directory).unwrap()
}

#[test]
fn journal_datos_observa_siblings_con_el_canonico_ausente_sin_autorizar() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let old_entries = [
        EntradaSnapshot::directorio("old"),
        EntradaSnapshot::archivo("old/value.bin", OLD_PAYLOAD),
    ];
    let (journal_path, journal_bytes) = crear_journal_con_backup_old(&fixture, &old_entries);
    let journal = Journal::parse(&journal_bytes).unwrap();
    let context_path = contexto_path(&fixture.ubicacion);
    let context_id = id_archivo(&context_path);
    let root_id = identidad_directorio(&fixture.root);
    let backup = fixture.ubicacion.backup().as_path().to_path_buf();
    let backup_payload = backup.join("old/value.bin");
    let backup_id = id_archivo(&backup_payload);
    let stage = fixture
        .ubicacion
        .stage(journal.transaction_id())
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&stage).unwrap();
    let stage_canary = stage.join("marker");
    fs::write(&stage_canary, STAGE_CANARY).unwrap();
    let stage_id = id_archivo(&stage_canary);
    let wrong_stage = fixture
        .ubicacion
        .stage("ffffffffffffffffffffffffffffffff")
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&wrong_stage).unwrap();
    fs::write(wrong_stage.join("marker"), b"wrong transaction decoy").unwrap();
    fs::remove_file(&journal_path).unwrap();

    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert!(matches!(
        owner.observar_inventario_recovery_anclado(),
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));
    let inventory = owner
        .observar_inventario_recovery_con_journal_datos(&journal)
        .unwrap();

    assert_inventory(
        inventory,
        ArtifactState::New,
        ArtifactState::Old,
        ArtifactState::Unknown,
    );
    assert!(!journal_path.exists());
    assert_eq!(fs::read(&context_path).unwrap(), fixture.bytes_contexto);
    assert!(id_archivo(&context_path) == context_id);
    assert!(identidad_directorio(&fixture.root) == root_id);
    assert_eq!(fs::read(&backup_payload).unwrap(), OLD_PAYLOAD);
    assert!(id_archivo(&backup_payload) == backup_id);
    assert_eq!(fs::read(&stage_canary).unwrap(), STAGE_CANARY);
    assert!(id_archivo(&stage_canary) == stage_id);
    assert_eq!(
        fs::read(wrong_stage.join("marker")).unwrap(),
        b"wrong transaction decoy"
    );
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn journal_datos_de_otro_parent_se_rechaza_con_canonico_ausente() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let journal_path = fixture.ubicacion.journal().as_path().to_path_buf();
    let journal_valido = Journal::parse(&fixture.bytes_journal).unwrap();
    let context_path = contexto_path(&fixture.ubicacion);
    let context_id = id_archivo(&context_path);
    let root_id = identidad_directorio(&fixture.root);
    fs::remove_file(&journal_path).unwrap();

    let foreign_path = temp.path().join("foreign-parent");
    fs::create_dir(&foreign_path).unwrap();
    let foreign = abrir_directorio_absoluto(&foreign_path, true).unwrap();
    let foreign_identity = metadata_directorio(&foreign)
        .unwrap()
        .journal_identity()
        .unwrap();
    let journal_foreign = Journal::new(
        journal_valido.transaction_id(),
        foreign_identity,
        journal_valido.old().clone(),
        journal_valido.new_fingerprint().clone(),
    )
    .unwrap();

    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert!(matches!(
        owner.observar_inventario_recovery_con_journal_datos(&journal_foreign),
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));

    assert!(!journal_path.exists());
    assert_eq!(fs::read(&context_path).unwrap(), fixture.bytes_contexto);
    assert!(id_archivo(&context_path) == context_id);
    assert!(identidad_directorio(&fixture.root) == root_id);
    assert!(!fixture.ubicacion.backup().as_path().exists());
    assert!(!fixture.ubicacion.stage(TXID).unwrap().as_path().exists());
    verificar_arbol(&fixture.root, &fixture.entradas);
}
