use super::*;

use crate::transaccion::anclado::DirectorioAnclado;
use crate::transaccion::inventario::inventariar_root_anclado;
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, Journal};
use crate::transaccion::raiz::UbicacionRaiz;
use crate::transaccion::snapshot::calcular_digest_raw_tree;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const TXN: &str = "0123456789abcdef0123456789abcdef";

fn crear_arbol(root: &Path, bytes: &[u8]) {
    fs::create_dir_all(root.join("empty-dir")).unwrap();
    fs::write(root.join("data.bin"), bytes).unwrap();
}

fn fingerprint(root: &Path) -> ArtifactFingerprint {
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
    ArtifactFingerprint::new(identity, digest)
}

fn identidad_parent(lock: &LockEscritura) -> DirectoryIdentity {
    DirectorioAnclado::parent_solo_lectura_desde_ubicacion(lock.ubicacion())
        .unwrap()
        .identidad_parent()
        .journal_identity()
        .unwrap()
}

fn preparar_fila(state: JournalState) -> (TempDir, PathBuf, LockEscritura, PathBuf, Journal) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    crear_arbol(&root, b"old-tree");
    let lock = LockEscritura::adquirir(&root).unwrap();
    let old = fingerprint(&root);
    let new_fixture = temp.path().join("new-fixture");
    crear_arbol(&new_fixture, b"new-tree");
    let mut journal =
        Journal::new(TXN, identidad_parent(&lock), old, fingerprint(&new_fixture)).unwrap();
    if state != JournalState::Prepared {
        journal.advance(JournalState::BackupReady).unwrap();
    }
    if state == JournalState::Installed {
        journal.advance(JournalState::Installed).unwrap();
    }
    let canonical = lock.ubicacion().journal().as_path().to_path_buf();
    fs::write(&canonical, journal.encode().unwrap()).unwrap();
    (temp, root, lock, canonical, journal)
}

fn escribir_installed(canonical: &Path, mut journal: Journal) -> Journal {
    journal.advance(JournalState::BackupReady).unwrap();
    journal.advance(JournalState::Installed).unwrap();
    fs::write(canonical, journal.encode().unwrap()).unwrap();
    journal
}

fn backup_path(lock: &LockEscritura) -> PathBuf {
    lock.ubicacion().backup().as_path().to_path_buf()
}

fn stage_path(lock: &LockEscritura) -> PathBuf {
    lock.ubicacion().stage(TXN).unwrap().as_path().to_path_buf()
}

fn assert_root_intact(root: &Path) {
    assert_eq!(fs::read(root.join("data.bin")).unwrap(), b"old-tree");
    assert!(root.join("empty-dir").is_dir());
}

#[test]
fn restore_old_restaura_backup_en_las_tres_fases_y_conserva_identidad_y_bytes() {
    for state in [
        JournalState::Prepared,
        JournalState::BackupReady,
        JournalState::Installed,
    ] {
        let (_temp, root, mut lock, canonical, journal) = preparar_fila(state);
        let backup = backup_path(&lock);
        fs::rename(&root, &backup).unwrap();

        assert_eq!(
            ejecutar_fila_journal_restore_old(&mut lock),
            Ok(ResultadoFilaJournalRestoreOld::Eliminado),
            "fase {state:?}"
        );

        assert!(!backup.exists());
        assert!(!canonical.exists());
        assert_root_intact(&root);
        assert!(fingerprint(&root) == *journal.old());
    }
}

#[test]
fn prepared_y_backup_ready_eliminan_solo_el_journal_y_conservan_root() {
    for state in [JournalState::Prepared, JournalState::BackupReady] {
        let (_temp, root, mut lock, canonical, _journal) = preparar_fila(state);

        assert_eq!(
            ejecutar_fila_journal_old(&mut lock),
            Ok(ResultadoFilaJournalOld::Eliminado),
            "fase {state:?}"
        );

        assert!(!canonical.exists());
        assert_root_intact(&root);
    }
}

#[test]
fn installed_root_old_sin_vecinos_elimina_journal_y_conserva_root() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Prepared);
    escribir_installed(&canonical, journal);

    assert_eq!(
        ejecutar_fila_journal_old(&mut lock),
        Ok(ResultadoFilaJournalOld::Eliminado)
    );
    assert!(!canonical.exists());
    assert_root_intact(&root);
}

#[test]
fn committed_root_old_sin_vecinos_falla_y_conserva_estado() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Prepared);
    let mut journal = escribir_installed(&canonical, journal);
    journal.advance(JournalState::Committed).unwrap();
    let bytes = journal.encode().unwrap();
    fs::write(&canonical, &bytes).unwrap();

    assert_eq!(
        ejecutar_fila_journal_old(&mut lock),
        Err(ErrorFilaJournalOld::FilaNoAutorizada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert_root_intact(&root);
}

#[test]
fn repeticion_tras_reinicio_sin_journal_es_no_aplicable_y_no_toca_root() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Prepared);
    escribir_installed(&canonical, journal);
    assert_eq!(
        ejecutar_fila_journal_old(&mut lock),
        Ok(ResultadoFilaJournalOld::Eliminado)
    );
    assert!(!canonical.exists());
    drop(lock);

    let mut restarted = LockEscritura::adquirir(&root).unwrap();
    assert_eq!(
        ejecutar_fila_journal_old(&mut restarted),
        Ok(ResultadoFilaJournalOld::NoAplicableJournalAusente)
    );
    assert_root_intact(&root);
}

