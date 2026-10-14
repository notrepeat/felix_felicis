use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::recovery::{
    ArtifactState, RecoveryDecision, RecoveryInventory, RecoveryStep, decide_recovery,
};

#[derive(Clone, Copy)]
struct ExpectedRow {
    phase: JournalState,
    root: ArtifactState,
    backup: ArtifactState,
    validated: bool,
    decision: RecoveryDecision,
    steps: &'static [RecoveryStep],
}

const NO_STEPS: &[RecoveryStep] = &[];
const JOURNAL_ONLY: &[RecoveryStep] = &[RecoveryStep::RemoveJournal];
const REMOVE_STAGE_AND_JOURNAL: &[RecoveryStep] =
    &[RecoveryStep::RemoveKnownStage, RecoveryStep::RemoveJournal];
const KEEP_OLD: &[RecoveryStep] = &[
    RecoveryStep::RemoveKnownStage,
    RecoveryStep::RemoveKnownBackup,
    RecoveryStep::RemoveJournal,
];
const RESTORE_OLD: &[RecoveryStep] = &[
    RecoveryStep::RemoveKnownStage,
    RecoveryStep::RestoreBackupToRoot,
    RecoveryStep::RemoveJournal,
];
const DISCARD_NEW_RESTORE_OLD: &[RecoveryStep] = &[
    RecoveryStep::RemoveKnownStage,
    RecoveryStep::RemoveRootNew,
    RecoveryStep::RestoreBackupToRoot,
    RecoveryStep::RemoveJournal,
];
const COMMIT_NEW: &[RecoveryStep] = &[
    RecoveryStep::RemoveKnownStage,
    RecoveryStep::RemoveKnownBackup,
    RecoveryStep::PersistCommitted,
    RecoveryStep::RemoveJournal,
];
const KEEP_COMMITTED_NEW: &[RecoveryStep] = &[
    RecoveryStep::RemoveKnownStage,
    RecoveryStep::RemoveKnownBackup,
    RecoveryStep::RemoveJournal,
];

#[test]
fn explicit_valid_rows_and_their_complement_cover_every_inventory() {
    use ArtifactState::{Absent, New, Old, PartialNew, Unknown};
    use JournalState::{BackupReady, Committed, Installed, Prepared};
    use RecoveryDecision::{
        DiscardNewRestoreOld, FailClosed, KeepOld, KeepValidatedCommittedNew,
        KeepValidatedNewAndCommit, RestoreOld,
    };

    // Independent oracle: only these state/root/backup/validation tuples are actionable.
    let valid_rows = [
        ExpectedRow {
            phase: Prepared,
            root: Old,
            backup: Absent,
            validated: false,
            decision: KeepOld,
            steps: JOURNAL_ONLY,
        },
        ExpectedRow {
            phase: Prepared,
            root: Old,
            backup: Old,
            validated: false,
            decision: KeepOld,
            steps: KEEP_OLD,
        },
        ExpectedRow {
            phase: Prepared,
            root: Absent,
            backup: Old,
            validated: false,
            decision: RestoreOld,
            steps: RESTORE_OLD,
        },
        ExpectedRow {
            phase: BackupReady,
            root: Old,
            backup: Absent,
            validated: false,
            decision: KeepOld,
            steps: JOURNAL_ONLY,
        },
        ExpectedRow {
            phase: BackupReady,
            root: Old,
            backup: Old,
            validated: false,
            decision: KeepOld,
            steps: KEEP_OLD,
        },
        ExpectedRow {
            phase: BackupReady,
            root: Absent,
            backup: Old,
            validated: false,
            decision: RestoreOld,
            steps: RESTORE_OLD,
        },
        ExpectedRow {
            phase: BackupReady,
            root: New,
            backup: Old,
            validated: false,
            decision: DiscardNewRestoreOld,
            steps: DISCARD_NEW_RESTORE_OLD,
        },
        ExpectedRow {
            phase: BackupReady,
            root: New,
            backup: Old,
            validated: true,
            decision: DiscardNewRestoreOld,
            steps: DISCARD_NEW_RESTORE_OLD,
        },
        ExpectedRow {
            phase: Installed,
            root: New,
            backup: Absent,
            validated: true,
            decision: KeepValidatedNewAndCommit,
            steps: COMMIT_NEW,
        },
        ExpectedRow {
            phase: Installed,
            root: New,
            backup: Old,
            validated: true,
            decision: KeepValidatedNewAndCommit,
            steps: COMMIT_NEW,
        },
        ExpectedRow {
            phase: Installed,
            root: New,
            backup: Old,
            validated: false,
            decision: DiscardNewRestoreOld,
            steps: DISCARD_NEW_RESTORE_OLD,
        },
        ExpectedRow {
            phase: Installed,
            root: Absent,
            backup: Old,
            validated: false,
            decision: RestoreOld,
            steps: RESTORE_OLD,
        },
        ExpectedRow {
            phase: Installed,
            root: Old,
            backup: Absent,
            validated: false,
            decision: KeepOld,
            steps: JOURNAL_ONLY,
        },
        ExpectedRow {
            phase: Installed,
            root: Old,
            backup: Old,
            validated: false,
            decision: KeepOld,
            steps: KEEP_OLD,
        },
        ExpectedRow {
            phase: Committed,
            root: New,
            backup: Absent,
            validated: true,
            decision: KeepValidatedCommittedNew,
            steps: KEEP_COMMITTED_NEW,
        },
        ExpectedRow {
            phase: Committed,
            root: New,
            backup: Old,
            validated: true,
            decision: KeepValidatedCommittedNew,
            steps: KEEP_COMMITTED_NEW,
        },
    ];

    let artifacts = [Absent, Old, New, PartialNew, Unknown];
    let phases = [Prepared, BackupReady, Installed, Committed];
    for phase in phases {
        for root in artifacts {
            for backup in artifacts {
                for stage in artifacts {
                    for validated in [false, true] {
                        let inventory = RecoveryInventory {
                            root,
                            backup,
                            stage,
                            root_new_validated: validated,
                        };
                        let row = valid_rows.iter().find(|row| {
                            (row.phase, row.root, row.backup, row.validated)
                                == (phase, root, backup, validated)
                        });
                        let expected = match (stage, row) {
                            (Absent, Some(row)) => (row.decision, row.steps),
                            (New, Some(row)) if row.decision == KeepOld && row.backup == Absent => {
                                (row.decision, REMOVE_STAGE_AND_JOURNAL)
                            }
                            (PartialNew, Some(row))
                                if row.decision == KeepOld
                                    && row.backup == Absent
                                    && root == Old
                                    && !validated =>
                            {
                                (row.decision, REMOVE_STAGE_AND_JOURNAL)
                            }
                            (New, Some(row)) => (row.decision, row.steps),
                            _ => (FailClosed, NO_STEPS),
                        };
                        let actual = decide_recovery(phase, inventory);
                        assert_eq!(actual, expected.0, "{phase:?} {inventory:?}");
                        assert_eq!(
                            actual.steps(inventory),
                            expected.1,
                            "{phase:?} {inventory:?}"
                        );
                    }
                }
            }
        }
    }
}
