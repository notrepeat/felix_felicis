use super::super::fixture_root_replay::{contexto_path, verificar_arbol};
use super::super::normal_v2_test_support::preparar_normal_v2;
use super::*;
use crate::transaccion::lock::LockEscritura;
use std::{cell::Cell, fs};

#[test]
fn committed_publica_contexto_original_y_habilita_retiro_abc() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, _) = preparar_normal_v2(dir.path(), JournalState::Committed);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    owner.publicar_intent_v3_desde_committed().unwrap();
    let observado = owner.observar_intent_v3().unwrap();
    assert_eq!(observado.intent.contexto_original(), fixture.bytes_contexto);
    drop(observado);
    for expected in [
        super::super::retiro_v3::EstadoPasoRetiroV3::AvanzoB,
        super::super::retiro_v3::EstadoPasoRetiroV3::AvanzoC,
        super::super::retiro_v3::EstadoPasoRetiroV3::Finalizado,
    ] {
        assert_eq!(owner.avanzar_retiro_v3().unwrap().estado(), expected);
    }
    verificar_arbol(&fixture.root, &fixture.entradas);
    assert!(!dir.path().join(".bundle.felix.admctx-retire").exists());
    assert!(!dir.path().join(".bundle.felix.admctx-retire.tmp").exists());
}

#[test]
fn precondicion_invalida_no_crea_temporal() {
    for state in [JournalState::Prepared, JournalState::Installed] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, _) = preparar_normal_v2(dir.path(), state);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        assert!(owner.publicar_intent_v3_desde_committed().is_err());
        assert!(!dir.path().join(".bundle.felix.admctx-retire.tmp").exists());
        assert_eq!(
            fs::read(fixture.ubicacion.journal().as_path()).unwrap(),
            fixture.bytes_journal
        );
    }
}

#[test]
fn root_cambiado_tras_tmp_conserva_temporal_y_no_publica() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, _) = preparar_normal_v2(dir.path(), JournalState::Committed);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let calls = Cell::new(0);
    let result = owner.publicar_intent_v3_con_hooks(
        || fs::write(fixture.root.join("canary"), b"mutated").unwrap(),
        |_, _, _, _| {
            calls.set(calls.get() + 1);
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(calls.get(), 0);
    assert!(dir.path().join(".bundle.felix.admctx-retire.tmp").exists());
    assert!(!dir.path().join(".bundle.felix.admctx-retire").exists());
    assert_eq!(
        fs::read(contexto_path(&fixture.ubicacion)).unwrap(),
        fixture.bytes_contexto
    );
}

#[test]
fn cambios_contexto_y_tmp_despues_de_sync_fallan_sin_rename() {
    for mutate_context in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, _) = preparar_normal_v2(dir.path(), JournalState::Committed);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let calls = Cell::new(0);
        let path = if mutate_context {
            contexto_path(&fixture.ubicacion)
        } else {
            dir.path().join(".bundle.felix.admctx-retire.tmp")
        };
        assert!(
            owner
                .publicar_intent_v3_con_hooks(
                    || fs::write(&path, b"changed").unwrap(),
                    |_, _, _, _| {
                        calls.set(1);
                        Ok(())
                    }
                )
                .is_err()
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(fs::read(path).unwrap(), b"changed");
        assert!(!dir.path().join(".bundle.felix.admctx-retire").exists());
    }
}

#[test]
fn fallo_syscall_antes_o_despues_de_efecto_no_limpia_evidencia() {
    for effect in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, _) = preparar_normal_v2(dir.path(), JournalState::Committed);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let calls = Cell::new(0);
        assert!(
            owner
                .publicar_intent_v3_con_hooks(
                    || {},
                    |p, f, t, c| {
                        calls.set(calls.get() + 1);
                        if effect {
                            contexto_admision::renombrar_no_reemplazar(p, f, t, c).unwrap();
                        }
                        Err(())
                    }
                )
                .is_err()
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(
            dir.path().join(".bundle.felix.admctx-retire").exists(),
            effect
        );
        assert_eq!(
            dir.path().join(".bundle.felix.admctx-retire.tmp").exists(),
            !effect
        );
        assert_eq!(
            fs::read(fixture.ubicacion.journal().as_path()).unwrap(),
            fixture.bytes_journal
        );
        assert_eq!(
            fs::read(contexto_path(&fixture.ubicacion)).unwrap(),
            fixture.bytes_contexto
        );
    }
}

#[test]
fn destino_que_aparece_antes_de_rename_no_se_reemplaza() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, _) = preparar_normal_v2(dir.path(), JournalState::Committed);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let canonical = dir.path().join(".bundle.felix.admctx-retire");
    assert!(
        owner
            .publicar_intent_v3_con_hooks(
                || {},
                |p, f, t, c| {
                    fs::write(&canonical, b"canary").unwrap();
                    contexto_admision::renombrar_no_reemplazar(p, f, t, c)
                }
            )
            .is_err()
    );
    assert_eq!(fs::read(canonical).unwrap(), b"canary");
    assert!(dir.path().join(".bundle.felix.admctx-retire.tmp").exists());
}
