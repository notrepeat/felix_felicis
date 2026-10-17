use std::fmt;

use crate::transaccion::journal::v2::{JournalKind, JournalV2};
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity};
use crate::transaccion::replay::new_esperado_v2::{
    ErrorNewEsperadoV2, NewEsperadoV2, calcular_new_esperado_v2,
};

use super::super::ContextoAdmision;

mod codec;

pub(super) const MAX_FRAME_BYTES: usize = codec::MAX_ADMENV_V2_BYTES;

/// Contexto de admisión ligado estructuralmente a los datos inmutables de JournalV2.
///
/// La coincidencia compara datos; no reproduce el contexto, observa el filesystem
/// ni concede autoridad de replay o recovery.
pub(crate) struct EnvelopeContextoAdmisionV2 {
    vinculo: VinculoJournalV2,
    contexto: ContextoAdmision,
}

impl EnvelopeContextoAdmisionV2 {
    /// Codifica el binding y el Context en el frame exterior privado v2.
    pub(crate) fn encode(&self) -> Result<Vec<u8>, ErrorCodecEnvelopeContextoAdmisionV2> {
        codec::encode(self)
    }

    /// Decodifica un frame exterior v2 con JournalV2 y Context canónicos.
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ErrorCodecEnvelopeContextoAdmisionV2> {
        codec::decode(bytes)
    }

    /// Conserva los campos vinculantes completos, incluido el kind cerrado de v2.
    pub(crate) fn ligar(contexto: ContextoAdmision, journal: &JournalV2) -> Self {
        Self {
            vinculo: VinculoJournalV2::capturar(journal),
            contexto,
        }
    }

    /// Comprueba igualdad estructural de datos sin interpretar el estado como binding.
    pub(crate) fn coincide_estructuralmente(
        &self,
        journal: &JournalV2,
    ) -> Result<(), ErrorEnvelopeContextoAdmisionV2> {
        if self.vinculo.coincide_estructuralmente(journal) {
            Ok(())
        } else {
            Err(ErrorEnvelopeContextoAdmisionV2::VinculoNoCoincide)
        }
    }

    /// Calcula la expectativa tipada solo tras validar el binding estructural.
    pub(crate) fn calcular_new_esperado(
        &self,
        journal: &JournalV2,
    ) -> Result<NewEsperadoV2, ErrorReplayEnvelopeContextoAdmisionV2> {
        self.coincide_estructuralmente(journal)
            .map_err(|_| ErrorReplayEnvelopeContextoAdmisionV2::VinculoNoCoincide)?;
        calcular_new_esperado_v2(&self.contexto, journal)
            .map_err(ErrorReplayEnvelopeContextoAdmisionV2::Replay)
    }
}

/// Error fijo y redactado del codec exterior ADMENV v2.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorCodecEnvelopeContextoAdmisionV2 {
    FramingInvalid,
    VersionUnsupported,
    JournalInvalid,
    ContextInvalid,
    ContextExceedsLimit,
    TotalExceedsLimit,
    LengthExceeded,
    ReservationFailed,
}

impl ErrorCodecEnvelopeContextoAdmisionV2 {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::FramingInvalid => "admission_envelope_v2_framing_invalid",
            Self::VersionUnsupported => "admission_envelope_v2_version_unsupported",
            Self::JournalInvalid => "admission_envelope_v2_journal_invalid",
            Self::ContextInvalid => "admission_envelope_v2_context_invalid",
            Self::ContextExceedsLimit => "admission_envelope_v2_context_exceeds_limit",
            Self::TotalExceedsLimit => "admission_envelope_v2_total_exceeds_limit",
            Self::LengthExceeded => "admission_envelope_v2_length_exceeded",
            Self::ReservationFailed => "admission_envelope_v2_reservation_failed",
        }
    }
}

impl fmt::Debug for ErrorCodecEnvelopeContextoAdmisionV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorCodecEnvelopeContextoAdmisionV2")
            .field(&self.code())
            .finish()
    }
}

