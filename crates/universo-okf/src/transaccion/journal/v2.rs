//! Journal interno v2: modelo y codec puro; no abre archivos.

use std::fmt;

use super::{ArtifactFingerprint, DirectoryIdentity, JournalError, JournalState, is_lower_hex};

mod codec;

pub(crate) const MAX_JOURNAL_V2_BYTES: usize = 1024;

const SCHEMA_V2: &str = "felix-journal-v2";

/// Clase cerrada de transacción que requiere Context anclado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JournalKind {
    ContextRequired,
}

/// Registro v2 de dominio. La representación de depuración omite sus datos.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct JournalV2 {
    transaction_id: String,
    kind: JournalKind,
    state: JournalState,
    parent: DirectoryIdentity,
    old: ArtifactFingerprint,
    new: ArtifactFingerprint,
}

impl JournalV2 {
    pub(crate) fn encode(&self) -> Result<Vec<u8>, JournalV2Error> {
        codec::encode(self)
    }

    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, JournalV2Error> {
        codec::parse(bytes)
    }

    pub(crate) fn new(
        transaction_id: impl Into<String>,
        parent: DirectoryIdentity,
        old: ArtifactFingerprint,
        new: ArtifactFingerprint,
    ) -> Result<Self, JournalV2Error> {
        let record = Self {
            transaction_id: transaction_id.into(),
            kind: JournalKind::ContextRequired,
            state: JournalState::Prepared,
            parent,
            old,
            new,
        };
        record.validate()?;
        Ok(record)
    }

    pub(crate) fn schema(&self) -> &'static str {
        SCHEMA_V2
    }

    pub(crate) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(crate) fn kind(&self) -> JournalKind {
        self.kind
    }

    pub(crate) fn state(&self) -> JournalState {
        self.state
    }

    pub(crate) fn parent(&self) -> &DirectoryIdentity {
        &self.parent
    }

    pub(crate) fn old(&self) -> &ArtifactFingerprint {
        &self.old
    }

    pub(crate) fn new_fingerprint(&self) -> &ArtifactFingerprint {
        &self.new
    }

    pub(crate) fn advance(&mut self, next: JournalState) -> Result<(), JournalV2Error> {
        self.state.advance(next).map_err(Into::into)
    }

    pub(crate) fn validate_successor(&self, candidate: &Self) -> Result<(), JournalV2Error> {
        let next = self
            .state
            .successor()
            .ok_or(JournalV2Error::TransitionInvalid)?;
        if candidate.state != next {
            return Err(JournalV2Error::TransitionInvalid);
        }

        let mut expected = self.clone();
        expected.advance(next)?;
        if &expected == candidate {
            Ok(())
        } else {
            Err(JournalV2Error::TransitionInvalid)
        }
    }

    fn validate(&self) -> Result<(), JournalV2Error> {
        if !is_lower_hex(&self.transaction_id, 32) {
            return Err(JournalV2Error::TransactionIdInvalid);
        }
        self.parent.validate()?;
        self.old.validate()?;
        self.new.validate()?;
        if self.old == self.new {
            return Err(JournalV2Error::FingerprintInvalid);
        }
        Ok(())
    }
}

impl fmt::Debug for JournalV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JournalV2([redacted])")
    }
}

/// Error v2 con códigos fijos; no conserva mensajes ni valores de los DTO.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum JournalV2Error {
    Malformed,
    TooLarge,
    SchemaUnsupported,
    KindUnsupported,
    DigestVersionUnsupported,
    TransactionIdInvalid,
    DigestInvalid,
    IdentityInvalid,
    PlatformInvalid,
    FingerprintInvalid,
    TransitionInvalid,
}

impl JournalV2Error {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Malformed => "journal_v2_malformed",
            Self::TooLarge => "journal_v2_too_large",
            Self::SchemaUnsupported => "journal_v2_schema_unsupported",
            Self::KindUnsupported => "journal_v2_kind_unsupported",
            Self::DigestVersionUnsupported => "journal_v2_digest_version_unsupported",
            Self::TransactionIdInvalid => "journal_v2_transaction_id_invalid",
            Self::DigestInvalid => "journal_v2_digest_invalid",
            Self::IdentityInvalid => "journal_v2_identity_invalid",
            Self::PlatformInvalid => "journal_v2_platform_invalid",
            Self::FingerprintInvalid => "journal_v2_fingerprint_invalid",
            Self::TransitionInvalid => "journal_v2_transition_invalid",
        }
    }
}

