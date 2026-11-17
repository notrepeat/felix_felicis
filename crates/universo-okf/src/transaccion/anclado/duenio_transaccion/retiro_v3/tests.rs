use super::super::fixture_root_replay::{contexto_path, verificar_arbol};
use super::super::retiro_c_v3::tests::fixture_con_sidecars;
use super::*;
use crate::transaccion::lock::LockEscritura;
use std::fs;

#[test]
fn ciclo_real_retira_journal_context_e_intent_en_tres_pasos() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, intent, frame) = fixture_con_sidecars(dir.path(), false, true);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    assert_eq!(
        owner.avanzar_retiro_v3().unwrap().estado(),
        EstadoPasoRetiroV3::AvanzoB
    );
    assert!(!fixture.ubicacion.journal().as_path().exists());
    assert!(contexto_path(&fixture.ubicacion).exists());
    assert_eq!(fs::read(&intent).unwrap(), frame);
    assert_eq!(
        owner.avanzar_retiro_v3().unwrap().estado(),
        EstadoPasoRetiroV3::AvanzoC
    );
    assert!(!contexto_path(&fixture.ubicacion).exists());
    assert_eq!(fs::read(&intent).unwrap(), frame);
    assert_eq!(
        owner.avanzar_retiro_v3().unwrap().estado(),
        EstadoPasoRetiroV3::Finalizado
    );
    assert!(!intent.exists());
    verificar_arbol(&fixture.root, &fixture.entradas);
}

#[test]
fn sidecar_sustituido_bytes_iguales_y_contexto_distinto_fallan_sin_mutacion() {
    for contexto in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, intent, frame) = fixture_con_sidecars(dir.path(), false, true);
        let path = if contexto {
            contexto_path(&fixture.ubicacion)
        } else {
            fixture.ubicacion.journal().as_path().to_path_buf()
        };
        let original = fs::read(&path).unwrap();
        if contexto {
            fs::write(&path, b"private changed payload").unwrap();
        } else {
            fs::rename(&path, path.with_extension("old")).unwrap();
            fs::write(&path, original).unwrap();
        }
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        assert!(owner.avanzar_retiro_v3().is_err());
        assert!(path.exists());
        assert_eq!(fs::read(&intent).unwrap(), frame);
    }
}

#[test]
fn callback_error_despues_efecto_avanza_solo_con_observacion_completa() {
    for fase_b in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, intent, frame) = fixture_con_sidecars(dir.path(), false, true);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        if fase_b {
            drop(owner.avanzar_retiro_v3().unwrap());
        }
        let target = if fase_b {
            contexto_path(&fixture.ubicacion)
        } else {
            fixture.ubicacion.journal().as_path().to_path_buf()
        };
        let result = owner
            .avanzar_retiro_v3_con_hooks(
                || {},
                |_, _, file| {
                    drop(file);
                    fs::remove_file(&target).unwrap();
                    Err(())
                },
            )
            .unwrap();
        assert_eq!(
            result.estado(),
            if fase_b {
                EstadoPasoRetiroV3::AvanzoC
            } else {
                EstadoPasoRetiroV3::AvanzoB
            }
        );
        assert_eq!(fs::read(intent).unwrap(), frame);
    }
}

#[test]
fn callback_sin_efecto_no_avanza_y_revalidacion_previa_impide_mutacion() {
    for change in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, intent, frame) = fixture_con_sidecars(dir.path(), false, true);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let calls = std::cell::Cell::new(0);
        let result = owner
            .avanzar_retiro_v3_con_hooks(
                || {
                    if change {
                        fs::write(fixture.root.join("intruso"), b"canary").unwrap();
                    }
                },
                |_, _, _| {
                    calls.set(calls.get() + 1);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(calls.get(), if change { 0 } else { 1 });
        assert_eq!(
            result.estado(),
            if change {
                EstadoPasoRetiroV3::Conflicto
            } else {
                EstadoPasoRetiroV3::Indeterminado
            }
        );
        assert!(fixture.ubicacion.journal().as_path().exists());
        assert_eq!(fs::read(intent).unwrap(), frame);
    }
}

#[test]
fn error_despues_unlink_con_root_cambiado_conserva_context_e_intent() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, intent, frame) = fixture_con_sidecars(dir.path(), false, true);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let result = owner
        .avanzar_retiro_v3_con_hooks(
            || {},
            |_, _, file| {
                drop(file);
                fs::remove_file(fixture.ubicacion.journal().as_path()).unwrap();
                fs::write(fixture.root.join("intruso"), b"canary").unwrap();
                Err(())
            },
        )
        .unwrap();
    assert_eq!(result.estado(), EstadoPasoRetiroV3::Indeterminado);
    assert!(contexto_path(&fixture.ubicacion).exists());
    assert_eq!(fs::read(intent).unwrap(), frame);
}

#[test]
fn reemplazo_sidecar_entre_observaciones_no_alcanza_syscall() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, intent, frame) = fixture_con_sidecars(dir.path(), false, true);
    let journal = fixture.ubicacion.journal().as_path().to_path_buf();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let result = owner
        .avanzar_retiro_v3_con_hooks(
            || {
                let bytes = fs::read(&journal).unwrap();
                fs::rename(&journal, journal.with_extension("old")).unwrap();
                fs::write(&journal, bytes).unwrap();
            },
            |_, _, _| panic!("replacement cannot authorize unlink"),
        )
        .unwrap();
    assert_eq!(result.estado(), EstadoPasoRetiroV3::Conflicto);
    assert!(journal.exists());
    assert_eq!(fs::read(intent).unwrap(), frame);
}
