//! Modelo y codec puro RETIRE v2; no abre archivos ni realiza I/O.

use std::fmt;

use crate::transaccion::anclado::identidad_archivo::MAX_PERSISTED_FILE_ID_FRAME_BYTES;
use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::v2::{JournalKind, JournalV2, MAX_JOURNAL_V2_BYTES};

mod binding;
mod codec;

pub(super) const RETIRE_V2_MAGIC: &[u8; 13] = b"FELIX-RETIRE\0";
pub(super) const RETIRE_V2_VERSION: u16 = 2;
const FIXED_RETIRE_V2_BYTES: usize = RETIRE_V2_MAGIC.len() + 2 + 2 + 3 * 2 + 8 + 8 + 32;
pub(super) const MAX_RETIRE_V2_BYTES: usize =
    FIXED_RETIRE_V2_BYTES + MAX_JOURNAL_V2_BYTES + 3 * MAX_PERSISTED_FILE_ID_FRAME_BYTES;

pub(super) const MAX_CONTEXT_ARTIFACT_V2_BYTES: u64 = 512 * 1024 * 1024;

#[derive(PartialEq, Eq)]
pub(super) enum ErrorCodecIntentRetiroV2 {
    TooLarge,
    Malformed,
    Allocation,
}

impl fmt::Debug for ErrorCodecIntentRetiroV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ErrorCodecIntentRetiroV2([redacted])")
    }
}

impl fmt::Display for ErrorCodecIntentRetiroV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid_retirement_envelope_v2")
    }
}

impl std::error::Error for ErrorCodecIntentRetiroV2 {}

/// Conserva el JournalV2 completo y las identidades como comparadores de datos.
#[derive(PartialEq, Eq)]
pub(super) struct IntentRetiroPersistidoV2 {
    journal: JournalV2,
    identidad_intent: IdentidadArchivoPersistida,
    identidad_journal: IdentidadArchivoPersistida,
    identidad_contexto: IdentidadArchivoPersistida,
    scope_parent: u64,
    longitud_contexto: u64,
    digest_contexto: [u8; 32],
}

impl fmt::Debug for IntentRetiroPersistidoV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IntentRetiroPersistidoV2([redacted])")
    }
}

#[derive(PartialEq, Eq)]
pub(super) struct ErrorIntentRetiroPersistidoV2;

impl fmt::Debug for ErrorIntentRetiroPersistidoV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ErrorIntentRetiroPersistidoV2")
    }
}

impl fmt::Display for ErrorIntentRetiroPersistidoV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid_persisted_retirement_intent_v2")
    }
}

impl std::error::Error for ErrorIntentRetiroPersistidoV2 {}

impl IntentRetiroPersistidoV2 {
    /// Codifica el mismo DTO v2 sin proyectar su Journal a v1.
    pub(super) fn encode(&self) -> Result<Vec<u8>, ErrorCodecIntentRetiroV2> {
        codec::encode(self)
    }

    /// El Journal se devuelve íntegro, sin proyectarlo al tipo v1.
    pub(super) fn journal(&self) -> &JournalV2 {
        &self.journal
    }

    /// Construye desde DTOs ya parseados; no consulta identidades nativas.
    pub(super) fn desde_datos_persistidos(
        journal: JournalV2,
        identidad_intent: IdentidadArchivoPersistida,
        identidad_journal: IdentidadArchivoPersistida,
        identidad_contexto: IdentidadArchivoPersistida,
        scope_parent: u64,
        longitud_contexto: u64,
        digest_contexto: [u8; 32],
    ) -> Result<Self, ErrorIntentRetiroPersistidoV2> {
        Self::new(
            journal,
            identidad_intent,
            identidad_journal,
            identidad_contexto,
            scope_parent,
            longitud_contexto,
            digest_contexto,
        )
    }

    fn new(
        journal: JournalV2,
        identidad_intent: IdentidadArchivoPersistida,
        identidad_journal: IdentidadArchivoPersistida,
        identidad_contexto: IdentidadArchivoPersistida,
        scope_parent: u64,
        longitud_contexto: u64,
        digest_contexto: [u8; 32],
    ) -> Result<Self, ErrorIntentRetiroPersistidoV2> {
        let record = Self {
            journal,
            identidad_intent,
            identidad_journal,
            identidad_contexto,
            scope_parent,
            longitud_contexto,
            digest_contexto,
        };
        record.validar()?;
        Ok(record)
    }

