//! Confirma Installed, limpia Committed y transfiere al retiro autocontenido.
use super::super::observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo::{Ausente, Presente};
use super::super::{ResultadoOwnerPostcheck, limpiar_backup_committed_v2::ErrorLimpiezaBackupV2};
use super::*;
use crate::transaccion::admision::envelope::selector::EnvelopeAdmisionVersionada;
use crate::transaccion::anclado::journal_temporal::{
    EstadoPublicacionSucesorV2, EvidenciaSucesorJournalV2, versionado::ObservacionJournalPersistido,
};
use crate::transaccion::journal::selector::JournalPersistido;
use crate::transaccion::recuperacion::{ErrorRecuperacion, reanudar_retiro_v3};

/// No libera evidencia de un intento ambiguo ni convierte su error en retry.
pub(crate) struct ErrorConfirmacionV2 {
    normal: Option<Box<ObservacionRecoveryV2>>,
    original: Option<Box<ObservacionJournalPersistido>>,
    sucesor: Option<Box<EvidenciaSucesorJournalV2>>,
    contexto:
        Option<Box<crate::transaccion::anclado::contexto_admision::ObservacionContextoVersionada>>,
    cleanup: Option<Box<ErrorLimpiezaBackupV2>>,
    retiro: Option<ErrorRecuperacion>,
}
impl ErrorConfirmacionV2 {
    fn nuevo() -> Self {
        Self {
            normal: None,
            original: None,
            sucesor: None,
            contexto: None,
            cleanup: None,
            retiro: None,
        }
    }
}
impl std::fmt::Debug for ErrorConfirmacionV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("recovery_v2_confirmation_unverified")
    }
}

