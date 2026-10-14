use super::*;

use super::super::dispatcher::{
    ErrorRecoverySoportado, ResultadoRecoverySoportado, ejecutar_recovery_soportado_con,
};
use crate::transaccion::anclado::{DirectorioAnclado, ErrorDirectorioAnclado};
use crate::transaccion::inventario::inventariar_root_anclado;
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, Journal};
use crate::transaccion::raiz::UbicacionRaiz;
use crate::transaccion::snapshot::calcular_digest_raw_tree;
use fs_at::OpenOptions as OpenOptionsAt;
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

fn preparar_discard_new(
    state: JournalState,
) -> (TempDir, PathBuf, LockEscritura, PathBuf, Journal) {
    let (temp, root, lock, canonical, journal) = preparar_fila(state);
    fs::rename(&root, backup_path(&lock)).unwrap();
    fs::rename(root.with_file_name("new-fixture"), &root).unwrap();
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

#[derive(Debug, PartialEq, Eq)]
enum EntradaParentAssertion {
    Directorio,
    Archivo {
        longitud: u64,
        bytes: Option<Vec<u8>>,
    },
}

fn inventario_parent_para_assertions(
    parent: &Path,
    journal: &Path,
    lockfile: &Path,
) -> Vec<(String, EntradaParentAssertion)> {
    fn visitar(
        parent: &Path,
        directorio: &Path,
        journal: &Path,
        lockfile: &Path,
        entradas: &mut Vec<(String, EntradaParentAssertion)>,
    ) {
        let mut hijos = fs::read_dir(directorio)
            .unwrap()
            .map(|entrada| entrada.unwrap())
            .collect::<Vec<_>>();
        hijos.sort_by_key(|entrada| entrada.file_name());

        for hijo in hijos {
            let ruta = hijo.path();
            let relativa = ruta
                .strip_prefix(parent)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let tipo = hijo.file_type().unwrap();
            if tipo.is_dir() {
                entradas.push((relativa, EntradaParentAssertion::Directorio));
                visitar(parent, &ruta, journal, lockfile, entradas);
            } else {
                assert!(
                    tipo.is_file(),
                    "fixture only contains files and directories"
                );
                let longitud = hijo.metadata().unwrap().len();
                let bytes = if ruta == lockfile {
                    None
                } else {
                    Some(fs::read(&ruta).unwrap())
                };
                if ruta == journal {
                    assert!(bytes.is_some(), "journal bytes are part of the inventory");
                }
                entradas.push((
                    relativa,
                    EntradaParentAssertion::Archivo { longitud, bytes },
                ));
            }
        }
    }

    let mut entradas = Vec::new();
    visitar(parent, parent, journal, lockfile, &mut entradas);
    entradas.sort_by(|izquierda, derecha| izquierda.0.cmp(&derecha.0));
    entradas
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
        let (_temp, root, mut lock, canonical, journal) = preparar_fila(state);
        let fingerprint_old = journal.old().clone();
        let fingerprint_before = fingerprint(&root);
        assert!(fingerprint_before == fingerprint_old);
        let backup = backup_path(&lock);
        let stage = stage_path(&lock);
        fs::rename(root.parent().unwrap().join("new-fixture"), &stage).unwrap();
        let journal_bytes = fs::read(&canonical).unwrap();
        let parent = canonical.parent().unwrap();
        let lockfile = lock.ubicacion().lock().as_path().to_path_buf();
        let journal_name = canonical
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let stage_name = stage.file_name().unwrap().to_string_lossy().into_owned();
        let canary = parent.join("keep-canary.bin");
        fs::write(&canary, b"untouched").unwrap();
        let mut parent_after_keep_old =
            inventario_parent_para_assertions(parent, &canonical, &lockfile);
        parent_after_keep_old.retain(|(name, _)| {
            name != &stage_name
                && !name.starts_with(&format!("{stage_name}/"))
                && name != &journal_name
        });
        let observaciones = Cell::new(0);

        assert_eq!(
            ejecutar_recovery_soportado_con(
                &mut lock,
                |lock, journal| {
                    observaciones.set(observaciones.get() + 1);
                    observar_inventario_recovery(lock, journal)
                },
                |_| {},
                |_| {
                    assert!(!stage.exists());
                    assert!(canonical.exists());
                    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
                    assert!(fingerprint(&root) == fingerprint_before);
                },
            ),
            Ok(ResultadoRecoverySoportado::KeepOld(
                ResultadoFilaJournalOld::Eliminado
            )),
            "KeepOld en {state:?}"
        );
        assert_eq!(observaciones.get(), 1, "KeepOld en {state:?}");
        assert!(!canonical.exists());
        assert!(!backup.exists());
        assert!(!stage.exists());
        assert!(root.is_dir());
        assert!(fingerprint(&root) == fingerprint_before);
        assert!(fingerprint(&root) == fingerprint_old);
        assert_eq!(fs::read(&canary).unwrap(), b"untouched");
        assert_root_intact(&root);
        assert_eq!(
            inventario_parent_para_assertions(parent, &canonical, &lockfile),
            parent_after_keep_old,
            "KeepOld en {state:?} solo debe retirar el journal esperado"
        );

        let (_temp, root, mut lock, canonical, journal) = preparar_fila(state);
        let fingerprint_old = journal.old().clone();
        assert!(fingerprint(&root) == fingerprint_old);
        let backup = backup_path(&lock);
        let stage = stage_path(&lock);
        let lockfile = lock.ubicacion().lock().as_path().to_path_buf();
        fs::rename(&root, &backup).unwrap();
        let journal_bytes = fs::read(&canonical).unwrap();
        let parent = canonical.parent().unwrap();
        let parent_before_restore_old =
            inventario_parent_para_assertions(parent, &canonical, &lockfile);
        let journal_name = canonical
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let backup_name = backup.file_name().unwrap().to_string_lossy().into_owned();
        let root_name = root.file_name().unwrap().to_string_lossy().into_owned();
        let journal_index = parent_before_restore_old
            .iter()
            .position(|(name, _)| name == &journal_name)
            .unwrap();
        assert_eq!(
            parent_before_restore_old[journal_index].1,
            EntradaParentAssertion::Archivo {
                longitud: journal_bytes.len() as u64,
                bytes: Some(journal_bytes.clone()),
            }
        );
        let mut parent_after_restore_old = parent_before_restore_old
            .into_iter()
            .filter_map(|(name, entry)| {
                if name == journal_name {
                    None
                } else if name == backup_name || name.starts_with(&format!("{backup_name}/")) {
                    Some((format!("{root_name}{}", &name[backup_name.len()..]), entry))
                } else {
                    Some((name, entry))
                }
            })
            .collect::<Vec<_>>();
        parent_after_restore_old.sort_by(|izquierda, derecha| izquierda.0.cmp(&derecha.0));
        let observaciones = Cell::new(0);

        assert_eq!(
            ejecutar_dispatcher_contando_observaciones(&mut lock, &observaciones),
            Ok(ResultadoRecoverySoportado::RestoreOld(
                ResultadoFilaJournalRestoreOld::Eliminado
            )),
            "RestoreOld en {state:?}"
        );
        assert_eq!(observaciones.get(), 1, "RestoreOld en {state:?}");
        assert!(root.is_dir());
        assert!(fingerprint(&root) == fingerprint_old);
        assert!(!canonical.exists());
        assert!(!backup.exists());
        assert!(!stage.exists());
        assert_root_intact(&root);
        assert_eq!(
            inventario_parent_para_assertions(parent, &canonical, &lockfile),
            parent_after_restore_old,
            "RestoreOld solo debe mover backup a root y retirar el journal esperado"
        );
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
fn restore_old_limpia_stage_new_y_restaura_backup() {
    for state in [
        JournalState::Prepared,
        JournalState::BackupReady,
        JournalState::Installed,
    ] {
        let (_temp, root, mut lock, canonical, journal) = preparar_fila(state);
        let backup = backup_path(&lock);
        let stage = stage_path(&lock);
        fs::rename(&root, &backup).unwrap();
        fs::rename(root.with_file_name("new-fixture"), &stage).unwrap();
        assert!(fingerprint(&stage) == *journal.new_fingerprint());

        assert_eq!(
            ejecutar_fila_journal_restore_old(&mut lock),
            Ok(ResultadoFilaJournalRestoreOld::Eliminado),
            "fase {state:?}"
        );
        assert!(!canonical.exists());
        assert!(!backup.exists());
        assert!(!stage.exists());
        assert_root_intact(&root);
        assert!(fingerprint(&root) == *journal.old());
    }
}

#[test]
fn restore_old_reintenta_limpieza_parcial_del_stage_tras_reinicio() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Installed);
    let backup = backup_path(&lock);
    let stage = stage_path(&lock);
    fs::rename(&root, &backup).unwrap();
    fs::rename(root.with_file_name("new-fixture"), &stage).unwrap();
    let journal_bytes = fs::read(&canonical).unwrap();

    let error = ejecutar_fila_journal_restore_old_con_limpieza(
        &mut lock,
        |_| {},
        |_| {},
        |stage_anclado| {
            let mut eliminaciones = 0;
            stage_anclado.limpiar_con(|parent, componente, es_directorio| {
                let opciones = OpenOptionsAt::default();
                let resultado = if es_directorio {
                    opciones.rmdir_at(parent, Path::new(componente))
                } else {
                    opciones.unlink_at(parent, Path::new(componente))
                };
                resultado.map_err(|_| ErrorDirectorioAnclado::Io)?;
                eliminaciones += 1;
                if eliminaciones == 1 {
                    Err(ErrorDirectorioAnclado::Io)
                } else {
                    Ok(())
                }
            })
        },
    )
    .unwrap_err();

    assert_eq!(error.codigo(), "stage_cleanup_io");
    assert!(stage.exists());
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert!(fingerprint(&stage).identity() == journal.new_fingerprint().identity());
    assert!(!(fingerprint(&stage) == *journal.new_fingerprint()));
    assert_root_intact(&backup);
    assert!(!root.exists());
    drop(error);
    drop(lock);

    let mut restarted = LockEscritura::adquirir(&root).unwrap();
    assert_eq!(
        ejecutar_fila_journal_restore_old(&mut restarted),
        Ok(ResultadoFilaJournalRestoreOld::Eliminado)
    );
    assert!(!stage.exists());
    assert!(!backup.exists());
    assert!(!canonical.exists());
    assert_root_intact(&root);
    assert!(fingerprint(&root) == *journal.old());
}

