//! Observación autocontenida de C; mantiene el owner y no habilita limpieza.
use super::DuenioTransaccionAnclado;
use super::observar_siblings_nuevo_flujo::FormaObservadaSlotsRetiroV2;
use crate::transaccion::anclado::identidad_archivo;
use crate::transaccion::anclado::intent_persistido_v3::archivo::IntentV3EnArchivo;
use crate::transaccion::anclado::intent_persistido_v3::familia::{self, FamiliaV3};
use crate::transaccion::journal::ArtifactFingerprint;

#[derive(Debug)]
pub(super) struct ErrorRetiroCV3;

/// Evidencia secuencial retenida; no snapshot ante escritores externos.
pub(super) struct ObservacionRetiroCV3<'owner, 'lock> {
    owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    observado: IntentV3EnArchivo,
}

impl<'lock> DuenioTransaccionAnclado<'lock> {
    pub(super) fn observar_retiro_c_v3_consistente(
        &mut self,
    ) -> Result<ObservacionRetiroCV3<'_, 'lock>, ErrorRetiroCV3> {
        self.parent_revalidado().map_err(|_| ErrorRetiroCV3)?;
        let resultado = self.observar_retiro_c_v3_datos();
        // La revalidación exterior domina tanto éxito como error.
        self.parent_revalidado().map_err(|_| ErrorRetiroCV3)?;
        let observado = resultado?;
        Ok(ObservacionRetiroCV3 {
            owner: self,
            observado,
        })
    }

    fn observar_retiro_c_v3_datos(&mut self) -> Result<IntentV3EnArchivo, ErrorRetiroCV3> {
        self.exigir_slots_c_v3()?;
        let observado = self.observar_intent_v3().map_err(|_| ErrorRetiroCV3)?;
        if !matches!(
            familia::clasificar(None, None, &observado.intent),
            Ok(FamiliaV3::CConContextoParaReplay(_))
        ) {
            return Err(ErrorRetiroCV3);
        }
        let replay = observado
            .intent
            .calcular_new_esperado()
            .map_err(|_| ErrorRetiroCV3)?;
        let esperado = ArtifactFingerprint::new(
            replay.journal().new_fingerprint().identity().clone(),
            replay.digest().clone(),
        );
        let raw = self
            .observar_siblings_recovery_v2_raw(replay.journal())
            .map_err(|_| ErrorRetiroCV3)?;
        if raw.root() != Some(&esperado) || raw.backup().is_some() || raw.stage().is_some() {
            return Err(ErrorRetiroCV3);
        }
        self.exigir_slots_c_v3()?;
        let fresca = self.observar_intent_v3().map_err(|_| ErrorRetiroCV3)?;
        if identidad_archivo(&observado.archivo).map_err(|_| ErrorRetiroCV3)?
            != identidad_archivo(&fresca.archivo).map_err(|_| ErrorRetiroCV3)?
            || observado.intent.metadata() != fresca.intent.metadata()
            || observado.intent.contexto_original() != fresca.intent.contexto_original()
        {
            return Err(ErrorRetiroCV3);
        }
        Ok(fresca)
    }

    fn exigir_slots_c_v3(&mut self) -> Result<(), ErrorRetiroCV3> {
        let slots = self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorRetiroCV3)?;
        if slots.clasificar_forma_slots_retiro_v2() != Ok(FormaObservadaSlotsRetiroV2::RetiroC) {
            return Err(ErrorRetiroCV3);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "retiro_c_v3_tests.rs"]
mod tests;
