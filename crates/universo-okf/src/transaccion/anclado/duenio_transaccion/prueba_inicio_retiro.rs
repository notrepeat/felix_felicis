//! Prueba fresca y estricta para comenzar el retiro experimental.

use std::fmt;

use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::recovery::ArtifactState;

use super::{ErrorPruebaReplayRootCoincidente, PruebaReplayAdmitidoRootCoincidente};

/// Retiene la prueba validada y su mismo owner/lock; no autoriza borrados.
pub(super) struct PruebaInicioRetiroContexto<'owner, 'lock> {
    _prueba: PruebaReplayAdmitidoRootCoincidente<'owner, 'lock>,
}

impl fmt::Debug for PruebaInicioRetiroContexto<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PruebaInicioRetiroContexto { .. }")
    }
}

impl<'owner, 'lock> PruebaReplayAdmitidoRootCoincidente<'owner, 'lock> {
    /// Revalida la prueba y requiere la fila Committed con root New, sin
    /// backup ni stage. No crea archivos ni devuelve un permiso de cleanup.
    pub(super) fn preparar_retiro_contexto(
        self,
    ) -> Result<PruebaInicioRetiroContexto<'owner, 'lock>, ErrorPruebaReplayRootCoincidente> {
        let prueba = self.revalidar()?;
        if prueba.journal.state() != JournalState::Committed {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let (journal_observado, inventario) = prueba
            ._owner
            .observar_inventario_recovery_anclado()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        if journal_observado != prueba.journal
            || inventario.root_new_validated
            || inventario.root != ArtifactState::New
            || inventario.backup != ArtifactState::Absent
            || inventario.stage != ArtifactState::Absent
        {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        Ok(PruebaInicioRetiroContexto { _prueba: prueba })
    }
}