#[test]
fn restore_old_reconcilia_error_de_limpieza_con_stage_ya_eliminado() {
    let (_temp, root, mut lock, canonical, _journal) = preparar_fila(JournalState::BackupReady);
    let backup = backup_path(&lock);
    let stage = stage_path(&lock);
    let stage_name = stage.file_name().unwrap().to_os_string();
    fs::rename(&root, &backup).unwrap();
    fs::rename(root.with_file_name("new-fixture"), &stage).unwrap();

    assert_eq!(
        ejecutar_fila_journal_restore_old_con_limpieza(
            &mut lock,
            |_| {},
            |_| {},
            |stage_anclado| {
                stage_anclado.limpiar_con(|parent, componente, es_directorio| {
                    let opciones = OpenOptionsAt::default();
                    let resultado = if es_directorio {
                        opciones.rmdir_at(parent, Path::new(componente))
                    } else {
                        opciones.unlink_at(parent, Path::new(componente))
                    };
                    resultado.map_err(|_| ErrorDirectorioAnclado::Io)?;
                    if es_directorio && componente == stage_name {
                        Err(ErrorDirectorioAnclado::Io)
                    } else {
                        Ok(())
                    }
                })
            },
        ),
        Ok(ResultadoFilaJournalRestoreOld::Eliminado)
    );
    assert!(!stage.exists());
    assert!(!backup.exists());
    assert!(!canonical.exists());
    assert_root_intact(&root);
}

