use super::super::preparar_apply::{
    ResultadoPreparacionApply,
    tests::{config, delta, fixture},
};
use super::*;
use crate::transaccion::{
    admision::envelope::v2::EnvelopeContextoAdmisionV2,
    captura::capturar_snapshot_soportado_anclado, lock::LockEscritura,
};
use std::fs;
fn instalar(owner: &mut DuenioTransaccionAnclado<'_>) -> JournalV2 {
    let location = owner._lock.ubicacion().clone();
    let snapshot = capturar_snapshot_soportado_anclado(
        &owner.abrir_root_lectura().unwrap().unwrap(),
        config(),
    )
    .unwrap();
    let ResultadoPreparacionApply::Preparada(prepared) = owner
        .preparar_apply(config(), snapshot.token(), &delta())
        .unwrap()
    else {
        panic!("reject")
    };
    let mut journal = prepared.journal.clone();
    let stagepath = location
        .ruta()
        .parent()
        .unwrap()
        .join(prepared.stage.componente());
    fs::rename(location.ruta(), location.backup().as_path()).unwrap();
    fs::rename(stagepath, location.ruta()).unwrap();
    for state in [
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ] {
        journal.advance(state).unwrap();
    }
    let envelope = EnvelopeContextoAdmisionV2::ligar(prepared.contexto, &journal);
    fs::write(location.journal().as_path(), journal.encode().unwrap()).unwrap();
    fs::write(
        super::super::fixture_root_replay::contexto_path(&location),
        envelope.encode().unwrap(),
    )
    .unwrap();
    journal
}
fn eliminar(parent: &File, nombre: &OsStr, directory: bool) -> Result<(), ErrorDirectorioAnclado> {
    let options = OpenOptionsAt::default();
    if directory {
        options.rmdir_at(parent, Path::new(nombre))
    } else {
        options.unlink_at(parent, Path::new(nombre))
    }
    .map_err(|_| ErrorDirectorioAnclado::Io)
}
#[test]
fn committed_limpia_backup_y_preserva_root_context_y_journal() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (_dir, location) = fixture();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = instalar(&mut owner);
    let root = fs::read(location.ruta().join("n/node.md")).unwrap();
    let j = fs::read(location.journal().as_path()).unwrap();
    owner.limpiar_backup_committed_v2(&journal).unwrap();
    assert!(!location.backup().as_path().exists());
    assert_eq!(fs::read(location.ruta().join("n/node.md")).unwrap(), root);
    assert_eq!(fs::read(location.journal().as_path()).unwrap(), j);
    owner.limpiar_backup_committed_v2(&journal).unwrap();
    owner.publicar_intent_v3_desde_committed().unwrap();
}
#[test]
fn fallo_tras_unlink_conserva_capability_y_retry_partial_old() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (_dir, location) = fixture();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = instalar(&mut owner);
    let mut calls = 0;
    let error = owner
        .limpiar_backup_committed_v2_con(
            &journal,
            || {},
            scope_filesystem,
            |p, n, d| {
                calls += 1;
                eliminar(p, n, d)?;
                Err(ErrorDirectorioAnclado::Io)
            },
        )
        .unwrap_err();
    assert_eq!(calls, 1);
    assert!(error.backup.is_some() && error.evidencia.is_some());
    assert!(location.journal().as_path().exists());
    owner.limpiar_backup_committed_v2(&journal).unwrap();
    assert!(!location.backup().as_path().exists());
}
#[test]
fn scope_erroneo_o_root_modificado_no_borra_backup() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for mutate in [false, true] {
        let (_dir, location) = fixture();
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let journal = instalar(&mut owner);
        let before = fs::read(location.backup().as_path().join("n/node.md")).unwrap();
        let mut calls = 0;
        let mut probes = 0;
        assert!(
            owner
                .limpiar_backup_committed_v2_con(
                    &journal,
                    || {
                        if mutate {
                            fs::write(location.ruta().join("payload.bin"), b"changed").unwrap();
                        }
                    },
                    |file| {
                        probes += 1;
                        let actual = scope_filesystem(file)?;
                        if !mutate && probes == 4 {
                            Ok(ScopeFilesystem(actual.0.wrapping_add(1)))
                        } else {
                            Ok(actual)
                        }
                    },
                    |p, n, d| {
                        calls += 1;
                        eliminar(p, n, d)
                    }
                )
                .is_err()
        );
        assert_eq!(calls, 0);
        assert_eq!(
            fs::read(location.backup().as_path().join("n/node.md")).unwrap(),
            before
        );
    }
}

