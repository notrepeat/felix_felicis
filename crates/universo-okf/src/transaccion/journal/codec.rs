//! Wire privado y codec canónico del journal v1.

use serde::{Deserialize, Serialize};

use super::{ArtifactFingerprint, DirectoryIdentity, Journal, JournalError, JournalState};
#[cfg(test)]
use super::{RawTreeDigest, SCHEMA_V1};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalWire {
    schema: String,
    transaction_id: String,
    state: JournalState,
    parent: DirectoryIdentity,
    old: ArtifactFingerprint,
    new: ArtifactFingerprint,
}

pub(super) fn encode(record: &Journal) -> Result<Vec<u8>, JournalError> {
    record.validate()?;
    serde_json::to_vec(&JournalWire {
        schema: record.schema.clone(),
        transaction_id: record.transaction_id.clone(),
        state: record.state,
        parent: record.parent.clone(),
        old: record.old.clone(),
        new: record.new.clone(),
    })
    .map_err(|_| JournalError::Malformed)
}

impl Journal {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, JournalError> {
        let wire: JournalWire =
            serde_json::from_slice(bytes).map_err(|_| JournalError::Malformed)?;
        let record = Self {
            schema: wire.schema,
            transaction_id: wire.transaction_id,
            state: wire.state,
            parent: wire.parent,
            old: wire.old,
            new: wire.new,
        };
        record.validate()?;
        if encode(&record)?.as_slice() != bytes {
            return Err(JournalError::Malformed);
        }
        Ok(record)
    }
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

    fn alter(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .replacen(from, to, 1)
            .into_bytes()
    }

    #[test]
    fn canonical_round_trip_preserves_the_domain_record() {
        let record = record();
        let bytes = record.encode().unwrap();
        let parsed = Journal::parse(&bytes).unwrap();
        assert_eq!(parsed, record);
        assert_eq!(parsed.encode().unwrap(), bytes);
    }

    #[test]
    fn parser_rejects_unknown_duplicate_and_noncanonical_fields() {
        let bytes = record().encode().unwrap();
        #[cfg(unix)]
        let nested_unknown = alter(&bytes, "\"inode\":1", "\"inode\":1,\"path\":\"secret\"");
        #[cfg(windows)]
        let nested_unknown = alter(&bytes, "\"volume\":7", "\"volume\":7,\"path\":\"secret\"");
        for invalid in [
            alter(&bytes, "\"state\":\"prepared\"", "\"state\":\"future\""),
            alter(
                &bytes,
                "\"state\":\"prepared\"",
                "\"state\":\"prepared\",\"state\":\"prepared\"",
            ),
            alter(
                &bytes,
                "\"state\":\"prepared\"",
                "\"state\":\"prepared\",\"extra\":0",
            ),
            nested_unknown,
            [b" ".as_slice(), bytes.as_slice()].concat(),
        ] {
            assert_eq!(Journal::parse(&invalid), Err(JournalError::Malformed));
        }
    }

    #[test]
    fn parser_rejects_invalid_version_transaction_digest_and_platform() {
        let bytes = record().encode().unwrap();
        let invalid_version = alter(&bytes, SCHEMA_V1, "felix-journal-v2");
        assert_eq!(
            Journal::parse(&invalid_version),
            Err(JournalError::UnsupportedVersion)
        );
        let invalid_id = alter(
            &bytes,
            "0123456789abcdef0123456789abcdef",
            "../secret000000000000000000000000",
        );
        assert_eq!(
            Journal::parse(&invalid_id),
            Err(JournalError::InvalidTransactionId)
        );
        let invalid_digest = alter(&bytes, &"a".repeat(64), &"A".repeat(64));
        assert_eq!(
            Journal::parse(&invalid_digest),
            Err(JournalError::InvalidDigest)
        );
        let digest = format!("\"hex\":\"{}\"", "a".repeat(64));
        let short_digest = format!("\"hex\":\"{}\"", "a".repeat(63));
        let invalid_digest_length = alter(&bytes, &digest, &short_digest);
        assert_eq!(
            Journal::parse(&invalid_digest_length),
            Err(JournalError::InvalidDigest)
        );
        let invalid_digest_version = alter(&bytes, "felix-raw-tree-sha256-v1", "sha256");
        assert_eq!(
            Journal::parse(&invalid_digest_version),
            Err(JournalError::UnsupportedDigestVersion)
        );
        #[cfg(unix)]
        let invalid_platform = alter(&bytes, "unix-v1", "unknown-v1");
        #[cfg(windows)]
        let invalid_platform = alter(&bytes, "windows-v1", "unknown-v1");
        assert_eq!(
            Journal::parse(&invalid_platform),
            Err(JournalError::Malformed)
        );
    }

    #[test]
    fn parser_rejects_a_recognized_foreign_platform_identity() {
        let record = record();
        #[cfg(unix)]
        let foreign = DirectoryIdentity::Windows {
            volume: 7,
            file_id: "00000000000000000000000000000001".to_owned(),
        };
        #[cfg(windows)]
        let foreign = DirectoryIdentity::Unix {
            device: 7,
            inode: 1,
        };
        let bytes = serde_json::to_vec(&JournalWire {
            schema: SCHEMA_V1.to_owned(),
            transaction_id: record.transaction_id.clone(),
            state: record.state,
            parent: foreign,
            old: record.old.clone(),
            new: record.new.clone(),
        })
        .unwrap();
        assert_eq!(Journal::parse(&bytes), Err(JournalError::InvalidPlatform));
    }

    #[cfg(windows)]
    #[test]
    fn parser_rejects_non_hex_windows_file_ids() {
        let record = record();
        let wire = JournalWire {
            schema: SCHEMA_V1.to_owned(),
            transaction_id: record.transaction_id.clone(),
            state: record.state,
            parent: DirectoryIdentity::Windows {
                volume: 7,
                file_id: "../secret".to_owned(),
            },
            old: record.old.clone(),
            new: record.new.clone(),
        };
        let bytes = serde_json::to_vec(&wire).unwrap();
        assert_eq!(Journal::parse(&bytes), Err(JournalError::InvalidIdentity));
    }

    #[test]
    fn constructors_reject_invalid_transaction_digest_platform_and_fingerprint() {
        let parent = identity(1);
        let old = fingerprint(2, 'a');
        let new = fingerprint(3, 'b');
        assert!(Journal::new("../not-an-id", parent.clone(), old.clone(), new.clone()).is_err());
        assert!(RawTreeDigest::from_hex("A".repeat(64)).is_err());
        assert!(RawTreeDigest::from_hex("a".repeat(63)).is_err());
        assert!(Journal::new("0".repeat(32), parent, old.clone(), old).is_err());
        #[cfg(unix)]
        let foreign = DirectoryIdentity::Windows {
            volume: 1,
            file_id: "00000000000000000000000000000001".to_owned(),
        };
        #[cfg(windows)]
        let foreign = DirectoryIdentity::Unix {
            device: 1,
            inode: 1,
        };
        assert!(
            Journal::new(
                "0".repeat(32),
                foreign,
                fingerprint(2, 'a'),
                fingerprint(3, 'b')
            )
            .is_err()
        );
        #[cfg(windows)]
        assert!(
            DirectoryIdentity::windows(1, "../secret")
                .validate()
                .is_err()
        );
    }
}
