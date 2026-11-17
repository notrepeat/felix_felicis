use super::*;

fn inv(
    root: ArtifactState,
    backup: ArtifactState,
    stage: ArtifactState,
    valid: bool,
) -> RecoveryInventory {
    RecoveryInventory {
        root,
        backup,
        stage,
        root_new_validated: valid,
    }
}

#[test]
fn committed_precede_cleanup_partial_old_solo_se_admite_en_backup_committed() {
    use ArtifactState::*;
    assert_eq!(
        decidir(JournalState::Installed, inv(New, Old, Absent, true)),
        AccionRecoveryV2::PersistirCommitted
    );
    assert_eq!(
        decidir(JournalState::Committed, inv(New, PartialOld, Absent, true)),
        AccionRecoveryV2::LimpiarBackupCommitted
    );
    assert_eq!(
        decidir(JournalState::Installed, inv(New, PartialOld, Absent, true)),
        AccionRecoveryV2::FailClosed
    );
    for state in [
        JournalState::Prepared,
        JournalState::BackupReady,
        JournalState::Installed,
        JournalState::Committed,
    ] {
        for invalid in [
            inv(Unknown, Old, Absent, true),
            inv(PartialOld, Old, Absent, true),
            inv(New, Old, PartialOld, true),
            inv(New, Unknown, Absent, true),
        ] {
            assert_eq!(decidir(state, invalid), AccionRecoveryV2::FailClosed);
        }
    }
    assert_eq!(
        decidir(JournalState::Committed, inv(New, Old, Absent, false)),
        AccionRecoveryV2::FailClosed
    );
}

#[test]
fn cortes_de_rename_y_retorno_old_conservan_ruta_rollback() {
    use ArtifactState::*;
    assert_eq!(
        decidir(JournalState::Prepared, inv(Old, Absent, PartialNew, false)),
        AccionRecoveryV2::LimpiarStageYRevertir
    );
    for state in [
        JournalState::Prepared,
        JournalState::BackupReady,
        JournalState::Installed,
    ] {
        assert_eq!(
            decidir(state, inv(Absent, Old, New, false)),
            AccionRecoveryV2::RestaurarOld
        );
        assert_eq!(
            decidir(state, inv(Old, Absent, Absent, false)),
            AccionRecoveryV2::LimpiarStageYRevertir
        );
    }
    assert_eq!(
        decidir(JournalState::BackupReady, inv(New, Old, Absent, true)),
        AccionRecoveryV2::DescartarNewYRestaurarOld
    );
    assert_eq!(
        decidir(JournalState::Committed, inv(Old, Absent, Absent, false)),
        AccionRecoveryV2::FailClosed
    );
}

#[test]
fn observacion_real_exige_replay_root_y_namespace_coherentes() {
    use super::super::normal_v2_test_support::preparar_normal_v2;
    use crate::transaccion::lock::LockEscritura;
    for state in [JournalState::Installed, JournalState::Committed] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, _) = preparar_normal_v2(dir.path(), state);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let observation = owner.observar_recovery_normal_v2().unwrap();
        assert_eq!(
            observation.accion,
            if state == JournalState::Installed {
                AccionRecoveryV2::PersistirCommitted
            } else {
                AccionRecoveryV2::PublicarIntentRetiro
            }
        );
        assert!(observation.inventario.root_new_validated);
        std::fs::write(fixture.root.join("foreign"), b"canary").unwrap();
        assert_eq!(
            owner.observar_recovery_normal_v2().unwrap().accion,
            AccionRecoveryV2::FailClosed
        );
        assert_eq!(
            std::fs::read(fixture.root.join("foreign")).unwrap(),
            b"canary"
        );
    }
}

