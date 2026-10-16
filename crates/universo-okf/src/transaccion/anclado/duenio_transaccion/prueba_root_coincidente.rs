//! Prueba opaca de replay soportado y root observado coincidente.

use std::fmt;
use std::sync::Arc;

use crate::transaccion::admision::envelope::ErrorEnvelopeAdmision;
use crate::transaccion::anclado::contexto_admision::EvidenciaLecturaContexto;
use crate::transaccion::anclado::journal_temporal::IdentidadArchivoJournal;
use crate::transaccion::anclado::{IdentidadDirectorio, ScopeFilesystem};
use crate::transaccion::journal::{Journal, JournalState};

use super::DuenioTransaccionAnclado;
use super::observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo;

/// Fallo fijo: no conserva qué dato de almacenamiento no pudo verificarse.
pub(super) struct ErrorPruebaReplayRootCoincidente;

impl fmt::Debug for ErrorPruebaReplayRootCoincidente {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ErrorPruebaReplayRootCoincidente")
    }
}

impl fmt::Display for ErrorPruebaReplayRootCoincidente {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("admission_replay_root_not_proven")
    }
}

impl std::error::Error for ErrorPruebaReplayRootCoincidente {}

/// Evidencia opaca estrecha. Mantiene vivo el mismo dueño y el lock;
/// sus campos son datos para revalidación futura, no authority de recovery.
/// Las observaciones son secuenciales y no forman snapshot contra actores externos.
pub(super) struct PruebaReplayAdmitidoRootCoincidente<'owner, 'lock> {
    _owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    _autoridad: Arc<()>,
    identidad_parent: IdentidadDirectorio,
    scope_parent: ScopeFilesystem,
    identidad_archivo_journal: IdentidadArchivoJournal,
    journal: Journal,
    evidencia_contexto: EvidenciaLecturaContexto,
}

impl fmt::Debug for PruebaReplayAdmitidoRootCoincidente<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PruebaReplayAdmitidoRootCoincidente { .. }")
    }
}

impl<'lock> DuenioTransaccionAnclado<'lock> {
    /// Emite solo si el journal elegible, el contexto replayable y el root
    /// observado concuerdan bajo los leases secuenciales del mismo owner.
    pub(super) fn probar_replay_admitido_root_coincidente<'owner>(
        &'owner mut self,
    ) -> Result<PruebaReplayAdmitidoRootCoincidente<'owner, 'lock>, ErrorPruebaReplayRootCoincidente>
    {
        let siblings = self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        if siblings.journal() != PresenciaSiblingNuevoFlujo::Presente
            || siblings.journal_temporal() != PresenciaSiblingNuevoFlujo::Ausente
            || siblings.contexto() != PresenciaSiblingNuevoFlujo::Presente
            || siblings.contexto_temporal() != PresenciaSiblingNuevoFlujo::Ausente
        {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let (identidad_archivo_journal, journal) = {
            let lease = self
                .journal_temporal()
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
            lease
                .cargar_canonico()
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?
                .ok_or(ErrorPruebaReplayRootCoincidente)?
        };
        let estado = journal.state();
        if !matches!(estado, JournalState::Installed | JournalState::Committed) {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let (envelope, evidencia_contexto) = {
            let lease = self
                .contexto_admision()
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
            lease
                .leer_canonico_con_evidencia()
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?
                .ok_or(ErrorPruebaReplayRootCoincidente)?
        };
        let replay = envelope
            .validar_replay_soportado(&journal)
            .map_err(mapear_error_replay)?;
        let observado = self
            .observar_root_raw()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?
            .ok_or(ErrorPruebaReplayRootCoincidente)?;
        if observado != *replay.new_fingerprint() {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let identidad_parent = self.identidad_parent;
        let scope_parent = self.scope_parent;
        let autoridad = Arc::clone(&self._autoridad);
        Ok(PruebaReplayAdmitidoRootCoincidente {
            _owner: self,
            _autoridad: autoridad,
            identidad_parent,
            scope_parent,
            identidad_archivo_journal,
            journal,
            evidencia_contexto,
        })
    }
}

fn mapear_error_replay(_error: ErrorEnvelopeAdmision) -> ErrorPruebaReplayRootCoincidente {
    ErrorPruebaReplayRootCoincidente
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "prueba_root_coincidente_tests.rs"]
mod tests;