#[test]
fn discard_new_restaura_old_en_filas_admitidas_y_retirando_journal_al_final() {
    for (state, validated) in [
        (JournalState::BackupReady, false),
        (JournalState::BackupReady, true),
        (JournalState::Installed, false),
    ] {
        let (_temp, root, mut lock, canonical, journal) = preparar_discard_new(state);
        let backup = backup_path(&lock);
        let stage = stage_path(&lock);
        let canary = root.with_file_name("keep-canary.bin");
        fs::write(&canary, b"untouched").unwrap();
        let parent = canonical.parent().unwrap();
        let lockfile = lock.ubicacion().lock().as_path().to_path_buf();
        let antes = inventario_parent_para_assertions(parent, &canonical, &lockfile);
        let raiz_esperada = root.file_name().unwrap().to_string_lossy().into_owned();
        let backup_esperado = backup.file_name().unwrap().to_string_lossy().into_owned();
        let journal_esperado = canonical
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut esperado = antes
            .into_iter()
            .filter(|(nombre, _)| {
                nombre != &raiz_esperada
                    && !nombre.starts_with(&format!("{raiz_esperada}/"))
                    && nombre != &backup_esperado
                    && !nombre.starts_with(&format!("{backup_esperado}/"))
                    && nombre != &journal_esperado
            })
            .collect::<Vec<_>>();
        esperado.extend([
            (raiz_esperada.clone(), EntradaParentAssertion::Directorio),
            (
                format!("{raiz_esperada}/data.bin"),
                EntradaParentAssertion::Archivo {
                    longitud: b"old-tree".len() as u64,
                    bytes: Some(b"old-tree".to_vec()),
                },
            ),
            (
                format!("{raiz_esperada}/empty-dir"),
                EntradaParentAssertion::Directorio,
            ),
        ]);
        esperado.sort_by(|izquierda, derecha| izquierda.0.cmp(&derecha.0));
        let antes_de_renombrar_visto = Cell::new(false);
        let antes_de_eliminar_visto = Cell::new(false);

        assert_eq!(
            ejecutar_recovery_soportado_con(
                &mut lock,
                |lock, journal| {
                    let mut inventario = observar_inventario_recovery(lock, journal)?;
                    inventario.root_new_validated = validated;
                    Ok(inventario)
                },
                |_| {
                    antes_de_renombrar_visto.set(true);
                    assert!(fingerprint(&root) == *journal.new_fingerprint());
                    assert!(fingerprint(&backup) == *journal.old());
                    assert!(!stage.exists());
                    assert!(canonical.exists());
                },
                |_| {
                    antes_de_eliminar_visto.set(true);
                    assert!(fingerprint(&root) == *journal.old());
                    assert!(!backup.exists());
                    assert!(!stage.exists());
                    assert!(canonical.exists());
                },
            ),
            Ok(ResultadoRecoverySoportado::DiscardNewRestoreOld(
                ResultadoFilaJournalDiscardNewRestoreOld::Eliminado
            )),
            "fase {state:?}, validado={validated}"
        );

        assert!(antes_de_renombrar_visto.get());
        assert!(antes_de_eliminar_visto.get());
        assert!(!canonical.exists());
        assert!(!backup.exists());
        assert!(!stage.exists());
        assert_eq!(fs::read(&canary).unwrap(), b"untouched");
        assert_root_intact(&root);
        assert_eq!(
            inventario_parent_para_assertions(parent, &canonical, &lockfile),
            esperado
        );
    }
}

