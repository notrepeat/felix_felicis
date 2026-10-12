use super::*;

use super::super::dispatcher::{
    ejecutar_recovery_soportado_con, ErrorRecoverySoportado, ResultadoRecoverySoportado,
};
use crate::transaccion::anclado::DirectorioAnclado;
use crate::transaccion::inventario::inventariar_root_anclado;
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, Journal};
use crate::transaccion::raiz::UbicacionRaiz;
use crate::transaccion::snapshot::calcular_digest_raw_tree;
use std::cell::Cell;
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

fn inventario_restore_old_inicial() -> RecoveryInventory {
    RecoveryInventory {
        root: ArtifactState::Absent,
        backup: ArtifactState::Old,
        stage: ArtifactState::Absent,
        root_new_validated: false,
    }
}

fn inventario_restore_old_restaurado() -> RecoveryInventory {
    RecoveryInventory {
        root: ArtifactState::Old,
        backup: ArtifactState::Absent,
        stage: ArtifactState::Absent,
        root_new_validated: false,
    }
}

fn inventario_restore_old_inesperado() -> RecoveryInventory {
    RecoveryInventory {
        root: ArtifactState::Unknown,
        backup: ArtifactState::Absent,
        stage: ArtifactState::Absent,
        root_new_validated: false,
    }
}

fn ejecutar_dispatcher_contando_observaciones(
    lock: &mut LockEscritura,
    observaciones: &Cell<usize>,
) -> Result<ResultadoRecoverySoportado, ErrorRecoverySoportado> {
    ejecutar_recovery_soportado_con(
        lock,
        |lock, journal| {
            observaciones.set(observaciones.get() + 1);
            observar_inventario_recovery(lock, journal)
        },
        |_| {},
        |_| {},
    )
}

#[test]
fn dispatcher_ejecuta_ambas_filas_en_las_tres_fases_con_una_observacion_inicial() {
    for state in [
        JournalState::Prepared,
        JournalState::BackupReady,
        JournalState::Installed,
    ] {
        let (_temp, root, mut lock, canonical, _) = preparar_fila(state);
        let observaciones = Cell::new(0);

        assert_eq!(
            ejecutar_dispatcher_contando_observaciones(&mut lock, &observaciones),
            Ok(ResultadoRecoverySoportado::KeepOld(
                ResultadoFilaJournalOld::Eliminado
            )),
            "KeepOld en {state:?}"
        );
        assert_eq!(observaciones.get(), 1, "KeepOld en {state:?}");
        assert!(!canonical.exists());
        assert_root_intact(&root);

        let (_temp, root, mut lock, canonical, _) = preparar_fila(state);
        fs::rename(&root, backup_path(&lock)).unwrap();
        let observaciones = Cell::new(0);

        assert_eq!(
            ejecutar_dispatcher_contando_observaciones(&mut lock, &observaciones),
            Ok(ResultadoRecoverySoportado::RestoreOld(
                ResultadoFilaJournalRestoreOld::Eliminado
            )),
            "RestoreOld en {state:?}"
        );
        assert_eq!(observaciones.get(), 1, "RestoreOld en {state:?}");
        assert!(!canonical.exists());
        assert!(!backup_path(&lock).exists());
        assert_root_intact(&root);
    }
}

#[test]
fn dispatcher_con_journal_ausente_no_observa() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    crear_arbol(&root, b"old-tree");
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let observaciones = Cell::new(0);

    assert_eq!(
        ejecutar_recovery_soportado_con(
            &mut lock,
            |_, _| {
                observaciones.set(observaciones.get() + 1);
                Err(ErrorObservadorRecovery::Io)
            },
            |_| {},
            |_| {},
        ),
        Ok(ResultadoRecoverySoportado::JournalAusente)
    );
    assert_eq!(observaciones.get(), 0);
    assert_root_intact(&root);
}

