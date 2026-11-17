//! Modelo y codec puro RETIRE v3; conserva el Context original como bytes.

use std::fmt;
use std::ops::Range;

use crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2;
use crate::transaccion::anclado::intent_persistido_v2::IntentRetiroPersistidoV2;
use crate::transaccion::anclado::intent_persistido_v3_preflight::MAX_CONTEXT_BYTES;
use sha2::{Digest, Sha256};

pub(super) mod archivo;
mod codec;
pub(super) mod familia;
mod lector;
mod replay;
pub(super) mod selector;
pub(super) use lector::ErrorLecturaIntentRetiroV3;

/// Frame autocontenido de retiro; no observa archivos ni concede autoridad.
pub(super) struct IntentRetiroPersistidoV3 {
    metadata: IntentRetiroPersistidoV2,
    frame: Vec<u8>,
    contexto: Range<usize>,
}

impl fmt::Debug for IntentRetiroPersistidoV3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IntentRetiroPersistidoV3([redacted])")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorCodecIntentRetiroV3 {
    TooLarge,
    Malformed,
    Allocation,
}

impl ErrorCodecIntentRetiroV3 {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::TooLarge => "retirement_v3_too_large",
            Self::Malformed => "retirement_v3_malformed",
            Self::Allocation => "retirement_v3_allocation_failed",
        }
    }
}

impl fmt::Debug for ErrorCodecIntentRetiroV3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl fmt::Display for ErrorCodecIntentRetiroV3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ErrorCodecIntentRetiroV3 {}

impl IntentRetiroPersistidoV3 {
    /// Lee un frame completo con preflight antes del payload. No valida ownership filesystem.
    pub(super) fn leer<R: std::io::Read>(
        reader: &mut R,
    ) -> Result<Self, ErrorLecturaIntentRetiroV3> {
        lector::leer(reader)
    }

    /// Decodifica un buffer poseído y conserva el payload como un rango del mismo Vec.
    pub(super) fn decode(frame: Vec<u8>) -> Result<Self, ErrorCodecIntentRetiroV3> {
        let (metadata_range, contexto) = codec::rangos(&frame)?;
        let metadata = IntentRetiroPersistidoV2::decode(&frame[metadata_range])
            .map_err(|_| ErrorCodecIntentRetiroV3::Malformed)?;
        validar_contexto(&metadata, &frame[contexto.clone()])?;
        Ok(Self {
            metadata,
            frame,
            contexto,
        })
    }

    /// Compone un frame nuevo; el payload original se copia byte por byte.
    pub(super) fn encode(
        metadata: &IntentRetiroPersistidoV2,
        contexto_original: &[u8],
    ) -> Result<Vec<u8>, ErrorCodecIntentRetiroV3> {
        validar_contexto(metadata, contexto_original)?;
        codec::codificar(metadata, contexto_original)
    }

    pub(super) fn metadata(&self) -> &IntentRetiroPersistidoV2 {
        &self.metadata
    }

    pub(super) fn contexto_original(&self) -> &[u8] {
        &self.frame[self.contexto.clone()]
    }
}

