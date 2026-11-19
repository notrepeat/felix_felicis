//! Wire privado y codec canónico del journal v1.

use serde::{Deserialize, Serialize};

use super::{
    ArtifactFingerprint, DirectoryIdentity, Journal, JournalError, JournalState, MAX_JOURNAL_BYTES,
};
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
    let bytes = serde_json::to_vec(&JournalWire {
        schema: record.schema.clone(),
        transaction_id: record.transaction_id.clone(),
        state: record.state,
        parent: record.parent.clone(),
        old: record.old.clone(),
        new: record.new.clone(),
    })
    .map_err(|_| JournalError::Malformed)?;
    if bytes.len() > MAX_JOURNAL_BYTES {
        return Err(JournalError::TooLarge);
    }
    Ok(bytes)
}

impl Journal {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, JournalError> {
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(JournalError::TooLarge);
        }
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

    fn maximum_valid_record() -> Journal {
        fn maximum_identity() -> DirectoryIdentity {
            #[cfg(unix)]
            {
                DirectoryIdentity::Unix {
                    device: u64::MAX,
                    inode: u64::MAX,
                }
            }
            #[cfg(windows)]
            {
                DirectoryIdentity::Windows {
                    volume: u64::MAX,
                    file_id: "f".repeat(32),
                }
            }
        }

        fn maximum_fingerprint(byte: char) -> ArtifactFingerprint {
            ArtifactFingerprint::new(
                maximum_identity(),
                RawTreeDigest::from_hex(byte.to_string().repeat(64)).unwrap(),
            )
        }

        let mut record = Journal::new(
            "f".repeat(32),
            maximum_identity(),
            maximum_fingerprint('a'),
            maximum_fingerprint('b'),
        )
        .unwrap();
        // `backup_ready` is the longest valid serialized state name.
        record.advance(JournalState::BackupReady).unwrap();
        record
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
    fn native_v1_golden_bytes_match_encoder_and_parser() {
        #[cfg(unix)]
        let golden = r#"{"schema":"felix-journal-v1","transaction_id":"0123456789abcdef0123456789abcdef","state":"prepared","parent":{"platform":"unix-v1","device":7,"inode":1},"old":{"identity":{"platform":"unix-v1","device":7,"inode":2},"digest":{"version":"felix-raw-tree-sha256-v1","hex":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}},"new":{"identity":{"platform":"unix-v1","device":7,"inode":3},"digest":{"version":"felix-raw-tree-sha256-v1","hex":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}}"#;
        #[cfg(windows)]
        let golden = r#"{"schema":"felix-journal-v1","transaction_id":"0123456789abcdef0123456789abcdef","state":"prepared","parent":{"platform":"windows-v1","volume":7,"file_id":"00000000000000000000000000000001"},"old":{"identity":{"platform":"windows-v1","volume":7,"file_id":"00000000000000000000000000000002"},"digest":{"version":"felix-raw-tree-sha256-v1","hex":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}},"new":{"identity":{"platform":"windows-v1","volume":7,"file_id":"00000000000000000000000000000003"},"digest":{"version":"felix-raw-tree-sha256-v1","hex":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}}"#;

        let record = record();
        assert_eq!(record.encode().unwrap().as_slice(), golden.as_bytes());
        assert_eq!(Journal::parse(golden.as_bytes()).unwrap(), record);
    }

    #[test]
    fn maximum_valid_record_round_trips_below_the_wire_limit() {
        let record = maximum_valid_record();
        let bytes = record.encode().unwrap();
        assert!(bytes.len() < MAX_JOURNAL_BYTES);
        assert_eq!(Journal::parse(&bytes).unwrap(), record);
    }

    #[test]
    fn parser_allows_exact_limit_bytes_through_to_strict_json_validation() {
        let canonical = record().encode().unwrap();
        let mut bytes = canonical[..canonical.len() - 1].to_vec();
        let unknown_field_prefix = br#","unknown":""#;
        let unknown_field_suffix = b"\"}";
        let value_len = MAX_JOURNAL_BYTES
            - bytes.len()
            - unknown_field_prefix.len()
            - unknown_field_suffix.len();

        // A real unknown JSON field exercises the exact byte boundary without
        // padding a valid journal or weakening the strict schema.
        bytes.extend_from_slice(unknown_field_prefix);
        bytes.extend(std::iter::repeat_n(b'x', value_len));
        bytes.extend_from_slice(unknown_field_suffix);

        assert_eq!(bytes.len(), MAX_JOURNAL_BYTES);
        assert_eq!(Journal::parse(&bytes), Err(JournalError::Malformed));
    }

    #[test]
    fn parser_rejects_1025_raw_bytes_with_a_stable_redacted_error() {
        let bytes = vec![0xff; MAX_JOURNAL_BYTES + 1];
        let error = Journal::parse(&bytes).unwrap_err();
        assert_eq!(error, JournalError::TooLarge);
        assert_eq!(error.code(), "journal_too_large");
        assert_eq!(error.to_string(), "journal_too_large");
        assert!(!format!("{error:?}").contains("1025"));
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
