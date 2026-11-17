use super::successor_observation_support as support;
use super::{crear_root, nombre_journal, nombre_temporal};
use crate::transaccion::anclado::duenio_transaccion::{
    DuenioTransaccionAnclado, ResultadoOwnerPostcheck,
};
use crate::transaccion::anclado::journal_temporal::{
    ErrorJournalAnclado, EstadoPublicacionSucesorV2,
};
use crate::transaccion::journal::{JournalState, selector::JournalPersistido};
use crate::transaccion::lock::LockEscritura;
use std::fs;

#[test]
fn publica_los_tres_sucesores_con_postcheck_owner_y_preserva_root() {
    for state in [
        JournalState::Prepared,
        JournalState::BackupReady,
        JournalState::Installed,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = crear_root(dir.path());
        let canario = support::crear_canario(&root);
        let mut lock = LockEscritura::adquirir(&root).unwrap();
        let (_, _, _, original) = support::observar_v2_en_fase(
            &root,
            &mut lock,
            "0123456789abcdef0123456789abcdef",
            state,
        );
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let recibo = match owner.preparar_sucesor_journal_v2(&original) {
            ResultadoOwnerPostcheck::PostcheckVerificado { cuerpo: Ok(recibo) } => recibo,
            _ => panic!("preparation failed"),
        };
        assert!(matches!(
            owner.publicar_sucesor_journal_v2(&recibo),
            ResultadoOwnerPostcheck::PostcheckVerificado {
                cuerpo: Ok(EstadoPublicacionSucesorV2::Confirmada)
            }
        ));
        let actual = owner.observar_journal_versionado().unwrap().unwrap();
        assert_eq!(actual.identidad(), recibo.identidad_temporal);
        assert!(matches!(actual.journal(), JournalPersistido::V2(j) if j == &recibo.candidato));
        assert!(!nombre_temporal(dir.path()).exists());
        support::comprobar_canario(&canario);
    }
}

#[test]
fn resultado_syscall_no_suplanta_observaciones() {
    for (mutate, success) in [(false, false), (false, true), (true, false)] {
        let dir = tempfile::tempdir().unwrap();
        let root = crear_root(dir.path());
        let mut lock = LockEscritura::adquirir(&root).unwrap();
        let (_, old_bytes, _, original) = support::observar_v2_en_fase(
            &root,
            &mut lock,
            "0123456789abcdef0123456789abcdef",
            JournalState::Prepared,
        );
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let lease = owner.journal_temporal_estricto().unwrap();
        let recibo = lease.preparar_sucesor_temporal_v2(&original).unwrap();
        let result = lease
            .publicar_sucesor_v2_con_prueba(
                &recibo,
                || {},
                |lease, source| {
                    if mutate {
                        super::super::reemplazar_plataforma(
                            &lease.parent,
                            &lease.nombre,
                            &lease.nombre_journal,
                            source,
                        )
                        .unwrap();
                    }
                    if success {
                        Ok(())
                    } else {
                        Err(ErrorJournalAnclado::Io)
                    }
                },
            )
            .unwrap();
        assert_eq!(
            result,
            if mutate {
                EstadoPublicacionSucesorV2::Confirmada
            } else {
                EstadoPublicacionSucesorV2::Conservado
            }
        );
        if !mutate {
            assert_eq!(fs::read(nombre_journal(&root)).unwrap(), old_bytes);
            assert!(nombre_temporal(dir.path()).exists());
        }
    }
}

#[test]
fn sustitucion_destino_o_mutacion_fuente_antes_syscall_preserva_desconocidos() {
    for source in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = crear_root(dir.path());
        let mut lock = LockEscritura::adquirir(&root).unwrap();
        let (_, old_bytes, _, original) = support::observar_v2_en_fase(
            &root,
            &mut lock,
            "0123456789abcdef0123456789abcdef",
            JournalState::Prepared,
        );
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let lease = owner.journal_temporal_estricto().unwrap();
        let recibo = lease.preparar_sucesor_temporal_v2(&original).unwrap();
        let result = lease.publicar_sucesor_v2_con_prueba(
            &recibo,
            || {
                if source {
                    fs::write(nombre_temporal(dir.path()), b"canary").unwrap();
                } else {
                    let path = nombre_journal(&root);
                    fs::rename(&path, path.with_extension("old")).unwrap();
                    fs::write(path, &old_bytes).unwrap();
                }
            },
            |_, _| panic!("unknown source or target cannot authorize replacement"),
        );
        assert!(result.is_err());
        if source {
            assert_eq!(fs::read(nombre_temporal(dir.path())).unwrap(), b"canary");
        }
        assert_eq!(fs::read(nombre_journal(&root)).unwrap(), old_bytes);
    }
}

#[test]
fn temporal_reaparecido_tras_replace_impide_confirmar_y_se_preserva() {
    let dir = tempfile::tempdir().unwrap();
    let root = crear_root(dir.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let (_, _, _, original) = support::observar_v2_en_fase(
        &root,
        &mut lock,
        "0123456789abcdef0123456789abcdef",
        JournalState::Prepared,
    );
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let recibo = lease.preparar_sucesor_temporal_v2(&original).unwrap();
    let result = lease
        .publicar_sucesor_v2_con_prueba(
            &recibo,
            || {},
            |lease, source| {
                super::super::reemplazar_plataforma(
                    &lease.parent,
                    &lease.nombre,
                    &lease.nombre_journal,
                    source,
                )
                .unwrap();
                fs::write(nombre_temporal(dir.path()), b"canary").unwrap();
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(result, EstadoPublicacionSucesorV2::Indeterminada);
    assert_eq!(fs::read(nombre_temporal(dir.path())).unwrap(), b"canary");
}

#[test]
fn temporal_desaparecido_sin_replace_no_confirma_sucesor() {
    let dir = tempfile::tempdir().unwrap();
    let root = crear_root(dir.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let (_, old_bytes, _, original) = support::observar_v2_en_fase(
        &root,
        &mut lock,
        "0123456789abcdef0123456789abcdef",
        JournalState::Prepared,
    );
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let recibo = lease.preparar_sucesor_temporal_v2(&original).unwrap();
    let result = lease
        .publicar_sucesor_v2_con_prueba(
            &recibo,
            || {},
            |_, _| {
                fs::remove_file(nombre_temporal(dir.path())).unwrap();
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(result, EstadoPublicacionSucesorV2::Indeterminada);
    assert_eq!(fs::read(nombre_journal(&root)).unwrap(), old_bytes);
}
