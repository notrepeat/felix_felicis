//! Rollback V2 acreditado por pareja persistida, inventario e identidades.
use super::terminal::ErrorTerminalRollbackV2;
use super::*;
use crate::transaccion::anclado::{DirectorioAnclado, ErrorDirectorioAnclado, scope_filesystem};
use crate::transaccion::inventario::LimitesInventario;
use fs_at::OpenOptions as OpenOptionsAt;
use std::path::Path;

type VinculoRollbackV2 = (
    crate::transaccion::anclado::IdentidadArchivo,
    (crate::transaccion::anclado::IdentidadArchivo, u64, [u8; 32]),
);

pub(crate) struct ErrorRollbackV2 {
    normal: Option<Box<ObservacionRecoveryV2>>,
    directorio: Option<Box<DirectorioAnclado>>,
    terminal: Option<ErrorTerminalRollbackV2>,
}
impl ErrorRollbackV2 {
    fn nuevo() -> Self {
        Self {
            normal: None,
            directorio: None,
            terminal: None,
        }
    }
}
impl std::fmt::Debug for ErrorRollbackV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("recovery_v2_rollback_unverified")
    }
}
impl DuenioTransaccionAnclado<'_> {
    pub(crate) fn reanudar_rollback_v2(&mut self) -> Result<(), ErrorRollbackV2> {
        use super::super::observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo::Ausente;
        if self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorRollbackV2::nuevo())?
            .contexto()
            == Ausente
        {
            return self.finalizar_rollback_v2();
        }
        self.reconciliar_sucesor_pendiente_v2()
            .map_err(|_| ErrorRollbackV2::nuevo())?;
        let vinculo = self.vinculo_rollback_v2()?;
        let mut anterior: Option<Box<ObservacionRecoveryV2>> = None;
        for _ in 0..3 {
            let observed = self.observar_recovery_normal_v2().map_err(|_| {
                let mut error = ErrorRollbackV2::nuevo();
                error.normal = anterior.take();
                error
            })?;
            if anterior
                .as_ref()
                .is_some_and(|old| old.journal != observed.journal)
            {
                let mut error = ErrorRollbackV2::nuevo();
                error.normal = anterior;
                return Err(error);
            }
            if !matches!(self.vinculo_rollback_v2(), Ok(current) if current == vinculo) {
                let mut error = ErrorRollbackV2::nuevo();
                error.normal = Some(Box::new(observed));
                return Err(error);
            }
            let result = match observed.accion {
                AccionRecoveryV2::LimpiarStageYRevertir => {
                    if let Err(mut error) = self.limpiar_stage_rollback_v2(&observed.journal) {
                        error.normal = Some(Box::new(observed));
                        return Err(error);
                    }
                    // La fila terminal vuelve a leer los datos; los lectores
                    // anteriores se cierran antes de retirar sidecars Windows.
                    drop(observed);
                    drop(anterior);
                    return self.finalizar_rollback_v2();
                }
                AccionRecoveryV2::RestaurarOld => self
                    .limpiar_stage_rollback_v2(&observed.journal)
                    .and_then(|()| self.mover_rollback_v2(&observed.journal, false)),
                AccionRecoveryV2::DescartarNewYRestaurarOld => {
                    self.mover_rollback_v2(&observed.journal, true)
                }
                _ => Err(ErrorRollbackV2::nuevo()),
            };
            if let Err(mut error) = result {
                error.normal = Some(Box::new(observed));
                return Err(error);
            }
            anterior = Some(Box::new(observed));
        }
        let mut error = ErrorRollbackV2::nuevo();
        error.normal = anterior;
        Err(error)
    }
    fn vinculo_rollback_v2(&mut self) -> Result<VinculoRollbackV2, ErrorRollbackV2> {
        let journal = self
            .observar_journal_versionado()
            .map_err(|_| ErrorRollbackV2::nuevo())?
            .ok_or_else(ErrorRollbackV2::nuevo)?;
        let contexto = self
            .observar_contexto_versionado()
            .map_err(|_| ErrorRollbackV2::nuevo())?
            .ok_or_else(ErrorRollbackV2::nuevo)?;
        Ok((
            journal.identidad().identidad_archivo(),
            contexto.evidencia_raw_retiro(),
        ))
    }
    fn finalizar_rollback_v2(&mut self) -> Result<(), ErrorRollbackV2> {
        self.retirar_terminal_rollback_v2().map_err(|error| {
            let mut out = ErrorRollbackV2::nuevo();
            out.terminal = Some(error);
            out
        })
    }
    fn mover_rollback_v2(
        &mut self,
        journal: &JournalV2,
        discard_new: bool,
    ) -> Result<(), ErrorRollbackV2> {
        let fresh = self
            .observar_recovery_normal_v2()
            .map_err(|_| ErrorRollbackV2::nuevo())?;
        let expected = if discard_new {
            AccionRecoveryV2::DescartarNewYRestaurarOld
        } else {
            AccionRecoveryV2::RestaurarOld
        };
        if fresh.journal != *journal || fresh.accion != expected {
            return Err(ErrorRollbackV2::nuevo());
        }
        let location = self._lock.ubicacion().clone();
        let root = location
            .ruta()
            .file_name()
            .ok_or_else(ErrorRollbackV2::nuevo)?;
        let backup = location
            .backup()
            .as_path()
            .file_name()
            .ok_or_else(ErrorRollbackV2::nuevo)?
            .to_os_string();
        let stage = location
            .stage(journal.transaction_id())
            .map_err(|_| ErrorRollbackV2::nuevo())?
            .as_path()
            .file_name()
            .ok_or_else(ErrorRollbackV2::nuevo)?
            .to_os_string();
        let (source, target, identity) = if discard_new {
            (
                root,
                stage.as_os_str(),
                journal.new_fingerprint().identity(),
            )
        } else {
            (backup.as_os_str(), root, journal.old().identity())
        };
        self.mover_sibling_v2(source, target, identity)
            .map_err(|_| ErrorRollbackV2::nuevo())?;
        let after = self
            .observar_recovery_normal_v2()
            .map_err(|_| ErrorRollbackV2::nuevo())?;
        let expected = if discard_new {
            AccionRecoveryV2::RestaurarOld
        } else {
            AccionRecoveryV2::LimpiarStageYRevertir
        };
        if after.journal != *journal || after.accion != expected {
            return Err(ErrorRollbackV2::nuevo());
        }
        Ok(())
    }
    fn limpiar_stage_rollback_v2(&mut self, journal: &JournalV2) -> Result<(), ErrorRollbackV2> {
        let before = self
            .observar_recovery_normal_v2()
            .map_err(|_| ErrorRollbackV2::nuevo())?;
        if before.journal != *journal
            || !matches!(
                before.accion,
                AccionRecoveryV2::LimpiarStageYRevertir | AccionRecoveryV2::RestaurarOld
            )
        {
            return Err(ErrorRollbackV2::nuevo());
        }
        let path = self
            ._lock
            .ubicacion()
            .stage(journal.transaction_id())
            .map_err(|_| ErrorRollbackV2::nuevo())?;
        let name = path
            .as_path()
            .file_name()
            .ok_or_else(ErrorRollbackV2::nuevo)?;
        let Some(directory) = self
            .abrir_sibling_lectura(name)
            .map_err(|_| ErrorRollbackV2::nuevo())?
        else {
            return Ok(());
        };
        let identity = directory
            .identidad_raiz
            .ok_or_else(ErrorRollbackV2::nuevo)?;
        if identity.journal_identity().as_ref() != Some(journal.new_fingerprint().identity()) {
            return Err(ErrorRollbackV2::nuevo());
        }
        let result = (|| {
            let fresh = self
                .observar_recovery_normal_v2()
                .map_err(|_| ErrorRollbackV2::nuevo())?;
            if fresh.journal != *journal || fresh.accion != before.accion {
                return Err(ErrorRollbackV2::nuevo());
            }
            crate::transaccion::anclado::limpieza::limpiar_directorio_identificado(
                &directory,
                name,
                identity,
                LimitesInventario::CONTRATO,
                &mut scope_filesystem,
                &mut |parent, name, is_dir| {
                    self.parent_revalidado()
                        .map_err(|_| ErrorDirectorioAnclado::Io)?;
                    let options = OpenOptionsAt::default();
                    if is_dir {
                        options.rmdir_at(parent, Path::new(name))
                    } else {
                        options.unlink_at(parent, Path::new(name))
                    }
                    .map_err(|_| ErrorDirectorioAnclado::Io)
                },
            )
            .map_err(|_| ErrorRollbackV2::nuevo())?;
            let after = self
                .observar_recovery_normal_v2()
                .map_err(|_| ErrorRollbackV2::nuevo())?;
            if after.journal != *journal
                || after.accion != before.accion
                || after.inventario.stage != ArtifactState::Absent
            {
                return Err(ErrorRollbackV2::nuevo());
            }
            Ok(())
        })();
        let result = if self.parent_revalidado().is_err() {
            Err(ErrorRollbackV2::nuevo())
        } else {
            result
        };
        result.map_err(|mut error| {
            error.directorio = Some(Box::new(directory));
            error
        })
    }
}