impl DuenioTransaccionAnclado<'_> {
    pub(crate) fn reanudar_confirmacion_v2(&mut self) -> Result<(), ErrorConfirmacionV2> {
        self.reconciliar_sucesor_pendiente_v2()
            .map_err(|_| ErrorConfirmacionV2::nuevo())?;
        let mut anterior: Option<Box<ObservacionRecoveryV2>> = None;
        // Una transición por fase. Nunca se reintenta una syscall ambigua.
        for _ in 0..3 {
            let observed = self.observar_recovery_normal_v2().map_err(|_| {
                let mut error = ErrorConfirmacionV2::nuevo();
                error.normal = anterior.take();
                error
            })?;
            if let Some(previous) = &anterior {
                let consistent = if previous.accion == AccionRecoveryV2::PersistirCommitted {
                    previous
                        .journal
                        .validate_successor(&observed.journal)
                        .is_ok()
                } else {
                    previous.journal == observed.journal
                };
                if !consistent {
                    let mut error = ErrorConfirmacionV2::nuevo();
                    error.normal = anterior;
                    return Err(error);
                }
            }
            let result = match observed.accion {
                AccionRecoveryV2::PersistirCommitted => {
                    self.confirmar_committed_v2(&observed.journal)
                }
                AccionRecoveryV2::LimpiarBackupCommitted => self
                    .limpiar_backup_committed_v2(&observed.journal)
                    .map_err(|error| {
                        let mut out = ErrorConfirmacionV2::nuevo();
                        out.cleanup = Some(Box::new(error));
                        out
                    }),
                AccionRecoveryV2::PublicarIntentRetiro => {
                    if self.publicar_intent_v3_desde_committed().is_err() {
                        let mut error = ErrorConfirmacionV2::nuevo();
                        error.normal = Some(Box::new(observed));
                        return Err(error);
                    }
                    // Intent ya contiene el Context exacto. El runner ABC toma
                    // esa evidencia y conserva sus errores; Windows necesita
                    // cerrar los lectores J/C anteriores antes del delete.
                    drop(observed);
                    drop(anterior);
                    return reanudar_retiro_v3(self).map_err(|error| {
                        let mut out = ErrorConfirmacionV2::nuevo();
                        out.retiro = Some(error);
                        out
                    });
                }
                _ => Err(ErrorConfirmacionV2::nuevo()),
            };
            if let Err(mut error) = result {
                error.normal = Some(Box::new(observed));
                return Err(error);
            }
            anterior = Some(Box::new(observed));
        }
        let mut error = ErrorConfirmacionV2::nuevo();
        error.normal = anterior;
        Err(error)
    }

    fn confirmar_committed_v2(&mut self, expected: &JournalV2) -> Result<(), ErrorConfirmacionV2> {
        let original = self
            .observar_journal_versionado()
            .map_err(|_| ErrorConfirmacionV2::nuevo())?
            .ok_or_else(ErrorConfirmacionV2::nuevo)?;
        if original.journal() != &JournalPersistido::V2(expected.clone()) {
            return Err(ErrorConfirmacionV2::nuevo());
        }
        let contexto = self
            .observar_contexto_versionado()
            .map_err(|_| ErrorConfirmacionV2::nuevo())?
            .ok_or_else(ErrorConfirmacionV2::nuevo)?;
        let recibo = match self.preparar_sucesor_journal_v2(&original) {
            ResultadoOwnerPostcheck::PostcheckVerificado { cuerpo: Ok(recibo) } => recibo,
            ResultadoOwnerPostcheck::PostcheckNoVerificado {
                cuerpo: Ok(recibo), ..
            } => {
                let mut error = ErrorConfirmacionV2::nuevo();
                error.sucesor = Some(Box::new(recibo.into_evidencia()));
                error.original = Some(Box::new(original));
                error.contexto = Some(Box::new(contexto));
                return Err(error);
            }
            _ => {
                let mut error = ErrorConfirmacionV2::nuevo();
                error.original = Some(Box::new(original));
                error.contexto = Some(Box::new(contexto));
                return Err(error);
            }
        };
        let result = (|| {
            let slots = self.observar_siblings_nuevo_flujo().map_err(|_| ())?;
            if slots.journal() != Presente
                || slots.contexto() != Presente
                || slots.journal_temporal() != Presente
                || slots.contexto_temporal() != Ausente
                || slots.intent_retiro() != Ausente
                || slots.intent_retiro_temporal() != Ausente
            {
                return Err(());
            }
            let fresh = self
                .observar_contexto_versionado()
                .map_err(|_| ())?
                .ok_or(())?;
            if fresh.evidencia_raw_retiro() != contexto.evidencia_raw_retiro() {
                return Err(());
            }
            let EnvelopeAdmisionVersionada::V2(envelope) = fresh.envelope_versionado() else {
                return Err(());
            };
            let replay = envelope.calcular_new_esperado(expected).map_err(|_| ())?;
            let raw = self
                .observar_siblings_recovery_v2_raw(expected)
                .map_err(|_| ())?;
            let expected_root = ArtifactFingerprint::new(
                expected.new_fingerprint().identity().clone(),
                replay.digest().clone(),
            );
            if expected.state() != JournalState::Installed
                || raw.root() != Some(&expected_root)
                || raw.stage().is_some()
                || (raw.backup().is_some() && raw.backup() != Some(expected.old()))
            {
                return Err(());
            }
            match self.publicar_sucesor_journal_v2(&recibo) {
                ResultadoOwnerPostcheck::PostcheckVerificado {
                    cuerpo: Ok(EstadoPublicacionSucesorV2::Confirmada),
                } => {}
                _ => return Err(()),
            }
            let final_state = self.observar_recovery_normal_v2().map_err(|_| ())?;
            let final_journal = self
                .observar_journal_versionado()
                .map_err(|_| ())?
                .ok_or(())?;
            if final_journal.identidad() != recibo.identidad_temporal()
                || final_journal.journal() != &JournalPersistido::V2(final_state.journal.clone())
            {
                return Err(());
            }

            if expected.validate_successor(&final_state.journal).is_err()
                || final_state.journal.state() != JournalState::Committed
                || !matches!(
                    final_state.accion,
                    AccionRecoveryV2::LimpiarBackupCommitted
                        | AccionRecoveryV2::PublicarIntentRetiro
                )
            {
                return Err(());
            }
            Ok(())
        })();
        let evidence = recibo.into_evidencia();
        let result = if self.parent_revalidado().is_err() {
            Err(())
        } else {
            result
        };
        result.map_err(|()| {
            let mut error = ErrorConfirmacionV2::nuevo();
            error.original = Some(Box::new(original));
            error.contexto = Some(Box::new(contexto));
            error.sucesor = Some(Box::new(evidence));
            error
        })
    }
}