#[test]
fn dispatcher_committed_falla_antes_de_observar_y_conserva_canarios() {
    let (_temp, root, mut lock, canonical, mut journal) = preparar_fila(JournalState::Prepared);
    journal.advance(JournalState::BackupReady).unwrap();
    journal.advance(JournalState::Installed).unwrap();
    journal.advance(JournalState::Committed).unwrap();
    let journal_bytes = journal.encode().unwrap();
    fs::write(&canonical, &journal_bytes).unwrap();
    let backup = backup_path(&lock);
    let backup_canary = b"regular-file-at-backup-name";
    fs::write(&backup, backup_canary).unwrap();
    let observaciones = Cell::new(0);

    assert_eq!(
        ejecutar_dispatcher_contando_observaciones(&mut lock, &observaciones),
        Err(ErrorRecoverySoportado::FaseNoSoportada)
    );
    assert_eq!(observaciones.get(), 0);
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert_eq!(fs::read(backup).unwrap(), backup_canary);
    assert_root_intact(&root);
}

#[test]
fn dispatcher_stage_presente_falla_cerrado_y_no_modifica_la_fila() {
    let (_temp, root, mut lock, canonical, _) = preparar_fila(JournalState::Installed);
    let stage = stage_path(&lock);
    crear_arbol(&stage, b"stage-canary");
    let journal_bytes = fs::read(&canonical).unwrap();
    let observaciones = Cell::new(0);

    assert_eq!(
        ejecutar_dispatcher_contando_observaciones(&mut lock, &observaciones),
        Err(ErrorRecoverySoportado::FilaNoSoportada)
    );
    assert_eq!(observaciones.get(), 1);
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert_root_intact(&root);
    assert_eq!(fs::read(stage.join("data.bin")).unwrap(), b"stage-canary");
}

#[test]
fn dispatcher_no_enmascara_error_keep_old_con_un_intento_restore_old() {
    let (_temp, root, mut lock, canonical, _) = preparar_fila(JournalState::Installed);
    let replacement = canonical.with_extension("replacement");
    fs::copy(&canonical, &replacement).unwrap();
    let replacement_bytes = fs::read(&replacement).unwrap();
    let observaciones = Cell::new(0);

    let result = ejecutar_recovery_soportado_con(
        &mut lock,
        |lock, journal| {
            observaciones.set(observaciones.get() + 1);
            observar_inventario_recovery(lock, journal)
        },
        |_| panic!("KeepOld no debe intentar RestoreOld"),
        |_| {
            fs::remove_file(&canonical).unwrap();
            fs::rename(&replacement, &canonical).unwrap();
        },
    );

    assert_eq!(
        result,
        Err(ErrorRecoverySoportado::KeepOld(
            ErrorFilaJournalOld::ConflictoAlEliminar(ErrorJournalAnclado::IdentityMismatch)
        ))
    );
    assert_eq!(observaciones.get(), 1);
    assert_eq!(fs::read(&canonical).unwrap(), replacement_bytes);
    assert_root_intact(&root);
}

#[test]
fn dispatcher_no_enmascara_error_restore_old_con_un_intento_keep_old() {
    let (_temp, root, mut lock, canonical, _) = preparar_fila(JournalState::Installed);
    let backup = backup_path(&lock);
    fs::rename(&root, &backup).unwrap();
    let journal_bytes = fs::read(&canonical).unwrap();
    let observaciones = Cell::new(0);

    let result = ejecutar_recovery_soportado_con(
        &mut lock,
        |lock, journal| {
            observaciones.set(observaciones.get() + 1);
            observar_inventario_recovery(lock, journal)
        },
        |_| fs::remove_dir_all(&backup).unwrap(),
        |_| panic!("RestoreOld fallido no debe intentar KeepOld"),
    );

    assert_eq!(
        result,
        Err(ErrorRecoverySoportado::RestoreOld(
            ErrorFilaJournalRestoreOld::Renombrar(ErrorRenombrarAnclado::FuenteAusente)
        ))
    );
    assert_eq!(observaciones.get(), 1);
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert!(!root.exists());
    assert!(!backup.exists());
}

#[test]
fn rename_con_error_y_postestado_restaurado_converge() {
    assert_eq!(
        reconciliar_intento_renombrado_restore_old(
            Err(ErrorRenombrarAnclado::Io),
            inventario_restore_old_restaurado(),
            inventario_restore_old_inicial(),
        ),
        Ok(())
    );
}

