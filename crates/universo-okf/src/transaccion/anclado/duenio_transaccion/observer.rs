//! Observador privado del journal y sus siblings bajo el parent retenido.

use std::ffi::OsStr;

use crate::transaccion::inventario::{LimitesInventario, inventariar_root_anclado_con_limites};
use crate::transaccion::journal::recovery::{
    ObservedArtifact, RecoveryInventory, classify_artifact, classify_backup_artifact,
    classify_stage_artifact,
};
use crate::transaccion::journal::{ArtifactFingerprint, Journal};
use crate::transaccion::snapshot::calcular_digest_raw_tree;

use super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado};

impl DuenioTransaccionAnclado<'_> {
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
        let parent_esperado = self
            .identidad_parent
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

        Ok((journal, inventario))
    }

    fn observar_sibling(
        &self,
        nombre: &OsStr,
    ) -> Result<Option<ObservedArtifact>, ErrorDuenioTransaccionAnclado> {
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
        Ok(Some(ObservedArtifact::new(ArtifactFingerprint::new(
            identidad, digest,
        ))))
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "observer_tests.rs"]
mod tests;

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "observer_storage_edges_tests.rs"]
mod storage_edges_tests;
