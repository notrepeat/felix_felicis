//! Prueba fresca y estricta para comenzar el retiro experimental.

use std::fmt;

use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::recovery::ArtifactState;

use super::{
    ErrorPruebaReplayRootCoincidente, IntentTemporalEscrito, PruebaReplayAdmitidoRootCoincidente,
};

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

impl<'owner, 'lock> PruebaInicioRetiroContexto<'owner, 'lock> {
    /// Crea y sincroniza el temporal exacto. Cualquier fallo posterior a
    /// `create_new` conserva el archivo y no implica que la operación no tuvo efecto.
    pub(super) fn escribir_intent_temporal(
        self,
    ) -> Result<IntentTemporalEscrito<'owner, 'lock>, ErrorPruebaReplayRootCoincidente> {
        if super::super::super::journal_temporal::raiz_es_remota(
            self._prueba._owner._lock.raiz_canonica(),
        ) {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let prueba = self._prueba.preparar_retiro_contexto()?._prueba;
        let nombre_temporal = {
            let ubicacion = prueba._owner._lock.ubicacion();
            let basename = ubicacion
                .ruta()
                .file_name()
                .ok_or(ErrorPruebaReplayRootCoincidente)?;
            let canonico =
                super::super::observar_siblings_nuevo_flujo::nombre_intent_retiro(basename)
                    .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
            super::super::super::contexto_admision::nombre_temporal_sibling(&canonico)
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?
        };

        let parent = prueba
            ._owner
            .parent_revalidado()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        let vacio = super::super::super::archivo_temporal::crear_vacio(&parent, &nombre_temporal)
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        let identidad_intent = vacio.identidad();
        let evidencia = &prueba.evidencia_contexto;
        let frame_esperado = super::super::super::intent_persistido::codificar_nuevo(
            &prueba.journal,
            identidad_intent,
            prueba.identidad_archivo_journal.identidad_archivo(),
            evidencia.identidad_archivo,
            prueba.scope_parent.0,
            evidencia.longitud,
            evidencia.sha256,
        )
        .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        let datos = vacio
            .escribir(&frame_esperado)
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        drop(parent);

        Ok(IntentTemporalEscrito {
            _inicio: PruebaInicioRetiroContexto { _prueba: prueba },
            datos,
            frame_esperado,
        })
    }
}
