//! Read-only inventory observation for journal v1 recovery.

use std::ffi::OsStr;
use std::fmt;

use crate::transaccion::anclado::{DirectorioAnclado, ErrorDirectorioAnclado};
use crate::transaccion::inventario::{LimitesInventario, inventariar_root_anclado_con_limites};
use crate::transaccion::journal::{ArtifactFingerprint, Journal};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use crate::transaccion::snapshot::{ErrorSnapshot, calcular_digest_raw_tree};

use super::{ObservedArtifact, RecoveryInventory, classify_artifact};

#[cfg(test)]
use crate::transaccion::inventario::inventariar_root_anclado;

/// Redacted failure while observing a recovery inventory under the writer lock.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorObservadorRecovery {
    Io,
    ParentIdentityUnavailable,
    ParentIdentityMismatch,
    InvalidStageName,
    Snapshot(ErrorSnapshot),
}

impl ErrorObservadorRecovery {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::Io => "recovery_observation_io",
            Self::ParentIdentityUnavailable => "recovery_parent_identity_unavailable",
            Self::ParentIdentityMismatch => "recovery_parent_identity_mismatch",
            Self::InvalidStageName => "recovery_stage_name_invalid",
            Self::Snapshot(error) => error.codigo(),
        }
    }
}

impl fmt::Debug for ErrorObservadorRecovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorObservadorRecovery")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorObservadorRecovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorObservadorRecovery {}

/// Opens and fingerprints root, backup, and this journal's stage under an
/// already-held writer lock. It never decides recovery or mutates an artifact.
/// Artifact scans are sequential because anchored directory clones may share
/// enumeration cursors.
pub(crate) fn observar_inventario_recovery(
    lock: &LockEscritura,
    journal: &Journal,
) -> Result<RecoveryInventory, ErrorObservadorRecovery> {
    observar_inventario_recovery_con_limites(lock, journal, LimitesInventario::CONTRATO)
}

fn observar_inventario_recovery_con_limites(
    lock: &LockEscritura,
    journal: &Journal,
    limites: LimitesInventario,
) -> Result<RecoveryInventory, ErrorObservadorRecovery> {
    let ubicacion = lock.ubicacion();
    let parent = DirectorioAnclado::parent_solo_lectura_desde_ubicacion(ubicacion)
        .map_err(|_| ErrorObservadorRecovery::Io)?;
    let identidad_parent = parent
        .identidad_parent()
        .journal_identity()
        .ok_or(ErrorObservadorRecovery::ParentIdentityUnavailable)?;
    if journal.parent() != &identidad_parent {
        return Err(ErrorObservadorRecovery::ParentIdentityMismatch);
    }

    let root_name = ubicacion
        .ruta()
        .file_name()
        .ok_or(ErrorObservadorRecovery::Io)?;
    let backup = ubicacion.backup();
    let backup_name = backup
        .as_path()
        .file_name()
        .ok_or(ErrorObservadorRecovery::Io)?;
    let stage = ubicacion
        .stage(journal.transaction_id())
        .map_err(|_| ErrorObservadorRecovery::InvalidStageName)?;
    let stage_name = stage
        .as_path()
        .file_name()
        .ok_or(ErrorObservadorRecovery::InvalidStageName)?;

    let root = observar_artifacto(&parent, ubicacion, root_name, limites)?;
    let backup = observar_artifacto(&parent, ubicacion, backup_name, limites)?;
    let stage = observar_artifacto(&parent, ubicacion, stage_name, limites)?;

    Ok(RecoveryInventory {
        root: classify_artifact(root.as_ref(), journal.old(), journal.new_fingerprint()),
        backup: classify_artifact(backup.as_ref(), journal.old(), journal.new_fingerprint()),
        stage: classify_artifact(stage.as_ref(), journal.old(), journal.new_fingerprint()),
        root_new_validated: false,
    })
}

fn observar_artifacto(
    parent: &DirectorioAnclado,
    ubicacion: &UbicacionRaiz,
    nombre: &OsStr,
    limites: LimitesInventario,
) -> Result<Option<ObservedArtifact>, ErrorObservadorRecovery> {
    let Some(artifact) = parent
        .abrir_artifacto_sibling_directorio_lectura(ubicacion, nombre)
        .map_err(mapear_error_anclado)?
    else {
        return Ok(None);
    };

    let identidad = artifact
        .identidad_raiz()
        .ok_or(ErrorObservadorRecovery::Io)?
        .journal_identity()
        .ok_or(ErrorObservadorRecovery::ParentIdentityUnavailable)?;
    let entries = inventariar_root_anclado_con_limites(artifact, limites)
        .map_err(ErrorObservadorRecovery::Snapshot)?;
    let digest = calcular_digest_raw_tree(&entries).map_err(ErrorObservadorRecovery::Snapshot)?;
    let fingerprint = ArtifactFingerprint::new(identidad, digest);
    Ok(Some(ObservedArtifact::new(fingerprint)))
}

fn mapear_error_anclado(_error: ErrorDirectorioAnclado) -> ErrorObservadorRecovery {
    ErrorObservadorRecovery::Io
}

#[cfg(test)]
#[path = "observer_tests.rs"]
mod tests;
