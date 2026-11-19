use super::*;

use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
    FixturePublicada, assert_storage_unchanged, contexto_path, id_archivo, materializar,
    preparar_con_tags, verificar_arbol,
};
use crate::transaccion::journal::Journal;
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::tempdir;

#[path = "prueba_root_coincidente_aux_tests.rs"]
mod aux_tests;

#[path = "prueba_root_coincidente_intent_tests.rs"]
mod intent_tests;

#[path = "prueba_root_coincidente_revalidacion_tests.rs"]
mod revalidation_tests;

#[path = "prueba_root_coincidente_conservar_new_tests.rs"]
mod conservar_new_tests;

struct Storage {
    bytes_contexto: Vec<u8>,
    bytes_journal: Vec<u8>,
    id_contexto: crate::transaccion::anclado::IdentidadArchivo,
    id_journal: crate::transaccion::anclado::IdentidadArchivo,
}

fn ruta_journal(fixture: &FixturePublicada) -> PathBuf {
    fixture.ubicacion.journal().as_path().to_path_buf()
}

fn preparar_soportado(parent: &Path) -> FixturePublicada {
    preparar_con_tags(parent, VERSION_MOTOR_V1, POLICY_ID_V1)
}

fn fijar_estado(fixture: &FixturePublicada, esperado: JournalState) -> Vec<u8> {
    let mut journal = Journal::parse(&fixture.bytes_journal).unwrap();
    for state in [
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ] {
        if journal.state() == esperado {
            break;
        }
        journal.advance(state).unwrap();
    }
    assert_eq!(journal.state(), esperado);
    let bytes = journal.encode().unwrap();
    fs::write(ruta_journal(fixture), &bytes).unwrap();
    bytes
}

fn capturar_storage(fixture: &FixturePublicada) -> Storage {
    let path_contexto = contexto_path(&fixture.ubicacion);
    let path_journal = ruta_journal(fixture);
    Storage {
        bytes_contexto: fs::read(&path_contexto).unwrap(),
        bytes_journal: fs::read(&path_journal).unwrap(),
        id_contexto: id_archivo(&path_contexto),
        id_journal: id_archivo(&path_journal),
    }
}

fn verificar_storage(fixture: &FixturePublicada, storage: &Storage) {
    assert_storage_unchanged(
        fixture,
        &storage.bytes_contexto,
        &storage.bytes_journal,
        storage.id_contexto,
        storage.id_journal,
    );
}

#[test]
fn emite_prueba_opaca_solo_en_installed_y_committed() {
    for estado in [JournalState::Installed, JournalState::Committed] {
        let temp = tempdir().unwrap();
        let fixture = preparar_soportado(&temp.path().join("parent"));
        let bytes_journal = fijar_estado(&fixture, estado);
        let storage = capturar_storage(&fixture);
        assert_eq!(storage.bytes_journal, bytes_journal);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();

        assert_eq!(prueba.journal.state(), estado);
        let journal_esperado = Journal::parse(&storage.bytes_journal).unwrap();
        assert!(prueba.journal == journal_esperado);
        assert_eq!(
            prueba.identidad_archivo_journal,
            fixture.id_journal_publicado
        );
        assert_eq!(
            prueba.evidencia_contexto.longitud,
            u64::try_from(storage.bytes_contexto.len()).unwrap()
        );
        assert_eq!(
            prueba.evidencia_contexto.identidad_archivo,
            storage.id_contexto
        );
        assert!(prueba.identidad_parent == prueba._owner.identidad_parent);
        assert!(prueba.scope_parent == prueba._owner.scope_parent);
        assert!(Arc::ptr_eq(&prueba._autoridad, &prueba._owner._autoridad));
        let debug = format!("{prueba:?}");
        assert_eq!(debug, "PruebaReplayAdmitidoRootCoincidente { .. }");
        assert!(!debug.contains(prueba.journal.transaction_id()));
        verificar_storage(&fixture, &storage);
        verificar_arbol(&fixture.root, &fixture.entradas);
    }
}

#[test]
fn no_emite_prueba_en_prepared_ni_backup_ready_y_preserva_storage() {
    for estado in [JournalState::Prepared, JournalState::BackupReady] {
        let temp = tempdir().unwrap();
        let fixture = preparar_soportado(&temp.path().join("parent"));
        fijar_estado(&fixture, estado);
        let storage = capturar_storage(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let error = owner.probar_replay_admitido_root_coincidente().unwrap_err();

        assert_eq!(error.to_string(), "admission_replay_root_not_proven");
        verificar_storage(&fixture, &storage);
        verificar_arbol(&fixture.root, &fixture.entradas);
    }
}

#[test]
fn txid_distinto_en_journal_mismo_id_no_emite_prueba_ni_muta_storage() {
    let temp = tempdir().unwrap();
    let fixture = preparar_soportado(&temp.path().join("parent"));
    let bytes_originales = fijar_estado(&fixture, JournalState::Installed);
    let journal_original = Journal::parse(&bytes_originales).unwrap();
    let path_journal = ruta_journal(&fixture);
    let path_contexto = contexto_path(&fixture.ubicacion);
    let id_original = id_archivo(&path_journal);
    let bytes_contexto_original = fs::read(&path_contexto).unwrap();
    let id_contexto_original = id_archivo(&path_contexto);
    let mut journal_distinto = Journal::new(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        journal_original.parent().clone(),
        journal_original.old().clone(),
        journal_original.new_fingerprint().clone(),
    )
    .unwrap();
    journal_distinto.advance(JournalState::BackupReady).unwrap();
    journal_distinto.advance(JournalState::Installed).unwrap();
    assert!(journal_distinto.transaction_id() != journal_original.transaction_id());
    assert!(journal_distinto.parent() == journal_original.parent());
    assert!(journal_distinto.old() == journal_original.old());
    assert!(journal_distinto.new_fingerprint() == journal_original.new_fingerprint());
    let bytes_journal_distinto = journal_distinto.encode().unwrap();

    fs::write(&path_journal, &bytes_journal_distinto).unwrap();
    assert!(id_archivo(&path_journal) == id_original);
    let storage = capturar_storage(&fixture);
    assert_eq!(storage.bytes_journal, bytes_journal_distinto);
    assert_eq!(storage.bytes_contexto, bytes_contexto_original);
    assert!(storage.id_contexto == id_contexto_original);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let error = owner.probar_replay_admitido_root_coincidente().unwrap_err();

    assert_eq!(error.to_string(), "admission_replay_root_not_proven");
    verificar_storage(&fixture, &storage);
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn root_con_mismos_bytes_e_identidad_distinta_no_emite_prueba() {
    let temp = tempdir().unwrap();
    let fixture = preparar_soportado(&temp.path().join("parent"));
    let bytes_journal = fijar_estado(&fixture, JournalState::Installed);
    let storage = capturar_storage(&fixture);
    let original = temp.path().join("root-original-retained");
    fs::rename(&fixture.root, &original).unwrap();
    materializar(&fixture.root, &fixture.entradas);
    verificar_arbol(&fixture.root, &fixture.entradas);
    verificar_arbol(&original, &fixture.entradas);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observado = owner.observar_root_raw().unwrap().unwrap();
    assert!(observado.identity() != fixture.esperado.identity());

    let error = owner.probar_replay_admitido_root_coincidente().unwrap_err();

    assert_eq!(error.to_string(), "admission_replay_root_not_proven");
    assert_eq!(storage.bytes_journal, bytes_journal);
    verificar_storage(&fixture, &storage);
    verificar_arbol(&fixture.root, &fixture.entradas);
    verificar_arbol(&original, &fixture.entradas);
}
