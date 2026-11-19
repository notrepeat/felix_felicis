//! Codec JSON canónico y estricto del journal v2.

use serde::{Deserialize, Serialize};

use super::super::{ArtifactFingerprint, DirectoryIdentity, JournalState};
use super::{JournalKind, JournalV2, JournalV2Error, MAX_JOURNAL_V2_BYTES, SCHEMA_V2};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalV2Wire {
    schema: String,
    kind: String,
    transaction_id: String,
    state: JournalState,
    parent: DirectoryIdentity,
    old: ArtifactFingerprint,
    new: ArtifactFingerprint,
}

pub(super) fn encode(record: &JournalV2) -> Result<Vec<u8>, JournalV2Error> {
    record.validate()?;
    let kind = match record.kind {
        JournalKind::ContextRequired => "context_required",
    };
    let bytes = serde_json::to_vec(&JournalV2Wire {
        schema: SCHEMA_V2.to_owned(),
        kind: kind.to_owned(),
        transaction_id: record.transaction_id.clone(),
        state: record.state,
        parent: record.parent.clone(),
        old: record.old.clone(),
        new: record.new.clone(),
    })
    .map_err(|_| JournalV2Error::Malformed)?;
    if bytes.len() > MAX_JOURNAL_V2_BYTES {
        return Err(JournalV2Error::TooLarge);
    }
    Ok(bytes)
}