#[test]
fn tmp_sucesor_exacto_se_descarta_sin_promover_y_desconocido_se_preserva() {
    use super::super::{ResultadoOwnerPostcheck, normal_v2_test_support::preparar_normal_v2};
    use crate::transaccion::lock::LockEscritura;
    for corrupt in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, _) = preparar_normal_v2(dir.path(), JournalState::Installed);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let original = owner.observar_journal_versionado().unwrap().unwrap();
        let recibo = match owner.preparar_sucesor_journal_v2(&original) {
            ResultadoOwnerPostcheck::PostcheckVerificado { cuerpo: Ok(recibo) } => recibo,
            _ => panic!("preparation failed"),
        };
        drop(recibo);
        let tmp = dir.path().join(fixture.ubicacion.nombre_journal_temporal());
        if corrupt {
            std::fs::write(&tmp, b"foreign-canary").unwrap();
        }
        let result = owner.reconciliar_sucesor_pendiente_v2();
        if corrupt {
            assert!(result.is_err());
            assert_eq!(std::fs::read(tmp).unwrap(), b"foreign-canary");
        } else {
            assert!(result.unwrap());
            assert!(!tmp.exists());
            assert_eq!(
                owner.observar_recovery_normal_v2().unwrap().accion,
                AccionRecoveryV2::PersistirCommitted
            );
        }
        assert_eq!(
            std::fs::read(fixture.ubicacion.journal().as_path()).unwrap(),
            fixture.bytes_journal
        );
    }
}

#[test]
fn forward_installed_y_committed_completan_retiro_y_preservan_root() {
    use super::super::normal_v2_test_support::preparar_normal_v2;
    use crate::transaccion::lock::LockEscritura;
    for state in [JournalState::Installed, JournalState::Committed] {
        let dir = tempfile::tempdir().unwrap();
        let (fixture, _) = preparar_normal_v2(dir.path(), state);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let before = owner
            .observar_recovery_normal_v2()
            .unwrap()
            .journal
            .new_fingerprint()
            .clone();
        owner.reanudar_confirmacion_v2().unwrap();
        let slots = owner.observar_siblings_nuevo_flujo().unwrap();
        use super::super::observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo::Ausente;
        assert_eq!(slots.journal(), Ausente);
        assert_eq!(slots.contexto(), Ausente);
        assert_eq!(slots.intent_retiro(), Ausente);
        assert_eq!(slots.journal_temporal(), Ausente);
        let root = owner.abrir_root_lectura().unwrap().unwrap();
        assert!(
            root.identidad_raiz.unwrap().journal_identity().as_ref() == Some(before.identity())
        );
    }
}

#[test]
#[ignore = "entry point for a real recovery process"]
fn proceso_forward_v2() {
    let root = std::env::var_os("FELIX_TEST_FORWARD_V2_ROOT").unwrap();
    let mut lock =
        crate::transaccion::lock::LockEscritura::adquirir(std::path::Path::new(&root)).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    owner.reanudar_confirmacion_v2().unwrap();
}

#[test]
fn nuevo_proceso_reabre_installed_y_finaliza_sin_memoria_del_writer() {
    use super::super::normal_v2_test_support::preparar_normal_v2;
    let dir = tempfile::tempdir().unwrap();
    let (fixture, _) = preparar_normal_v2(dir.path(), JournalState::Installed);
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "transaccion::anclado::duenio_transaccion::recovery_v2::tests::proceso_forward_v2",
            "--ignored",
            "--nocapture",
        ])
        .env("FELIX_TEST_FORWARD_V2_ROOT", &fixture.root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!fixture.ubicacion.journal().as_path().exists());
    assert!(!super::super::fixture_root_replay::contexto_path(&fixture.ubicacion).exists());
    assert!(fixture.root.exists());
}

#[test]
fn forward_no_modifica_root_ni_sidecars_si_replay_no_coincide() {
    use super::super::normal_v2_test_support::{
        capturar_storage, preparar_normal_v2, verificar_storage,
    };
    use crate::transaccion::lock::LockEscritura;
    let dir = tempfile::tempdir().unwrap();
    let (fixture, _) = preparar_normal_v2(dir.path(), JournalState::Installed);
    let storage = capturar_storage(&fixture);
    std::fs::write(fixture.root.join("foreign-canary"), b"preserve").unwrap();
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let error = owner.reanudar_confirmacion_v2().unwrap_err();
    assert_eq!(format!("{error:?}"), "recovery_v2_confirmation_unverified");
    verificar_storage(&fixture, &storage);
    assert_eq!(
        std::fs::read(fixture.root.join("foreign-canary")).unwrap(),
        b"preserve"
    );
}
