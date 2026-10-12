//! Transiciones y decisión pura de recovery del protocolo journal v1.

use super::{ArtifactFingerprint, JournalState};

#[path = "recovery/observer.rs"]
mod observer;
#[allow(unused_imports)]
pub(crate) use observer::{ErrorObservadorRecovery, observar_inventario_recovery};

#[path = "recovery/preflight.rs"]
mod preflight;
pub(crate) use preflight::{
    ContextoPreflightRecovery, ErrorPreflightRecovery, FilaRecoverySoportada,
    ResultadoPreflightRecovery, clasificar_fila_recovery, preflight_recovery,
};

#[path = "recovery/dispatcher.rs"]
mod dispatcher;
#[allow(unused_imports)]
pub(crate) use dispatcher::{
    ErrorRecoverySoportado, ResultadoRecoverySoportado, ejecutar_recovery_soportado,
};

#[path = "recovery/runner.rs"]
mod runner;
#[allow(unused_imports)]
pub(crate) use runner::{
    ErrorFilaJournalOld, ErrorFilaJournalRestoreOld, ResultadoFilaJournalOld,
    ResultadoFilaJournalRestoreOld, ejecutar_fila_journal_old, ejecutar_fila_journal_restore_old,
};

/// Resultado abstracto de clasificar root, backup o stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactState {
    Absent,
    Old,
    New,
    Unknown,
}

/// Fingerprint observado que el caller obtuvo al inspeccionar un sibling.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ObservedArtifact(ArtifactFingerprint);

impl ObservedArtifact {
    pub(crate) fn new(fingerprint: ArtifactFingerprint) -> Self {
        Self(fingerprint)
    }
}

/// Solo una coincidencia completa de identidad y digest clasifica old/new.
pub(crate) fn classify_artifact(
    observed: Option<&ObservedArtifact>,
    old: &ArtifactFingerprint,
    new: &ArtifactFingerprint,
) -> ArtifactState {
    let Some(observed) = observed else {
        return ArtifactState::Absent;
    };
    if observed.0 == *old {
        ArtifactState::Old
    } else if observed.0 == *new {
        ArtifactState::New
    } else {
        ArtifactState::Unknown
    }
}

/// Clasificación entregada después de inspeccionar root, backup y stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecoveryInventory {
    pub(crate) root: ArtifactState,
    pub(crate) backup: ArtifactState,
    pub(crate) stage: ArtifactState,
    /// True solo si root coincide por identidad/digest y recarga contra el candidato.
    pub(crate) root_new_validated: bool,
}

/// Operación abstracta en el orden en que debe ejecutarse bajo el lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryStep {
    RemoveKnownStage,
    RemoveKnownBackup,
    RemoveRootNew,
    RestoreBackupToRoot,
    PersistCommitted,
    RemoveJournal,
}

/// Decisión y receta ordenada; `FailClosed` siempre tiene una secuencia vacía.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryDecision {
    KeepOld,
    RestoreOld,
    DiscardNewRestoreOld,
    KeepValidatedNewAndCommit,
    KeepValidatedCommittedNew,
    FailClosed,
}

const NO_STEPS: &[RecoveryStep] = &[];
const JOURNAL_ONLY_STEPS: &[RecoveryStep] = &[RecoveryStep::RemoveJournal];
const REMOVE_STAGE_AND_JOURNAL_STEPS: &[RecoveryStep] =
    &[RecoveryStep::RemoveKnownStage, RecoveryStep::RemoveJournal];
const KEEP_OLD_STEPS: &[RecoveryStep] = &[
    RecoveryStep::RemoveKnownStage,
    RecoveryStep::RemoveKnownBackup,
    RecoveryStep::RemoveJournal,
];
const RESTORE_OLD_STEPS: &[RecoveryStep] = &[
    RecoveryStep::RemoveKnownStage,
    RecoveryStep::RestoreBackupToRoot,
    RecoveryStep::RemoveJournal,
];
const DISCARD_NEW_RESTORE_OLD_STEPS: &[RecoveryStep] = &[
    RecoveryStep::RemoveKnownStage,
    RecoveryStep::RemoveRootNew,
    RecoveryStep::RestoreBackupToRoot,
    RecoveryStep::RemoveJournal,
];
const COMMIT_NEW_STEPS: &[RecoveryStep] = &[
    RecoveryStep::RemoveKnownStage,
    RecoveryStep::RemoveKnownBackup,
    RecoveryStep::PersistCommitted,
    RecoveryStep::RemoveJournal,
];
const KEEP_COMMITTED_NEW_STEPS: &[RecoveryStep] = &[
    RecoveryStep::RemoveKnownStage,
    RecoveryStep::RemoveKnownBackup,
    RecoveryStep::RemoveJournal,
];

impl RecoveryDecision {
    pub(crate) fn steps(self, inventory: RecoveryInventory) -> &'static [RecoveryStep] {
        if self == Self::KeepOld
            && inventory.root == ArtifactState::Old
            && inventory.backup == ArtifactState::Absent
        {
            return match inventory.stage {
                ArtifactState::Absent => JOURNAL_ONLY_STEPS,
                ArtifactState::New => REMOVE_STAGE_AND_JOURNAL_STEPS,
                ArtifactState::Old | ArtifactState::Unknown => NO_STEPS,
            };
        }
        match self {
            Self::KeepOld => KEEP_OLD_STEPS,
            Self::RestoreOld => RESTORE_OLD_STEPS,
            Self::DiscardNewRestoreOld => DISCARD_NEW_RESTORE_OLD_STEPS,
            Self::KeepValidatedNewAndCommit => COMMIT_NEW_STEPS,
            Self::KeepValidatedCommittedNew => KEEP_COMMITTED_NEW_STEPS,
            Self::FailClosed => NO_STEPS,
        }
    }
}

/// El caller marca root validado solo tras recargarlo y compararlo con el candidato.
pub(crate) fn decide_recovery(
    state: JournalState,
    inventory: RecoveryInventory,
) -> RecoveryDecision {
    use ArtifactState::{Absent, New, Old, Unknown};
    use JournalState::{BackupReady, Committed, Installed, Prepared};
    use RecoveryDecision::{
        DiscardNewRestoreOld, FailClosed, KeepOld, KeepValidatedCommittedNew,
        KeepValidatedNewAndCommit, RestoreOld,
    };

    let RecoveryInventory {
        root,
        backup,
        stage,
        root_new_validated,
    } = inventory;
    if root == Unknown
        || backup == Unknown
        || stage == Unknown
        || (root_new_validated && root != New)
        || !matches!(stage, Absent | New)
    {
        return FailClosed;
    }

    match (state, root, backup, root_new_validated) {
        (Prepared, Old, Absent | Old, false) => KeepOld,
        (Prepared, Absent, Old, false) => RestoreOld,
        (BackupReady, Old, Absent | Old, false) => KeepOld,
        (BackupReady, Absent, Old, false) => RestoreOld,
        (BackupReady, New, Old, false | true) => DiscardNewRestoreOld,
        (Installed, New, Absent | Old, true) => KeepValidatedNewAndCommit,
        (Installed, New, Old, false) => DiscardNewRestoreOld,
        (Installed, Absent, Old, false) => RestoreOld,
        (Installed, Old, Absent, false) => KeepOld,
        (Installed, Old, Old, false) => KeepOld,
        (Committed, New, Absent | Old, true) => KeepValidatedCommittedNew,
        _ => FailClosed,
    }
}

#[cfg(test)]
#[path = "recovery/critical_tests.rs"]
mod critical_tests;

#[cfg(test)]
#[path = "recovery/truth_table.rs"]
mod truth_table;
