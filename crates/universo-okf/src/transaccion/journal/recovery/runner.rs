//! Ejecución acotada de la fila root-old sin backup, stage ni validación new.

use std::fmt;

use crate::transaccion::anclado::journal_temporal::{
    ErrorJournalAnclado, ResultadoEliminacionJournal,
};
use crate::transaccion::journal::JournalState;
use crate::transaccion::lock::LockEscritura;

use super::{
    ArtifactState, ErrorObservadorRecovery, RecoveryDecision, RecoveryInventory, RecoveryStep,
    decide_recovery, observar_inventario_recovery,
};

/// Resultado explícito de ejecutar o no la fila acotada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResultadoFilaJournalOld {
    /// No se cargó un journal canónico; no se observó ni autorizó ninguna fila.
    NoAplicableJournalAusente,
    /// La capability confirmó que esta llamada eliminó el journal esperado.
    Eliminado,
    /// El contexto esperado fue validado, pero el journal ya no estaba al retirar.
    AusenteTrasContexto,
}

/// Fallo cerrado al cargar, observar, decidir o retirar el journal de la fila.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorFilaJournalOld {
    Journal(ErrorJournalAnclado),
    Observacion(ErrorObservadorRecovery),
    FilaNoAutorizada,
    ConflictoAlEliminar(ErrorJournalAnclado),
    EliminacionIndeterminada(ErrorJournalAnclado),
}

impl ErrorFilaJournalOld {
    const fn codigo(self) -> &'static str {
        match self {
            Self::Journal(error) => error.codigo(),
            Self::Observacion(error) => error.codigo(),
            Self::FilaNoAutorizada => "recovery_row_not_authorized",
            Self::ConflictoAlEliminar(error) => error.codigo(),
            Self::EliminacionIndeterminada(error) => error.codigo(),
        }
    }
}

impl fmt::Debug for ErrorFilaJournalOld {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorFilaJournalOld")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorFilaJournalOld {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorFilaJournalOld {}

/// Ejecuta únicamente la fila root=old + backup/stage ausentes en Prepared,
/// BackupReady o Installed. El lock cubre carga, observación y eliminación.
pub(crate) fn ejecutar_fila_journal_old(
    lock: &mut LockEscritura,
) -> Result<ResultadoFilaJournalOld, ErrorFilaJournalOld> {
    ejecutar_fila_journal_old_con(lock, |_| {})
}

fn ejecutar_fila_journal_old_con(
    lock: &mut LockEscritura,
    antes_de_eliminar: impl FnOnce(&LockEscritura),
) -> Result<ResultadoFilaJournalOld, ErrorFilaJournalOld> {
    let (identidad_esperada, journal) = {
        let temporal = lock
            .journal_temporal_anclado()
            .map_err(ErrorFilaJournalOld::Journal)?;
        let Some(contexto) = temporal
            .cargar_canonico()
            .map_err(ErrorFilaJournalOld::Journal)?
        else {
            return Ok(ResultadoFilaJournalOld::NoAplicableJournalAusente);
        };
        contexto
    };

    if !matches!(
        journal.state(),
        JournalState::Prepared | JournalState::BackupReady | JournalState::Installed
    ) {
        return Err(ErrorFilaJournalOld::FilaNoAutorizada);
    }

    let inventario =
        observar_inventario_recovery(lock, &journal).map_err(ErrorFilaJournalOld::Observacion)?;
    let fila_esperada = RecoveryInventory {
        root: ArtifactState::Old,
        backup: ArtifactState::Absent,
        stage: ArtifactState::Absent,
        root_new_validated: false,
    };
    let decision = decide_recovery(journal.state(), inventario);
    if inventario != fila_esperada
        || decision != RecoveryDecision::KeepOld
        || decision.steps(inventario) != [RecoveryStep::RemoveJournal]
    {
        return Err(ErrorFilaJournalOld::FilaNoAutorizada);
    }

    antes_de_eliminar(lock);

    let temporal = lock
        .journal_temporal_anclado()
        .map_err(ErrorFilaJournalOld::Journal)?;
    match temporal.eliminar_canonico_si_coincide(identidad_esperada, &journal) {
        ResultadoEliminacionJournal::Eliminado => Ok(ResultadoFilaJournalOld::Eliminado),
        ResultadoEliminacionJournal::Ausente => Ok(ResultadoFilaJournalOld::AusenteTrasContexto),
        ResultadoEliminacionJournal::Conflicto(error) => {
            Err(ErrorFilaJournalOld::ConflictoAlEliminar(error))
        }
        ResultadoEliminacionJournal::Indeterminado(error) => {
            Err(ErrorFilaJournalOld::EliminacionIndeterminada(error))
        }
    }
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