pub(super) fn parse(bytes: &[u8]) -> Result<JournalV2, JournalV2Error> {
    if bytes.len() > MAX_JOURNAL_V2_BYTES {
        return Err(JournalV2Error::TooLarge);
    }
    let wire: JournalV2Wire =
        serde_json::from_slice(bytes).map_err(|_| JournalV2Error::Malformed)?;
    if wire.schema != SCHEMA_V2 {
        return Err(JournalV2Error::SchemaUnsupported);
    }
    let kind = match wire.kind.as_str() {
        "context_required" => JournalKind::ContextRequired,
        _ => return Err(JournalV2Error::KindUnsupported),
    };
    let record = JournalV2 {
        transaction_id: wire.transaction_id,
        kind,
        state: wire.state,
        parent: wire.parent,
        old: wire.old,
        new: wire.new,
    };
    record.validate()?;
    if encode(&record)?.as_slice() != bytes {
        return Err(JournalV2Error::Malformed);
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::super::super::RawTreeDigest;
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

    fn replace_once(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .replacen(from, to, 1)
            .into_bytes()
    }

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

    fn maximum_record() -> JournalV2 {
        let mut record = JournalV2::new(
            "f".repeat(32),
            maximum_identity(),
            ArtifactFingerprint::new(
                maximum_identity(),
                RawTreeDigest::from_hex("a".repeat(64)).unwrap(),
            ),
            ArtifactFingerprint::new(
                maximum_identity(),
                RawTreeDigest::from_hex("b".repeat(64)).unwrap(),
            ),
        )
        .unwrap();
        record.advance(JournalState::BackupReady).unwrap();
        record
    }

    #[test]
    fn canonical_round_trip_uses_schema_kind_and_fixed_field_order() {
        let record = record();
        let bytes = record.encode().unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        let keys = [
            "\"schema\":",
            "\"kind\":",
            "\"transaction_id\":",
            "\"state\":",
            "\"parent\":",
            "\"old\":",
            "\"new\":",
        ];
        let positions = keys.map(|key| text.find(key).unwrap());
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(text.starts_with("{\"schema\":\"felix-journal-v2\",\"kind\":\"context_required\""));
        assert_eq!(JournalV2::parse(&bytes), Ok(record));
        assert_eq!(JournalV2::parse(&bytes).unwrap().encode().unwrap(), bytes);
    }

    #[test]
    fn parser_rejects_unknown_schema_and_kind_without_fallback() {
        let bytes = record().encode().unwrap();
        let unknown_schema = replace_once(&bytes, SCHEMA_V2, "felix-journal-v3");
        let unknown_kind = replace_once(&bytes, "context_required", "future_kind");
        assert_eq!(
            JournalV2::parse(&unknown_schema),
            Err(JournalV2Error::SchemaUnsupported)
        );
        assert_eq!(
            JournalV2::parse(&unknown_kind),
            Err(JournalV2Error::KindUnsupported)
        );
    }

    #[test]
    fn parser_rejects_duplicate_and_unknown_root_or_nested_fields() {
        let bytes = record().encode().unwrap();
        let duplicate_schema = replace_once(
            &bytes,
            "\"schema\":\"felix-journal-v2\"",
            "\"schema\":\"felix-journal-v2\",\"schema\":\"felix-journal-v2\"",
        );
        let duplicate_kind = replace_once(
            &bytes,
            "\"kind\":\"context_required\"",
            "\"kind\":\"context_required\",\"kind\":\"context_required\"",
        );
        let unknown_root = replace_once(
            &bytes,
            "\"state\":\"prepared\"",
            "\"state\":\"prepared\",\"unexpected\":0",
        );
        #[cfg(unix)]
        let unknown_nested = replace_once(&bytes, "\"inode\":1", "\"inode\":1,\"unexpected\":0");
        #[cfg(windows)]
        let unknown_nested = replace_once(&bytes, "\"volume\":7", "\"volume\":7,\"unexpected\":0");
        for invalid in [
            duplicate_schema,
            duplicate_kind,
            unknown_root,
            unknown_nested,
        ] {
            assert_eq!(JournalV2::parse(&invalid), Err(JournalV2Error::Malformed));
        }
    }

    #[test]
    fn parser_rejects_noncanonical_whitespace_and_trailing_bytes() {
        let bytes = record().encode().unwrap();
        let leading_space = [b" ".as_slice(), bytes.as_slice()].concat();
        let trailing_space = [bytes.as_slice(), b" "].concat();
        let trailing_data = [bytes.as_slice(), b"x"].concat();
        let reordered_fields = replace_once(
            &bytes,
            "\"schema\":\"felix-journal-v2\",\"kind\":\"context_required\"",
            "\"kind\":\"context_required\",\"schema\":\"felix-journal-v2\"",
        );
        for noncanonical in [
            leading_space,
            trailing_space,
            trailing_data,
            reordered_fields,
        ] {
            assert_eq!(
                JournalV2::parse(&noncanonical),
                Err(JournalV2Error::Malformed)
            );
        }
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
        let bytes = serde_json::to_vec(&JournalV2Wire {
            schema: SCHEMA_V2.to_owned(),
            kind: "context_required".to_owned(),
            transaction_id: record.transaction_id.clone(),
            state: record.state,
            parent: foreign,
            old: record.old.clone(),
            new: record.new.clone(),
        })
        .unwrap();
        assert_eq!(
            JournalV2::parse(&bytes),
            Err(JournalV2Error::PlatformInvalid)
        );
    }

    #[test]
    fn parser_enforces_quota_before_decode_and_exact_limit_reaches_strict_json() {
        let canonical = record().encode().unwrap();
        let mut exact = canonical[..canonical.len() - 1].to_vec();
        let unknown_prefix = br#","unknown":""#;
        let unknown_suffix = b"\"}";
        let padding =
            MAX_JOURNAL_V2_BYTES - exact.len() - unknown_prefix.len() - unknown_suffix.len();
        exact.extend_from_slice(unknown_prefix);
        exact.extend(std::iter::repeat_n(b'x', padding));
        exact.extend_from_slice(unknown_suffix);
        assert_eq!(exact.len(), MAX_JOURNAL_V2_BYTES);
        assert_eq!(JournalV2::parse(&exact), Err(JournalV2Error::Malformed));

        let over_limit = vec![0xff; MAX_JOURNAL_V2_BYTES + 1];
        assert_eq!(JournalV2::parse(&over_limit), Err(JournalV2Error::TooLarge));
    }

    #[test]
    fn maximum_native_record_encoder_measures_within_v2_quota() {
        let record = maximum_record();
        let bytes = record.encode().unwrap();
        let measured_bytes = bytes.len();
        eprintln!("maximum native Journal v2 record: {measured_bytes} bytes");
        assert!(
            measured_bytes <= MAX_JOURNAL_V2_BYTES,
            "maximum native Journal v2 record measured {measured_bytes} bytes"
        );
        assert_eq!(JournalV2::parse(&bytes), Ok(record));
    }
}
