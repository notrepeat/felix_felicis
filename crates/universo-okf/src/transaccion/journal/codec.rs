//! Encoder canónico del journal v1. El parser estricto se mantiene privado.

use serde::Serialize;

use super::{Journal, JournalError};

#[derive(Serialize)]
struct JournalWire<'a> {
    schema: &'a str,
    transaction_id: &'a str,
    state: super::JournalState,
    parent: &'a super::DirectoryIdentity,
    old: &'a super::ArtifactFingerprint,
    new: &'a super::ArtifactFingerprint,
}

pub(super) fn encode(record: &Journal) -> Result<Vec<u8>, JournalError> {
    record.validate()?;
    serde_json::to_vec(&JournalWire {
        schema: &record.schema,
        transaction_id: &record.transaction_id,
        state: record.state,
        parent: &record.parent,
        old: &record.old,
        new: &record.new,
    })
    .map_err(|_| JournalError::Malformed)
}
