//! Protocolo puro y versionado para registrar una publicación transaccional.
//!
//! No abre rutas ni ejecuta operaciones del filesystem. El caller aporta las
//! identidades y fingerprints observados; recovery consume datos ya clasificados.

use std::fmt;

use serde::{Deserialize, Serialize};

mod codec;
pub(crate) mod recovery;
pub(crate) mod v2;

const SCHEMA_V1: &str = "felix-journal-v1";
const RAW_TREE_DIGEST_V1: &str = "felix-raw-tree-sha256-v1";
/// Tamaño máximo del JSON canónico persistido del journal, en bytes.
pub(crate) const MAX_JOURNAL_BYTES: usize = 1024;

/// Registro v1. La representación de depuración omite toda identidad y digest.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Journal {
    schema: String,
    transaction_id: String,
    state: JournalState,
    parent: DirectoryIdentity,
    old: ArtifactFingerprint,
    new: ArtifactFingerprint,
}

impl Journal {
    pub(crate) fn new(
        transaction_id: impl Into<String>,
        parent: DirectoryIdentity,
        old: ArtifactFingerprint,
        new: ArtifactFingerprint,
    ) -> Result<Self, JournalError> {
        let record = Self {
            schema: SCHEMA_V1.to_owned(),
            transaction_id: transaction_id.into(),
            state: JournalState::Prepared,
            parent,
            old,
            new,
        };
        record.validate()?;
        Ok(record)
    }

    pub(crate) fn state(&self) -> JournalState {
        self.state
    }

    pub(crate) fn transaction_id(&self) -> &str {
        &self.transaction_id
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

    pub(crate) fn advance(&mut self, next: JournalState) -> Result<(), JournalError> {
        self.state.advance(next)
    }

    pub(crate) fn validate_successor(&self, candidate: &Self) -> Result<(), JournalError> {
        let next = self
            .state
            .successor()
            .ok_or(JournalError::InvalidTransition)?;
        if candidate.state != next {
            return Err(JournalError::InvalidTransition);
        }

        let mut expected = self.clone();
        expected.advance(next)?;
        if &expected == candidate {
            Ok(())
        } else {
            Err(JournalError::InvalidTransition)
        }
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, JournalError> {
        codec::encode(self)
    }

    fn validate(&self) -> Result<(), JournalError> {
        if self.schema != SCHEMA_V1 {
            return Err(JournalError::UnsupportedVersion);
        }
        if !is_lower_hex(&self.transaction_id, 32) {
            return Err(JournalError::InvalidTransactionId);
        }
        self.parent.validate()?;
        self.old.validate()?;
        self.new.validate()?;
        if self.old == self.new {
            return Err(JournalError::InvalidFingerprint);
        }
        Ok(())
    }
}

impl fmt::Debug for Journal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Journal([redacted])")
    }
}

/// Estado persistido; las únicas transiciones válidas son monotónicas y contiguas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JournalState {
    Prepared,
    BackupReady,
    Installed,
    Committed,
}

impl JournalState {
    fn successor(self) -> Option<Self> {
        match self {
            Self::Prepared => Some(Self::BackupReady),
            Self::BackupReady => Some(Self::Installed),
            Self::Installed => Some(Self::Committed),
            Self::Committed => None,
        }
    }

    fn advance(&mut self, next: Self) -> Result<(), JournalError> {
        if *self == next {
            return Ok(());
        }
        if (*self).successor() != Some(next) {
            return Err(JournalError::InvalidTransition);
        }
        *self = next;
        Ok(())
    }
}

/// Identidad de directorio con representación serializable del sistema actual.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "platform", deny_unknown_fields)]
pub(crate) enum DirectoryIdentity {
    #[serde(rename = "unix-v1")]
    Unix { device: u64, inode: u64 },
    #[serde(rename = "windows-v1")]
    Windows { volume: u64, file_id: String },
}

impl DirectoryIdentity {
    fn validate(&self) -> Result<(), JournalError> {
        if let Self::Windows { file_id, .. } = self
            && !is_lower_hex(file_id, 32)
        {
            return Err(JournalError::InvalidIdentity);
        }
        let current = match self {
            #[cfg(unix)]
            Self::Unix { .. } => true,
            #[cfg(not(unix))]
            Self::Unix { .. } => false,
            #[cfg(windows)]
            Self::Windows { .. } => true,
            #[cfg(not(windows))]
            Self::Windows { .. } => false,
        };
        if current {
            Ok(())
        } else {
            Err(JournalError::InvalidPlatform)
        }
    }

    #[cfg(unix)]
    pub(crate) fn unix(device: u64, inode: u64) -> Self {
        Self::Unix { device, inode }
    }

    #[cfg(windows)]
    pub(crate) fn windows(volume: u64, file_id: impl Into<String>) -> Self {
        Self::Windows {
            volume,
            file_id: file_id.into(),
        }
    }
}

/// SHA-256 raw-tree versionado; no usa ni expone `SnapshotToken`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawTreeDigest {
    version: String,
    hex: String,
}

impl RawTreeDigest {
    pub(crate) fn from_digest(digest: [u8; 32]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            hex.push(HEX[(byte >> 4) as usize] as char);
            hex.push(HEX[(byte & 0x0f) as usize] as char);
        }
        Self {
            version: RAW_TREE_DIGEST_V1.to_owned(),
            hex,
        }
    }

    pub(crate) fn from_hex(hex: impl Into<String>) -> Result<Self, JournalError> {
        let digest = Self {
            version: RAW_TREE_DIGEST_V1.to_owned(),
            hex: hex.into(),
        };
        digest.validate()?;
        Ok(digest)
    }

    fn validate(&self) -> Result<(), JournalError> {
        if self.version != RAW_TREE_DIGEST_V1 {
            return Err(JournalError::UnsupportedDigestVersion);
        }
        if !is_lower_hex(&self.hex, 64) {
            return Err(JournalError::InvalidDigest);
        }
        Ok(())
    }
}

