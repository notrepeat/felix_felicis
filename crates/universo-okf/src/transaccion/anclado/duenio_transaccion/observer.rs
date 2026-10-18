//! Observador privado del journal y sus siblings bajo el parent retenido.

use std::ffi::OsStr;
use std::fmt;

use crate::transaccion::inventario::{LimitesInventario, inventariar_root_anclado_con_limites};
use crate::transaccion::journal::recovery::{
    ObservedArtifact, RecoveryInventory, classify_artifact, classify_backup_artifact,
    classify_stage_artifact,
};
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{ArtifactFingerprint, Journal};
use crate::transaccion::snapshot::calcular_digest_raw_tree;

use super::super::{metadata_directorio, scope_filesystem};
use super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado};

/// Error fijo al observar siblings versionados bajo un owner retenido.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorObservacionSiblingsRecoveryV2 {
    OwnerUnverified,
    ObservationUnverified,
}

impl ErrorObservacionSiblingsRecoveryV2 {
    pub(super) const fn codigo(self) -> &'static str {
        match self {
            Self::OwnerUnverified => "recovery_v2_owner_unverified",
            Self::ObservationUnverified => "recovery_v2_observation_unverified",
        }
    }
}

impl fmt::Debug for ErrorObservacionSiblingsRecoveryV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl fmt::Display for ErrorObservacionSiblingsRecoveryV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorObservacionSiblingsRecoveryV2 {}

/// Conserva la entrada J2 y las tres muestras raw, sin clasificarlas.
#[must_use]
pub(super) struct SiblingsRecoveryV2Raw {
    journal: JournalV2,
    root: Option<ArtifactFingerprint>,
    backup: Option<ArtifactFingerprint>,
    stage: Option<ArtifactFingerprint>,
}

impl SiblingsRecoveryV2Raw {
    pub(super) fn journal(&self) -> &JournalV2 {
        &self.journal
    }

    pub(super) fn root(&self) -> Option<&ArtifactFingerprint> {
        self.root.as_ref()
    }

    pub(super) fn backup(&self) -> Option<&ArtifactFingerprint> {
        self.backup.as_ref()
    }

    pub(super) fn stage(&self) -> Option<&ArtifactFingerprint> {
        self.stage.as_ref()
    }
}

impl fmt::Debug for SiblingsRecoveryV2Raw {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SiblingsRecoveryV2Raw { .. }")
    }
}

