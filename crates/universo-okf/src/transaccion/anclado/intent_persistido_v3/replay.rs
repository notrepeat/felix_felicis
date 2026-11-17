use super::IntentRetiroPersistidoV3;
use crate::transaccion::admision::envelope::v2::{
    EnvelopeContextoAdmisionV2, ErrorReplayEnvelopeContextoAdmisionV2,
};
use crate::transaccion::replay::new_esperado_v2::NewEsperadoV2;

#[derive(Debug)]
pub(in crate::transaccion::anclado) enum ErrorReplayIntentV3 {
    ContextInvalid,
    Replay(ErrorReplayEnvelopeContextoAdmisionV2),
}

impl IntentRetiroPersistidoV3 {
    /// Reproduce datos autocontenidos; no observa archivos ni autoriza recovery.
    pub(in crate::transaccion::anclado) fn calcular_new_esperado(
        &self,
    ) -> Result<NewEsperadoV2, ErrorReplayIntentV3> {
        let context = EnvelopeContextoAdmisionV2::decode(self.contexto_original())
            .map_err(|_| ErrorReplayIntentV3::ContextInvalid)?;
        context
            .calcular_new_esperado(self.metadata().journal())
            .map_err(ErrorReplayIntentV3::Replay)
    }
}