impl From<JournalError> for JournalV2Error {
    fn from(error: JournalError) -> Self {
        match error {
            JournalError::Malformed => Self::Malformed,
            JournalError::TooLarge => Self::TooLarge,
            JournalError::UnsupportedVersion => Self::SchemaUnsupported,
            JournalError::UnsupportedDigestVersion => Self::DigestVersionUnsupported,
            JournalError::InvalidTransactionId => Self::TransactionIdInvalid,
            JournalError::InvalidDigest => Self::DigestInvalid,
            JournalError::InvalidIdentity => Self::IdentityInvalid,
            JournalError::InvalidPlatform => Self::PlatformInvalid,
            JournalError::InvalidFingerprint => Self::FingerprintInvalid,
            JournalError::InvalidTransition => Self::TransitionInvalid,
        }
    }
}

impl fmt::Debug for JournalV2Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl fmt::Display for JournalV2Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for JournalV2Error {}

#[cfg(test)]
mod tests {
    use super::super::RawTreeDigest;
    use super::*;

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

    fn identity(id: u64) -> DirectoryIdentity {
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(7, id)
        }
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(7, format!("{id:032x}"))
        }
    }

    fn fingerprint(id: u64, byte: char) -> ArtifactFingerprint {
        ArtifactFingerprint::new(
            identity(id),
            RawTreeDigest::from_hex(byte.to_string().repeat(64)).unwrap(),
        )
    }

    fn record() -> JournalV2 {
        JournalV2::new(
            TRANSACTION_ID,
            identity(1),
            fingerprint(2, 'a'),
            fingerprint(3, 'b'),
        )
        .unwrap()
    }

    #[test]
    fn constructor_validates_identity_header_and_distinct_fingerprints() {
        let record = record();
        assert_eq!(record.schema(), SCHEMA_V2);
        assert_eq!(record.kind(), JournalKind::ContextRequired);
        assert_eq!(record.transaction_id(), TRANSACTION_ID);
        assert_eq!(record.state(), JournalState::Prepared);
        assert!(record.parent() == &identity(1));
        assert!(record.old() == &fingerprint(2, 'a'));
        assert!(record.new_fingerprint() == &fingerprint(3, 'b'));
        assert_eq!(
            JournalV2::new(
                &TRANSACTION_ID[..31],
                identity(1),
                fingerprint(2, 'a'),
                fingerprint(3, 'b'),
            )
            .unwrap_err(),
            JournalV2Error::TransactionIdInvalid
        );
        assert_eq!(
            JournalV2::new(
                "0123456789ABCDEF0123456789abcdef",
                identity(1),
                fingerprint(2, 'a'),
                fingerprint(3, 'b'),
            )
            .unwrap_err(),
            JournalV2Error::TransactionIdInvalid
        );
        let old = fingerprint(2, 'a');
        assert_eq!(
            JournalV2::new(TRANSACTION_ID, identity(1), old.clone(), old).unwrap_err(),
            JournalV2Error::FingerprintInvalid
        );
        assert_eq!(format!("{record:?}"), "JournalV2([redacted])");
    }

    #[test]
    fn constructor_maps_typed_dto_validation_errors() {
        let unsupported_digest = ArtifactFingerprint::new(
            identity(2),
            RawTreeDigest {
                version: "future-digest".to_owned(),
                hex: "a".repeat(64),
            },
        );
        assert_eq!(
            JournalV2::new(
                TRANSACTION_ID,
                identity(1),
                unsupported_digest,
                fingerprint(3, 'b'),
            )
            .unwrap_err(),
            JournalV2Error::DigestVersionUnsupported
        );

        let invalid_digest = ArtifactFingerprint::new(
            identity(2),
            RawTreeDigest {
                version: "felix-raw-tree-sha256-v1".to_owned(),
                hex: "A".repeat(64),
            },
        );
        assert_eq!(
            JournalV2::new(
                TRANSACTION_ID,
                identity(1),
                invalid_digest,
                fingerprint(3, 'b'),
            )
            .unwrap_err(),
            JournalV2Error::DigestInvalid
        );

        let invalid_identity = DirectoryIdentity::Windows {
            volume: 7,
            file_id: "../secret".to_owned(),
        };
        assert_eq!(
            JournalV2::new(
                TRANSACTION_ID,
                identity(1),
                ArtifactFingerprint::new(
                    invalid_identity,
                    RawTreeDigest::from_hex("a".repeat(64)).unwrap()
                ),
                fingerprint(3, 'b'),
            )
            .unwrap_err(),
            JournalV2Error::IdentityInvalid
        );

        #[cfg(unix)]
        let foreign = DirectoryIdentity::Windows {
            volume: 7,
            file_id: "00000000000000000000000000000002".to_owned(),
        };
        #[cfg(windows)]
        let foreign = DirectoryIdentity::Unix {
            device: 7,
            inode: 2,
        };
        assert_eq!(
            JournalV2::new(
                TRANSACTION_ID,
                foreign,
                fingerprint(2, 'a'),
                fingerprint(3, 'b'),
            )
            .unwrap_err(),
            JournalV2Error::PlatformInvalid
        );
    }

    #[test]
    fn transitions_are_sequential_and_idempotent() {
        let mut record = record();
        for state in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            record.advance(state).unwrap();
            assert_eq!(record.state(), state);
            record.advance(state).unwrap();
        }
        assert_eq!(
            record.advance(JournalState::Prepared),
            Err(JournalV2Error::TransitionInvalid)
        );
    }

    #[test]
    fn successor_requires_the_next_state_and_identical_payload() {
        let current = record();
        let mut successor = current.clone();
        successor.advance(JournalState::BackupReady).unwrap();
        assert_eq!(current.validate_successor(&successor), Ok(()));

        let mut changed_state = successor.clone();
        changed_state.state = JournalState::Committed;
        let mut changed_id = successor.clone();
        changed_id.transaction_id = "f".repeat(32);
        let mut changed_parent = successor.clone();
        changed_parent.parent = identity(9);
        let mut changed_old = successor.clone();
        changed_old.old = fingerprint(8, 'c');
        let mut changed_new = successor;
        changed_new.new = fingerprint(9, 'd');

        for candidate in [
            changed_state,
            changed_id,
            changed_parent,
            changed_old,
            changed_new,
        ] {
            assert_eq!(
                current.validate_successor(&candidate),
                Err(JournalV2Error::TransitionInvalid)
            );
        }
    }

    #[test]
    fn journal_errors_map_to_fixed_redacted_v2_codes() {
        let cases = [
            (JournalError::Malformed, JournalV2Error::Malformed),
            (JournalError::TooLarge, JournalV2Error::TooLarge),
            (
                JournalError::UnsupportedVersion,
                JournalV2Error::SchemaUnsupported,
            ),
            (
                JournalError::UnsupportedDigestVersion,
                JournalV2Error::DigestVersionUnsupported,
            ),
            (
                JournalError::InvalidTransactionId,
                JournalV2Error::TransactionIdInvalid,
            ),
            (JournalError::InvalidDigest, JournalV2Error::DigestInvalid),
            (
                JournalError::InvalidIdentity,
                JournalV2Error::IdentityInvalid,
            ),
            (
                JournalError::InvalidPlatform,
                JournalV2Error::PlatformInvalid,
            ),
            (
                JournalError::InvalidFingerprint,
                JournalV2Error::FingerprintInvalid,
            ),
            (
                JournalError::InvalidTransition,
                JournalV2Error::TransitionInvalid,
            ),
        ];
        for (source, expected) in cases {
            let error = JournalV2Error::from(source);
            assert_eq!(error, expected);
            assert_eq!(format!("{error:?}"), error.code());
            assert_eq!(error.to_string(), error.code());
        }
        assert_eq!(
            JournalV2Error::KindUnsupported.code(),
            "journal_v2_kind_unsupported"
        );
    }
}