#[test]
fn discard_new_converge_si_el_rename_reporta_error_despues_de_mover_root() {
    let (_temp, root, mut lock, canonical, journal) =
        preparar_discard_new(JournalState::BackupReady);
    let backup = backup_path(&lock);
    let stage = stage_path(&lock);
    let contexto = match preflight_recovery(&mut lock).unwrap() {
        ResultadoPreflightRecovery::Contexto(contexto) => *contexto,
        _ => unreachable!("el journal preparado debe producir contexto"),
    };

    assert_eq!(
        ejecutar_fila_journal_discard_new_restore_old_desde_contexto_con_renombrador(
            &mut lock,
            contexto,
            |_| {},
            |_| {
                assert!(fingerprint(&root) == *journal.old());
                assert!(!backup.exists());
                assert!(!stage.exists());
                assert!(canonical.exists());
            },
            |lock, journal| {
                renombrar_root_new_a_stage(lock, journal)?;
                Err(ErrorRenombrarAnclado::Io)
            },
        ),
        Ok(ResultadoFilaJournalDiscardNewRestoreOld::Eliminado)
    );
    assert_root_intact(&root);
    assert!(!canonical.exists());
}

#[test]
fn restore_old_no_reconcilia_scope_no_verificado_y_retry_reclasifica() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::BackupReady);
    let backup = backup_path(&lock);
    fs::rename(&root, &backup).unwrap();
    let contexto = match preflight_recovery(&mut lock).unwrap() {
        ResultadoPreflightRecovery::Contexto(contexto) => *contexto,
        _ => unreachable!("el journal debe producir contexto"),
    };
    let observacion_previa = contexto.inventario;
    let journal_bytes = fs::read(&canonical).unwrap();
    let antes_de_eliminar = Cell::new(false);

    assert_eq!(
        restaurar_backup_y_retirar_journal_con_renombrador(
            &mut lock,
            &contexto,
            observacion_previa,
            |_| antes_de_eliminar.set(true),
            |_, _, _| {
                fs::rename(&backup, &root).unwrap();
                Ok(Err(ErrorRenombrarAnclado::ResultadoNoVerificado))
            },
        ),
        Err(ErrorFilaJournalRestoreOld::Renombrar(
            ErrorRenombrarAnclado::ResultadoNoVerificado
        ))
    );
    assert!(!antes_de_eliminar.get());
    assert!(canonical.exists());
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert!(fingerprint(&root) == *journal.old());
    assert!(!backup.exists());

    assert_eq!(
        ejecutar_recovery_soportado_con(&mut lock, observar_inventario_recovery, |_| {}, |_| {}),
        Ok(ResultadoRecoverySoportado::KeepOld(
            ResultadoFilaJournalOld::Eliminado
        ))
    );
    assert!(!canonical.exists());
}

