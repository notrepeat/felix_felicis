//! Decoder owned del envelope exterior, sin I/O ni autoridad de recovery.

use crate::transaccion::admision::codec::{
    ErrorCodecAdmision, decodificar as decodificar_contexto,
};
use crate::transaccion::journal::{Journal, JournalState};

use super::super::EnvelopeContextoAdmision;
use super::ErrorCodecEnvelopeAdmision;
use super::lectura::preflight;

/// Valida el framing, el journal canónico `Prepared` y reconstruye el contexto.
pub(crate) fn decodificar(
    bytes: &[u8],
) -> Result<EnvelopeContextoAdmision, ErrorCodecEnvelopeAdmision> {
    let wire = preflight(bytes)?;
    let journal =
        Journal::parse(wire.journal).map_err(|_| ErrorCodecEnvelopeAdmision::JournalInvalido)?;
    if journal.state() != JournalState::Prepared {
        return Err(ErrorCodecEnvelopeAdmision::JournalInvalido);
    }
    let contexto = decodificar_contexto(wire.context).map_err(mapear_error_contexto)?;
    Ok(EnvelopeContextoAdmision::ligar(contexto, &journal))
}

fn mapear_error_contexto(error: ErrorCodecAdmision) -> ErrorCodecEnvelopeAdmision {
    match error {
        ErrorCodecAdmision::TotalExcedeLimite => ErrorCodecEnvelopeAdmision::TotalExcedeLimite,
        ErrorCodecAdmision::LongitudExcedida => ErrorCodecEnvelopeAdmision::LongitudExcedida,
        ErrorCodecAdmision::ReservaFallida => ErrorCodecEnvelopeAdmision::ReservaFallida,
        _ => ErrorCodecEnvelopeAdmision::ContextoInvalido,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, tests as fixtures};
    use crate::transaccion::journal::{ArtifactFingerprint, RawTreeDigest};

    use super::super::{HEADER_BYTES, JOURNAL_LENGTH_BYTES, MAGIC};

    fn decoded_fixture() -> (Vec<u8>, Journal) {
        let contexto = fixtures::contexto(b"\n");
        let journal = fixtures::journal_valido(&contexto);
        let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);
        (super::super::codificar(&envelope).unwrap(), journal)
    }

    fn error(bytes: &[u8]) -> ErrorCodecEnvelopeAdmision {
        match decodificar(bytes) {
            Ok(_) => panic!("se esperaba rechazo del envelope"),
            Err(error) => error,
        }
    }

    fn replace_journal(bytes: &mut Vec<u8>, journal: &[u8]) {
        let start = HEADER_BYTES + JOURNAL_LENGTH_BYTES;
        let old_len = usize::try_from(u32::from_be_bytes(
            bytes[HEADER_BYTES..start].try_into().unwrap(),
        ))
        .unwrap();
        bytes.splice(start..start + old_len, journal.iter().copied());
        let new_len = u32::try_from(journal.len()).unwrap();
        bytes[HEADER_BYTES..start].copy_from_slice(&new_len.to_be_bytes());
        let total = u64::try_from(bytes.len()).unwrap();
        bytes[MAGIC.len() + 2..HEADER_BYTES].copy_from_slice(&total.to_be_bytes());
    }

    fn context_start(bytes: &[u8]) -> usize {
        let start = HEADER_BYTES;
        let journal_len = usize::try_from(u32::from_be_bytes(
            bytes[start..start + JOURNAL_LENGTH_BYTES]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        start + JOURNAL_LENGTH_BYTES + journal_len + 8
    }

    #[test]
    fn decodifica_roundtrip_y_reproduce_el_replay_en_las_cuatro_fases() {
        let (wire, journal) = decoded_fixture();
        let decoded = decodificar(&wire).unwrap();
        assert_eq!(super::super::codificar(&decoded).unwrap(), wire);

        let fases = [
            JournalState::Prepared,
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ];
        let mut journal_fase = journal.clone();
        for fase in fases {
            if journal_fase.state() != fase {
                journal_fase.advance(fase).unwrap();
            }
            let esperado = decoded
                .validar_replay(&journal_fase, fixtures::MOTOR, fixtures::POLICY)
                .unwrap();
            assert_eq!(esperado.transaction_id(), journal.transaction_id());
            assert!(esperado.parent() == journal.parent());
            assert!(esperado.old() == journal.old());
            assert!(esperado.new_fingerprint() == journal.new_fingerprint());
        }
    }

    #[test]
    fn roundtrip_conserva_el_binding_de_txid_parent_old_y_new() {
        let (wire, journal) = decoded_fixture();
        let decoded = decodificar(&wire).unwrap();
        let digest_distinto = RawTreeDigest::from_hex("f".repeat(64)).unwrap();
        let huella_distinta =
            |id| ArtifactFingerprint::new(fixtures::identidad(id), digest_distinto.clone());
        let alternativas = [
            Journal::new(
                "1123456789abcdef0123456789abcdef",
                journal.parent().clone(),
                journal.old().clone(),
                journal.new_fingerprint().clone(),
            )
            .unwrap(),
            Journal::new(
                journal.transaction_id(),
                fixtures::identidad(9),
                journal.old().clone(),
                journal.new_fingerprint().clone(),
            )
            .unwrap(),
            Journal::new(
                journal.transaction_id(),
                journal.parent().clone(),
                huella_distinta(10),
                journal.new_fingerprint().clone(),
            )
            .unwrap(),
            Journal::new(
                journal.transaction_id(),
                journal.parent().clone(),
                journal.old().clone(),
                huella_distinta(11),
            )
            .unwrap(),
        ];

        for otro in alternativas {
            let error = decoded
                .validar_replay(&otro, fixtures::MOTOR, fixtures::POLICY)
                .unwrap_err();
            assert_eq!(error.codigo(), "admission_envelope_journal_mismatch");
        }
    }

    #[test]
    fn rechaza_journal_no_canonico_no_prepared_y_contexto_invalido() {
        let (wire, journal) = decoded_fixture();
        let journal_bytes = journal.encode().unwrap();

        let mut noncanonical = journal_bytes.clone();
        noncanonical.insert(1, b' ');
        let mut wire_noncanonical = wire.clone();
        replace_journal(&mut wire_noncanonical, &noncanonical);
        assert_eq!(
            error(&wire_noncanonical),
            ErrorCodecEnvelopeAdmision::JournalInvalido
        );

        let mut backup_ready = journal;
        backup_ready.advance(JournalState::BackupReady).unwrap();
        let mut wire_nonprepared = wire.clone();
        replace_journal(&mut wire_nonprepared, &backup_ready.encode().unwrap());
        assert_eq!(
            error(&wire_nonprepared),
            ErrorCodecEnvelopeAdmision::JournalInvalido
        );

        let mut wire_contexto_invalido = wire;
        let start_contexto = context_start(&wire_contexto_invalido);
        wire_contexto_invalido[start_contexto] ^= 1;
        assert_eq!(
            error(&wire_contexto_invalido),
            ErrorCodecEnvelopeAdmision::ContextoInvalido
        );
    }
}