/// Identidad más digest raw esperado para un árbol old/new.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactFingerprint {
    identity: DirectoryIdentity,
    digest: RawTreeDigest,
}

impl ArtifactFingerprint {
    pub(crate) fn new(identity: DirectoryIdentity, digest: RawTreeDigest) -> Self {
        Self { identity, digest }
    }

    pub(crate) fn identity(&self) -> &DirectoryIdentity {
        &self.identity
    }
}

impl ArtifactFingerprint {
    fn validate(&self) -> Result<(), JournalError> {
        self.identity.validate()?;
        self.digest.validate()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum JournalError {
    Malformed,
    TooLarge,
    UnsupportedVersion,
    UnsupportedDigestVersion,
    InvalidTransactionId,
    InvalidDigest,
    InvalidIdentity,
    InvalidPlatform,
    InvalidFingerprint,
    InvalidTransition,
}

impl JournalError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Malformed => "journal_malformed",
            Self::TooLarge => "journal_too_large",
            Self::UnsupportedVersion => "journal_version_unsupported",
            Self::UnsupportedDigestVersion => "journal_digest_version_unsupported",
            Self::InvalidTransactionId => "journal_transaction_id_invalid",
            Self::InvalidDigest => "journal_digest_invalid",
            Self::InvalidIdentity => "journal_identity_invalid",
            Self::InvalidPlatform => "journal_platform_invalid",
            Self::InvalidFingerprint => "journal_fingerprint_invalid",
            Self::InvalidTransition => "journal_transition_invalid",
        }
    }
}

impl fmt::Debug for JournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl fmt::Display for JournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for JournalError {}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn record() -> Journal {
        Journal::new(
            "0123456789abcdef0123456789abcdef",
            identity(1),
            fingerprint(2, 'a'),
            fingerprint(3, 'b'),
        )
        .unwrap()
    }

    #[test]
    fn encoder_emits_v1_state_and_redacted_debug() {
        let record = record();
        let bytes = record.encode().unwrap();
        let prefix = br#"{"schema":"felix-journal-v1","transaction_id":"0123456789abcdef0123456789abcdef","state":"prepared","parent":{"platform":""#;
        assert!(bytes.starts_with(prefix));
        assert!(!bytes.contains(&b' '));
        let debug = format!("{record:?}");
        assert!(!debug.contains(record.transaction_id()));
        assert!(!debug.contains(&"a".repeat(64)));
        assert!(!debug.contains(&"b".repeat(64)));
        assert!(!debug.contains("device"));
    }

    #[test]
    fn state_transitions_are_sequential_monotonic_and_idempotent() {
        let mut journal = record();
        assert_eq!(journal.advance(JournalState::Prepared), Ok(()));

        let states = [
            JournalState::Prepared,
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ];
        for original in states {
            for next in states {
                let mut state = original;
                let expected = original == next || original.successor() == Some(next);
                let result = state.advance(next);

                assert_eq!(result.is_ok(), expected, "{original:?} -> {next:?}");
                if expected {
                    assert_eq!(state, next);
                } else {
                    assert_eq!(result, Err(JournalError::InvalidTransition));
                    assert_eq!(state, original);
                }
            }
        }
    }

    #[test]
    fn record_successor_validation_accepts_only_the_exact_next_state() {
        let states = [
            JournalState::Prepared,
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ];
        let mut accepted = 0;

        for old_state in states {
            for candidate_state in states {
                let mut old = record();
                old.state = old_state;
                let mut candidate = old.clone();
                candidate.state = candidate_state;
                let expected = matches!(
                    (old_state, candidate_state),
                    (JournalState::Prepared, JournalState::BackupReady)
                        | (JournalState::BackupReady, JournalState::Installed)
                        | (JournalState::Installed, JournalState::Committed)
                );

                let result = old.validate_successor(&candidate);
                assert_eq!(
                    result.is_ok(),
                    expected,
                    "{old_state:?} -> {candidate_state:?}"
                );
                if expected {
                    accepted += 1;
                } else {
                    assert_eq!(result, Err(JournalError::InvalidTransition));
                }
            }
        }

        assert_eq!(accepted, 3);
    }

    #[test]
    fn record_successor_validation_rejects_identity_and_fingerprint_mutations() {
        fn exact_next(old: &Journal) -> Journal {
            let mut candidate = old.clone();
            let next = old.state.successor().unwrap();
            candidate.advance(next).unwrap();
            candidate
        }

        fn assert_rejected(old: &Journal, candidate: &Journal) {
            let error = old.validate_successor(candidate).unwrap_err();
            assert_eq!(error, JournalError::InvalidTransition);
            assert_eq!(format!("{error:?}"), "journal_transition_invalid");
            assert_eq!(format!("{error}"), "journal_transition_invalid");
        }

        let old = record();

        let mut candidate = exact_next(&old);
        candidate.transaction_id = "f123456789abcdef0123456789abcdef".to_owned();
        assert_rejected(&old, &candidate);

        let mut candidate = exact_next(&old);
        candidate.parent = identity(99);
        assert_rejected(&old, &candidate);

        let mut candidate = exact_next(&old);
        candidate.old = fingerprint(2, 'c');
        assert_rejected(&old, &candidate);

        let mut candidate = exact_next(&old);
        candidate.new = fingerprint(3, 'd');
        assert_rejected(&old, &candidate);

        let mut candidate = exact_next(&old);
        candidate.schema = "private-schema-canary".to_owned();
        assert_rejected(&old, &candidate);
    }
}
