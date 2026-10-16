//! Prueba opaca de replay soportado y root observado coincidente.

use std::fmt;
use std::sync::Arc;

use crate::transaccion::admision::envelope::ErrorEnvelopeAdmision;
use crate::transaccion::anclado::contexto_admision::EvidenciaLecturaContexto;
use crate::transaccion::anclado::journal_temporal::IdentidadArchivoJournal;
use crate::transaccion::anclado::{IdentidadDirectorio, ScopeFilesystem};
use crate::transaccion::journal::recovery::{
    ArtifactState, RecoveryDecision, RecoveryInventory, RecoveryStep, decide_recovery,
};
use crate::transaccion::journal::{Journal, JournalState};

use super::super::archivo_temporal::DatosArchivoTemporal;
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
            || siblings.intent_retiro() != PresenciaSiblingNuevoFlujo::Ausente
            || siblings.intent_retiro_temporal() != PresenciaSiblingNuevoFlujo::Ausente
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

impl PruebaReplayAdmitidoRootCoincidente<'_, '_> {
    /// Consume la prueba anterior y la reemite solo si persisten los datos
    /// exactos que la sustentaron bajo el mismo owner y guard.
    pub(super) fn revalidar(self) -> Result<Self, ErrorPruebaReplayRootCoincidente> {
        let Self {
            _owner,
            _autoridad,
            identidad_parent,
            scope_parent,
            identidad_archivo_journal,
            journal,
            evidencia_contexto,
        } = self;

        if !Arc::ptr_eq(&_autoridad, &_owner._autoridad)
            || identidad_parent != _owner.identidad_parent
            || scope_parent != _owner.scope_parent
        {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let siblings = _owner
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        if siblings.journal() != PresenciaSiblingNuevoFlujo::Presente
            || siblings.journal_temporal() != PresenciaSiblingNuevoFlujo::Ausente
            || siblings.contexto() != PresenciaSiblingNuevoFlujo::Presente
            || siblings.contexto_temporal() != PresenciaSiblingNuevoFlujo::Ausente
            || siblings.intent_retiro() != PresenciaSiblingNuevoFlujo::Ausente
            || siblings.intent_retiro_temporal() != PresenciaSiblingNuevoFlujo::Ausente
        {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let (identidad_journal_actual, journal_actual) = {
            let lease = _owner
                .journal_temporal()
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
            lease
                .cargar_canonico()
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?
                .ok_or(ErrorPruebaReplayRootCoincidente)?
        };
        if identidad_journal_actual != identidad_archivo_journal || journal_actual != journal {
            return Err(ErrorPruebaReplayRootCoincidente);
        }
        if !matches!(
            journal_actual.state(),
            JournalState::Installed | JournalState::Committed
        ) {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let (envelope, evidencia_actual) = {
            let lease = _owner
                .contexto_admision()
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
            lease
                .leer_canonico_con_evidencia()
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?
                .ok_or(ErrorPruebaReplayRootCoincidente)?
        };
        if evidencia_actual.identidad_archivo != evidencia_contexto.identidad_archivo
            || evidencia_actual.longitud != evidencia_contexto.longitud
            || evidencia_actual.sha256 != evidencia_contexto.sha256
        {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let replay = envelope
            .validar_replay_soportado(&journal_actual)
            .map_err(mapear_error_replay)?;
        let root_observado = _owner
            .observar_root_raw()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?
            .ok_or(ErrorPruebaReplayRootCoincidente)?;
        if root_observado != *replay.new_fingerprint() {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        Ok(Self {
            _owner,
            _autoridad,
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

#[path = "prueba_inicio_retiro.rs"]
mod prueba_inicio_retiro;

/// Estado intermedio tras crear, escribir y sincronizar el temporal.
/// Todavía no demuestra que el nombre visible conserve estos bytes.
pub(super) struct IntentTemporalEscrito<'owner, 'lock> {
    _inicio: prueba_inicio_retiro::PruebaInicioRetiroContexto<'owner, 'lock>,
    datos: DatosArchivoTemporal,
    frame_esperado: Vec<u8>,
}

impl fmt::Debug for IntentTemporalEscrito<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IntentTemporalEscrito { .. }")
    }
}

/// Observación privada y secuencial de la fila que conservaría `New`.
/// No ejecuta la decisión ni concede una capability de mutación.
pub(super) struct ObservacionConservarNew<'owner, 'lock> {
    prueba: PruebaReplayAdmitidoRootCoincidente<'owner, 'lock>,
    decision: RecoveryDecision,
    pasos: &'static [RecoveryStep],
}

impl fmt::Debug for ObservacionConservarNew<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionConservarNew { .. }")
    }
}

impl<'owner, 'lock> PruebaReplayAdmitidoRootCoincidente<'owner, 'lock> {
    /// Revalida la prueba y deriva la fila de conservación solo desde la
    /// observación actual del journal y sus siblings bajo el mismo owner.
    pub(super) fn observar_fila_conservar_new(
        self,
    ) -> Result<ObservacionConservarNew<'owner, 'lock>, ErrorPruebaReplayRootCoincidente> {
        let prueba = self.revalidar()?;
        let (journal_observado, inventario) = prueba
            ._owner
            .observar_inventario_recovery_anclado()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        if journal_observado != prueba.journal || inventario.root_new_validated {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let fase = journal_observado.state();
        if !matches!(fase, JournalState::Installed | JournalState::Committed)
            || inventario.root != ArtifactState::New
            || inventario.stage != ArtifactState::Absent
            || !matches!(
                inventario.backup,
                ArtifactState::Absent | ArtifactState::Old
            )
        {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let inventario_validado = RecoveryInventory {
            root_new_validated: true,
            ..inventario
        };
        let (decision_esperada, pasos_esperados): (RecoveryDecision, &'static [RecoveryStep]) =
            match fase {
                JournalState::Installed => (
                    RecoveryDecision::KeepValidatedNewAndCommit,
                    &[
                        RecoveryStep::RemoveKnownStage,
                        RecoveryStep::RemoveKnownBackup,
                        RecoveryStep::PersistCommitted,
                        RecoveryStep::RemoveJournal,
                    ],
                ),
                JournalState::Committed => (
                    RecoveryDecision::KeepValidatedCommittedNew,
                    &[
                        RecoveryStep::RemoveKnownStage,
                        RecoveryStep::RemoveKnownBackup,
                        RecoveryStep::RemoveJournal,
                    ],
                ),
                _ => return Err(ErrorPruebaReplayRootCoincidente),
            };
        let decision = decide_recovery(fase, inventario_validado);
        let pasos = decision.steps(inventario_validado);
        if decision != decision_esperada || pasos != pasos_esperados {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        Ok(ObservacionConservarNew {
            prueba,
            decision,
            pasos,
        })
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "prueba_root_coincidente_tests.rs"]
mod tests;
