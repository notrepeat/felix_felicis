//! Decoder estricto de ADMENV v2; no hace I/O ni concede autoridad.

use crate::transaccion::admision::codec::decodificar as decodificar_contexto;
use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::v2::{JournalKind, JournalV2};

use super::super::{EnvelopeContextoAdmisionV2, ErrorCodecEnvelopeContextoAdmisionV2};
use super::lectura::preflight;
use super::mapear_error_contexto;

pub(super) fn decode(
    bytes: &[u8],
) -> Result<EnvelopeContextoAdmisionV2, ErrorCodecEnvelopeContextoAdmisionV2> {
    let (journal_bytes, context_bytes) = preflight(bytes)?;
    let journal = JournalV2::parse(journal_bytes)
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid)?;
    if journal.state() != JournalState::Prepared || journal.kind() != JournalKind::ContextRequired {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid);
    }

    let context = decodificar_contexto(context_bytes).map_err(mapear_error_contexto)?;
    Ok(EnvelopeContextoAdmisionV2::ligar(context, &journal))
}

#[cfg(test)]
mod tests {
    use super::super::{CONTEXT_LENGTH_BYTES, HEADER_BYTES, JOURNAL_LENGTH_BYTES, MAGIC, VERSION};
    use super::*;

    use crate::transaccion::admision::envelope::tests as fixtures;
    use crate::transaccion::journal::{ArtifactFingerprint, Journal, RawTreeDigest};

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

    fn digest(byte: char) -> RawTreeDigest {
        RawTreeDigest::from_hex(byte.to_string().repeat(64)).unwrap()
    }

    fn fingerprint(id: u64, byte: char) -> ArtifactFingerprint {
        ArtifactFingerprint::new(fixtures::identidad(id), digest(byte))
    }

    fn journal_v2(
        transaction_id: &str,
        parent_id: u64,
        old_id: u64,
        old_digest: char,
        new_id: u64,
        new_digest: char,
    ) -> JournalV2 {
        JournalV2::new(
            transaction_id,
            fixtures::identidad(parent_id),
            fingerprint(old_id, old_digest),
            fingerprint(new_id, new_digest),
        )
        .unwrap()
    }

    fn record() -> JournalV2 {
        journal_v2(TRANSACTION_ID, 1, 2, 'a', 3, 'b')
    }

