//! Codec puro del frame exterior FELIX-ADMENV v2.

use crate::transaccion::journal::v2::{JournalKind, JournalV2, MAX_JOURNAL_V2_BYTES};

use super::super::super::codec::{
    ErrorCodecAdmision, codificar_con_limite as codificar_contexto_con_limite,
};
use super::{EnvelopeContextoAdmisionV2, ErrorCodecEnvelopeContextoAdmisionV2};

const MAGIC: &[u8; 12] = b"FELIX-ADMENV";
const VERSION: u16 = 2;
const HEADER_BYTES: usize = MAGIC.len() + 2 + 8;
const JOURNAL_LENGTH_BYTES: usize = 4;
const CONTEXT_LENGTH_BYTES: usize = 8;

mod decodificar;
mod lectura;
pub(super) fn decode(
    bytes: &[u8],
) -> Result<EnvelopeContextoAdmisionV2, ErrorCodecEnvelopeContextoAdmisionV2> {
    decodificar::decode(bytes)
}

pub(super) const MAX_ADMENV_V2_BYTES: usize = 512 * 1024 * 1024;
pub(super) const MAX_CONTEXT_ARTIFACT_V2_BYTES: usize = 512 * 1024 * 1024;

pub(super) fn encode(
    envelope: &EnvelopeContextoAdmisionV2,
) -> Result<Vec<u8>, ErrorCodecEnvelopeContextoAdmisionV2> {
    encode_with_limits(envelope, MAX_ADMENV_V2_BYTES, MAX_CONTEXT_ARTIFACT_V2_BYTES)
}

fn encode_with_limits(
    envelope: &EnvelopeContextoAdmisionV2,
    frame_limit: usize,
    context_limit: usize,
) -> Result<Vec<u8>, ErrorCodecEnvelopeContextoAdmisionV2> {
    let frame_limit = frame_limit.min(MAX_ADMENV_V2_BYTES);
    let context_limit = context_limit.min(MAX_CONTEXT_ARTIFACT_V2_BYTES);

    let journal = match envelope.vinculo.kind {
        JournalKind::ContextRequired => JournalV2::new(
            envelope.vinculo.transaction_id.clone(),
            envelope.vinculo.parent.clone(),
            envelope.vinculo.old.clone(),
            envelope.vinculo.new.clone(),
        ),
    }
    .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid)?;
    let journal_bytes = journal
        .encode()
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid)?;
    if journal_bytes.is_empty() || journal_bytes.len() > MAX_JOURNAL_V2_BYTES {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid);
    }

    let journal_start = HEADER_BYTES
        .checked_add(JOURNAL_LENGTH_BYTES)
        .ok_or(ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    let journal_end = journal_start
        .checked_add(journal_bytes.len())
        .ok_or(ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    let context_start = journal_end
        .checked_add(CONTEXT_LENGTH_BYTES)
        .ok_or(ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    let remaining_total = frame_limit
        .checked_sub(context_start)
        .ok_or(ErrorCodecEnvelopeContextoAdmisionV2::TotalExceedsLimit)?;
    let context_budget = remaining_total.min(context_limit);

    let context_bytes = codificar_contexto_con_limite(&envelope.contexto, context_budget)
        .map_err(mapear_error_contexto)?;
    let total_len = context_start
        .checked_add(context_bytes.len())
        .ok_or(ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    if total_len > frame_limit {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::TotalExceedsLimit);
    }

    let journal_len = u32::try_from(journal_bytes.len())
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    let total_len_u64 = u64::try_from(total_len)
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    let context_len_u64 = u64::try_from(context_bytes.len())
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;

    let mut output = Vec::new();
    output
        .try_reserve_exact(total_len)
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::ReservationFailed)?;
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&VERSION.to_be_bytes());
    output.extend_from_slice(&total_len_u64.to_be_bytes());
    output.extend_from_slice(&journal_len.to_be_bytes());
    output.extend_from_slice(&journal_bytes);
    output.extend_from_slice(&context_len_u64.to_be_bytes());
    output.extend_from_slice(&context_bytes);

    if output.len() != total_len {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded);
    }
    Ok(output)
}