impl fmt::Display for ErrorCodecEnvelopeContextoAdmisionV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ErrorCodecEnvelopeContextoAdmisionV2 {}

impl fmt::Debug for EnvelopeContextoAdmisionV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EnvelopeContextoAdmisionV2 { .. }")
    }
}

#[derive(Clone, PartialEq, Eq)]
struct VinculoJournalV2 {
    transaction_id: String,
    kind: JournalKind,
    parent: DirectoryIdentity,
    old: ArtifactFingerprint,
    new: ArtifactFingerprint,
}

impl VinculoJournalV2 {
    fn capturar(journal: &JournalV2) -> Self {
        Self {
            transaction_id: journal.transaction_id().to_owned(),
            kind: journal.kind(),
            parent: journal.parent().clone(),
            old: journal.old().clone(),
            new: journal.new_fingerprint().clone(),
        }
    }

    fn coincide_estructuralmente(&self, journal: &JournalV2) -> bool {
        self.transaction_id == journal.transaction_id()
            && self.kind == journal.kind()
            && self.parent == *journal.parent()
            && self.old == *journal.old()
            && self.new == *journal.new_fingerprint()
    }
}

/// Error fijo y redactado de comparación estructural; no expresa autorización.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorEnvelopeContextoAdmisionV2 {
    VinculoNoCoincide,
}

impl ErrorEnvelopeContextoAdmisionV2 {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::VinculoNoCoincide => "admission_envelope_v2_binding_mismatch",
        }
    }
}

impl fmt::Debug for ErrorEnvelopeContextoAdmisionV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorEnvelopeContextoAdmisionV2")
            .field(&self.code())
            .finish()
    }
}

impl fmt::Display for ErrorEnvelopeContextoAdmisionV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ErrorEnvelopeContextoAdmisionV2 {}

/// Error fijo al ligar el cálculo raw tipado al envelope v2.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorReplayEnvelopeContextoAdmisionV2 {
    VinculoNoCoincide,
    Replay(ErrorNewEsperadoV2),
}

impl ErrorReplayEnvelopeContextoAdmisionV2 {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::VinculoNoCoincide => "admission_envelope_v2_binding_mismatch",
            Self::Replay(error) => error.codigo(),
        }
    }
}

impl fmt::Debug for ErrorReplayEnvelopeContextoAdmisionV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorReplayEnvelopeContextoAdmisionV2")
            .field(&self.code())
            .finish()
    }
}

impl fmt::Display for ErrorReplayEnvelopeContextoAdmisionV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ErrorReplayEnvelopeContextoAdmisionV2 {}

#[cfg(test)]
mod tests {
    use super::super::tests::{contexto, contexto_con_tags, fingerprint, identidad};
    use super::*;
    use crate::serializar::serializar_candidato;
    use crate::transaccion::journal::v2::JournalV2;
    use crate::transaccion::journal::{
        ArtifactFingerprint, DirectoryIdentity, JournalState, RawTreeDigest,
    };
    use crate::transaccion::replay::reproponer;
    use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
    use crate::transaccion::snapshot::calcular_digest_raw_tree;
    use crate::transaccion::stage::{inventario_objetivo_para_replay, planificar_staging};
    use universo_core::ResultadoPropuesta;

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

    fn digest(byte: char) -> RawTreeDigest {
        RawTreeDigest::from_hex(byte.to_string().repeat(64)).unwrap()
    }

    fn journal(
        transaction_id: &str,
        parent_id: u64,
        old_id: u64,
        old_digest: char,
        new_id: u64,
        new_digest: char,
    ) -> JournalV2 {
        JournalV2::new(
            transaction_id,
            identidad(parent_id),
            fingerprint(old_id, digest(old_digest)),
            fingerprint(new_id, digest(new_digest)),
        )
        .unwrap()
    }

    fn assert_mismatch(
        envelope: &EnvelopeContextoAdmisionV2,
        candidate: &JournalV2,
        expected_transaction_id: &str,
    ) {
        let error = envelope.coincide_estructuralmente(candidate).unwrap_err();

        assert_eq!(error.code(), "admission_envelope_v2_binding_mismatch");
        assert_eq!(error.to_string(), error.code());
        assert_eq!(
            format!("{error:?}"),
            format!("ErrorEnvelopeContextoAdmisionV2({:?})", error.code())
        );
        assert!(!format!("{error:?}").contains(expected_transaction_id));
    }

