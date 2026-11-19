#![cfg(all(test, any(target_os = "linux", windows)))]

use super::{DuenioTransaccionAnclado, ErrorObservacionSiblingsRecoveryV2};
use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    contexto_path, id_archivo, preparar, verificar_arbol,
};
use crate::transaccion::anclado::{
    abrir_directorio_absoluto, abrir_directorio_hijo_lectura, metadata_directorio,
};
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{ArtifactFingerprint, Journal, RawTreeDigest};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::snapshot::{EntradaSnapshot, calcular_digest_raw_tree};
use std::cell::Cell;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

const V2_TRANSACTION_ID: &str = "abcdef0123456789abcdef0123456789";
const DECOY_BYTES: &[u8] = b"stage for the other transaction";
const BACKUP_BYTES: &[u8] = b"retained backup observation";

fn journal_v1(fixture: &super::super::fixture_root_replay::FixturePublicada) -> Journal {
    Journal::parse(&fixture.bytes_journal).unwrap()
}

fn journal_v2(
    fixture: &super::super::fixture_root_replay::FixturePublicada,
    transaction_id: &str,
) -> JournalV2 {
    let journal = journal_v1(fixture);
    JournalV2::new(
        transaction_id,
        journal.parent().clone(),
        journal.old().clone(),
        journal.new_fingerprint().clone(),
    )
    .unwrap()
}

fn directory_identity(path: &Path) -> crate::transaccion::journal::DirectoryIdentity {
    let parent = abrir_directorio_absoluto(path.parent().unwrap(), true).unwrap();
    let directory = abrir_directorio_hijo_lectura(&parent, path.file_name().unwrap()).unwrap();
    metadata_directorio(&directory)
        .unwrap()
        .journal_identity()
        .unwrap()
}

fn raw_digest(bytes: &[u8]) -> RawTreeDigest {
    calcular_digest_raw_tree(&[EntradaSnapshot::archivo("marker", bytes)]).unwrap()
}

fn assert_storage_unchanged(
    fixture: &super::super::fixture_root_replay::FixturePublicada,
    journal_id: crate::transaccion::anclado::IdentidadArchivo,
    context_id: crate::transaccion::anclado::IdentidadArchivo,
) {
    let journal_path = fixture.ubicacion.journal().as_path().to_path_buf();
    let context = contexto_path(&fixture.ubicacion);
    assert_eq!(fs::read(&journal_path).unwrap(), fixture.bytes_journal);
    assert_eq!(fs::read(&context).unwrap(), fixture.bytes_contexto);
    assert_eq!(id_archivo(&journal_path), journal_id);
    assert_eq!(id_archivo(&context), context_id);
}

#[test]
fn nominal_retiene_j2_y_muestras_raw_y_preserva_registros_al_cerrar_owner() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let journal = journal_v2(&fixture, V2_TRANSACTION_ID);
    let journal_path = fixture.ubicacion.journal().as_path().to_path_buf();
    let context_path = contexto_path(&fixture.ubicacion);
    let journal_id = id_archivo(&journal_path);
    let context_id = id_archivo(&context_path);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let observado = owner.observar_siblings_recovery_v2_raw(&journal).unwrap();

    assert_eq!(observado.journal(), &journal);
    assert!(observado.root() == Some(&fixture.esperado));
    assert!(observado.backup().is_none());
    assert!(observado.stage().is_none());
    let debug = format!("{observado:?}");
    assert!(!debug.contains(V2_TRANSACTION_ID));
    drop(owner);
    drop(lock);
    assert_storage_unchanged(&fixture, journal_id, context_id);
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn observa_stage_por_transaction_id_v2_y_no_por_el_id_decoy_v1() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let v1 = journal_v1(&fixture);
    let journal = journal_v2(&fixture, V2_TRANSACTION_ID);
    let stage_v2 = fixture
        .ubicacion
        .stage(journal.transaction_id())
        .unwrap()
        .as_path()
        .to_path_buf();
    let stage_decoy = fixture
        .ubicacion
        .stage(v1.transaction_id())
        .unwrap()
        .as_path()
        .to_path_buf();
    fs::create_dir(&stage_decoy).unwrap();
    fs::write(stage_decoy.join("marker"), DECOY_BYTES).unwrap();
    fs::rename(&fixture.root, &stage_v2).unwrap();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let observado = owner.observar_siblings_recovery_v2_raw(&journal).unwrap();

    assert!(observado.root().is_none());
    assert!(observado.backup().is_none());
    assert!(observado.stage() == Some(&fixture.esperado));
    assert_eq!(fs::read(stage_decoy.join("marker")).unwrap(), DECOY_BYTES);
    drop(owner);
    drop(lock);
}

