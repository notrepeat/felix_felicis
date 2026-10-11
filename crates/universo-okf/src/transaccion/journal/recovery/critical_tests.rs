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
fn installed_new_without_candidate_validation_restores_old_before_journal_removal() {
    use ArtifactState::{Absent, New, Old};
    use JournalState::Installed;
    use RecoveryDecision::DiscardNewRestoreOld;
    use RecoveryStep as S;

    let seen = inventory(New, Old, Absent, false);
    let decision = decide_recovery(Installed, seen);
    assert_eq!(decision, DiscardNewRestoreOld);
    assert_eq!(
        decision.steps(),
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

    let decision = decide_recovery(Installed, inventory(New, Absent, Absent, false));
    assert_eq!(decision, FailClosed);
    assert!(decision.steps().is_empty());
}

#[test]
fn committed_new_requires_validation_before_cleanup() {
    use ArtifactState::{Absent, New, Old};
    use JournalState::Committed;
    use RecoveryDecision::{FailClosed, KeepValidatedCommittedNew};
    use RecoveryStep as S;

    let unvalidated = decide_recovery(Committed, inventory(New, Old, Absent, false));
    assert_eq!(unvalidated, FailClosed);
    assert!(unvalidated.steps().is_empty());

    let validated = decide_recovery(Committed, inventory(New, Old, Absent, true));
    assert_eq!(validated, KeepValidatedCommittedNew);
    assert_eq!(
        validated.steps(),
        &[S::RemoveKnownStage, S::RemoveKnownBackup, S::RemoveJournal]
    );
}
