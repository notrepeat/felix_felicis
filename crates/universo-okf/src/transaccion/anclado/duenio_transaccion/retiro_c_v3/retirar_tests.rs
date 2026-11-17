use super::super::tests::fixture;
use super::*;
use crate::transaccion::lock::LockEscritura;
use std::fs;

#[test]
fn retiro_real_confirma_ausencia_y_preserva_root() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, intent, _) = fixture(dir.path());
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let result = owner.observar_retiro_c_v3_consistente().unwrap().retirar();
    assert_eq!(result.estado(), EstadoRetiroCV3::Eliminado);
    assert!(!intent.exists());
    crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::verificar_arbol(
        &fixture.root,
        &fixture.entradas,
    );
}

#[test]
fn retorno_del_callback_no_determina_si_hubo_efecto() {
    for (remove, success) in [(false, false), (false, true), (true, false)] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, intent, frame) = fixture(dir.path());
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let calls = std::cell::Cell::new(0);
        let result = owner
            .observar_retiro_c_v3_consistente()
            .unwrap()
            .retirar_con_intento_para_test(|_, _, file| {
                calls.set(calls.get() + 1);
                drop(file);
                if remove {
                    fs::remove_file(&intent).unwrap();
                }
                if success { Ok(()) } else { Err(()) }
            });
        assert_eq!(calls.get(), 1);
        assert_eq!(
            result.estado(),
            if remove {
                EstadoRetiroCV3::Ausente
            } else {
                EstadoRetiroCV3::Indeterminado
            }
        );
        if !remove {
            assert_eq!(fs::read(&intent).unwrap(), frame);
        }
    }
}

#[test]
fn proof_obsoleto_no_autoriza_borrado_de_root_o_intent_cambiados() {
    for change in 0..3 {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, intent, frame) = fixture(dir.path());
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let proof = owner.observar_retiro_c_v3_consistente().unwrap();
        match change {
            0 => {
                fs::write(fixture.root.join("intruso"), b"canary").unwrap();
            }
            1 => {
                fs::rename(&intent, intent.with_extension("old")).unwrap();
                fs::write(&intent, &frame).unwrap();
            }
            _ => {
                fs::write(
                    dir.path().join(fixture.ubicacion.nombre_journal_temporal()),
                    b"canary",
                )
                .unwrap();
            }
        }
        let result = proof.retirar_con_intento_para_test(|_, _, _| panic!("no mutation allowed"));
        assert_eq!(result.estado(), EstadoRetiroCV3::Conflicto);
        assert_eq!(fs::read(&intent).unwrap(), frame);
    }
}

#[test]
fn ausencia_previa_se_confirma_sin_intento_y_no_oculta_root_cambiado() {
    for change_root in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, intent, _) = fixture(dir.path());
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let proof = owner.observar_retiro_c_v3_consistente().unwrap();
        fs::remove_file(&intent).unwrap();
        if change_root {
            fs::write(fixture.root.join("intruso"), b"canary").unwrap();
        }
        let result = proof.retirar_con_intento_para_test(|_, _, _| panic!("already absent"));
        assert_eq!(
            result.estado(),
            if change_root {
                EstadoRetiroCV3::Conflicto
            } else {
                EstadoRetiroCV3::Ausente
            }
        );
    }
}

#[test]
fn reaparicion_o_temporal_tras_intento_no_reporta_eliminado() {
    for temporary in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, intent, frame) = fixture(dir.path());
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let result = owner
            .observar_retiro_c_v3_consistente()
            .unwrap()
            .retirar_con_intento_para_test(|_, _, file| {
                drop(file);
                fs::rename(&intent, intent.with_extension("old")).unwrap();
                fs::write(&intent, &frame).unwrap();
                if temporary {
                    fs::write(
                        dir.path().join(fixture.ubicacion.nombre_journal_temporal()),
                        b"canary",
                    )
                    .unwrap();
                }
                Ok(())
            });
        assert_eq!(
            result.estado(),
            if temporary {
                EstadoRetiroCV3::Indeterminado
            } else {
                EstadoRetiroCV3::Conflicto
            }
        );
        assert_eq!(fs::read(intent).unwrap(), frame);
    }
}

#[test]
fn root_mutado_despues_del_unlink_impide_confirmar_exito() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, intent, _) = fixture(dir.path());
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let result = owner
        .observar_retiro_c_v3_consistente()
        .unwrap()
        .retirar_con_intento_para_test(|_, _, file| {
            drop(file);
            fs::remove_file(&intent).unwrap();
            fs::write(fixture.root.join("intruso"), b"canary").unwrap();
            Ok(())
        });
    assert_eq!(result.estado(), EstadoRetiroCV3::Indeterminado);
    assert_eq!(fs::read(fixture.root.join("intruso")).unwrap(), b"canary");
}

#[cfg(unix)]
#[test]
fn perdida_real_del_lock_despues_del_unlink_conserva_resultado_indeterminado() {
    let dir = tempfile::tempdir().unwrap();
    let (fixture, intent, _) = fixture(dir.path());
    let lock_path = fixture.ubicacion.lock().as_path().to_path_buf();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let result = owner
        .observar_retiro_c_v3_consistente()
        .unwrap()
        .retirar_con_intento_para_test(|_, _, file| {
            drop(file);
            fs::remove_file(&intent).unwrap();
            fs::rename(&lock_path, lock_path.with_extension("old")).unwrap();
            fs::write(&lock_path, b"replacement_lock_canary").unwrap();
            Ok(())
        });
    assert_eq!(result.estado(), EstadoRetiroCV3::Indeterminado);
    assert!(!intent.exists());
    assert_eq!(fs::read(&lock_path).unwrap(), b"replacement_lock_canary");
    crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::verificar_arbol(
        &fixture.root,
        &fixture.entradas,
    );
}
