//! Cleanup de Old exclusivamente después de Committed v2 y replay New.
use super::{
    DuenioTransaccionAnclado,
    observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo as Presencia,
};
use crate::transaccion::{
    admision::envelope::selector::EnvelopeAdmisionVersionada,
    anclado::{
        DirectorioAnclado, ErrorDirectorioAnclado, IdentidadDirectorio, ScopeFilesystem,
        contexto_admision::ObservacionContextoVersionada,
        journal_temporal::versionado::ObservacionJournalPersistido, scope_filesystem,
    },
    inventario::LimitesInventario,
    journal::{JournalState, selector::JournalPersistido, v2::JournalV2},
};
use fs_at::OpenOptions as OpenOptionsAt;
use std::{ffi::OsStr, fs::File, path::Path};

struct BackupCommittedV2 {
    directorio: DirectorioAnclado,
    identidad: IdentidadDirectorio,
}
struct EvidenciaCommitted {
    journal: ObservacionJournalPersistido,
    contexto: ObservacionContextoVersionada,
}
impl EvidenciaCommitted {
    fn coincide(&self, other: &Self) -> bool {
        self.journal.journal() == other.journal.journal()
            && self.journal.identidad().identidad_archivo()
                == other.journal.identidad().identidad_archivo()
            && self.contexto.evidencia_raw_retiro() == other.contexto.evidencia_raw_retiro()
    }
}
/// El error conserva cualquier capability y las lecturas que autorizaron el intento.
pub(crate) struct ErrorLimpiezaBackupV2 {
    backup: Option<Box<BackupCommittedV2>>,
    evidencia: Option<Box<EvidenciaCommitted>>,
}
impl ErrorLimpiezaBackupV2 {
    fn nuevo() -> Self {
        Self {
            backup: None,
            evidencia: None,
        }
    }
}
impl std::fmt::Debug for ErrorLimpiezaBackupV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("backup_cleanup_unverified")
    }
}
impl DuenioTransaccionAnclado<'_> {
    pub(crate) fn limpiar_backup_committed_v2(
        &mut self,
        journal: &JournalV2,
    ) -> Result<(), ErrorLimpiezaBackupV2> {
        self.limpiar_backup_committed_v2_con(
            journal,
            || {},
            scope_filesystem,
            |parent, nombre, directory| {
                let options = OpenOptionsAt::default();
                if directory {
                    options.rmdir_at(parent, Path::new(nombre))
                } else {
                    options.unlink_at(parent, Path::new(nombre))
                }
                .map_err(|_| ErrorDirectorioAnclado::Io)
            },
        )
    }
    fn limpiar_backup_committed_v2_con<H, F, S>(
        &mut self,
        journal: &JournalV2,
        antes_cleanup: H,
        mut scope: S,
        mut eliminar: F,
    ) -> Result<(), ErrorLimpiezaBackupV2>
    where
        H: FnOnce(),
        F: FnMut(&File, &OsStr, bool) -> Result<(), ErrorDirectorioAnclado>,
        S: FnMut(&File) -> Result<ScopeFilesystem, ErrorDirectorioAnclado>,
    {
        let evidencia = self.evidencia_cleanup_backup_v2(journal)?;
        let ruta = self._lock.ubicacion().backup();
        let nombre = ruta
            .as_path()
            .file_name()
            .ok_or_else(ErrorLimpiezaBackupV2::nuevo)?;
        let backup = self
            .abrir_sibling_lectura(nombre)
            .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?;
        let Some(directorio) = backup else {
            let final_state = self.evidencia_cleanup_backup_v2(journal)?;
            if !evidencia.coincide(&final_state) {
                return Err(ErrorLimpiezaBackupV2::nuevo());
            }
            self.parent_revalidado()
                .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?;
            return Ok(());
        };
        let identidad = directorio
            .identidad_raiz
            .ok_or_else(ErrorLimpiezaBackupV2::nuevo)?;
        if identidad.journal_identity().as_ref() != Some(journal.old().identity()) {
            return Err(ErrorLimpiezaBackupV2::nuevo());
        }
        let backup = BackupCommittedV2 {
            directorio,
            identidad,
        };
        antes_cleanup();
        let result = (|| {
            let before = self.evidencia_cleanup_backup_v2(journal)?;
            if !evidencia.coincide(&before) {
                return Err(ErrorLimpiezaBackupV2::nuevo());
            }
            // El walker repite scope e identidad por entrada y antes del unlink.
            // El owner se verifica además inmediatamente antes de cada syscall.
            super::super::limpieza::limpiar_directorio_identificado(
                &backup.directorio,
                nombre,
                backup.identidad,
                LimitesInventario::CONTRATO,
                &mut scope,
                &mut |parent, name, directory| {
                    self.parent_revalidado()
                        .map_err(|_| ErrorDirectorioAnclado::Io)?;
                    eliminar(parent, name, directory)
                },
            )
            .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?;
            let after = self.evidencia_cleanup_backup_v2(journal)?;
            if !evidencia.coincide(&after)
                || self
                    .abrir_sibling_lectura(nombre)
                    .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?
                    .is_some()
            {
                return Err(ErrorLimpiezaBackupV2::nuevo());
            }
            let parent = self
                .parent_revalidado()
                .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?;
            crate::transaccion::durabilidad::sincronizar_directorio(
                &parent,
                crate::transaccion::durabilidad::Punto::BackupEliminado,
            )
            .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?;
            Ok(())
        })();
        let result = if self.parent_revalidado().is_err() {
            Err(ErrorLimpiezaBackupV2::nuevo())
        } else {
            result
        };
        result.map_err(|mut error| {
            error.backup = Some(Box::new(backup));
            error.evidencia = Some(Box::new(evidencia));
            error
        })
    }
    fn evidencia_cleanup_backup_v2(
        &mut self,
        esperado: &JournalV2,
    ) -> Result<EvidenciaCommitted, ErrorLimpiezaBackupV2> {
        if esperado.state() != JournalState::Committed
            || esperado.old().identity() == esperado.new_fingerprint().identity()
        {
            return Err(ErrorLimpiezaBackupV2::nuevo());
        }
        let slots = self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?;
        if slots.journal() != Presencia::Presente
            || slots.contexto() != Presencia::Presente
            || [
                slots.journal_temporal(),
                slots.contexto_temporal(),
                slots.intent_retiro(),
                slots.intent_retiro_temporal(),
            ]
            .iter()
            .any(|p| *p != Presencia::Ausente)
        {
            return Err(ErrorLimpiezaBackupV2::nuevo());
        }
        let journal = self
            .observar_journal_versionado()
            .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?
            .ok_or_else(ErrorLimpiezaBackupV2::nuevo)?;
        if journal.journal() != &JournalPersistido::V2(esperado.clone()) {
            return Err(ErrorLimpiezaBackupV2::nuevo());
        }
        let contexto = self
            .observar_contexto_versionado()
            .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?
            .ok_or_else(ErrorLimpiezaBackupV2::nuevo)?;
        let EnvelopeAdmisionVersionada::V2(envelope) = contexto.envelope_versionado() else {
            return Err(ErrorLimpiezaBackupV2::nuevo());
        };
        let replay = envelope
            .calcular_new_esperado(esperado)
            .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?;
        let raw = self
            .observar_siblings_recovery_v2_raw(esperado)
            .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?;
        if raw.root() != Some(esperado.new_fingerprint())
            || crate::transaccion::journal::ArtifactFingerprint::new(
                esperado.new_fingerprint().identity().clone(),
                replay.digest().clone(),
            ) != *esperado.new_fingerprint()
            || raw.stage().is_some()
            || raw
                .backup()
                .is_some_and(|b| b.identity() != esperado.old().identity())
        {
            return Err(ErrorLimpiezaBackupV2::nuevo());
        }
        self.parent_revalidado()
            .map_err(|_| ErrorLimpiezaBackupV2::nuevo())?;
        Ok(EvidenciaCommitted { journal, contexto })
    }
}

#[cfg(test)]
#[path = "limpiar_backup_committed_v2_tests.rs"]
mod tests;
