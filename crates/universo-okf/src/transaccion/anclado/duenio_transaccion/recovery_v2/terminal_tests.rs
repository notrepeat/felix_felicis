use super::*;
use crate::transaccion::anclado::duenio_transaccion::preparar_apply::{
    ResultadoPreparacionApply,
    tests::{config, delta, fixture},
};
use crate::transaccion::{captura::capturar_snapshot_soportado_anclado, lock::LockEscritura};
use std::fs;

fn preparar(owner: &mut DuenioTransaccionAnclado<'_>) -> JournalV2 {
    preparar_con_stage(owner, false)
}
fn preparar_con_stage(owner: &mut DuenioTransaccionAnclado<'_>, keep_stage: bool) -> JournalV2 {
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
        panic!("reject");
    };
    let stage = location
        .ruta()
        .parent()
        .unwrap()
        .join(prepared.stage.componente());
    let published = prepared.publicar_prepared().unwrap();
    let journal = published.estado.journal.clone();
    drop(published);
    if !keep_stage {
        fs::remove_dir_all(stage).unwrap();
    } // Fixture: corte posterior al cleanup Stage de rollback.
    journal
}
#[test]
fn terminal_retira_contexto_antes_journal_y_preserva_old() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (_dir, location) = fixture();
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = preparar(&mut owner);
    owner.retirar_terminal_rollback_v2_con(|| {
        assert!(location.journal().as_path().exists());
        assert!(!crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::contexto_path(&location).exists());
    }).unwrap();
    owner.validar_terminal_vacio(&journal).unwrap();
}
#[test]
fn journal_only_rechaza_root_cambiado_y_contexto_desconocido() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for context_unknown in [false, true] {
        let (_dir, location) = fixture();
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        preparar(&mut owner);
        let context =
            crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::contexto_path(
                &location,
            );
        if context_unknown {
            fs::write(&context, b"unknown").unwrap();
        } else {
            fs::remove_file(&context).unwrap();
            fs::write(location.ruta().join("canary"), b"preserve").unwrap();
        }
        let journal = fs::read(location.journal().as_path()).unwrap();
        assert!(owner.retirar_terminal_rollback_v2().is_err());
        assert_eq!(fs::read(location.journal().as_path()).unwrap(), journal);
        if context_unknown {
            assert_eq!(fs::read(context).unwrap(), b"unknown");
        }
    }
}
#[test]
#[ignore = "child process terminates at Context removal"]
fn proceso_corte_terminal() {
    let root = std::env::var_os("FELIX_TEST_ROLLBACK_TERMINAL_ROOT").unwrap();
    let mut lock = LockEscritura::adquirir(std::path::Path::new(&root)).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    owner
        .retirar_terminal_rollback_v2_con(|| std::process::exit(93))
        .unwrap();
    panic!("hook not reached");
}
#[test]
fn corte_proceso_contexto_reabre_journal_only_y_finaliza() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (_dir, location) = fixture();
    {
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        preparar(&mut owner);
    }
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "transaccion::anclado::duenio_transaccion::recovery_v2::terminal::tests::proceso_corte_terminal", "--ignored", "--nocapture"])
        .env("FELIX_TEST_ROLLBACK_TERMINAL_ROOT", location.ruta()).output().unwrap();
    assert_eq!(
        result.status.code(),
        Some(93),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(location.journal().as_path().exists());
    assert!(
        !crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::contexto_path(
            &location
        )
        .exists()
    );
    let mut lock = LockEscritura::adquirir_en(&location).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    owner.retirar_terminal_rollback_v2().unwrap();
    assert!(!location.journal().as_path().exists());
}

#[test]
fn rollback_prepared_limpia_stage_completo_o_parcial_y_finaliza_old() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for partial in [false, true] {
        let (_dir, location) = fixture();
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let journal = preparar_con_stage(&mut owner, true);
        let stage = location.stage(journal.transaction_id()).unwrap();
        if partial {
            fs::remove_file(stage.as_path().join("n/node.md")).unwrap();
        }
        owner.reanudar_rollback_v2().unwrap();
        owner.validar_terminal_vacio(&journal).unwrap();
        assert!(!stage.as_path().exists());
    }
}
#[test]
fn terminal_detecta_reemplazo_mismos_bytes_journal_o_root_alterado_tras_contexto() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    for replace_journal in [false, true] {
        let (_dir, location) = fixture();
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        preparar(&mut owner);
        let bytes = fs::read(location.journal().as_path()).unwrap();
        let error = owner
            .retirar_terminal_rollback_v2_con(|| {
                if replace_journal {
                    let old = location
                        .ruta()
                        .parent()
                        .unwrap()
                        .join("retained-old-journal");
                    fs::rename(location.journal().as_path(), old).unwrap();
                    fs::write(location.journal().as_path(), &bytes).unwrap();
                } else {
                    fs::write(location.ruta().join("foreign-canary"), b"preserve").unwrap();
                }
            })
            .unwrap_err();
        assert!(error.evidencia.as_ref().unwrap().contexto.is_some());
        assert_eq!(fs::read(location.journal().as_path()).unwrap(), bytes);
    }
}
