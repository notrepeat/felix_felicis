//! Selección de recovery bajo el owner único; huérfanos conocidos fallan cerrado.
use super::super::observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo::{Ausente, Presente};
use super::*;
use super::{forward::ErrorConfirmacionV2, rollback::ErrorRollbackV2};
use crate::transaccion::recuperacion::{ErrorRecuperacion, reanudar_retiro_v3};

pub(crate) struct ErrorRecoveryPendienteV2 {
    _evidencia: Option<Evidencia>,
}
enum Evidencia {
    Forward(Box<ErrorConfirmacionV2>),
    Rollback(Box<ErrorRollbackV2>),
    Retiro(ErrorRecuperacion),
}
impl ErrorRecoveryPendienteV2 {
    fn nuevo() -> Self {
        Self { _evidencia: None }
    }
}
impl std::fmt::Debug for ErrorRecoveryPendienteV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("recovery_pending_unverified")
    }
}
impl DuenioTransaccionAnclado<'_> {
    pub(crate) fn recuperar_pendiente_v2(&mut self) -> Result<(), ErrorRecoveryPendienteV2> {
        self.parent_revalidado()
            .map_err(|_| ErrorRecoveryPendienteV2::nuevo())?;
        let result = self.recuperar_pendiente_v2_interno();
        if self.parent_revalidado().is_err() {
            // Un error ya propietario nunca se reemplaza por un error vacío.
            return Err(result.err().unwrap_or_else(ErrorRecoveryPendienteV2::nuevo));
        }
        result
    }
    fn recuperar_pendiente_v2_interno(&mut self) -> Result<(), ErrorRecoveryPendienteV2> {
        let slots = self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorRecoveryPendienteV2::nuevo())?;
        if slots.contexto_temporal() != Ausente || slots.intent_retiro_temporal() != Ausente {
            return Err(ErrorRecoveryPendienteV2::nuevo());
        }
        if slots.intent_retiro() == Presente {
            return reanudar_retiro_v3(self).map_err(|error| ErrorRecoveryPendienteV2 {
                _evidencia: Some(Evidencia::Retiro(error)),
            });
        }
        if slots.journal() == Presente {
            if slots.contexto() == Ausente {
                return self
                    .reanudar_rollback_v2()
                    .map_err(|error| ErrorRecoveryPendienteV2 {
                        _evidencia: Some(Evidencia::Rollback(Box::new(error))),
                    });
            }
            self.reconciliar_sucesor_pendiente_v2()
                .map_err(|_| ErrorRecoveryPendienteV2::nuevo())?;
            let observed = self
                .observar_recovery_normal_v2()
                .map_err(|_| ErrorRecoveryPendienteV2::nuevo())?;
            let action = observed.accion;
            drop(observed); // Los runners vuelven a observar antes de mutar.
            return match action {
                AccionRecoveryV2::PersistirCommitted
                | AccionRecoveryV2::LimpiarBackupCommitted
                | AccionRecoveryV2::PublicarIntentRetiro => self
                    .reanudar_confirmacion_v2()
                    .map_err(|error| ErrorRecoveryPendienteV2 {
                        _evidencia: Some(Evidencia::Forward(Box::new(error))),
                    }),
                AccionRecoveryV2::LimpiarStageYRevertir
                | AccionRecoveryV2::RestaurarOld
                | AccionRecoveryV2::DescartarNewYRestaurarOld => self
                    .reanudar_rollback_v2()
                    .map_err(|error| ErrorRecoveryPendienteV2 {
                        _evidencia: Some(Evidencia::Rollback(Box::new(error))),
                    }),
                AccionRecoveryV2::FailClosed => Err(ErrorRecoveryPendienteV2::nuevo()),
            };
        }
        if slots.contexto() != Ausente || slots.journal_temporal() != Ausente {
            return Err(ErrorRecoveryPendienteV2::nuevo());
        }
        self.exigir_stages_ausentes(None)
            .map_err(|_| ErrorRecoveryPendienteV2::nuevo())?;
        let backup = self._lock.ubicacion().backup();
        let name = backup
            .as_path()
            .file_name()
            .ok_or_else(ErrorRecoveryPendienteV2::nuevo)?;
        if self
            .abrir_sibling_lectura(name)
            .map_err(|_| ErrorRecoveryPendienteV2::nuevo())?
            .is_some()
            || self
                .abrir_root_lectura()
                .map_err(|_| ErrorRecoveryPendienteV2::nuevo())?
                .is_none()
        {
            return Err(ErrorRecoveryPendienteV2::nuevo());
        }
        Ok(())
    }
}