fn validar_contexto(
    metadata: &IntentRetiroPersistidoV2,
    contexto_original: &[u8],
) -> Result<(), ErrorCodecIntentRetiroV3> {
    if contexto_original.is_empty() {
        return Err(ErrorCodecIntentRetiroV3::Malformed);
    }
    if u64::try_from(contexto_original.len()).map_err(|_| ErrorCodecIntentRetiroV3::TooLarge)?
        > MAX_CONTEXT_BYTES
    {
        return Err(ErrorCodecIntentRetiroV3::TooLarge);
    }
    let longitud =
        u64::try_from(contexto_original.len()).map_err(|_| ErrorCodecIntentRetiroV3::TooLarge)?;
    let (longitud_esperada, digest_esperado) = metadata.evidencia_contexto_original();
    let digest: [u8; 32] = Sha256::digest(contexto_original).into();
    if longitud != longitud_esperada || digest != digest_esperado {
        return Err(ErrorCodecIntentRetiroV3::Malformed);
    }

    let contexto = EnvelopeContextoAdmisionV2::decode(contexto_original)
        .map_err(|_| ErrorCodecIntentRetiroV3::Malformed)?;
    contexto
        .coincide_estructuralmente(metadata.journal())
        .map_err(|_| ErrorCodecIntentRetiroV3::Malformed)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2;
    use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
    use crate::transaccion::journal::v2::{JournalKind, JournalV2};
    use crate::transaccion::journal::{
        ArtifactFingerprint, DirectoryIdentity, JournalState, RawTreeDigest,
    };
    use sha2::{Digest, Sha256};

    use super::{ErrorCodecIntentRetiroV3, IntentRetiroPersistidoV3};
    use crate::transaccion::anclado::intent_persistido_v2::IntentRetiroPersistidoV2;
    use crate::transaccion::anclado::intent_persistido_v3_preflight::{
        HEADER_BYTES, KIND_CONTEXT_V2, RETIRE_V3_MAGIC, RETIRE_V3_VERSION,
    };

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
    const CONTEXT_MARKER: &[u8] = b"; private-retire-v3-context\n";

    pub(super) fn file_identity(seed: u64) -> IdentidadArchivoPersistida {
        #[cfg(unix)]
        let payload = format!(r#"{{"platform":"unix-v1","device":17,"inode":{seed}}}"#);
        #[cfg(windows)]
        let payload = format!(r#"{{"platform":"windows-v1","volume":17,"file_id":"{seed:032x}"}}"#);
        let mut frame = b"FELIX-FILEID\0".to_vec();
        frame.extend_from_slice(&1_u16.to_be_bytes());
        frame.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
        frame.extend_from_slice(payload.as_bytes());
        IdentidadArchivoPersistida::decode(&mut Cursor::new(frame)).unwrap()
    }

    fn directory_identity(seed: u64) -> DirectoryIdentity {
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(17, seed)
        }
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(17, format!("{seed:032x}"))
        }
    }

    fn journal(transaction_id: &str, state: JournalState) -> JournalV2 {
        let mut journal = JournalV2::new(
            transaction_id,
            directory_identity(29),
            ArtifactFingerprint::new(
                directory_identity(31),
                RawTreeDigest::from_digest([0x11; 32]),
            ),
            ArtifactFingerprint::new(
                directory_identity(32),
                RawTreeDigest::from_digest([0x22; 32]),
            ),
        )
        .unwrap();
        for next in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            if journal.state() == state {
                break;
            }
            journal.advance(next).unwrap();
        }
        journal
    }

    fn context_frame(journal: &JournalV2, marker: &[u8]) -> Vec<u8> {
        let context = crate::transaccion::admision::envelope::tests::contexto(marker);
        EnvelopeContextoAdmisionV2::ligar(context, journal)
            .encode()
            .unwrap()
    }

    pub(super) fn metadata(
        journal: JournalV2,
        length: u64,
        digest: [u8; 32],
    ) -> IntentRetiroPersistidoV2 {
        IntentRetiroPersistidoV2::desde_datos_persistidos(
            journal,
            file_identity(41),
            file_identity(42),
            file_identity(43),
            53,
            length,
            digest,
        )
        .unwrap()
    }

    pub(super) fn committed_fixture() -> (JournalV2, Vec<u8>, IntentRetiroPersistidoV2) {
        let prepared = journal(TRANSACTION_ID, JournalState::Prepared);
        let committed = journal(TRANSACTION_ID, JournalState::Committed);
        let context = context_frame(&prepared, CONTEXT_MARKER);
        let digest: [u8; 32] = Sha256::digest(&context).into();
        let metadata = metadata(
            committed.clone(),
            u64::try_from(context.len()).unwrap(),
            digest,
        );
        (committed, context, metadata)
    }

    #[test]
    fn frame_roundtrip_retains_the_original_context_range_and_redacts_debug() {
        let (journal, context, metadata) = committed_fixture();
        let frame = IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap();
        assert_eq!(&frame[..RETIRE_V3_MAGIC.len()], RETIRE_V3_MAGIC);
        assert_eq!(
            u16::from_be_bytes(frame[13..15].try_into().unwrap()),
            RETIRE_V3_VERSION
        );
        assert_eq!(frame[15], KIND_CONTEXT_V2);
        let metadata_length = usize::from(u16::from_be_bytes(frame[16..18].try_into().unwrap()));
        assert_eq!(
            u64::from_be_bytes(frame[18..26].try_into().unwrap()),
            u64::try_from(context.len()).unwrap()
        );

        let decoded = IntentRetiroPersistidoV3::decode(frame.clone()).unwrap();
        assert!(decoded.metadata().journal() == &journal);
        assert_eq!(
            decoded.metadata().journal().kind(),
            JournalKind::ContextRequired
        );
        assert_eq!(
            decoded.metadata().journal().state(),
            JournalState::Committed
        );
        assert!(decoded.contexto_original() == context.as_slice());
        assert!(EnvelopeContextoAdmisionV2::decode(decoded.contexto_original()).is_ok());
        assert!(
            decoded.contexto_original().as_ptr()
                == decoded
                    .frame
                    .as_ptr()
                    .wrapping_add(HEADER_BYTES + metadata_length)
        );
        assert!(decoded.frame == frame);
        assert_eq!(
            format!("{decoded:?}"),
            "IntentRetiroPersistidoV3([redacted])"
        );
        assert!(!format!("{decoded:?}").contains(TRANSACTION_ID));
        assert!(!format!("{decoded:?}").contains("private-retire-v3-context"));
    }

    #[test]
    fn codec_requires_committed_metadata_and_exact_context_evidence_and_binding() {
        let prepared = journal(TRANSACTION_ID, JournalState::Prepared);
        let committed = journal(TRANSACTION_ID, JournalState::Committed);
        let context = context_frame(&prepared, CONTEXT_MARKER);
        let digest: [u8; 32] = Sha256::digest(&context).into();
        let length = u64::try_from(context.len()).unwrap();

        assert!(
            IntentRetiroPersistidoV2::desde_datos_persistidos(
                prepared,
                file_identity(41),
                file_identity(42),
                file_identity(43),
                53,
                length,
                digest,
            )
            .is_err()
        );
        assert!(
            IntentRetiroPersistidoV3::encode(
                &metadata(committed.clone(), length + 1, digest),
                &context,
            )
            .is_err()
        );
        assert!(
            IntentRetiroPersistidoV3::encode(
                &metadata(committed.clone(), length, [0x7a; 32]),
                &context,
            )
            .is_err()
        );
        let mut corrupted = IntentRetiroPersistidoV3::encode(
            &metadata(committed.clone(), length, digest),
            &context,
        )
        .unwrap();
        let last_byte = corrupted.len() - 1;
        corrupted[last_byte] ^= 1;
        assert!(IntentRetiroPersistidoV3::decode(corrupted).is_err());

        let other_prepared = journal("1123456789abcdef0123456789abcdef", JournalState::Prepared);
        let other_context = context_frame(&other_prepared, CONTEXT_MARKER);
        let other_digest: [u8; 32] = Sha256::digest(&other_context).into();
        let mismatched = metadata(
            committed,
            u64::try_from(other_context.len()).unwrap(),
            other_digest,
        );
        assert!(IntentRetiroPersistidoV3::encode(&mismatched, &other_context).is_err());
    }
    #[test]
    fn bounded_reader_accepts_exact_frame_and_rejects_truncation_and_trailing_bytes() {
        let (_, context, metadata) = committed_fixture();
        let frame = IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap();
        let decoded = IntentRetiroPersistidoV3::leer(&mut Cursor::new(&frame)).unwrap();
        assert_eq!(decoded.contexto_original(), context);
        for cut in [0, HEADER_BYTES - 1, HEADER_BYTES, frame.len() - 1] {
            assert!(IntentRetiroPersistidoV3::leer(&mut Cursor::new(&frame[..cut])).is_err());
        }
        let mut trailing = frame;
        trailing.push(0);
        assert!(IntentRetiroPersistidoV3::leer(&mut Cursor::new(trailing)).is_err());
    }

    #[test]
    fn bounded_reader_rejects_oversized_header_without_reading_payload() {
        let (_, context, metadata) = committed_fixture();
        let mut frame = IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap();
        frame[18..26].copy_from_slice(&u64::MAX.to_be_bytes());
        let mut reader = Cursor::new(frame);
        assert!(matches!(
            IntentRetiroPersistidoV3::leer(&mut reader),
            Err(super::ErrorLecturaIntentRetiroV3::Codec(
                ErrorCodecIntentRetiroV3::TooLarge
            ))
        ));
        assert_eq!(reader.position(), HEADER_BYTES as u64);
    }

    #[test]
    fn bounded_reader_preserves_io_failures_and_retries_interrupted_reads() {
        struct Fragmented<'a> {
            bytes: &'a [u8],
            interrupted: bool,
        }
        impl std::io::Read for Fragmented<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.interrupted = !self.interrupted;
                if self.interrupted {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                let count = buffer.len().min(self.bytes.len()).min(7);
                buffer[..count].copy_from_slice(&self.bytes[..count]);
                self.bytes = &self.bytes[count..];
                Ok(count)
            }
        }
        let (_, context, metadata) = committed_fixture();
        let frame = IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap();
        let mut reader = Fragmented {
            bytes: &frame,
            interrupted: false,
        };
        assert_eq!(
            IntentRetiroPersistidoV3::leer(&mut reader)
                .unwrap()
                .contexto_original(),
            context
        );
        struct Failed;
        impl std::io::Read for Failed {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("private-path-must-not-escape"))
            }
        }
        assert!(matches!(
            IntentRetiroPersistidoV3::leer(&mut Failed),
            Err(super::ErrorLecturaIntentRetiroV3::Io)
        ));
    }
    #[test]
    fn self_contained_replay_preserves_committed_journal_and_rejects_unsupported_policy() {
        use crate::serializar::serializar_candidato;
        use crate::transaccion::replay::{
            reproponer,
            semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1},
        };
        use crate::transaccion::snapshot::calcular_digest_raw_tree;
        use crate::transaccion::stage::{inventario_objetivo_para_replay, planificar_staging};
        use universo_core::ResultadoPropuesta;
        let context = crate::transaccion::admision::envelope::tests::contexto_con_tags(
            CONTEXT_MARKER,
            VERSION_MOTOR_V1,
            POLICY_ID_V1,
        );
        let proposal = reproponer(&context, VERSION_MOTOR_V1, POLICY_ID_V1).unwrap();
        let ResultadoPropuesta::Admitida(candidate) = &proposal.resultado else {
            panic!("fixture rejected")
        };
        let documents =
            serializar_candidato(proposal.snapshot.estructura(), candidate, &[]).unwrap();
        let plan = planificar_staging(&proposal.snapshot, &documents).unwrap();
        let entries = inventario_objetivo_para_replay(&plan).unwrap();
        let mut committed = JournalV2::new(
            TRANSACTION_ID,
            directory_identity(29),
            ArtifactFingerprint::new(
                directory_identity(31),
                calcular_digest_raw_tree(&context.entradas_base()).unwrap(),
            ),
            ArtifactFingerprint::new(
                directory_identity(32),
                calcular_digest_raw_tree(&entries).unwrap(),
            ),
        )
        .unwrap();
        let original = EnvelopeContextoAdmisionV2::ligar(context, &committed)
            .encode()
            .unwrap();
        for state in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            committed.advance(state).unwrap();
        }
        let meta = metadata(
            committed.clone(),
            original.len() as u64,
            Sha256::digest(&original).into(),
        );
        let intent = IntentRetiroPersistidoV3::decode(
            IntentRetiroPersistidoV3::encode(&meta, &original).unwrap(),
        )
        .unwrap();
        let replay = intent.calcular_new_esperado().unwrap();
        assert!(replay.journal() == &committed);
        assert!(replay.digest() == &calcular_digest_raw_tree(&entries).unwrap());
        let (_, bytes, meta) = committed_fixture();
        let unsupported = IntentRetiroPersistidoV3::decode(
            IntentRetiroPersistidoV3::encode(&meta, &bytes).unwrap(),
        )
        .unwrap();
        assert!(unsupported.calcular_new_esperado().is_err());
    }
}
