//! Ejecución acotada de la fila root-old sin backup, stage ni validación new.

use std::fmt;

use crate::transaccion::anclado::journal_temporal::{
    ErrorJournalAnclado, ResultadoEliminacionJournal,
};
use crate::transaccion::anclado::renombrar::ErrorRenombrarAnclado;
#[cfg(test)]
use crate::transaccion::journal::JournalState;
use crate::transaccion::lock::LockEscritura;

use super::{
    ArtifactState, ErrorObservadorRecovery, FilaRecoverySoportada, RecoveryInventory,
    ResultadoPreflightRecovery, clasificar_fila_recovery, observar_inventario_recovery,
    preflight_recovery,
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

/// Resultado explícito de intentar restaurar el backup validado como root.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResultadoFilaJournalRestoreOld {
    NoAplicableJournalAusente,
    Eliminado,
    AusenteTrasContexto,
}

/// Fallo cerrado al cargar, observar, restaurar o retirar el journal.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorFilaJournalRestoreOld {
    Journal(ErrorJournalAnclado),
    Observacion(ErrorObservadorRecovery),
    Renombrar(ErrorRenombrarAnclado),
    FilaNoAutorizada,
    ConflictoAlEliminar(ErrorJournalAnclado),
    EliminacionIndeterminada(ErrorJournalAnclado),
}

impl ErrorFilaJournalRestoreOld {
    const fn codigo(self) -> &'static str {
        match self {
            Self::Journal(error) => error.codigo(),
            Self::Observacion(error) => error.codigo(),
            Self::Renombrar(error) => error.codigo(),
            Self::FilaNoAutorizada => "recovery_row_not_authorized",
            Self::ConflictoAlEliminar(error) => error.codigo(),
            Self::EliminacionIndeterminada(error) => error.codigo(),
        }
    }
}

impl fmt::Debug for ErrorFilaJournalRestoreOld {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorFilaJournalRestoreOld")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorFilaJournalRestoreOld {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorFilaJournalRestoreOld {}

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
    let contexto = match preflight_recovery(lock).map_err(mapear_error_preflight_old)? {
        ResultadoPreflightRecovery::JournalAusente => {
            return Ok(ResultadoFilaJournalOld::NoAplicableJournalAusente);
        }
        ResultadoPreflightRecovery::FaseNoSoportada => {
            return Err(ErrorFilaJournalOld::FilaNoAutorizada);
        }
        ResultadoPreflightRecovery::Contexto(contexto) => contexto,
    };
    if clasificar_fila_recovery(&contexto.journal, contexto.inventario)
        != Some(FilaRecoverySoportada::KeepOld)
    {
        return Err(ErrorFilaJournalOld::FilaNoAutorizada);
    }

    antes_de_eliminar(lock);

    let temporal = lock
        .journal_temporal_anclado()
        .map_err(ErrorFilaJournalOld::Journal)?;
    match temporal.eliminar_canonico_si_coincide(contexto.identidad_journal, &contexto.journal) {
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

/// Restaura únicamente la fila root=absent + backup=old, con stage ausente y
/// sin validación new, en Prepared, BackupReady o Installed.
#[allow(dead_code)]
pub(crate) fn ejecutar_fila_journal_restore_old(
    lock: &mut LockEscritura,
) -> Result<ResultadoFilaJournalRestoreOld, ErrorFilaJournalRestoreOld> {
    ejecutar_fila_journal_restore_old_con(lock, |_| {}, |_| {})
}

fn ejecutar_fila_journal_restore_old_con(
    lock: &mut LockEscritura,
    antes_de_renombrar: impl FnOnce(&LockEscritura),
    antes_de_eliminar: impl FnOnce(&LockEscritura),
) -> Result<ResultadoFilaJournalRestoreOld, ErrorFilaJournalRestoreOld> {
    let contexto = match preflight_recovery(lock).map_err(mapear_error_preflight_restore_old)? {
        ResultadoPreflightRecovery::JournalAusente => {
            return Ok(ResultadoFilaJournalRestoreOld::NoAplicableJournalAusente);
        }
        ResultadoPreflightRecovery::FaseNoSoportada => {
            return Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada);
        }
        ResultadoPreflightRecovery::Contexto(contexto) => contexto,
    };
    if clasificar_fila_recovery(&contexto.journal, contexto.inventario)
        != Some(FilaRecoverySoportada::RestoreOld)
    {
        return Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada);
    }

