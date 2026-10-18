#![cfg(all(test, any(target_os = "linux", windows)))]

use super::{RecoveryInventory, SiblingsRecoveryV2Raw};
use crate::transaccion::journal::recovery::ArtifactState;
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, RawTreeDigest};

const TRANSACTION_ID: &str = "abcdef0123456789abcdef0123456789";

fn directory_identity(value: u64) -> DirectoryIdentity {
    #[cfg(unix)]
    {
        DirectoryIdentity::unix(7, value)
    }
    #[cfg(windows)]
    {
        DirectoryIdentity::windows(7, format!("{value:032x}"))
    }
}

fn fingerprint(identity: u64, digest: char) -> ArtifactFingerprint {
    ArtifactFingerprint::new(
        directory_identity(identity),
        RawTreeDigest::from_hex(digest.to_string().repeat(64)).unwrap(),
    )
}

fn journal(old: ArtifactFingerprint, new: ArtifactFingerprint) -> JournalV2 {
    JournalV2::new(TRANSACTION_ID, directory_identity(99), old, new).unwrap()
}

fn observed(
    journal: JournalV2,
    root: Option<ArtifactFingerprint>,
    backup: Option<ArtifactFingerprint>,
    stage: Option<ArtifactFingerprint>,
) -> SiblingsRecoveryV2Raw {
    SiblingsRecoveryV2Raw {
        journal,
        root,
        backup,
        stage,
    }
}

fn assert_slots(
    inventory: RecoveryInventory,
    root: ArtifactState,
    backup: ArtifactState,
    stage: ArtifactState,
) {
    assert_eq!(inventory.root, root);
    assert_eq!(inventory.backup, backup);
    assert_eq!(inventory.stage, stage);
    assert!(!inventory.root_new_validated);
}

#[test]
fn clasifica_los_tres_slots_y_conserva_ausencias_como_datos() {
    let old = fingerprint(1, '1');
    let new = fingerprint(2, '2');
    let record = journal(old.clone(), new.clone());
    let raw = observed(record.clone(), Some(new), Some(old), None);

    assert_eq!(raw.journal(), &record);
    assert_slots(
        raw.clasificar_inventario_recovery_v2(),
        ArtifactState::New,
        ArtifactState::Old,
        ArtifactState::Absent,
    );

    let all_absent = observed(record, None, None, None);
    assert_slots(
        all_absent.clasificar_inventario_recovery_v2(),
        ArtifactState::Absent,
        ArtifactState::Absent,
        ArtifactState::Absent,
    );
}

#[test]
fn una_coincidencia_exige_identidad_y_digest_completos() {
    let old = fingerprint(1, '1');
    let new = fingerprint(2, '2');
    let record = journal(old, new.clone());
    let wrong_identity_same_digest = fingerprint(3, '2');
    let same_identity_wrong_digest = fingerprint(2, '3');

    let identity_mismatch = observed(record.clone(), Some(wrong_identity_same_digest), None, None);
    let digest_mismatch = observed(record, Some(same_identity_wrong_digest), None, None);

    assert_slots(
        identity_mismatch.clasificar_inventario_recovery_v2(),
        ArtifactState::Unknown,
        ArtifactState::Absent,
        ArtifactState::Absent,
    );
    assert_slots(
        digest_mismatch.clasificar_inventario_recovery_v2(),
        ArtifactState::Unknown,
        ArtifactState::Absent,
        ArtifactState::Absent,
    );
}

#[test]
fn parciales_de_backup_y_stage_son_por_identidad_y_cierran_si_ids_coinciden() {
    let old = fingerprint(1, '1');
    let new = fingerprint(2, '2');
    let partial_backup = fingerprint(1, '3');
    let partial_stage = fingerprint(2, '4');
    let distinct_ids = observed(
        journal(old, new),
        None,
        Some(partial_backup),
        Some(partial_stage),
    );

    assert_slots(
        distinct_ids.clasificar_inventario_recovery_v2(),
        ArtifactState::Absent,
        ArtifactState::PartialOld,
        ArtifactState::PartialNew,
    );

    let same_identity_old = fingerprint(5, '5');
    let same_identity_new = fingerprint(5, '6');
    let same_identity_changed = observed(
        journal(same_identity_old, same_identity_new),
        None,
        Some(fingerprint(5, '7')),
        Some(fingerprint(5, '8')),
    );

    assert_slots(
        same_identity_changed.clasificar_inventario_recovery_v2(),
        ArtifactState::Absent,
        ArtifactState::Unknown,
        ArtifactState::Unknown,
    );
}
