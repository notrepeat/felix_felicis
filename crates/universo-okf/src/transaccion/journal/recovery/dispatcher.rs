//! Dispatch of the exact recovery rows currently supported by runners.

use crate::transaccion::journal::Journal;
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::stage::StageAnclado;

use super::preflight::preflight_recovery_con_observador;
use super::runner::{
    ejecutar_fila_journal_old_desde_contexto, ejecutar_fila_journal_restore_old_desde_contexto,
};
use super::{
    ErrorFilaJournalOld, ErrorFilaJournalRestoreOld, ErrorObservadorRecovery,
    ErrorPreflightRecovery, FilaRecoverySoportada, RecoveryInventory, ResultadoFilaJournalOld,
    ResultadoFilaJournalRestoreOld, ResultadoPreflightRecovery, clasificar_fila_recovery,
    observar_inventario_recovery,
};

/// Result of dispatching one supported journal row.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResultadoRecoverySoportado {
    JournalAusente,
    KeepOld(ResultadoFilaJournalOld),
    RestoreOld(ResultadoFilaJournalRestoreOld),
}

/// Preflight and row-specific errors remain distinguishable to callers.
#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ErrorRecoverySoportado {
    Preflight(ErrorPreflightRecovery),
    FaseNoSoportada,
    FilaNoSoportada,
    KeepOld(ErrorFilaJournalOld),
    RestoreOld(ErrorFilaJournalRestoreOld),
}

impl ErrorRecoverySoportado {
    pub(crate) fn recuperar_stage_parcial(self) -> Option<StageAnclado> {
        match self {
            Self::KeepOld(error) => error.recuperar_stage_parcial(),
            _ => None,
        }
    }
}

/// Performs one initial preflight load/observation, then runs only the exact supported row
/// selected by that snapshot.
#[allow(dead_code)]
pub(crate) fn ejecutar_recovery_soportado(
    lock: &mut LockEscritura,
) -> Result<ResultadoRecoverySoportado, ErrorRecoverySoportado> {
    ejecutar_recovery_soportado_con(lock, observar_inventario_recovery, |_| {}, |_| {})
}

pub(super) fn ejecutar_recovery_soportado_con(
    lock: &mut LockEscritura,
    observar: impl FnOnce(
        &LockEscritura,
        &Journal,
    ) -> Result<RecoveryInventory, ErrorObservadorRecovery>,
    antes_de_renombrar: impl FnOnce(&LockEscritura),
    antes_de_eliminar: impl FnOnce(&LockEscritura),
) -> Result<ResultadoRecoverySoportado, ErrorRecoverySoportado> {
    let contexto = preflight_recovery_con_observador(lock, observar)
        .map_err(ErrorRecoverySoportado::Preflight)?;
    let contexto = match contexto {
        ResultadoPreflightRecovery::JournalAusente => {
            return Ok(ResultadoRecoverySoportado::JournalAusente);
        }
        ResultadoPreflightRecovery::FaseNoSoportada => {
            return Err(ErrorRecoverySoportado::FaseNoSoportada);
        }
        ResultadoPreflightRecovery::Contexto(contexto) => contexto,
    };

    let fila = clasificar_fila_recovery(&contexto.journal, contexto.inventario)
        .ok_or(ErrorRecoverySoportado::FilaNoSoportada)?;
    match fila {
        FilaRecoverySoportada::KeepOld => {
            ejecutar_fila_journal_old_desde_contexto(lock, *contexto, antes_de_eliminar)
                .map(ResultadoRecoverySoportado::KeepOld)
                .map_err(ErrorRecoverySoportado::KeepOld)
        }
        FilaRecoverySoportada::RestoreOld => ejecutar_fila_journal_restore_old_desde_contexto(
            lock,
            *contexto,
            antes_de_renombrar,
            antes_de_eliminar,
        )
        .map(ResultadoRecoverySoportado::RestoreOld)
        .map_err(ErrorRecoverySoportado::RestoreOld),
    }
}