#[test]
fn ausencia_de_los_tres_siblings_se_conserva_sin_clasificacion() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let journal = journal_v2(&fixture, V2_TRANSACTION_ID);
    fs::remove_dir_all(&fixture.root).unwrap();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let observado = owner.observar_siblings_recovery_v2_raw(&journal).unwrap();

    assert!(observado.root().is_none());
    assert!(observado.backup().is_none());
    assert!(observado.stage().is_none());
}

#[test]
fn fingerprint_raw_conserva_identidad_nativa_y_digest() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let journal = journal_v2(&fixture, V2_TRANSACTION_ID);
    let backup = fixture.ubicacion.backup().as_path().to_path_buf();
    fs::create_dir(&backup).unwrap();
    fs::write(backup.join("marker"), BACKUP_BYTES).unwrap();
    let esperado = ArtifactFingerprint::new(directory_identity(&backup), raw_digest(BACKUP_BYTES));
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let observado = owner.observar_siblings_recovery_v2_raw(&journal).unwrap();

    assert!(observado.backup() == Some(&esperado));
}

#[test]
fn parent_ajeno_se_rechaza_como_observacion_no_verificada() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let journal = journal_v1(&fixture);
    let foreign_path = temp.path().join("foreign-parent");
    fs::create_dir(&foreign_path).unwrap();
    let journal_foreign = JournalV2::new(
        V2_TRANSACTION_ID,
        directory_identity(&foreign_path),
        journal.old().clone(),
        journal.new_fingerprint().clone(),
    )
    .unwrap();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    assert_eq!(
        owner
            .observar_siblings_recovery_v2_raw(&journal_foreign)
            .unwrap_err(),
        ErrorObservacionSiblingsRecoveryV2::ObservationUnverified,
    );
}

#[test]
fn postcheck_exterior_domina_muestra_y_error_del_cuerpo() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    let journal = journal_v2(&fixture, V2_TRANSACTION_ID);
    let foreign_path = fixture.root.parent().unwrap().join("foreign-parent");
    fs::create_dir(&foreign_path).unwrap();
    let journal_foreign = JournalV2::new(
        V2_TRANSACTION_ID,
        directory_identity(&foreign_path),
        journal.old().clone(),
        journal.new_fingerprint().clone(),
    )
    .unwrap();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let candidate_count = Cell::new(0);
    let candidate = owner.observar_siblings_recovery_v2_raw_con_prueba(&journal, |_| {
        candidate_count.set(candidate_count.get() + 1);
        Err(())
    });
    let body_error_count = Cell::new(0);
    let body_error = owner.observar_siblings_recovery_v2_raw_con_prueba(&journal_foreign, |_| {
        body_error_count.set(body_error_count.get() + 1);
        Err(())
    });

    assert_eq!(
        candidate.unwrap_err(),
        ErrorObservacionSiblingsRecoveryV2::OwnerUnverified
    );
    assert_eq!(
        body_error.unwrap_err(),
        ErrorObservacionSiblingsRecoveryV2::OwnerUnverified
    );
    assert_eq!(candidate_count.get(), 1);
    assert_eq!(body_error_count.get(), 1);
}
