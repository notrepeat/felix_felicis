//! Clasificador puro de bytes canónicos para Journal v1 y v2.

use std::fmt;

use serde::Deserialize;

use super::v2::{JournalV2, JournalV2Error, MAX_JOURNAL_V2_BYTES};
use super::{Journal, JournalError, MAX_JOURNAL_BYTES, SCHEMA_V1};

const SCHEMA_V2: &str = "felix-journal-v2";
pub(crate) const MAX_VERSIONED_JOURNAL_BYTES: usize =
    max_versioned_journal_bytes(MAX_JOURNAL_BYTES, MAX_JOURNAL_V2_BYTES);

const fn max_versioned_journal_bytes(v1_quota: usize, v2_quota: usize) -> usize {
    if v1_quota > v2_quota {
        v1_quota
    } else {
        v2_quota
    }
}

fn preflight_versioned_length(
    observed_len: usize,
    v1_quota: usize,
    v2_quota: usize,
) -> Result<(), JournalPersistidoError> {
    if observed_len > max_versioned_journal_bytes(v1_quota, v2_quota) {
        Err(JournalPersistidoError::TooLarge)
    } else {
        Ok(())
    }
}

#[derive(Deserialize)]
struct SchemaProbe {
    schema: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum JournalPersistido {
    V1(Journal),
    V2(JournalV2),
}

impl fmt::Debug for JournalPersistido {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JournalPersistido([redacted])")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum JournalPersistidoError {
    TooLarge,
    Malformed,
    SchemaUnsupported,
    V1(JournalError),
    V2(JournalV2Error),
}

impl JournalPersistidoError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::TooLarge => "journal_selector_too_large",
            Self::Malformed => "journal_selector_malformed",
            Self::SchemaUnsupported => "journal_selector_schema_unsupported",
            Self::V1(error) => error.code(),
            Self::V2(error) => error.code(),
        }
    }
}

impl fmt::Debug for JournalPersistidoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl fmt::Display for JournalPersistidoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for JournalPersistidoError {}

pub(crate) fn parse(bytes: &[u8]) -> Result<JournalPersistido, JournalPersistidoError> {
    preflight_versioned_length(bytes.len(), MAX_JOURNAL_BYTES, MAX_JOURNAL_V2_BYTES)?;

    let probe: SchemaProbe =
        serde_json::from_slice(bytes).map_err(|_| JournalPersistidoError::Malformed)?;
    match probe.schema.as_str() {
        SCHEMA_V1 => Journal::parse(bytes)
            .map(JournalPersistido::V1)
            .map_err(JournalPersistidoError::V1),
        SCHEMA_V2 => JournalV2::parse(bytes)
            .map(JournalPersistido::V2)
            .map_err(JournalPersistidoError::V2),
        _ => Err(JournalPersistidoError::SchemaUnsupported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, RawTreeDigest};

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

    fn record_v1() -> Journal {
        Journal::new(
            TRANSACTION_ID,
            identity(1),
            fingerprint(2, 'a'),
            fingerprint(3, 'b'),
        )
        .unwrap()
    }

    fn record_v2() -> JournalV2 {
        JournalV2::new(
            TRANSACTION_ID,
            identity(1),
            fingerprint(2, 'a'),
            fingerprint(3, 'b'),
        )
        .unwrap()
    }

    fn replace_schema(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .replacen(from, to, 1)
            .into_bytes()
    }

    #[test]
    fn classifies_valid_native_records_without_projecting_v2_to_v1() {
        let v1 = record_v1();
        let v2 = record_v2();
        assert_eq!(parse(&v1.encode().unwrap()), Ok(JournalPersistido::V1(v1)));
        assert_eq!(parse(&v2.encode().unwrap()), Ok(JournalPersistido::V2(v2)));
    }

    #[test]
    fn schema_probe_requires_one_string_field_in_a_complete_object() {
        let invalid_inputs: &[&[u8]] = &[
            br#"{}"#,
            br#"{"schema":"felix-journal-v1","schema":"felix-journal-v1"}"#,
            br#"{"schema":"felix-journal-v1","schema":"felix-journal-v2"}"#,
            br#"{"schema":7}"#,
            b"[]",
            br#"{"schema":"felix-journal-v1""#,
            br#"{"schema":"felix-journal-v1"} trailing"#,
        ];
        for bytes in invalid_inputs {
            assert_eq!(parse(bytes), Err(JournalPersistidoError::Malformed));
        }
    }

    #[test]
    fn unknown_schema_fails_closed() {
        assert_eq!(
            parse(br#"{"schema":"felix-journal-future"}"#),
            Err(JournalPersistidoError::SchemaUnsupported)
        );
    }

    #[test]
    fn malformed_known_schema_uses_only_its_selected_parser() {
        let v1_as_v2 = replace_schema(&record_v1().encode().unwrap(), SCHEMA_V1, SCHEMA_V2);
        let v2_as_v1 = replace_schema(&record_v2().encode().unwrap(), SCHEMA_V2, SCHEMA_V1);
        assert_eq!(
            parse(&v1_as_v2),
            Err(JournalPersistidoError::V2(JournalV2Error::Malformed))
        );
        assert_eq!(
            parse(&v2_as_v1),
            Err(JournalPersistidoError::V1(JournalError::Malformed))
        );
    }

    #[test]
    fn quota_rejects_oversized_invalid_bytes_before_schema_probe() {
        let oversized_invalid = vec![0xff; MAX_VERSIONED_JOURNAL_BYTES + 1];
        assert_eq!(
            parse(&oversized_invalid),
            Err(JournalPersistidoError::TooLarge)
        );
    }

    #[test]
    fn quota_preflight_is_inclusive_and_uses_both_family_limits() {
        for (v1_quota, v2_quota) in [(1024, 1536), (1536, 1024), (1024, 1024)] {
            let max = max_versioned_journal_bytes(v1_quota, v2_quota);
            assert_eq!(preflight_versioned_length(max, v1_quota, v2_quota), Ok(()));
            assert_eq!(
                preflight_versioned_length(max + 1, v1_quota, v2_quota),
                Err(JournalPersistidoError::TooLarge)
            );
        }

        assert_eq!(
            MAX_VERSIONED_JOURNAL_BYTES,
            max_versioned_journal_bytes(MAX_JOURNAL_BYTES, MAX_JOURNAL_V2_BYTES)
        );
    }

    #[test]
    fn selector_errors_and_versioned_records_debug_redact_payloads() {
        let id = TRANSACTION_ID;
        let digest = "a".repeat(64);
        let error = JournalPersistidoError::Malformed;
        let record = JournalPersistido::V1(record_v1());
        for output in [
            format!("{error:?}"),
            error.to_string(),
            format!("{record:?}"),
        ] {
            assert!(!output.contains(id));
            assert!(!output.contains(&digest));
        }
    }
}