impl DuenioTransaccionAnclado<'_> {
    /// Observa los tres nombres derivados de J2, sin clasificar sus muestras.
    pub(super) fn observar_siblings_recovery_v2_raw(
        &mut self,
        journal: &JournalV2,
    ) -> Result<SiblingsRecoveryV2Raw, ErrorObservacionSiblingsRecoveryV2> {
        self.observar_v2_raw_con_postcheck(journal, |owner| {
            owner.parent_revalidado().map_err(|_| ())
        })
    }

    #[cfg(test)]
    pub(super) fn observar_siblings_recovery_v2_raw_con_prueba<P>(
        &mut self,
        journal: &JournalV2,
        postcheck: P,
    ) -> Result<SiblingsRecoveryV2Raw, ErrorObservacionSiblingsRecoveryV2>
    where
        P: FnOnce(&Self) -> Result<std::fs::File, ()>,
    {
        self.observar_v2_raw_con_postcheck(journal, postcheck)
    }

    fn observar_v2_raw_con_postcheck<P>(
        &mut self,
        journal: &JournalV2,
        postcheck: P,
    ) -> Result<SiblingsRecoveryV2Raw, ErrorObservacionSiblingsRecoveryV2>
    where
        P: FnOnce(&Self) -> Result<std::fs::File, ()>,
    {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionSiblingsRecoveryV2::OwnerUnverified)?;
        drop(parent);

        let resultado = self.observar_siblings_recovery_v2_raw_interno(journal);

        let parent =
            postcheck(self).map_err(|_| ErrorObservacionSiblingsRecoveryV2::OwnerUnverified)?;
        drop(parent);
        resultado
    }

    fn observar_siblings_recovery_v2_raw_interno(
        &mut self,
        journal: &JournalV2,
    ) -> Result<SiblingsRecoveryV2Raw, ErrorObservacionSiblingsRecoveryV2> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionSiblingsRecoveryV2::OwnerUnverified)?;
        let identidad_parent = metadata_directorio(&parent)
            .map_err(|_| ErrorObservacionSiblingsRecoveryV2::OwnerUnverified)?;
        let scope_parent = scope_filesystem(&parent)
            .map_err(|_| ErrorObservacionSiblingsRecoveryV2::OwnerUnverified)?;
        if identidad_parent != self.identidad_parent || scope_parent != self.scope_parent {
            return Err(ErrorObservacionSiblingsRecoveryV2::OwnerUnverified);
        }
        let parent_esperado = identidad_parent
            .journal_identity()
            .ok_or(ErrorObservacionSiblingsRecoveryV2::OwnerUnverified)?;
        if journal.parent() != &parent_esperado {
            return Err(ErrorObservacionSiblingsRecoveryV2::ObservationUnverified);
        }
        drop(parent);

        let root = self
            .observar_root_raw()
            .map_err(|_| ErrorObservacionSiblingsRecoveryV2::ObservationUnverified)?;
        let ubicacion = self._lock.ubicacion();
        let ruta_backup = ubicacion.backup();
        let nombre_backup = ruta_backup
            .as_path()
            .file_name()
            .ok_or(ErrorObservacionSiblingsRecoveryV2::ObservationUnverified)?;
        let ruta_stage = ubicacion
            .stage(journal.transaction_id())
            .map_err(|_| ErrorObservacionSiblingsRecoveryV2::ObservationUnverified)?;
        let nombre_stage = ruta_stage
            .as_path()
            .file_name()
            .ok_or(ErrorObservacionSiblingsRecoveryV2::ObservationUnverified)?;
        let backup = self
            .observar_sibling_raw(nombre_backup)
            .map_err(|_| ErrorObservacionSiblingsRecoveryV2::ObservationUnverified)?;
        let stage = self
            .observar_sibling_raw(nombre_stage)
            .map_err(|_| ErrorObservacionSiblingsRecoveryV2::ObservationUnverified)?;

        Ok(SiblingsRecoveryV2Raw {
            journal: journal.clone(),
            root,
            backup,
            stage,
        })
    }

    /// Lee el journal canónico bajo este owner y observa sus tres siblings
    /// desde el parent retenido. El resultado es información, no autoridad.
    pub(crate) fn observar_inventario_recovery_anclado(
        &mut self,
    ) -> Result<(Journal, RecoveryInventory), ErrorDuenioTransaccionAnclado> {
        let journal = {
            let lease = self
                .journal_temporal()
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
            lease
                .cargar_canonico()
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?
                .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?
                .1
        };
        let inventario = self.observar_inventario_recovery_con_journal_datos(&journal)?;
        Ok((journal, inventario))
    }

    /// Observa siblings usando un Journal ya disponible como dato.
    ///
    /// No carga ni requiere el Journal canónico; tampoco produce una prueba de
    /// replay, admisión, cleanup o aplicación.
    pub(super) fn observar_inventario_recovery_con_journal_datos(
        &mut self,
        journal: &Journal,
    ) -> Result<RecoveryInventory, ErrorDuenioTransaccionAnclado> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let identidad_parent = metadata_directorio(&parent)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let scope_parent =
            scope_filesystem(&parent).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        if identidad_parent != self.identidad_parent || scope_parent != self.scope_parent {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }
        let parent_esperado = identidad_parent
            .journal_identity()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        if journal.parent() != &parent_esperado {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        // Keep the root observer's validation path; its opener delegates to
        // the same retained-parent checks as backup and stage.
        let root = self
            .observar_root_raw()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?
            .map(ObservedArtifact::new);
        let ubicacion = self._lock.ubicacion();
        let ruta_backup = ubicacion.backup();
        let nombre_backup = ruta_backup
            .as_path()
            .file_name()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let ruta_stage = ubicacion
            .stage(journal.transaction_id())
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let nombre_stage = ruta_stage
            .as_path()
            .file_name()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;

        // Absence is data. Keep observing the remaining siblings rather than
        // treating the first absent name as a recovery decision.
        let backup = self.observar_sibling(nombre_backup)?;
        let stage = self.observar_sibling(nombre_stage)?;
        let old = journal.old();
        let new = journal.new_fingerprint();
        let inventario = RecoveryInventory {
            root: classify_artifact(root.as_ref(), old, new),
            backup: classify_backup_artifact(backup.as_ref(), old, new),
            stage: classify_stage_artifact(stage.as_ref(), old, new),
            root_new_validated: false,
        };

        Ok(inventario)
    }

    fn observar_sibling(
        &self,
        nombre: &OsStr,
    ) -> Result<Option<ObservedArtifact>, ErrorDuenioTransaccionAnclado> {
        self.observar_sibling_raw(nombre)
            .map(|fingerprint| fingerprint.map(ObservedArtifact::new))
    }

    fn observar_sibling_raw(
        &self,
        nombre: &OsStr,
    ) -> Result<Option<ArtifactFingerprint>, ErrorDuenioTransaccionAnclado> {
        let Some(ancla) = self.abrir_sibling_lectura(nombre)? else {
            return Ok(None);
        };
        let identidad = ancla
            .identidad_raiz()
            .and_then(|identidad| identidad.journal_identity())
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let entradas = inventariar_root_anclado_con_limites(ancla, LimitesInventario::CONTRATO)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let digest = calcular_digest_raw_tree(&entradas)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        Ok(Some(ArtifactFingerprint::new(identidad, digest)))
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "observer_tests.rs"]
mod tests;

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "observer_storage_edges_tests.rs"]
mod storage_edges_tests;

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "observer_journal_datos_tests.rs"]
mod journal_datos_tests;

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "observer_recovery_v2_raw_tests.rs"]
mod recovery_v2_raw_tests;