#[test]
fn discard_new_no_reconcilia_outcome_no_verificado_y_retry_restore_old() {
    let (_temp, root, mut lock, canonical, _journal) =
        preparar_discard_new(JournalState::BackupReady);
    let backup = backup_path(&lock);
    let stage = stage_path(&lock);
    let contexto = match preflight_recovery(&mut lock).unwrap() {
        ResultadoPreflightRecovery::Contexto(contexto) => *contexto,
        _ => unreachable!("el journal debe producir contexto"),
    };
    let journal_bytes = fs::read(&canonical).unwrap();
    let antes_de_eliminar = Cell::new(false);

    assert_eq!(
        ejecutar_fila_journal_discard_new_restore_old_desde_contexto_con_renombrador(
            &mut lock,
            contexto,
            |_| {},
            |_| antes_de_eliminar.set(true),
            |lock, journal| {
                renombrar_root_new_a_stage(lock, journal)?;
                Err(ErrorRenombrarAnclado::ResultadoNoVerificado)
            },
        ),
        Err(ErrorFilaJournalRestoreOld::Renombrar(
            ErrorRenombrarAnclado::ResultadoNoVerificado
        ))
    );
    assert!(!antes_de_eliminar.get());
    assert!(canonical.exists());
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert!(!root.exists());
    assert!(backup.is_dir() && stage.is_dir());

    assert_eq!(
        ejecutar_recovery_soportado_con(&mut lock, observar_inventario_recovery, |_| {}, |_| {}),
        Ok(ResultadoRecoverySoportado::RestoreOld(
            ResultadoFilaJournalRestoreOld::Eliminado
        ))
    );
    assert!(!canonical.exists());
    assert!(!backup.exists() && !stage.exists());
    assert_root_intact(&root);
}

#[test]
fn reinicio_despues_de_mover_root_a_stage_reanuda_por_restore_old() {
    let (_temp, root, mut lock, canonical, journal) = preparar_discard_new(JournalState::Installed);
    let backup = backup_path(&lock);
    let stage = stage_path(&lock);
    renombrar_root_new_a_stage(&mut lock, &journal).unwrap();
    assert!(!root.exists());
    assert!(fingerprint(&stage) == *journal.new_fingerprint());
    assert!(fingerprint(&backup) == *journal.old());
    drop(lock);

    let mut restarted = LockEscritura::adquirir(&root).unwrap();
    assert_eq!(
        ejecutar_dispatcher_contando_observaciones(&mut restarted, &Cell::new(0)),
        Ok(ResultadoRecoverySoportado::RestoreOld(
            ResultadoFilaJournalRestoreOld::Eliminado
        ))
    );
    assert!(!stage.exists());
    assert!(!backup.exists());
    assert!(!canonical.exists());
    assert_root_intact(&root);
}