fn mapear_error_contexto(error: ErrorCodecAdmision) -> ErrorCodecEnvelopeContextoAdmisionV2 {
    match error {
        ErrorCodecAdmision::TotalExcedeLimite => {
            ErrorCodecEnvelopeContextoAdmisionV2::TotalExceedsLimit
        }
        ErrorCodecAdmision::LongitudExcedida => {
            ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded
        }
        ErrorCodecAdmision::ReservaFallida => {
            ErrorCodecEnvelopeContextoAdmisionV2::ReservationFailed
        }
        ErrorCodecAdmision::ContextoInvalido
        | ErrorCodecAdmision::FramingInvalido
        | ErrorCodecAdmision::VersionNoSoportada
        | ErrorCodecAdmision::LimiteComponenteExcedido => {
            ErrorCodecEnvelopeContextoAdmisionV2::ContextInvalid
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::tests::{contexto, fingerprint, identidad};
    use super::*;
    use crate::transaccion::journal::JournalState;
    use crate::transaccion::journal::RawTreeDigest;

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

    fn journal() -> JournalV2 {
        JournalV2::new(
            TRANSACTION_ID,
            identidad(1),
            fingerprint(2, RawTreeDigest::from_hex("a".repeat(64)).unwrap()),
            fingerprint(3, RawTreeDigest::from_hex("b".repeat(64)).unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn encoder_emite_layout_exacto_y_conserva_kind_v2() {
        let record = journal();
        let context = contexto(b"\n");
        let journal_bytes = record.encode().unwrap();
        let context_bytes = crate::transaccion::admision::codec::codificar(&context).unwrap();
        let envelope = EnvelopeContextoAdmisionV2::ligar(context, &record);
        let frame = envelope.encode().unwrap();

        let journal_start = HEADER_BYTES + JOURNAL_LENGTH_BYTES;
        let journal_end = journal_start + journal_bytes.len();
        let context_start = journal_end + CONTEXT_LENGTH_BYTES;
        let total_len = context_start + context_bytes.len();
        let mut expected = Vec::new();
        expected.extend_from_slice(MAGIC);
        expected.extend_from_slice(&VERSION.to_be_bytes());
        expected.extend_from_slice(&u64::try_from(total_len).unwrap().to_be_bytes());
        expected.extend_from_slice(&u32::try_from(journal_bytes.len()).unwrap().to_be_bytes());
        expected.extend_from_slice(&journal_bytes);
        expected.extend_from_slice(&u64::try_from(context_bytes.len()).unwrap().to_be_bytes());
        expected.extend_from_slice(&context_bytes);

        assert_eq!(&frame[..MAGIC.len()], MAGIC);
        assert_eq!(
            u16::from_be_bytes(frame[MAGIC.len()..MAGIC.len() + 2].try_into().unwrap()),
            VERSION
        );
        assert_eq!(
            u64::from_be_bytes(frame[MAGIC.len() + 2..HEADER_BYTES].try_into().unwrap()),
            u64::try_from(total_len).unwrap()
        );
        assert_eq!(
            u32::from_be_bytes(frame[HEADER_BYTES..journal_start].try_into().unwrap()),
            u32::try_from(journal_bytes.len()).unwrap()
        );
        assert_eq!(
            u64::from_be_bytes(frame[journal_end..context_start].try_into().unwrap()),
            u64::try_from(context_bytes.len()).unwrap()
        );
        assert_eq!(frame, expected);

        let parsed = JournalV2::parse(&frame[journal_start..journal_end]).unwrap();
        assert_eq!(parsed.kind(), JournalKind::ContextRequired);
        assert_eq!(parsed, record);
    }

    #[test]
    fn encoder_persists_prepared_without_mutating_later_journal_state() {
        let mut record = journal();
        record.advance(JournalState::BackupReady).unwrap();
        let original = record.clone();
        let envelope = EnvelopeContextoAdmisionV2::ligar(contexto(b"\n"), &record);
        let frame = envelope.encode().unwrap();

        let journal_start = HEADER_BYTES + JOURNAL_LENGTH_BYTES;
        let journal_len =
            u32::from_be_bytes(frame[HEADER_BYTES..journal_start].try_into().unwrap()) as usize;
        let parsed = JournalV2::parse(&frame[journal_start..journal_start + journal_len]).unwrap();
        assert_eq!(parsed.state(), JournalState::Prepared);
        assert_eq!(record.state(), JournalState::BackupReady);
        assert_eq!(record, original);
        assert!(envelope.coincide_estructuralmente(&record).is_ok());
    }

    #[test]
    fn combined_quota_accepts_exact_frame_and_rejects_one_byte_less() {
        let record = journal();
        let envelope = EnvelopeContextoAdmisionV2::ligar(contexto(b"\n"), &record);
        let expected = encode_with_limits(
            &envelope,
            MAX_ADMENV_V2_BYTES,
            MAX_CONTEXT_ARTIFACT_V2_BYTES,
        )
        .unwrap();

        assert_eq!(
            encode_with_limits(&envelope, expected.len(), MAX_CONTEXT_ARTIFACT_V2_BYTES,),
            Ok(expected.clone())
        );
        assert_eq!(
            encode_with_limits(&envelope, expected.len() - 1, MAX_CONTEXT_ARTIFACT_V2_BYTES,),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::TotalExceedsLimit)
        );
    }

    #[test]
    fn encoder_errors_are_fixed_and_redacted() {
        let record = journal();
        let envelope =
            EnvelopeContextoAdmisionV2::ligar(contexto(b"; sensitive-context\n"), &record);
        let error = encode_with_limits(&envelope, 0, MAX_CONTEXT_ARTIFACT_V2_BYTES).unwrap_err();

        assert_eq!(error.code(), "admission_envelope_v2_total_exceeds_limit");
        assert_eq!(error.to_string(), error.code());
        assert_eq!(
            format!("{error:?}"),
            format!("ErrorCodecEnvelopeContextoAdmisionV2({:?})", error.code())
        );
        assert!(!error.to_string().contains(TRANSACTION_ID));
        assert!(!format!("{error:?}").contains("sensitive-context"));
    }

    #[test]
    fn context_quota_is_inclusive_and_independent_with_holgado_frame() {
        let record = journal();
        let context = contexto(b"; context-quota\n");
        let context_bytes = crate::transaccion::admision::codec::codificar(&context).unwrap();
        let envelope = EnvelopeContextoAdmisionV2::ligar(context, &record);

        assert!(encode_with_limits(&envelope, MAX_ADMENV_V2_BYTES, context_bytes.len(),).is_ok());
        assert_eq!(
            encode_with_limits(&envelope, MAX_ADMENV_V2_BYTES, context_bytes.len() - 1,),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::TotalExceedsLimit)
        );
    }
}
