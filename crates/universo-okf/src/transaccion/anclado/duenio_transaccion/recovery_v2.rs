//! Contrato V2: confirmar Committed antes de limpiar el backup.
//! La tabla V1 permanece intacta. Los estados sin evidencia fallan cerrado.
use super::familia_persistida::FaseFamiliaPersistida;
#[path = "recovery_v2/forward.rs"]
mod forward;
#[path = "recovery_v2/temporal.rs"]
mod temporal;

use super::{DuenioTransaccionAnclado, retained_v2_pair::NormalV2};
use crate::transaccion::journal::recovery::{ArtifactState, RecoveryInventory};
use crate::transaccion::journal::{ArtifactFingerprint, JournalState, v2::JournalV2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AccionRecoveryV2 {
    LimpiarStageYRevertir,
    RestaurarOld,
    DescartarNewYRestaurarOld,
    PersistirCommitted,
    LimpiarBackupCommitted,
    PublicarIntentRetiro,
    FailClosed,
}

#[derive(Debug)]
pub(super) struct ErrorRecoveryV2;

/// Datos y handles observados; no constituyen por sí solos permiso de mutación.
pub(super) struct ObservacionRecoveryV2 {
    normal: NormalV2,
    journal: JournalV2,
    inventario: RecoveryInventory,
    accion: AccionRecoveryV2,
}

impl DuenioTransaccionAnclado<'_> {
    pub(super) fn observar_recovery_normal_v2(
        &mut self,
    ) -> Result<ObservacionRecoveryV2, ErrorRecoveryV2> {
        self.parent_revalidado().map_err(|_| ErrorRecoveryV2)?;
        let result = self.observar_recovery_normal_v2_interno();
        self.parent_revalidado().map_err(|_| ErrorRecoveryV2)?;
        result
    }

    fn observar_recovery_normal_v2_interno(
        &mut self,
    ) -> Result<ObservacionRecoveryV2, ErrorRecoveryV2> {
        let normal = self.observar_normal_v2().map_err(|_| ErrorRecoveryV2)?;
        let FaseFamiliaPersistida::NormalV2 { journal, contexto } =
            normal.fase().map_err(|_| ErrorRecoveryV2)?
        else {
            return Err(ErrorRecoveryV2);
        };
        if journal.old().identity() == journal.new_fingerprint().identity() {
            return Err(ErrorRecoveryV2);
        }
        let replay = contexto
            .calcular_new_esperado(journal)
            .map_err(|_| ErrorRecoveryV2)?;
        let raw = self
            .observar_siblings_recovery_v2_raw(journal)
            .map_err(|_| ErrorRecoveryV2)?;
        let expected = ArtifactFingerprint::new(
            journal.new_fingerprint().identity().clone(),
            replay.digest().clone(),
        );
        let inventario = RecoveryInventory {
            root: clasificar(raw.root(), journal, false, false),
            backup: clasificar(raw.backup(), journal, true, false),
            stage: clasificar(raw.stage(), journal, false, true),
            root_new_validated: raw.root() == Some(&expected),
        };
        let accion = decidir(journal.state(), inventario);
        let journal = journal.clone();
        Ok(ObservacionRecoveryV2 {
            normal,
            journal,
            inventario,
            accion,
        })
    }
}

fn clasificar(
    actual: Option<&ArtifactFingerprint>,
    journal: &JournalV2,
    partial_old: bool,
    partial_new: bool,
) -> ArtifactState {
    use ArtifactState::*;
    match actual {
        None => Absent,
        Some(actual) if actual == journal.old() => Old,
        Some(actual) if actual == journal.new_fingerprint() => New,
        Some(actual) if partial_old && actual.identity() == journal.old().identity() => PartialOld,
        Some(actual)
            if partial_new && actual.identity() == journal.new_fingerprint().identity() =>
        {
            PartialNew
        }
        _ => Unknown,
    }
}

fn decidir(state: JournalState, inv: RecoveryInventory) -> AccionRecoveryV2 {
    use AccionRecoveryV2::*;
    use ArtifactState::*;
    use JournalState::*;
    match (
        state,
        inv.root,
        inv.backup,
        inv.stage,
        inv.root_new_validated,
    ) {
        (Prepared, Old, Absent, Absent | New | PartialNew, false)
        | (BackupReady | Installed, Old, Absent, Absent, false) => LimpiarStageYRevertir,
        (Prepared | BackupReady | Installed, Absent, Old, Absent | New | PartialNew, false) => {
            RestaurarOld
        }
        (BackupReady, New, Old, Absent, _) | (Installed, New, Old, Absent, false) => {
            DescartarNewYRestaurarOld
        }
        (Installed, New, Absent | Old, Absent, true) => PersistirCommitted,
        (Committed, New, Old | PartialOld, Absent, true) => LimpiarBackupCommitted,
        (Committed, New, Absent, Absent, true) => PublicarIntentRetiro,
        _ => FailClosed,
    }
}

#[cfg(test)]
#[path = "recovery_v2_tests.rs"]
mod tests;