    fn frame(journal: &[u8], context: &[u8]) -> Vec<u8> {
        let total = HEADER_BYTES
            + JOURNAL_LENGTH_BYTES
            + journal.len()
            + CONTEXT_LENGTH_BYTES
            + context.len();
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_be_bytes());
        bytes.extend_from_slice(&u64::try_from(total).unwrap().to_be_bytes());
        bytes.extend_from_slice(&u32::try_from(journal.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(journal);
        bytes.extend_from_slice(&u64::try_from(context.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(context);
        bytes
    }

    fn valid_context_bytes() -> Vec<u8> {
        crate::transaccion::admision::codec::codificar(&fixtures::contexto(b"\n")).unwrap()
    }

    fn valid_frame(journal: &JournalV2) -> Vec<u8> {
        let context = fixtures::contexto(b"\n");
        EnvelopeContextoAdmisionV2::ligar(context, journal)
            .encode()
            .unwrap()
    }

    fn expect_error(
        bytes: &[u8],
        expected: ErrorCodecEnvelopeContextoAdmisionV2,
    ) -> ErrorCodecEnvelopeContextoAdmisionV2 {
        let error = match decode(bytes) {
            Ok(_) => panic!("se esperaba rechazo del envelope v2"),
            Err(error) => error,
        };
        assert_eq!(error, expected);
        error
    }

    fn assert_binding_mismatch(envelope: &EnvelopeContextoAdmisionV2, candidate: &JournalV2) {
        let error = envelope.coincide_estructuralmente(candidate).unwrap_err();
        assert_eq!(error.code(), "admission_envelope_v2_binding_mismatch");
        assert_eq!(error.to_string(), error.code());
        assert!(!format!("{error:?}").contains(TRANSACTION_ID));
    }

    #[test]
    fn decode_roundtrip_conserva_binding_completo_y_kind() {
        let journal = record();
        let wire = valid_frame(&journal);
        let decoded = decode(&wire).unwrap();
        let encoded_again = decoded.encode().unwrap();
        assert_eq!(encoded_again, wire);
        assert!(decoded.coincide_estructuralmente(&journal).is_ok());

        let (journal_bytes, _) = preflight(&encoded_again).unwrap();
        let parsed = JournalV2::parse(journal_bytes).unwrap();
        assert_eq!(parsed.kind(), JournalKind::ContextRequired);
    }

    #[test]
    fn decode_binding_rechaza_txid_parent_old_y_new_distintos() {
        let journal = record();
        let decoded = decode(&valid_frame(&journal)).unwrap();
        let alternatives = [
            journal_v2("1123456789abcdef0123456789abcdef", 1, 2, 'a', 3, 'b'),
            journal_v2(TRANSACTION_ID, 9, 2, 'a', 3, 'b'),
            journal_v2(TRANSACTION_ID, 1, 8, 'c', 3, 'b'),
            journal_v2(TRANSACTION_ID, 1, 2, 'a', 9, 'd'),
        ];

        for candidate in &alternatives {
            assert_binding_mismatch(&decoded, candidate);
        }
    }

    #[test]
    fn decode_outer_version_v1_o_desconocida_no_hace_fallback() {
        let wire = valid_frame(&record());
        for version in [1, 3, u16::MAX] {
            let mut candidate = wire.clone();
            candidate[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&version.to_be_bytes());
            expect_error(
                &candidate,
                ErrorCodecEnvelopeContextoAdmisionV2::VersionUnsupported,
            );
        }
    }

    #[test]
    fn decode_rechaza_journal_v1_bajo_framing_v2() {
        let context = fixtures::contexto(b"\n");
        let journal_v1: Journal = fixtures::journal_valido(&context);
        let frame = frame(
            &journal_v1.encode().unwrap(),
            &crate::transaccion::admision::codec::codificar(&context).unwrap(),
        );

        expect_error(&frame, ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid);
    }

    #[test]
    fn decode_rechaza_journal_no_canonico() {
        let journal = record();
        let mut journal_bytes = journal.encode().unwrap();
        journal_bytes.insert(0, b' ');
        let frame = frame(&journal_bytes, &valid_context_bytes());

        expect_error(&frame, ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid);
    }

    #[test]
    fn decode_rechaza_journal_no_prepared() {
        let mut journal = record();
        journal.advance(JournalState::BackupReady).unwrap();
        let frame = frame(&journal.encode().unwrap(), &valid_context_bytes());

        expect_error(&frame, ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid);
    }

    #[test]
    fn decode_rechaza_platform_foreign_del_journal() {
        let journal = record();
        let text = String::from_utf8(journal.encode().unwrap()).unwrap();
        #[cfg(unix)]
        let foreign = text.replacen(
            "\"platform\":\"unix-v1\",\"device\":1,\"inode\":2",
            "\"platform\":\"windows-v1\",\"volume\":1,\"file_id\":\"00000000000000000000000000000001\"",
            1,
        );
        #[cfg(windows)]
        let foreign = text.replacen(
            "\"platform\":\"windows-v1\",\"volume\":1,\"file_id\":\"00000000000000000000000000000001\"",
            "\"platform\":\"unix-v1\",\"device\":1,\"inode\":2",
            1,
        );
        assert_ne!(foreign, text);
        let frame = frame(foreign.as_bytes(), &valid_context_bytes());

        expect_error(&frame, ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid);
    }

    #[test]
    fn decode_context_invalido_trailing_y_truncado_retorna_error_redactado() {
        let journal_bytes = record().encode().unwrap();
        let valid_context = valid_context_bytes();
        let mut trailing_context = valid_context.clone();
        trailing_context.push(b'x');
        let truncated_context = &valid_context[..valid_context.len() - 1];
        let cases = [
            frame(&journal_bytes, b"private-context"),
            frame(&journal_bytes, &trailing_context),
            frame(&journal_bytes, truncated_context),
        ];

        for candidate in &cases {
            let error = expect_error(
                candidate,
                ErrorCodecEnvelopeContextoAdmisionV2::ContextInvalid,
            );
            assert_eq!(error.to_string(), error.code());
            assert!(!format!("{error:?}").contains("private-context"));
        }
    }
}
