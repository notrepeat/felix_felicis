use super::*;
use crate::transaccion::journal::{
    ArtifactFingerprint, DirectoryIdentity, JournalState, RawTreeDigest,
};

fn identity(id: u64) -> DirectoryIdentity {
    #[cfg(unix)]
    {
        DirectoryIdentity::unix(1, id)
    }
    #[cfg(windows)]
    {
        DirectoryIdentity::windows(1, format!("{id:032x}"))
    }
}

fn fingerprint(id: u64, byte: char) -> ArtifactFingerprint {
    ArtifactFingerprint::new(
        identity(id),
        RawTreeDigest::from_hex(byte.to_string().repeat(64)).unwrap(),
    )
}

fn inventory(
    root: ArtifactState,
    backup: ArtifactState,
    stage: ArtifactState,
    root_new_validated: bool,
) -> RecoveryInventory {
    RecoveryInventory {
        root,
        backup,
        stage,
        root_new_validated,
    }
}

#[test]
fn classification_requires_the_entire_identity_and_raw_tree_digest() {
    use ArtifactState::{Absent, New, Old, Unknown};
    let old = fingerprint(2, 'a');
    let new = fingerprint(3, 'b');
    let wrong_digest = fingerprint(2, 'c');

    assert_eq!(classify_artifact(None, &old, &new), Absent);
    assert_eq!(
        classify_artifact(Some(&ObservedArtifact::new(old.clone())), &old, &new),
        Old
    );
    assert_eq!(
        classify_artifact(Some(&ObservedArtifact::new(new.clone())), &old, &new),
        New
    );
    assert_eq!(
        classify_artifact(Some(&ObservedArtifact::new(wrong_digest)), &old, &new),
        Unknown
    );
}

#[test]
fn partial_stage_requires_the_journal_new_identity_and_never_counts_as_validated_new() {
    use ArtifactState::{PartialNew, Unknown};
    let old = fingerprint(2, 'a');
    let new = fingerprint(3, 'b');
    let changed_new = fingerprint(3, 'c');
    let changed_other = fingerprint(4, 'c');

    assert_eq!(
        classify_stage_artifact(Some(&ObservedArtifact::new(changed_new)), &old, &new),
        PartialNew
    );
    assert_eq!(
        classify_stage_artifact(Some(&ObservedArtifact::new(changed_other)), &old, &new),
        Unknown
    );
}

#[test]
fn partial_old_never_produces_recovery_steps_in_any_inventory_role() {
    use ArtifactState::{Absent, Old, PartialOld};
    use JournalState::Installed;
    use RecoveryDecision::{FailClosed, KeepOld};

    for seen in [
        inventory(PartialOld, Absent, Absent, false),
        inventory(Absent, PartialOld, Absent, false),
        inventory(Absent, Absent, PartialOld, false),
    ] {
        assert_eq!(decide_recovery(Installed, seen), FailClosed);
        assert!(decide_recovery(Installed, seen).steps(seen).is_empty());
        assert!(KeepOld.steps(seen).is_empty());
    }

    let exact_old = inventory(Old, Absent, Absent, false);
    assert_eq!(decide_recovery(Installed, exact_old), KeepOld);
}

#[test]
fn installed_new_without_candidate_validation_restores_old_before_journal_removal() {
    use ArtifactState::{Absent, New, Old};
    use JournalState::Installed;
    use RecoveryDecision::DiscardNewRestoreOld;
    use RecoveryStep as S;

    let seen = inventory(New, Old, Absent, false);
    let decision = decide_recovery(Installed, seen);
    assert_eq!(decision, DiscardNewRestoreOld);
    assert_eq!(
        decision.steps(seen),
        &[
            S::RemoveKnownStage,
            S::RemoveRootNew,
            S::RestoreBackupToRoot,
            S::RemoveJournal,
        ]
    );
}

#[test]
fn installed_invalid_candidate_without_old_backup_fails_without_mutation() {
    use ArtifactState::{Absent, New};
    use JournalState::Installed;
    use RecoveryDecision::FailClosed;

    let seen = inventory(New, Absent, Absent, false);
    let decision = decide_recovery(Installed, seen);
    assert_eq!(decision, FailClosed);
    assert!(decision.steps(seen).is_empty());
}

#[test]
fn installed_old_without_backup_replays_the_completed_restore_safely() {
    use ArtifactState::{Absent, New, Old};
    use JournalState::Installed;
    use RecoveryDecision::KeepOld;
    use RecoveryStep as S;

    let root_only = inventory(Old, Absent, Absent, false);
    let decision = decide_recovery(Installed, root_only);
    assert_eq!(decision, KeepOld);
    assert_eq!(decision.steps(root_only), &[S::RemoveJournal]);

    let residual_stage = inventory(Old, Absent, New, false);
    let decision = decide_recovery(Installed, residual_stage);
    assert_eq!(decision, KeepOld);
    assert_eq!(
        decision.steps(residual_stage),
        &[S::RemoveKnownStage, S::RemoveJournal]
    );
}

#[test]
fn committed_new_requires_validation_before_cleanup() {
    use ArtifactState::{Absent, New, Old};
    use JournalState::Committed;
    use RecoveryDecision::{FailClosed, KeepValidatedCommittedNew};
    use RecoveryStep as S;

    let unvalidated_seen = inventory(New, Old, Absent, false);
    let unvalidated = decide_recovery(Committed, unvalidated_seen);
    assert_eq!(unvalidated, FailClosed);
    assert!(unvalidated.steps(unvalidated_seen).is_empty());

    let validated_seen = inventory(New, Old, Absent, true);
    let validated = decide_recovery(Committed, validated_seen);
    assert_eq!(validated, KeepValidatedCommittedNew);
    assert_eq!(
        validated.steps(validated_seen),
        &[S::RemoveKnownStage, S::RemoveKnownBackup, S::RemoveJournal]
    );
}
