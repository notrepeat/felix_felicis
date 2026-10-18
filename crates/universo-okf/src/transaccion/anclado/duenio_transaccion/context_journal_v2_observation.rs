//! Observa Contexto V2 y su binding como datos bajo un lease del owner.

use std::fmt;

use super::super::contexto_admision::{ObservacionContextoVersionada, ReciboContextoPublicado};
use super::DuenioTransaccionAnclado;
use crate::transaccion::admision::envelope::selector::EnvelopeAdmisionVersionada;
use crate::transaccion::journal::ArtifactFingerprint;
use crate::transaccion::journal::v2::JournalV2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorObservacionContextoJournalV2 {
    LeaseContexto,
    LecturaContexto,
    ReciboNoCoincide,
    ReplayNoCoincide,
}

impl fmt::Debug for ErrorObservacionContextoJournalV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nombre = match self {
            Self::LeaseContexto => "LeaseContexto",
            Self::LecturaContexto => "LecturaContexto",
            Self::ReciboNoCoincide => "ReciboNoCoincide",
            Self::ReplayNoCoincide => "ReplayNoCoincide",
        };
        formatter.write_str(nombre)
    }
}

pub(super) enum EstadoObservacionContextoJournalV2 {
    Observado(ObservacionContextoVersionada),
    Rechazado {
        observacion: ObservacionContextoVersionada,
        error: ErrorObservacionContextoJournalV2,
    },
    Ausente,
    Error(ErrorObservacionContextoJournalV2),
}

impl fmt::Debug for EstadoObservacionContextoJournalV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Observado(_) => formatter.write_str("Observado"),
            Self::Rechazado { error, .. } => {
                formatter.debug_tuple("Rechazado").field(error).finish()
            }
            Self::Ausente => formatter.write_str("Ausente"),
            Self::Error(error) => formatter.debug_tuple("Error").field(error).finish(),
        }
    }
}

/// Una sola lectura secuencial; no confirma que el basename siga estable.
pub(super) fn observar_contexto_con_journal_v2(
    owner: &mut DuenioTransaccionAnclado<'_>,
    recibo: &ReciboContextoPublicado,
    journal: &JournalV2,
) -> EstadoObservacionContextoJournalV2 {
    let lease = match owner.contexto_admision() {
        Ok(lease) => lease,
        Err(_) => {
            return EstadoObservacionContextoJournalV2::Error(
                ErrorObservacionContextoJournalV2::LeaseContexto,
            );
        }
    };
    let observacion = match lease.observar_versionado() {
        Ok(Some(observacion)) => observacion,
        Ok(None) => return EstadoObservacionContextoJournalV2::Ausente,
        Err(_) => {
            return EstadoObservacionContextoJournalV2::Error(
                ErrorObservacionContextoJournalV2::LecturaContexto,
            );
        }
    };

    let error = if recibo
        .validar_observacion_v2(&lease, &observacion, journal)
        .is_err()
    {
        Some(ErrorObservacionContextoJournalV2::ReciboNoCoincide)
    } else if !replay_coincide(&observacion, journal) {
        Some(ErrorObservacionContextoJournalV2::ReplayNoCoincide)
    } else {
        None
    };
    match error {
        Some(error) => EstadoObservacionContextoJournalV2::Rechazado { observacion, error },
        None => EstadoObservacionContextoJournalV2::Observado(observacion),
    }
}

fn replay_coincide(observacion: &ObservacionContextoVersionada, journal: &JournalV2) -> bool {
    let EnvelopeAdmisionVersionada::V2(envelope) = observacion.envelope_versionado() else {
        return false;
    };
    let Ok(replay) = envelope.calcular_new_esperado(journal) else {
        return false;
    };
    let esperado = ArtifactFingerprint::new(
        replay.journal().new_fingerprint().identity().clone(),
        replay.digest().clone(),
    );
    &esperado == journal.new_fingerprint()
}

#[cfg(test)]
#[path = "context_journal_v2_observation_tests.rs"]
mod tests;