    fn validar(&self) -> Result<(), ErrorIntentRetiroPersistidoV2> {
        if self.journal.kind() != JournalKind::ContextRequired
            || self.journal.state() != JournalState::Committed
            || self.longitud_contexto == 0
            || self.longitud_contexto > MAX_CONTEXT_ARTIFACT_V2_BYTES
        {
            return Err(ErrorIntentRetiroPersistidoV2);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
    use crate::transaccion::journal::v2::{JournalKind, JournalV2};
    use crate::transaccion::journal::{
        ArtifactFingerprint, DirectoryIdentity, JournalState, RawTreeDigest,
    };

    use super::{
        ErrorIntentRetiroPersistidoV2, IntentRetiroPersistidoV2, MAX_CONTEXT_ARTIFACT_V2_BYTES,
    };

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
    const CONTEXT_DIGEST: [u8; 32] = [0xD4; 32];

    fn identity_file_id(value: u64) -> IdentidadArchivoPersistida {
        #[cfg(unix)]
        let payload = format!(r#"{{"platform":"unix-v1","device":17,"inode":{value}}}"#);
        #[cfg(windows)]
        let payload =
            format!(r#"{{"platform":"windows-v1","volume":17,"file_id":"{value:032x}"}}"#);

        let mut frame = b"FELIX-FILEID\0".to_vec();
        frame.extend_from_slice(&1_u16.to_be_bytes());
        let payload_length = u32::try_from(payload.len()).unwrap();
        frame.extend_from_slice(&payload_length.to_be_bytes());
        frame.extend_from_slice(payload.as_bytes());
        IdentidadArchivoPersistida::decode(&mut Cursor::new(frame)).unwrap()
    }

    fn directory_identity(value: u64) -> DirectoryIdentity {
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(17, value)
        }
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(17, format!("{value:032x}"))
        }
    }

    fn journal(state: JournalState, transaction_id: &str, parent: u64) -> JournalV2 {
        let digest_old = RawTreeDigest::from_digest([0x11; 32]);
        let digest_new = RawTreeDigest::from_digest([0x22; 32]);
        let mut record = JournalV2::new(
            transaction_id,
            directory_identity(parent),
            ArtifactFingerprint::new(directory_identity(31), digest_old),
            ArtifactFingerprint::new(directory_identity(32), digest_new),
        )
        .unwrap();
        for next in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            if record.state() == state {
                break;
            }
            record.advance(next).unwrap();
        }
        record
    }

    fn record(
        state: JournalState,
        context_length: u64,
    ) -> Result<IntentRetiroPersistidoV2, ErrorIntentRetiroPersistidoV2> {
        IntentRetiroPersistidoV2::desde_datos_persistidos(
            journal(state, TRANSACTION_ID, 29),
            identity_file_id(41),
            identity_file_id(42),
            identity_file_id(43),
            53,
            context_length,
            CONTEXT_DIGEST,
        )
    }

    #[test]
    fn committed_model_retains_full_journal_and_redacts_debug() {
        let journal = journal(JournalState::Committed, TRANSACTION_ID, 29);
        let record = record(JournalState::Committed, 1).unwrap();

        assert!(record.journal() == &journal);
        assert_eq!(record.journal().kind(), JournalKind::ContextRequired);
        assert_eq!(record.journal().state(), JournalState::Committed);
        assert_eq!(
            format!("{record:?}"),
            "IntentRetiroPersistidoV2([redacted])"
        );
        assert!(!format!("{record:?}").contains(TRANSACTION_ID));
        assert!(!format!("{record:?}").contains("00000000000000000000000000000029"));
    }

    #[test]
    fn constructor_requires_committed_state_and_bounded_context_length() {
        for state in [
            JournalState::Prepared,
            JournalState::BackupReady,
            JournalState::Installed,
        ] {
            assert!(
                IntentRetiroPersistidoV2::desde_datos_persistidos(
                    journal(state, TRANSACTION_ID, 29),
                    identity_file_id(41),
                    identity_file_id(42),
                    identity_file_id(43),
                    53,
                    1,
                    CONTEXT_DIGEST,
                )
                .is_err()
            );
        }

        for length in [0, MAX_CONTEXT_ARTIFACT_V2_BYTES + 1] {
            let error = record(JournalState::Committed, length).unwrap_err();
            assert_eq!(format!("{error:?}"), "ErrorIntentRetiroPersistidoV2");
            assert_eq!(error.to_string(), "invalid_persisted_retirement_intent_v2");
        }
        assert!(record(JournalState::Committed, 1).is_ok());
        assert!(record(JournalState::Committed, MAX_CONTEXT_ARTIFACT_V2_BYTES).is_ok());
    }

    #[test]
    fn validation_error_has_fixed_redacted_text() {
        let error = record(JournalState::Prepared, 1).unwrap_err();
        assert_eq!(format!("{error:?}"), "ErrorIntentRetiroPersistidoV2");
        assert_eq!(error.to_string(), "invalid_persisted_retirement_intent_v2");
    }
}
