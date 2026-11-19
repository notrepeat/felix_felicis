use super::super::preparar_apply::{
    ResultadoPreparacionApply,
    tests::{config, delta, fixture},
};
use super::*;
use crate::transaccion::{captura::capturar_snapshot_soportado_anclado, lock::LockEscritura};
use std::fs;
fn preparar<'owner, 'lock>(
    owner: &'owner mut DuenioTransaccionAnclado<'lock>,
) -> Box<PreparacionApply<'owner, 'lock>> {
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
    prepared
}
#[test]
fn publica_pareja_prepared_sin_renombrar_root_y_conserva_stage() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (_dir, location) = fixture();
    let before = fs::read(location.ruta().join("n/node.md")).unwrap();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let mut publicada = preparar(&mut owner).publicar_prepared().unwrap();
    publicada.estado.validar_artefactos().unwrap();
    publicada.estado.validar_slots(true, true, false).unwrap();
    assert_eq!(fs::read(location.ruta().join("n/node.md")).unwrap(), before);
    assert!(!location.backup().as_path().exists());
    assert_eq!(
        JournalV2::parse(&fs::read(location.journal().as_path()).unwrap())
            .unwrap()
            .state(),
        JournalState::Prepared
    );
}
#[test]
fn cambios_entre_publicaciones_conservan_stage_y_sidecars_sin_rename_root() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for cut in [1, 2, 3] {
        let (_dir, location) = fixture();
        let before = fs::read(location.ruta().join("n/node.md")).unwrap();
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let error = match preparar(&mut owner).publicar_prepared_con_hook(|step| {
            if step == cut {
                fs::write(location.ruta().join("payload.bin"), b"changed").unwrap();
            }
        }) {
            Err(e) => e,
            Ok(_) => panic!("accepted changedroot"),
        };
        assert_eq!(fs::read(location.ruta().join("n/node.md")).unwrap(), before);
        assert!(
            error.estado.stage.fingerprint_raw().unwrap()
                == *error.estado.journal.new_fingerprint()
        );
        assert!(super::super::fixture_root_replay::contexto_path(&location).exists());
        assert_eq!(location.journal().as_path().exists(), cut == 3);
        assert!(!location.backup().as_path().exists());
    }
}
#[test]
fn temporal_reemplazado_no_se_publica_ni_se_limpia() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (_dir, location) = fixture();
    let temporal = location
        .ruta()
        .parent()
        .unwrap()
        .join(location.nombre_journal_temporal());
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let result = preparar(&mut owner).publicar_prepared_con_hook(|step| {
        if step == 2 {
            fs::write(&temporal, b"canary").unwrap();
        }
    });
    assert!(result.is_err());
    assert_eq!(fs::read(temporal).unwrap(), b"canary");
    assert!(!location.journal().as_path().exists());
}

#[test]
#[ignore = "invocado por el padre para cortar publicación Prepared"]
fn prepared_child_corta_publicacion() {
    let Some(root) = std::env::var_os("FELIX_PREPARED_CHILD_ROOT").map(std::path::PathBuf::from)
    else {
        return;
    };
    assert_eq!(
        root.parent().unwrap().file_name().unwrap(),
        std::env::var_os("FELIX_PREPARED_CHILD_NONCE").unwrap()
    );
    let cut: u8 = std::env::var("FELIX_PREPARED_CHILD_CUT")
        .unwrap()
        .parse()
        .unwrap();
    assert!(cut <= 2);
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let result = preparar(&mut owner).publicar_prepared_con_hook(|step| {
        if step == cut {
            std::process::exit(90 + i32::from(cut));
        }
    });
    assert!(result.is_err(), "child no cortó publicación");
}

#[test]
fn cortes_iniciales_hijo_conservan_root_y_bloquean_otra_preparacion() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for cut in 0..=2 {
        let (dir, location) = fixture();
        let root = fs::read(location.ruta().join("n/node.md")).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--test-threads=1",
                "prepared_child_corta_publicacion",
            ])
            .env("FELIX_PREPARED_CHILD_ROOT", location.ruta())
            .env(
                "FELIX_PREPARED_CHILD_NONCE",
                dir.path().file_name().unwrap(),
            )
            .env("FELIX_PREPARED_CHILD_CUT", cut.to_string())
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
        assert_eq!(output.status.code(), Some(90 + cut), "{output:?}");
        assert_eq!(fs::read(location.ruta().join("n/node.md")).unwrap(), root);
        assert!(!location.journal().as_path().exists());
        assert!(!location.backup().as_path().exists());
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let snapshot = capturar_snapshot_soportado_anclado(
            &owner.abrir_root_lectura().unwrap().unwrap(),
            config(),
        )
        .unwrap();
        let before = fs::read_dir(dir.path()).unwrap().count();
        assert!(
            owner
                .preparar_apply(config(), snapshot.token(), &delta())
                .is_err()
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), before);
        let slots = owner.observar_siblings_nuevo_flujo().unwrap();
        assert_eq!(
            slots.contexto(),
            if cut == 0 {
                Presencia::Ausente
            } else {
                Presencia::Presente
            }
        );
        assert_eq!(
            slots.contexto_temporal(),
            if cut == 0 {
                Presencia::Presente
            } else {
                Presencia::Ausente
            }
        );
        assert_eq!(
            slots.journal_temporal(),
            if cut == 2 {
                Presencia::Presente
            } else {
                Presencia::Ausente
            }
        );
    }
}

#[test]
fn canonicos_reemplazados_por_mismos_bytes_no_confirman_prepared() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for context in [false, true] {
        let (dir, location) = fixture();
        let path = if context {
            super::super::fixture_root_replay::contexto_path(&location)
        } else {
            location.journal().as_path().to_owned()
        };
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let result = preparar(&mut owner).publicar_prepared_con_hook(|step| {
            if step == 3 {
                let bytes = fs::read(&path).unwrap();
                fs::rename(&path, dir.path().join("held-original")).unwrap();
                fs::write(&path, bytes).unwrap();
            }
        });
        assert!(result.is_err());
        assert_eq!(
            fs::read(&path).unwrap(),
            fs::read(dir.path().join("held-original")).unwrap()
        );
        assert!(location.journal().as_path().exists());
        assert!(!location.backup().as_path().exists());
    }
}