#[test]
fn discard_new_conserva_canarios_si_hay_cualquier_stage_junto_a_root_new() {
    let (_temp, root, mut lock, canonical, journal) =
        preparar_discard_new(JournalState::BackupReady);
    let backup = backup_path(&lock);
    let stage = stage_path(&lock);
    crear_arbol(&stage, b"stage-canary");
    let journal_bytes = fs::read(&canonical).unwrap();
    let parent = canonical.parent().unwrap();
    let lockfile = lock.ubicacion().lock().as_path().to_path_buf();
    let antes = inventario_parent_para_assertions(parent, &canonical, &lockfile);

    assert_eq!(
        ejecutar_dispatcher_contando_observaciones(&mut lock, &Cell::new(0)),
        Err(ErrorRecoverySoportado::FilaNoSoportada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert!(fingerprint(&root) == *journal.new_fingerprint());
    assert!(fingerprint(&backup) == *journal.old());
    assert_eq!(fs::read(stage.join("data.bin")).unwrap(), b"stage-canary");
    assert_eq!(
        inventario_parent_para_assertions(parent, &canonical, &lockfile),
        antes
    );
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
    let stage = stage_path(&lock);
    crear_arbol(&stage, b"new-tree");
    assert!(
        fingerprint(&stage).identity() != journal.new_fingerprint().identity(),
        "el basename esperado contiene un árbol con identidad distinta"
    );
    let parent = canonical.parent().unwrap();
    let lockfile = lock.ubicacion().lock().as_path().to_path_buf();
    let canary = parent.join("keep-canary.bin");
    fs::write(&canary, b"untouched").unwrap();
    let bytes = fs::read(&canonical).unwrap();
    let inventario_antes = inventario_parent_para_assertions(parent, &canonical, &lockfile);

    let raiz_anclada = DirectorioAnclado::desde_ubicacion(lock.ubicacion()).unwrap();
    assert!(
        raiz_anclada
            .reabrir_stage_sibling(TXN, journal.parent(), journal.new_fingerprint().identity(),)
            .unwrap_err()
            == ErrorDirectorioAnclado::Io,
        "recovery no debe reabrir el basename esperado con otra identidad"
    );

    assert_eq!(
        ejecutar_fila_journal_old(&mut lock),
        Err(ErrorFilaJournalOld::FilaNoAutorizada)
    );
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert_eq!(fs::read(stage.join("data.bin")).unwrap(), b"new-tree");
    assert_eq!(fs::read(&canary).unwrap(), b"untouched");
    assert_eq!(Journal::parse(&bytes).unwrap(), journal);
    assert_root_intact(&root);
    assert_eq!(
        inventario_parent_para_assertions(parent, &canonical, &lockfile),
        inventario_antes
    );
}

#[test]
fn cleanup_ambiguo_reobserva_y_retira_journal_solo_despues_del_stage() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::BackupReady);
    let backup = backup_path(&lock);
    let stage = stage_path(&lock);
    fs::rename(root.parent().unwrap().join("new-fixture"), &stage).unwrap();
    let stage_name = stage.file_name().unwrap().to_os_string();
    let stage_name_text = stage_name.to_string_lossy().into_owned();
    let parent = canonical.parent().unwrap();
    let lockfile = lock.ubicacion().lock().as_path().to_path_buf();
    let journal_name = canonical
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let canary = parent.join("keep-canary.bin");
    fs::write(&canary, b"untouched").unwrap();
    let journal_bytes = fs::read(&canonical).unwrap();
    let fingerprint_old = journal.old().clone();
    let mut inventario_esperado = inventario_parent_para_assertions(parent, &canonical, &lockfile);
    inventario_esperado.retain(|(name, _)| {
        name != &stage_name_text
            && !name.starts_with(&format!("{stage_name_text}/"))
            && name != &journal_name
    });
    let stage_eliminado_antes_del_error = Cell::new(false);
    let pre_delete_observado = Cell::new(false);

    assert_eq!(
        ejecutar_fila_journal_old_con_limpieza(
            &mut lock,
            |_| {
                pre_delete_observado.set(true);
                assert!(!stage.exists());
                assert!(canonical.exists());
                assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
                assert!(!backup.exists());
                assert!(fingerprint(&root) == fingerprint_old);
            },
            |stage| {
                stage.limpiar_con(|parent, componente, es_directorio| {
                    let opciones = OpenOptionsAt::default();
                    let resultado = if es_directorio {
                        opciones.rmdir_at(parent, Path::new(componente))
                    } else {
                        opciones.unlink_at(parent, Path::new(componente))
                    };
                    resultado.map_err(|_| ErrorDirectorioAnclado::Io)?;
                    if es_directorio && componente == stage_name {
                        stage_eliminado_antes_del_error.set(true);
                        Err(ErrorDirectorioAnclado::Io)
                    } else {
                        Ok(())
                    }
                })
            },
        ),
        Ok(ResultadoFilaJournalOld::Eliminado)
    );

    assert!(stage_eliminado_antes_del_error.get());
    assert!(pre_delete_observado.get());
    assert!(!stage.exists());
    assert!(!backup.exists());
    assert!(!canonical.exists());
    assert_eq!(fs::read(&canary).unwrap(), b"untouched");
    assert!(fingerprint(&root) == fingerprint_old);
    assert_root_intact(&root);
    assert_eq!(
        inventario_parent_para_assertions(parent, &canonical, &lockfile),
        inventario_esperado
    );
}