    antes_de_renombrar(lock);

    let destino = lock
        .ubicacion()
        .ruta()
        .file_name()
        .ok_or(ErrorFilaJournalRestoreOld::FilaNoAutorizada)?
        .to_os_string();
    let intento_renombrar = {
        let mut espacio = lock
            .espacio_transaccional_anclado()
            .map_err(ErrorFilaJournalRestoreOld::Renombrar)?;
        let mut backup = espacio
            .reabrir_backup_sibling(
                |identidad| {
                    identidad.journal_identity().as_ref() == Some(contexto.journal.parent())
                },
                |identidad| {
                    identidad.journal_identity().as_ref() == Some(contexto.journal.old().identity())
                },
            )
            .map_err(ErrorFilaJournalRestoreOld::Renombrar)?;
        let intento = espacio.renombrar_directorio(&mut backup, &destino);
        drop(backup);
        intento
    };

    // Reobservar incluso tras error: algunas plataformas pueden completar el
    // rename y reportar un fallo al caller.
    let inventario_posterior = observar_inventario_recovery(lock, &contexto.journal)
        .map_err(ErrorFilaJournalRestoreOld::Observacion)?;
    reconciliar_intento_renombrado_restore_old(
        intento_renombrar,
        inventario_posterior,
        contexto.inventario,
    )?;

    antes_de_eliminar(lock);

    let temporal = lock
        .journal_temporal_anclado()
        .map_err(ErrorFilaJournalRestoreOld::Journal)?;
    match temporal.eliminar_canonico_si_coincide(contexto.identidad_journal, &contexto.journal) {
        ResultadoEliminacionJournal::Eliminado => Ok(ResultadoFilaJournalRestoreOld::Eliminado),
        ResultadoEliminacionJournal::Ausente => {
            Ok(ResultadoFilaJournalRestoreOld::AusenteTrasContexto)
        }
        ResultadoEliminacionJournal::Conflicto(error) => {
            Err(ErrorFilaJournalRestoreOld::ConflictoAlEliminar(error))
        }
        ResultadoEliminacionJournal::Indeterminado(error) => {
            Err(ErrorFilaJournalRestoreOld::EliminacionIndeterminada(error))
        }
    }
}

fn mapear_error_preflight_old(error: super::ErrorPreflightRecovery) -> ErrorFilaJournalOld {
    match error {
        super::ErrorPreflightRecovery::Journal(error) => ErrorFilaJournalOld::Journal(error),
        super::ErrorPreflightRecovery::Observacion(error) => {
            ErrorFilaJournalOld::Observacion(error)
        }
    }
}

fn mapear_error_preflight_restore_old(
    error: super::ErrorPreflightRecovery,
) -> ErrorFilaJournalRestoreOld {
    match error {
        super::ErrorPreflightRecovery::Journal(error) => ErrorFilaJournalRestoreOld::Journal(error),
        super::ErrorPreflightRecovery::Observacion(error) => {
            ErrorFilaJournalRestoreOld::Observacion(error)
        }
    }
}

fn reconciliar_intento_renombrado_restore_old(
    intento: Result<(), ErrorRenombrarAnclado>,
    inventario_posterior: RecoveryInventory,
    fila_inicial: RecoveryInventory,
) -> Result<(), ErrorFilaJournalRestoreOld> {
    let fila_restaurada = RecoveryInventory {
        root: ArtifactState::Old,
        backup: ArtifactState::Absent,
        stage: ArtifactState::Absent,
        root_new_validated: false,
    };
    if inventario_posterior == fila_restaurada {
        return Ok(());
    }
    if inventario_posterior == fila_inicial {
        if let Err(error) = intento {
            return Err(ErrorFilaJournalRestoreOld::Renombrar(error));
        }
    }
    Err(ErrorFilaJournalRestoreOld::FilaNoAutorizada)
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