    #[test]
    fn conserva_kind_y_omite_las_transiciones_del_binding_estructural() {
        let journal = journal(TRANSACTION_ID, 1, 2, 'a', 3, 'b');
        let envelope = EnvelopeContextoAdmisionV2::ligar(contexto(b"\n"), &journal);

        assert_eq!(journal.kind(), JournalKind::ContextRequired);
        assert_eq!(envelope.vinculo.kind, JournalKind::ContextRequired);
        assert!(envelope.coincide_estructuralmente(&journal).is_ok());

        let mut successor = journal.clone();
        for state in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            successor.advance(state).unwrap();
            assert!(envelope.coincide_estructuralmente(&successor).is_ok());
        }

        let debug = format!("{envelope:?}");
        assert!(!debug.contains(TRANSACTION_ID));
        assert!(!debug.contains("engine-envelope/1"));
    }

    #[test]
    fn rechaza_por_separado_transaction_id_y_parent_distintos() {
        let baseline = journal(TRANSACTION_ID, 1, 2, 'a', 3, 'b');
        let envelope = EnvelopeContextoAdmisionV2::ligar(contexto(b"\n"), &baseline);

        assert_mismatch(
            &envelope,
            &journal("1123456789abcdef0123456789abcdef", 1, 2, 'a', 3, 'b'),
            "1123456789abcdef0123456789abcdef",
        );
        assert_mismatch(
            &envelope,
            &journal(TRANSACTION_ID, 9, 2, 'a', 3, 'b'),
            TRANSACTION_ID,
        );
    }

    #[test]
    fn rechaza_por_separado_fingerprints_old_y_new_distintos() {
        let baseline = journal(TRANSACTION_ID, 1, 2, 'a', 3, 'b');
        let envelope = EnvelopeContextoAdmisionV2::ligar(contexto(b"\n"), &baseline);

        assert_mismatch(
            &envelope,
            &journal(TRANSACTION_ID, 1, 8, 'c', 3, 'b'),
            TRANSACTION_ID,
        );
        assert_mismatch(
            &envelope,
            &journal(TRANSACTION_ID, 1, 2, 'a', 9, 'd'),
            TRANSACTION_ID,
        );
    }

    fn digest_new_raw(
        contexto: &crate::transaccion::admision::ContextoAdmision,
        motor: &str,
        policy: &str,
    ) -> RawTreeDigest {
        let reproducida = reproponer(contexto, motor, policy).unwrap();
        let ResultadoPropuesta::Admitida(candidato) = &reproducida.resultado else {
            panic!("la propuesta fixture debe ser admitida");
        };
        let documentos =
            serializar_candidato(reproducida.snapshot.estructura(), candidato, &[]).unwrap();
        let plan = planificar_staging(&reproducida.snapshot, &documentos).unwrap();
        let entradas = inventario_objetivo_para_replay(&plan).unwrap();
        calcular_digest_raw_tree(&entradas).unwrap()
    }

    fn journal_v2_valido(
        contexto: &crate::transaccion::admision::ContextoAdmision,
        motor: &str,
        policy: &str,
    ) -> JournalV2 {
        let digest_old = calcular_digest_raw_tree(&contexto.entradas_base()).unwrap();
        JournalV2::new(
            TRANSACTION_ID,
            identidad(1),
            fingerprint(2, digest_old),
            fingerprint(3, digest_new_raw(contexto, motor, policy)),
        )
        .unwrap()
    }

    fn rearmar_journal_v2(
        transaction_id: &str,
        parent: &DirectoryIdentity,
        old: &ArtifactFingerprint,
        new: &ArtifactFingerprint,
    ) -> JournalV2 {
        JournalV2::new(transaction_id, parent.clone(), old.clone(), new.clone()).unwrap()
    }

    fn assert_replay_binding_mismatch(
        envelope: &EnvelopeContextoAdmisionV2,
        candidate: &JournalV2,
        sensitive: &str,
    ) {
        let error = envelope.calcular_new_esperado(candidate).unwrap_err();

        assert_eq!(error.code(), "admission_envelope_v2_binding_mismatch");
        assert_eq!(error.to_string(), error.code());
        assert_eq!(
            format!("{error:?}"),
            format!("ErrorReplayEnvelopeContextoAdmisionV2({:?})", error.code())
        );
        assert!(!format!("{error:?}").contains(sensitive));
    }

    #[test]
    fn calcula_expectativa_v2_tras_binding_y_preserva_estado_actual() {
        let contexto = contexto_con_tags(b"\n", VERSION_MOTOR_V1, POLICY_ID_V1);
        let preparado = journal_v2_valido(&contexto, VERSION_MOTOR_V1, POLICY_ID_V1);
        let envelope = EnvelopeContextoAdmisionV2::ligar(contexto, &preparado);

        let resultado = envelope.calcular_new_esperado(&preparado).unwrap();
        assert!(resultado.journal() == &preparado);
        assert_eq!(resultado.journal().kind(), JournalKind::ContextRequired);
        assert_eq!(resultado.journal().state(), JournalState::Prepared);
        assert_eq!(format!("{resultado:?}"), "NewEsperadoV2 { .. }");

        let mut sucesor = preparado;
        for estado in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            sucesor.advance(estado).unwrap();
            let resultado = envelope.calcular_new_esperado(&sucesor).unwrap();
            assert!(resultado.journal() == &sucesor);
            assert_eq!(resultado.journal().state(), estado);
        }

        let debug = format!("{envelope:?}");
        assert!(!debug.contains(TRANSACTION_ID));
        assert!(!debug.contains("felix_felicis:proposal-semantics/v1"));
    }

    #[test]
    fn replay_tipado_rechaza_por_separado_los_cuatro_campos_del_binding() {
        let contexto = contexto_con_tags(b"\n", VERSION_MOTOR_V1, POLICY_ID_V1);
        let baseline = journal_v2_valido(&contexto, VERSION_MOTOR_V1, POLICY_ID_V1);
        let envelope = EnvelopeContextoAdmisionV2::ligar(contexto, &baseline);

        let transaction_id = rearmar_journal_v2(
            "1123456789abcdef0123456789abcdef",
            baseline.parent(),
            baseline.old(),
            baseline.new_fingerprint(),
        );
        let parent = rearmar_journal_v2(
            TRANSACTION_ID,
            &identidad(9),
            baseline.old(),
            baseline.new_fingerprint(),
        );
        let old = rearmar_journal_v2(
            TRANSACTION_ID,
            baseline.parent(),
            &fingerprint(8, digest('c')),
            baseline.new_fingerprint(),
        );
        let new = rearmar_journal_v2(
            TRANSACTION_ID,
            baseline.parent(),
            baseline.old(),
            &fingerprint(9, digest('d')),
        );

        assert_replay_binding_mismatch(&envelope, &transaction_id, "1123456789abcdef");
        assert_replay_binding_mismatch(&envelope, &parent, TRANSACTION_ID);
        assert_replay_binding_mismatch(&envelope, &old, TRANSACTION_ID);
        assert_replay_binding_mismatch(&envelope, &new, TRANSACTION_ID);
    }

    #[test]
    fn binding_invalido_precede_tags_semanticos_no_soportados() {
        let contexto = contexto_con_tags(b"\n", "future-engine/1", "future-policy/1");
        let baseline = journal_v2_valido(&contexto, "future-engine/1", "future-policy/1");
        let envelope = EnvelopeContextoAdmisionV2::ligar(contexto, &baseline);
        let candidate = rearmar_journal_v2(
            "1123456789abcdef0123456789abcdef",
            baseline.parent(),
            baseline.old(),
            baseline.new_fingerprint(),
        );

        assert_replay_binding_mismatch(&envelope, &candidate, "1123456789abcdef");
    }
}