#[test]
fn txn_invalido_falla_antes_de_mutar_siblings_y_journal() {
    let (_temp, root, lock, canonical, journal) = preparar_fila(JournalState::Prepared);
    let stage = stage_path(&lock);
    fs::rename(root.parent().unwrap().join("new-fixture"), &stage).unwrap();
    let parent = canonical.parent().unwrap();
    let lockfile = lock.ubicacion().lock().as_path().to_path_buf();
    let canary = parent.join("keep-canary.bin");
    fs::write(&canary, b"untouched").unwrap();
    let journal_bytes = fs::read(&canonical).unwrap();
    let inventario_antes = inventario_parent_para_assertions(parent, &canonical, &lockfile);
    let raiz_anclada = DirectorioAnclado::desde_ubicacion(lock.ubicacion()).unwrap();

    assert_eq!(
        raiz_anclada
            .reabrir_stage_sibling(
                "../invalid",
                journal.parent(),
                journal.new_fingerprint().identity(),
            )
            .unwrap_err(),
        ErrorDirectorioAnclado::ComponenteInvalido
    );
    assert_eq!(fs::read(&canonical).unwrap(), journal_bytes);
    assert_eq!(fs::read(stage.join("data.bin")).unwrap(), b"new-tree");
    assert_eq!(fs::read(&canary).unwrap(), b"untouched");
    assert_root_intact(&root);
    assert_eq!(
        inventario_parent_para_assertions(parent, &canonical, &lockfile),
        inventario_antes
    );
}

#[test]
fn keep_old_reintenta_limpieza_parcial_del_stage_tras_reinicio() {
    let (_temp, root, mut lock, canonical, journal) = preparar_fila(JournalState::Installed);
    let stage = stage_path(&lock);
    fs::rename(root.parent().unwrap().join("new-fixture"), &stage).unwrap();
    let mut eliminaciones = 0;
    let error = ejecutar_fila_journal_old_con_limpieza(
        &mut lock,
        |_| {},
        |stage| {
            stage.limpiar_con(|parent, componente, es_directorio| {
                if eliminaciones == 1 {
                    return Err(ErrorDirectorioAnclado::Io);
                }
                eliminaciones += 1;
                let opciones = OpenOptionsAt::default();
                let resultado = if es_directorio {
                    opciones.rmdir_at(parent, Path::new(componente))
                } else {
                    opciones.unlink_at(parent, Path::new(componente))
                };
                resultado.map_err(|_| ErrorDirectorioAnclado::Io)
            })
        },
    )
    .unwrap_err();
    assert_eq!(error.codigo(), "stage_cleanup_io");
    assert!(!stage.join("data.bin").exists());
    assert!(canonical.exists());
    assert_root_intact(&root);
    drop(error);
    drop(lock);

    let mut restarted = LockEscritura::adquirir(&root).unwrap();
    assert_eq!(
        ejecutar_fila_journal_old(&mut restarted),
        Ok(ResultadoFilaJournalOld::Eliminado)
    );
    assert!(!stage.exists());
    assert!(!canonical.exists());
    assert_root_intact(&root);
    assert!(fingerprint(&root) == *journal.old());
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
