use std::ops::Range;

use crate::transaccion::journal::{ArtifactFingerprint, Journal, JournalState, RawTreeDigest};

use super::super::IntentRetiroPersistido;
use super::{MAGIC, MAX_ENVELOPE_BYTES, record, rejected};

fn journal_range(frame: &[u8]) -> Range<usize> {
    let length_offset = MAGIC.len() + 2;
    let length = usize::from(u16::from_be_bytes(
        frame[length_offset..length_offset + 2].try_into().unwrap(),
    ));
    let start = length_offset + 2;
    start..start + length
}

fn with_journal(frame: &[u8], journal: &[u8]) -> Vec<u8> {
    let old_range = journal_range(frame);
    let old_length_offset = MAGIC.len() + 2;
    let new_length = u16::try_from(journal.len()).unwrap().to_be_bytes();
    let mut result = Vec::with_capacity(frame.len() - old_range.len() + journal.len());
    result.extend_from_slice(&frame[..old_length_offset]);
    result.extend_from_slice(&new_length);
    result.extend_from_slice(journal);
    result.extend_from_slice(&frame[old_range.end..]);
    result
}

fn identity_magic_offsets(frame: &[u8]) -> Vec<usize> {
    let mut offset = MAGIC.len() + 2;
    let journal_length = usize::from(u16::from_be_bytes(
        frame[offset..offset + 2].try_into().unwrap(),
    ));
    offset += 2 + journal_length;

    let mut offsets = Vec::with_capacity(3);
    for _ in 0..3 {
        let identity_length = usize::from(u16::from_be_bytes(
            frame[offset..offset + 2].try_into().unwrap(),
        ));
        offset += 2;
        offsets.push(offset);
        offset += identity_length;
    }
    offsets
}

fn journal_in_state(state: JournalState) -> Journal {
    let old = ArtifactFingerprint::new(super::directorio(1), RawTreeDigest::from_digest([1; 32]));
    let new = ArtifactFingerprint::new(super::directorio(2), RawTreeDigest::from_digest([2; 32]));
    let mut journal = Journal::new(
        "0123456789abcdef0123456789abcdef",
        super::directorio(3),
        old,
        new,
    )
    .unwrap();
    let transitions: &[JournalState] = match state {
        JournalState::Prepared => &[],
        JournalState::BackupReady => &[JournalState::BackupReady],
        JournalState::Installed => &[JournalState::BackupReady, JournalState::Installed],
        JournalState::Committed => &[
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ],
    };
    for next in transitions {
        journal.advance(*next).unwrap();
    }
    journal
}

#[test]
fn rechaza_truncamiento_en_cada_frontera_de_byte() {
    let temp = tempfile::tempdir().unwrap();
    let intent = record(&temp, 1);
    let frame = intent.encode().unwrap();

    for end in 0..frame.len() {
        rejected(&frame[..end]);
    }
}

#[test]
fn rechaza_magic_y_cada_frame_de_identidad_corruptos() {
    let temp = tempfile::tempdir().unwrap();
    let intent = record(&temp, 1);
    let frame = intent.encode().unwrap();

    let mut bad_magic = frame.clone();
    bad_magic[0] ^= 1;
    rejected(&bad_magic);

    for offset in identity_magic_offsets(&frame) {
        let mut bad_identity = frame.clone();
        bad_identity[offset] ^= 1;
        rejected(&bad_identity);
    }
}

#[test]
fn rechaza_journal_con_campo_desconocido_real_o_json_no_canonico() {
    let temp = tempfile::tempdir().unwrap();
    let intent = record(&temp, 1);
    let frame = intent.encode().unwrap();
    let range = journal_range(&frame);
    let journal = &frame[range];

    assert_eq!(journal.last(), Some(&b'}'));
    let mut unknown_field = journal[..journal.len() - 1].to_vec();
    unknown_field.extend_from_slice(b",\"unknown_field\":0}");
    rejected(&with_journal(&frame, &unknown_field));

    assert_eq!(journal.first(), Some(&b'{'));
    let mut noncanonical = Vec::with_capacity(journal.len() + 1);
    noncanonical.push(b'{');
    noncanonical.push(b' ');
    noncanonical.extend_from_slice(&journal[1..]);
    rejected(&with_journal(&frame, &noncanonical));
}

#[test]
fn constructor_exige_fase_committed_y_longitud_dentro_de_cuota() {
    let temp = tempfile::tempdir().unwrap();
    let template = record(&temp, 1);
    let construir = |journal, longitud| {
        IntentRetiroPersistido::new(
            journal,
            template.identidad_intent.clone(),
            template.identidad_journal.clone(),
            template.identidad_contexto.clone(),
            template.scope_parent,
            longitud,
            template.digest_contexto,
        )
    };

    for phase in [
        JournalState::Prepared,
        JournalState::BackupReady,
        JournalState::Installed,
    ] {
        assert!(construir(journal_in_state(phase), 1).is_err());
    }
    assert!(construir(journal_in_state(JournalState::Committed), 0).is_err());
    assert!(
        construir(
            journal_in_state(JournalState::Committed),
            u64::try_from(MAX_ENVELOPE_BYTES).unwrap() + 1,
        )
        .is_err()
    );
    assert!(construir(journal_in_state(JournalState::Committed), 1).is_ok());
}
