use super::super::preparar_apply::{
    ResultadoPreparacionApply,
    tests::{config, delta, fixture},
};
use super::*;
use crate::transaccion::{captura::capturar_snapshot_soportado_anclado, lock::LockEscritura};
use std::fs;
fn preparar<'owner, 'lock>(
    owner: &'owner mut DuenioTransaccionAnclado<'lock>,
) -> PreparacionPublicada<'owner, 'lock> {
    let snapshot = capturar_snapshot_soportado_anclado(
        &owner.abrir_root_lectura().unwrap().unwrap(),
        config(),
    )
    .unwrap();
    let ResultadoPreparacionApply::Preparada(prepared) = owner
        .preparar_apply(config(), snapshot.token(), &delta())
        .unwrap()
    else {
        panic!("rejected")
    };
    prepared.publicar_prepared().unwrap()
}
#[test]
fn instala_new_y_confirma_hasta_cleanup_total() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (dir, location) = fixture();
    let old = fs::read(location.ruta().join("n/node.md")).unwrap();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let instalada = preparar(&mut owner).instalar().unwrap();
    assert_eq!(instalada.estado.journal.state(), JournalState::Installed);
    assert_eq!(
        fs::read(location.backup().as_path().join("n/node.md")).unwrap(),
        old
    );
    assert_ne!(fs::read(location.ruta().join("n/node.md")).unwrap(), old);
    assert_eq!(
        fs::read(location.ruta().join("payload.bin")).unwrap(),
        b"raw auxiliary"
    );
    instalada.finalizar().unwrap();
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}
#[test]
fn journal_sustituido_y_backup_ocupado_impiden_primer_rename() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for case in [false, true] {
        let (dir, location) = fixture();
        let before = fs::read(location.ruta().join("n/node.md")).unwrap();
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let prepared = preparar(&mut owner);
        if case {
            fs::create_dir(location.backup().as_path()).unwrap();
            fs::write(location.backup().as_path().join("canary"), b"untouched").unwrap();
        } else {
            let path = location.journal().as_path().to_owned();
            let bytes = fs::read(&path).unwrap();
            fs::rename(&path, dir.path().join("held")).unwrap();
            fs::write(path, bytes).unwrap();
        }
        assert!(prepared.instalar().is_err());
        assert_eq!(fs::read(location.ruta().join("n/node.md")).unwrap(), before);
        if case {
            assert_eq!(
                fs::read(location.backup().as_path().join("canary")).unwrap(),
                b"untouched"
            );
        }
    }
}
#[test]
fn cambios_postrename_retienen_evidencia_sin_reintento() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for cut in [1, 2, 3, 4] {
        let (_dir, location) = fixture();
        let old = fs::read(location.ruta().join("n/node.md")).unwrap();
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let prepared = preparar(&mut owner);
        let stagepath = location
            .ruta()
            .parent()
            .unwrap()
            .join(prepared.estado.stage.componente());
        let error = match prepared.instalar_con_hook(|step| {
            if step == cut {
                let target = if cut <= 2 {
                    stagepath.clone()
                } else {
                    location.ruta().to_owned()
                };
                fs::write(target.join("payload.bin"), b"changed").unwrap();
            }
        }) {
            Err(e) => e,
            Ok(_) => panic!("accepted changedtree"),
        };
        assert_eq!(
            fs::read(location.backup().as_path().join("n/node.md")).unwrap(),
            old
        );
        assert!(location.journal().as_path().exists());
        assert!(error.estado.contexto_publicado.is_some());
        assert_eq!(location.ruta().exists(), cut >= 3);
    }
}

#[test]
#[ignore = "invocado por padre para cortar instalación entre rename y journal"]
fn instalar_child_writer_o_recovery() {
    let Some(root) = std::env::var_os("FELIX_INSTALL_CHILD_ROOT").map(std::path::PathBuf::from)
    else {
        return;
    };
    assert_eq!(
        root.parent().unwrap().file_name().unwrap(),
        std::env::var_os("FELIX_INSTALL_CHILD_NONCE").unwrap()
    );
    let cut: u8 = std::env::var("FELIX_INSTALL_CHILD_CUT")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=4).contains(&cut));
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    if std::env::var_os("FELIX_INSTALL_CHILD_RECOVERY").is_some() {
        let journal = crate::transaccion::journal::v2::JournalV2::parse(
            &fs::read(owner._lock.ubicacion().journal().as_path()).unwrap(),
        )
        .unwrap();
        owner.recuperar_pendiente_v2().unwrap();
        let final_state = owner.observar_siblings_recovery_v2_raw(&journal).unwrap();
        assert!(final_state.backup().is_none() && final_state.stage().is_none());
        assert!(
            final_state.root()
                == Some(if cut == 4 {
                    journal.new_fingerprint()
                } else {
                    journal.old()
                })
        );
        assert!(!owner._lock.ubicacion().journal().as_path().exists());
        return;
    }
    let result = preparar(&mut owner).instalar_con_hook(|step| {
        if step == cut {
            std::process::exit(100 + i32::from(cut))
        }
    });
    assert!(result.is_err(), "child no cortó");
}
#[test]
fn cortes_writer_se_recuperan_desde_disco_en_otro_proceso() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for cut in 1..=4 {
        let (dir, location) = fixture();
        let old = fs::read(location.ruta().join("n/node.md")).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--test-threads=1",
                "instalar_child_writer_o_recovery",
            ])
            .env("FELIX_INSTALL_CHILD_ROOT", location.ruta())
            .env("FELIX_INSTALL_CHILD_NONCE", dir.path().file_name().unwrap())
            .env("FELIX_INSTALL_CHILD_CUT", cut.to_string())
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
                panic!("timeout")
            };
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(100 + cut), "{output:?}");
        let journal = crate::transaccion::journal::v2::JournalV2::parse(
            &fs::read(location.journal().as_path()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            journal.state(),
            match cut {
                1 => JournalState::Prepared,
                2 | 3 => JournalState::BackupReady,
                _ => JournalState::Installed,
            }
        );
        assert_eq!(
            fs::read(location.backup().as_path().join("n/node.md")).unwrap(),
            old
        );
        assert_eq!(location.ruta().exists(), cut >= 3);
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let observed = owner.observar_siblings_recovery_v2_raw(&journal).unwrap();
        assert!(observed.backup() == Some(journal.old()));
        if cut <= 2 {
            assert!(observed.stage() == Some(journal.new_fingerprint()));
            assert!(observed.root().is_none());
        } else {
            assert!(observed.stage().is_none());
            assert!(observed.root() == Some(journal.new_fingerprint()));
        }
        drop(owner);
        drop(lock);
        let mut recovery = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--test-threads=1",
                "instalar_child_writer_o_recovery",
            ])
            .env("FELIX_INSTALL_CHILD_ROOT", location.ruta())
            .env("FELIX_INSTALL_CHILD_NONCE", dir.path().file_name().unwrap())
            .env("FELIX_INSTALL_CHILD_CUT", cut.to_string())
            .env("FELIX_INSTALL_CHILD_RECOVERY", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if recovery.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() > deadline {
                recovery.kill().unwrap();
                panic!("recovery timeout")
            };
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = recovery.wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        if cut < 4 {
            assert_eq!(fs::read(location.ruta().join("n/node.md")).unwrap(), old);
        } else {
            assert_ne!(fs::read(location.ruta().join("n/node.md")).unwrap(), old);
        }
    }
}
