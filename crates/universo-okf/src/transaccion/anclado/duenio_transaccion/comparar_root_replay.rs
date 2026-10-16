//! Compara la expectativa raw del replay con una observación anclada del root.

use std::fmt;

use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, ErrorEnvelopeAdmision};
use crate::transaccion::anclado::contexto_admision::ErrorContextoAdmisionAnclado;
use crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado;
use crate::transaccion::journal::{ArtifactFingerprint, Journal};

use super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado};

/// Relación observada entre el root raw y la huella `new` reproducida.
///
/// Es un dato de comparación bajo el lock, no una capability ni una decisión
/// de recovery, apply o cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RelacionRootReplay {
    Ausente,
    Coincide,
    Diferente,
}

/// Huellas comparadas, conservadas como datos sin autoridad filesystem.
pub(crate) struct ComparacionRootReplay {
    esperado: ArtifactFingerprint,
    observado: Option<ArtifactFingerprint>,
    relacion: RelacionRootReplay,
}

impl ComparacionRootReplay {
    pub(crate) fn esperado(&self) -> &ArtifactFingerprint {
        &self.esperado
    }

    pub(crate) fn observado(&self) -> Option<&ArtifactFingerprint> {
        self.observado.as_ref()
    }

    pub(crate) fn relacion(&self) -> RelacionRootReplay {
        self.relacion
    }
}

impl fmt::Debug for ComparacionRootReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ComparacionRootReplay { .. }")
    }
}

/// Error estable y redactado al cargar y comparar la evidencia transaccional.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorComparacionRootReplay {
    Journal(ErrorJournalAnclado),
    Contexto(ErrorContextoAdmisionAnclado),
    Envelope(ErrorEnvelopeAdmision),
    Owner(ErrorDuenioTransaccionAnclado),
    JournalAusente,
    ContextoAusente,
}

impl ErrorComparacionRootReplay {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::Journal(error) => error.codigo(),
            Self::Contexto(error) => error.codigo(),
            Self::Envelope(error) => error.codigo(),
            Self::Owner(error) => error.codigo(),
            Self::JournalAusente => "root_replay_journal_missing",
            Self::ContextoAusente => "root_replay_context_missing",
        }
    }
}

impl fmt::Debug for ErrorComparacionRootReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorComparacionRootReplay")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorComparacionRootReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorComparacionRootReplay {}

impl DuenioTransaccionAnclado<'_> {
    /// Carga Journal y Envelope en leases secuenciales, reproduce `new` y
    /// compara la huella completa con el root observado desde el parent retenido.
    ///
    /// Los parámetros identifican la configuración de replay solicitada. El
    /// resultado describe datos bajo el lock y no habilita recovery ni cleanup.
    pub(crate) fn comparar_root_replay(
        &mut self,
        motor_soportado: &str,
        policy_soportada: &str,
    ) -> Result<ComparacionRootReplay, ErrorComparacionRootReplay> {
        let (journal, envelope) = self.cargar_journal_y_envelope()?;
        let replay = envelope
            .validar_replay(&journal, motor_soportado, policy_soportada)
            .map_err(ErrorComparacionRootReplay::Envelope)?;
        self.comparar_fingerprint_root(replay.new_fingerprint().clone())
    }

    /// Selecciona el par archivado cerrado sin aceptar tags del llamador.
    /// El resultado sigue siendo una comparación de datos, no recovery.
    pub(crate) fn comparar_root_replay_soportado(
        &mut self,
    ) -> Result<ComparacionRootReplay, ErrorComparacionRootReplay> {
        let (journal, envelope) = self.cargar_journal_y_envelope()?;
        let replay = envelope
            .validar_replay_soportado(&journal)
            .map_err(ErrorComparacionRootReplay::Envelope)?;
        self.comparar_fingerprint_root(replay.new_fingerprint().clone())
    }

    fn cargar_journal_y_envelope(
        &mut self,
    ) -> Result<(Journal, EnvelopeContextoAdmision), ErrorComparacionRootReplay> {
        let journal = {
            let lease = self
                .journal_temporal()
                .map_err(ErrorComparacionRootReplay::Journal)?;
            // cargar_canonico valida Journal.parent contra el parent retenido.
            let (_, journal) = lease
                .cargar_canonico()
                .map_err(ErrorComparacionRootReplay::Journal)?
                .ok_or(ErrorComparacionRootReplay::JournalAusente)?;
            journal
        };
        let envelope = {
            let lease = self
                .contexto_admision()
                .map_err(ErrorComparacionRootReplay::Contexto)?;
            let (envelope, _) = lease
                .leer_canonico()
                .map_err(ErrorComparacionRootReplay::Contexto)?
                .ok_or(ErrorComparacionRootReplay::ContextoAusente)?;
            envelope
        };
        Ok((journal, envelope))
    }

    fn comparar_fingerprint_root(
        &mut self,
        esperado: ArtifactFingerprint,
    ) -> Result<ComparacionRootReplay, ErrorComparacionRootReplay> {
        let observado = self
            .observar_root_raw()
            .map_err(ErrorComparacionRootReplay::Owner)?;
        let relacion = match observado.as_ref() {
            None => RelacionRootReplay::Ausente,
            Some(actual) if actual == &esperado => RelacionRootReplay::Coincide,
            Some(_) => RelacionRootReplay::Diferente,
        };

        Ok(ComparacionRootReplay {
            esperado,
            observado,
            relacion,
        })
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "comparar_root_replay_tests.rs"]
mod tests;
