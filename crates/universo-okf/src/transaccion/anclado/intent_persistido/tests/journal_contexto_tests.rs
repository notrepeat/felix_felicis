//! IDs nativos medidos en handles fixture; longitudes y digests son datos
//! sintéticos, sin probe de montaje.

use std::fs::{self, File};
use std::path::Path;

use crate::transaccion::anclado::identidad_archivo::{IdentidadArchivo, identidad_archivo};
use crate::transaccion::journal::{Journal, JournalState};

use super::record;

fn native_identity(file: &File) -> IdentidadArchivo {
    identidad_archivo(file).unwrap()
}

fn committed_with_changed_transaction_id(journal: &Journal) -> Journal {
    let mut changed = Journal::new(
        "1123456789abcdef0123456789abcdef",
        journal.parent().clone(),
        journal.old().clone(),
        journal.new_fingerprint().clone(),
    )
    .unwrap();
    for state in [
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ] {
        changed.advance(state).unwrap();
    }
    changed
}

fn fixed_error(error: impl std::fmt::Debug + std::fmt::Display) {
    assert_eq!(format!("{error:?}"), "ErrorIntentRetiroPersistido");
    assert_eq!(error.to_string(), "invalid_persisted_retirement_intent");
}

fn file_at(path: &Path) -> File {
    File::open(path).unwrap()
}

#[test]
fn vincula_journal_con_misma_identidad_y_registro_completo() {
    let temp = tempfile::tempdir().unwrap();
    let intent = record(&temp, 1);
    let journal_file = file_at(&temp.path().join("journal"));
    let journal_id = native_identity(&journal_file);

    intent
        .validar_vinculo_journal(journal_id, intent.journal())
        .unwrap();
}

#[test]
fn journal_distinto_con_registro_igual_falla_con_error_fijo() {
    let temp = tempfile::tempdir().unwrap();
    let intent = record(&temp, 1);
    let original_path = temp.path().join("journal");
    let decoy_path = temp.path().join("journal-decoy");
    fs::write(&decoy_path, fs::read(&original_path).unwrap()).unwrap();
    let original = file_at(&original_path);
    let decoy = file_at(&decoy_path);
    let original_id = native_identity(&original);
    let decoy_id = native_identity(&decoy);
    assert_ne!(original_id, decoy_id);

    let error = intent
        .validar_vinculo_journal(decoy_id, intent.journal())
        .unwrap_err();
    fixed_error(error);
}

#[test]
fn journal_con_misma_identidad_y_otro_campo_committed_falla() {
    let temp = tempfile::tempdir().unwrap();
    let intent = record(&temp, 1);
    let journal_file = file_at(&temp.path().join("journal"));
    let journal_id = native_identity(&journal_file);
    let changed = committed_with_changed_transaction_id(intent.journal());
    assert_eq!(changed.state(), JournalState::Committed);
    assert_ne!(intent.journal(), &changed);

    let error = intent
        .validar_vinculo_journal(journal_id, &changed)
        .unwrap_err();
    fixed_error(error);
}

#[test]
fn vincula_contexto_con_misma_identidad_longitud_y_digest() {
    let temp = tempfile::tempdir().unwrap();
    let intent = record(&temp, 1);
    let context_file = file_at(&temp.path().join("context"));
    let context_id = native_identity(&context_file);

    intent
        .validar_vinculo_contexto(context_id, 1, [0x5a; 32])
        .unwrap();
}

#[test]
fn contexto_distinto_con_longitud_y_digest_iguales_falla() {
    let temp = tempfile::tempdir().unwrap();
    let intent = record(&temp, 1);
    let original_path = temp.path().join("context");
    let decoy_path = temp.path().join("context-decoy");
    fs::write(&decoy_path, fs::read(&original_path).unwrap()).unwrap();
    let original = file_at(&original_path);
    let decoy = file_at(&decoy_path);
    let original_id = native_identity(&original);
    let decoy_id = native_identity(&decoy);
    assert_ne!(original_id, decoy_id);

    let error = intent
        .validar_vinculo_contexto(decoy_id, 1, [0x5a; 32])
        .unwrap_err();
    fixed_error(error);
}

#[test]
fn longitud_contexto_distinta_con_id_y_digest_iguales_falla() {
    let temp = tempfile::tempdir().unwrap();
    let intent = record(&temp, 1);
    let context_file = file_at(&temp.path().join("context"));
    let context_id = native_identity(&context_file);

    let error = intent
        .validar_vinculo_contexto(context_id, 2, [0x5a; 32])
        .unwrap_err();
    fixed_error(error);
}

#[test]
fn digest_contexto_distinto_con_id_y_longitud_iguales_falla() {
    let temp = tempfile::tempdir().unwrap();
    let intent = record(&temp, 1);
    let context_file = file_at(&temp.path().join("context"));
    let context_id = native_identity(&context_file);

    let error = intent
        .validar_vinculo_contexto(context_id, 1, [0x5b; 32])
        .unwrap_err();
    fixed_error(error);
}
