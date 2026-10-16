#![cfg(all(test, any(target_os = "linux", windows)))]

use super::super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado};
use super::tests::{OLD_PAYLOAD, TXID, assert_inventory, crear_journal_con_backup_old, observar};
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    contexto_path, id_archivo, preparar, verificar_arbol,
};
use crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado;
use crate::transaccion::anclado::{
    IdentidadDirectorio, abrir_directorio_absoluto, abrir_directorio_hijo_lectura,
    metadata_directorio,
};
use crate::transaccion::journal::Journal;
use crate::transaccion::journal::recovery::ArtifactState;
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::snapshot::{EntradaSnapshot, TipoEntradaSnapshot};
use std::fs;
use tempfile::tempdir;

const STAGE_CANARY: &[u8] = b"observer stage survives missing journal";

fn identidad_directorio(path: &std::path::Path) -> IdentidadDirectorio {
    let parent = abrir_directorio_absoluto(path.parent().unwrap(), true).unwrap();
    let directorio = abrir_directorio_hijo_lectura(&parent, path.file_name().unwrap()).unwrap();
    metadata_directorio(&directorio).unwrap()
}

#[test]
fn root_ausente_aun_clasifica_backup_old_y_stage_new() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let old_entries = [
        EntradaSnapshot::directorio("old"),
        EntradaSnapshot::archivo("old/value.bin", OLD_PAYLOAD),
    ];
    let (journal_path, journal_bytes) = crear_journal_con_backup_old(&fixture, &old_entries);
    let identidad_root_original = identidad_directorio(&fixture.root);
    let backup = fixture.ubicacion.backup().as_path().to_path_buf();
    let stage = fixture
        .ubicacion
        .stage(TXID)
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::rename(&fixture.root, &stage).unwrap();
    let context_path = contexto_path(&fixture.ubicacion);
    let context_id = id_archivo(&context_path);
    let journal_id = id_archivo(&journal_path);
    let ruta_payload = fixture
        .entradas
        .iter()
        .find(|entrada| entrada.tipo() == TipoEntradaSnapshot::Archivo)
        .unwrap()
        .ruta();
    let stage_payload = stage.join(ruta_payload);
    let stage_id = id_archivo(&stage_payload);
    let stage_directory_id = identidad_directorio(&stage);
    let stage_bytes = fs::read(&stage_payload).unwrap();
    let backup_payload = backup.join("old/value.bin");
    let backup_payload_id = id_archivo(&backup_payload);

    let (journal, inventory) = observar(&fixture);

    assert_eq!(journal.transaction_id(), TXID);
    assert_inventory(
        inventory,
        ArtifactState::Absent,
        ArtifactState::Old,
        ArtifactState::New,
    );
    assert!(!fixture.root.exists());
    assert_eq!(fs::read(&journal_path).unwrap(), journal_bytes);
    assert!(id_archivo(&journal_path) == journal_id);
    assert_eq!(fs::read(&context_path).unwrap(), fixture.bytes_contexto);
    assert!(id_archivo(&context_path) == context_id);
    assert!(stage_directory_id == identidad_root_original);
    assert!(identidad_directorio(&stage) == identidad_root_original);
    assert_eq!(fs::read(&stage_payload).unwrap(), stage_bytes);
    assert!(id_archivo(&stage_payload) == stage_id);
    assert_eq!(fs::read(&backup_payload).unwrap(), OLD_PAYLOAD);
    assert!(id_archivo(&backup_payload) == backup_payload_id);
    verificar_arbol(&stage, &fixture.entradas);
}

#[test]
fn journal_ausente_falla_cerrado_y_preserva_contexto_root_backup_y_stage() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let journal = fixture.ubicacion.journal().as_path().to_path_buf();
    let context = contexto_path(&fixture.ubicacion);
    let context_id = id_archivo(&context);
    let root_id = identidad_directorio(&fixture.root);
    let stage = fixture
        .ubicacion
        .stage(TXID)
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&stage).unwrap();
    let stage_canary = stage.join("marker");
    fs::write(&stage_canary, STAGE_CANARY).unwrap();
    let stage_id = id_archivo(&stage_canary);
    let stage_id_directory = identidad_directorio(&stage);
    fs::remove_file(&journal).unwrap();

    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert!(matches!(
        owner.observar_inventario_recovery_anclado(),
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));

    assert!(!journal.exists());
    assert_eq!(fs::read(&context).unwrap(), fixture.bytes_contexto);
    assert!(id_archivo(&context) == context_id);
    assert_eq!(fs::read(&stage_canary).unwrap(), STAGE_CANARY);
    assert!(id_archivo(&stage_canary) == stage_id);
    assert!(identidad_directorio(&stage) == stage_id_directory);
    assert!(!fixture.ubicacion.backup().as_path().exists());
    assert!(identidad_directorio(&fixture.root) == root_id);
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn parent_record_distinto_rechaza_journal_y_preserva_storage() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let context = contexto_path(&fixture.ubicacion);
    let journal_path = fixture.ubicacion.journal().as_path().to_path_buf();
    let context_id = id_archivo(&context);
    let journal_id = id_archivo(&journal_path);
    let root_id = identidad_directorio(&fixture.root);
    let journal_valido = Journal::parse(&fixture.bytes_journal).unwrap();

    let otro_parent_path = temp.path().join("otro-parent");
    fs::create_dir(&otro_parent_path).unwrap();
    let otro_parent = abrir_directorio_absoluto(&otro_parent_path, true).unwrap();
    let identidad_parent_distinto = metadata_directorio(&otro_parent)
        .unwrap()
        .journal_identity()
        .unwrap();
    let parent_actual =
        abrir_directorio_absoluto(fixture.ubicacion.ruta().parent().unwrap(), true).unwrap();
    let identidad_parent_actual = metadata_directorio(&parent_actual)
        .unwrap()
        .journal_identity()
        .unwrap();
    assert!(identidad_parent_distinto != identidad_parent_actual);
    let journal_incompatible = Journal::new(
        journal_valido.transaction_id(),
        identidad_parent_distinto,
        journal_valido.old().clone(),
        journal_valido.new_fingerprint().clone(),
    )
    .unwrap();
    let bytes_incompatibles = journal_incompatible.encode().unwrap();
    fs::write(&journal_path, &bytes_incompatibles).unwrap();

    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let rechazo_lease = {
        let lease = owner.journal_temporal().unwrap();
        lease.cargar_canonico()
    };
    assert!(matches!(
        rechazo_lease,
        Err(ErrorJournalAnclado::ParentIdentityMismatch)
    ));
    assert!(matches!(
        owner.observar_inventario_recovery_anclado(),
        Err(ErrorDuenioTransaccionAnclado::NoVerificable)
    ));

    assert_eq!(fs::read(&journal_path).unwrap(), bytes_incompatibles);
    assert!(id_archivo(&journal_path) == journal_id);
    assert_eq!(fs::read(&context).unwrap(), fixture.bytes_contexto);
    assert!(id_archivo(&context) == context_id);
    assert!(!fixture.ubicacion.backup().as_path().exists());
    assert!(!fixture.ubicacion.stage(TXID).unwrap().as_path().exists());
    assert!(identidad_directorio(&fixture.root) == root_id);
    verificar_arbol(&fixture.root, &fixture.entradas);
}