#[test]
fn ausencia_despues_de_establecer_contexto_no_afirma_eliminacion() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Prepared);
    escribir_installed(&canonical, journal);

    let result = ejecutar_fila_journal_old_con(&mut lock, |_| {
        fs::remove_file(&canonical).unwrap();
    });

    assert_eq!(result, Ok(ResultadoFilaJournalOld::AusenteTrasContexto));
    assert!(!canonical.exists());
    assert_root_intact(&root);
}

#[test]
fn journal_ausente_al_inicio_no_autoriza_observar_ni_reconstruir_contexto() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    crear_arbol(&root, b"old-tree");
    let mut lock = LockEscritura::adquirir(&root).unwrap();

    assert_eq!(
        ejecutar_fila_journal_old(&mut lock),
        Ok(ResultadoFilaJournalOld::NoAplicableJournalAusente)
    );
    assert_root_intact(&root);
}

#[test]
fn stage_presente_falla_cerrado_y_conserva_journal_y_root() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Prepared);
    crear_arbol(&stage_path(&lock), b"stage-canary");
    let bytes = fs::read(&canonical).unwrap();

    assert_eq!(
        ejecutar_fila_journal_old(&mut lock),
        Err(ErrorFilaJournalOld::FilaNoAutorizada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert_eq!(Journal::parse(&bytes).unwrap(), journal);
    assert_root_intact(&root);
}

#[test]
fn backup_presente_falla_cerrado_y_conserva_journal_y_root() {
    let (_temp, root, mut lock, canonical, _journal) = preparar_fila(JournalState::Prepared);
    crear_arbol(&backup_path(&lock), b"backup-canary");
    let bytes = fs::read(&canonical).unwrap();

    assert_eq!(
        ejecutar_fila_journal_old(&mut lock),
        Err(ErrorFilaJournalOld::FilaNoAutorizada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert_root_intact(&root);
}

#[test]
fn root_cambiado_falla_cerrado_y_conserva_journal() {
    let (_temp, root, mut lock, canonical, _journal) = preparar_fila(JournalState::BackupReady);
    let bytes = fs::read(&canonical).unwrap();
    fs::write(root.join("data.bin"), b"tampered-tree").unwrap();

    assert_eq!(
        ejecutar_fila_journal_old(&mut lock),
        Err(ErrorFilaJournalOld::FilaNoAutorizada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert_eq!(fs::read(root.join("data.bin")).unwrap(), b"tampered-tree");
}

#[test]
fn journal_reemplazado_despues_de_observar_falla_y_conserva_reemplazo() {
    let (_temp, root, mut lock, canonical, _journal) = preparar_fila(JournalState::Prepared);
    let replacement = canonical.with_extension("replacement");
    fs::copy(&canonical, &replacement).unwrap();
    let replacement_bytes = fs::read(&replacement).unwrap();

    let result = ejecutar_fila_journal_old_con(&mut lock, |_| {
        fs::remove_file(&canonical).unwrap();
        fs::rename(&replacement, &canonical).unwrap();
    });

    assert_eq!(
        result,
        Err(ErrorFilaJournalOld::ConflictoAlEliminar(
            ErrorJournalAnclado::IdentityMismatch
        ))
    );
    assert_eq!(fs::read(&canonical).unwrap(), replacement_bytes);
    assert_root_intact(&root);
}

#[test]
fn journal_cambiado_despues_de_observar_falla_y_conserva_registro() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Prepared);
    let mut changed = journal;
    changed.advance(JournalState::BackupReady).unwrap();
    let changed_bytes = changed.encode().unwrap();

    let result = ejecutar_fila_journal_old_con(&mut lock, |_| {
        fs::write(&canonical, &changed_bytes).unwrap();
    });

    assert_eq!(
        result,
        Err(ErrorFilaJournalOld::ConflictoAlEliminar(
            ErrorJournalAnclado::JournalMismatch
        ))
    );
    assert_eq!(fs::read(&canonical).unwrap(), changed_bytes);
    assert_root_intact(&root);
}

#[test]
fn journal_malformado_falla_antes_de_observar_y_se_conserva() {
    let (_temp, root, mut lock, canonical, _journal) = preparar_fila(JournalState::Prepared);
    let malformed = b"malformed-journal-canary";
    fs::write(&canonical, malformed).unwrap();

    assert_eq!(
        ejecutar_fila_journal_old(&mut lock),
        Err(ErrorFilaJournalOld::Journal(
            ErrorJournalAnclado::InvalidJournal(
                crate::transaccion::journal::JournalError::Malformed
            )
        ))
    );
    assert_eq!(fs::read(&canonical).unwrap(), malformed);
    assert_root_intact(&root);
}