#[test]
fn rename_con_error_y_estado_inicial_propaga_el_error() {
    assert_eq!(
        reconciliar_intento_renombrado_restore_old(
            Err(ErrorRenombrarAnclado::DestinoExistente),
            inventario_restore_old_inicial(),
            inventario_restore_old_inicial(),
        ),
        Err(ErrorFilaJournalRestoreOld::Renombrar(
            ErrorRenombrarAnclado::DestinoExistente
        ))
    );
}

#[test]
fn rename_con_estado_posterior_inesperado_falla_cerrado() {
    assert_eq!(
        reconciliar_intento_renombrado_restore_old(
            Err(ErrorRenombrarAnclado::Io),
            inventario_restore_old_inesperado(),
            inventario_restore_old_inicial(),
        ),
        Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada)
    );
}

#[test]
fn rename_ok_con_estado_inicial_falla_cerrado() {
    assert_eq!(
        reconciliar_intento_renombrado_restore_old(
            Ok(()),
            inventario_restore_old_inicial(),
            inventario_restore_old_inicial(),
        ),
        Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada)
    );
}

#[test]
fn rename_ok_con_estado_inesperado_falla_cerrado() {
    assert_eq!(
        reconciliar_intento_renombrado_restore_old(
            Ok(()),
            inventario_restore_old_inesperado(),
            inventario_restore_old_inicial(),
        ),
        Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada)
    );
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
fn restore_old_rechaza_committed_y_conserva_backup_y_journal() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Prepared);
    let mut journal = escribir_installed(&canonical, journal);
    journal.advance(JournalState::Committed).unwrap();
    let bytes = journal.encode().unwrap();
    fs::write(&canonical, &bytes).unwrap();
    let backup = backup_path(&lock);
    fs::rename(&root, &backup).unwrap();

    assert_eq!(
        ejecutar_fila_journal_restore_old(&mut lock),
        Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert!(fingerprint(&backup) == *journal.old());
    assert!(!root.exists());
}

#[test]
fn restore_old_sin_journal_no_autoriza_mover_el_backup() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    crear_arbol(&root, b"old-tree");
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let backup = backup_path(&lock);
    let canonical = lock.ubicacion().journal().as_path().to_path_buf();
    fs::rename(&root, &backup).unwrap();

    assert_eq!(
        ejecutar_fila_journal_restore_old(&mut lock),
        Ok(ResultadoFilaJournalRestoreOld::NoAplicableJournalAusente)
    );
    assert!(!root.exists());
    assert_eq!(fs::read(backup.join("data.bin")).unwrap(), b"old-tree");
    assert!(!canonical.exists());
}

#[test]
fn restore_old_rechaza_y_preserva_stage_new_valido() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    let new_fixture = temp.path().join("new-fixture");
    crear_arbol(&root, b"old-tree");
    crear_arbol(&new_fixture, b"new-tree");
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let journal = Journal::new(
        TXN,
        identidad_parent(&lock),
        fingerprint(&root),
        fingerprint(&new_fixture),
    )
    .unwrap();
    let canonical = lock.ubicacion().journal().as_path().to_path_buf();
    let journal_bytes = journal.encode().unwrap();
    fs::write(&canonical, &journal_bytes).unwrap();

    let backup = backup_path(&lock);
    let stage = stage_path(&lock);
    fs::rename(&root, &backup).unwrap();
    fs::rename(&new_fixture, &stage).unwrap();
    assert!(fingerprint(&stage) == *journal.new_fingerprint());

    assert_eq!(
        ejecutar_fila_journal_restore_old(&mut lock),
        Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert!(!root.exists());
    assert_root_intact(&backup);
    assert_eq!(fs::read(stage.join("data.bin")).unwrap(), b"new-tree");
}

#[test]
fn restore_old_no_reemplaza_un_root_que_aparece_antes_del_rename() {
    let (_temp, root, mut lock, canonical, _journal) = preparar_fila(JournalState::Prepared);
    let backup = backup_path(&lock);
    fs::rename(&root, &backup).unwrap();
    let journal_bytes = fs::read(&canonical).unwrap();

    let result = ejecutar_fila_journal_restore_old_con(
        &mut lock,
        |_| crear_arbol(&root, b"destination-canary"),
        |_| {},
    );

    assert_eq!(result, Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada));
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert_eq!(
        fs::read(root.join("data.bin")).unwrap(),
        b"destination-canary"
    );
    assert_eq!(fs::read(backup.join("data.bin")).unwrap(), b"old-tree");
}