#[test]
fn installed_backup_sustituido_y_contexto_cambiado_se_preservan() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for case in 0..3 {
        let (dir, location) = fixture();
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let mut journal = instalar(&mut owner);
        let backup = location.backup().as_path().to_owned();
        let bytes = fs::read(backup.join("n/node.md")).unwrap();
        if case == 0 {
            journal = JournalV2::new(
                journal.transaction_id(),
                journal.parent().clone(),
                journal.old().clone(),
                journal.new_fingerprint().clone(),
            )
            .unwrap();
            journal.advance(JournalState::BackupReady).unwrap();
            journal.advance(JournalState::Installed).unwrap();
            fs::write(location.journal().as_path(), journal.encode().unwrap()).unwrap();
        }
        if case == 1 {
            fs::rename(&backup, dir.path().join("old-held")).unwrap();
            fs::create_dir_all(backup.join("n")).unwrap();
            fs::write(backup.join("n/node.md"), &bytes).unwrap();
            fs::write(backup.join("payload.bin"), b"raw auxiliary").unwrap();
        }
        let mut calls = 0;
        assert!(
            owner
                .limpiar_backup_committed_v2_con(
                    &journal,
                    || {
                        if case == 2 {
                            fs::write(
                                super::super::fixture_root_replay::contexto_path(&location),
                                b"changed",
                            )
                            .unwrap();
                        }
                    },
                    scope_filesystem,
                    |p, n, d| {
                        calls += 1;
                        eliminar(p, n, d)
                    }
                )
                .is_err()
        );
        assert_eq!(calls, 0);
        assert_eq!(fs::read(backup.join("n/node.md")).unwrap(), bytes);
    }
}

#[test]
#[ignore = "invocado por padre para cortar cleanup Committed tras unlink real"]
fn backup_committed_child_cut() {
    let Some(root) = std::env::var_os("FELIX_BACKUP_V2_CHILD_ROOT").map(std::path::PathBuf::from)
    else {
        return;
    };
    let nonce = std::env::var_os("FELIX_BACKUP_V2_CHILD_NONCE").unwrap();
    assert_eq!(root.parent().unwrap().file_name().unwrap(), nonce);
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let journal =
        JournalV2::parse(&fs::read(lock.ubicacion().journal().as_path()).unwrap()).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let result = owner.limpiar_backup_committed_v2_con(
        &journal,
        || {},
        scope_filesystem,
        |p, n, d| {
            eliminar(p, n, d)?;
            std::process::exit(89)
        },
    );
    panic!("child did not exit: {result:?}");
}

#[test]
fn terminacion_hijo_y_padre_readquiere_cleanup_partial_old_con_evidencia_intacta() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (dir, location) = fixture();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = instalar(&mut owner);
    let j = fs::read(location.journal().as_path()).unwrap();
    let cp = super::super::fixture_root_replay::contexto_path(&location);
    let c = fs::read(&cp).unwrap();
    drop(owner);
    drop(lock);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--test-threads=1",
            "backup_committed_child_cut",
        ])
        .env("FELIX_BACKUP_V2_CHILD_ROOT", location.ruta())
        .env(
            "FELIX_BACKUP_V2_CHILD_NONCE",
            dir.path().file_name().unwrap(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("child timeout")
        };
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(89), "{output:?}");
    assert_eq!(fs::read(location.journal().as_path()).unwrap(), j);
    assert_eq!(fs::read(&cp).unwrap(), c);
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    owner.limpiar_backup_committed_v2(&journal).unwrap();
    assert!(!location.backup().as_path().exists());
    owner.publicar_intent_v3_desde_committed().unwrap();
}