#[test]
fn restore_old_rechaza_backup_con_identidad_ajena_aunque_sus_bytes_coincidan() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Prepared);
    let backup = backup_path(&lock);
    let old_moved = root.with_file_name("saved-old");
    fs::rename(&root, &old_moved).unwrap();
    crear_arbol(&backup, b"old-tree");
    let journal_bytes = fs::read(&canonical).unwrap();

    assert!(!(fingerprint(&backup) == *journal.old()));
    assert_eq!(
        ejecutar_fila_journal_restore_old(&mut lock),
        Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert_root_intact(&old_moved);
    assert_root_intact(&backup);
    assert!(!root.exists());
}

#[test]
fn restore_old_rechaza_backup_con_digest_distinto_y_conserva_canarios() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Prepared);
    let backup = backup_path(&lock);
    fs::rename(&root, &backup).unwrap();
    fs::write(backup.join("data.bin"), b"tampered-tree").unwrap();
    let journal_bytes = fs::read(&canonical).unwrap();

    assert!(fingerprint(&backup).identity() == journal.old().identity());
    assert!(!(fingerprint(&backup) == *journal.old()));
    assert_eq!(
        ejecutar_fila_journal_restore_old(&mut lock),
        Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert_eq!(fs::read(backup.join("data.bin")).unwrap(), b"tampered-tree");
    assert!(!root.exists());
}

#[test]
fn reinicio_despues_de_restore_old_converge_con_la_fila_keep_old_existente() {
    let (_temp, root, lock, canonical, journal) = preparar_fila(JournalState::Installed);
    let backup = backup_path(&lock);
    fs::rename(&root, &backup).unwrap();
    fs::rename(&backup, &root).unwrap();
    drop(lock);

    let mut restarted = LockEscritura::adquirir(&root).unwrap();
    assert_eq!(
        ejecutar_fila_journal_old(&mut restarted),
        Ok(ResultadoFilaJournalOld::Eliminado)
    );
    assert!(!canonical.exists());
    assert_root_intact(&root);
    assert!(fingerprint(&root) == *journal.old());
}

#[test]
fn restore_old_preserva_reemplazo_de_journal_detectado_al_eliminar() {
    let (_temp, root, mut lock, canonical, _journal) = preparar_fila(JournalState::Prepared);
    let backup = backup_path(&lock);
    fs::rename(&root, &backup).unwrap();
    let replacement = canonical.with_extension("replacement");
    fs::copy(&canonical, &replacement).unwrap();
    let replacement_bytes = fs::read(&replacement).unwrap();

    let result = ejecutar_fila_journal_restore_old_con(
        &mut lock,
        |_| {},
        |_| {
            fs::remove_file(&canonical).unwrap();
            fs::rename(&replacement, &canonical).unwrap();
        },
    );

    assert_eq!(
        result,
        Err(ErrorFilaJournalRestoreOld::ConflictoAlEliminar(
            ErrorJournalAnclado::IdentityMismatch
        ))
    );
    assert_eq!(fs::read(&canonical).unwrap(), replacement_bytes);
    assert_root_intact(&root);
    assert!(!backup.exists());
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
fn committed_rechaza_antes_de_observar_un_backup_que_fallaria_cerrado() {
    let (_temp, root, mut lock, canonical, mut journal) = preparar_fila(JournalState::Prepared);
    journal.advance(JournalState::BackupReady).unwrap();
    journal.advance(JournalState::Installed).unwrap();
    journal.advance(JournalState::Committed).unwrap();
    let journal_bytes = journal.encode().unwrap();
    fs::write(&canonical, &journal_bytes).unwrap();

    let backup = backup_path(&lock);
    let canary = b"regular-file-at-backup-name";
    fs::write(&backup, canary).unwrap();
    assert!(observar_inventario_recovery(&lock, &journal).is_err());

    assert_eq!(
        ejecutar_fila_journal_old(&mut lock),
        Err(ErrorFilaJournalOld::FilaNoAutorizada)
    );
    assert_eq!(
        ejecutar_fila_journal_restore_old(&mut lock),
        Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert_eq!(fs::read(backup).unwrap(), canary);
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
