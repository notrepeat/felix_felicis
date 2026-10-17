//! Encoder y preflight estructural del frame RETIRE v2.

use crate::transaccion::anclado::identidad_archivo::MAX_PERSISTED_FILE_ID_FRAME_BYTES;
use crate::transaccion::journal::v2::MAX_JOURNAL_V2_BYTES;

use super::{
    ErrorCodecIntentRetiroV2, IntentRetiroPersistidoV2, MAX_RETIRE_V2_BYTES, RETIRE_V2_MAGIC,
    RETIRE_V2_VERSION,
};

const VERSION_BYTES: usize = 2;
const JOURNAL_LENGTH_BYTES: usize = 2;
const IDENTITY_LENGTH_BYTES: usize = 2;
const SCOPE_BYTES: usize = 8;
const CONTEXT_LENGTH_BYTES: usize = 8;
const CONTEXT_DIGEST_BYTES: usize = 32;
const FIXED_BYTES: usize = RETIRE_V2_MAGIC.len()
    + VERSION_BYTES
    + JOURNAL_LENGTH_BYTES
    + 3 * IDENTITY_LENGTH_BYTES
    + SCOPE_BYTES
    + CONTEXT_LENGTH_BYTES
    + CONTEXT_DIGEST_BYTES;

pub(super) fn encode(
    record: &IntentRetiroPersistidoV2,
) -> Result<Vec<u8>, ErrorCodecIntentRetiroV2> {
    record
        .validar()
        .map_err(|_| ErrorCodecIntentRetiroV2::Malformed)?;

    let journal = record
        .journal
        .encode()
        .map_err(|_| ErrorCodecIntentRetiroV2::Malformed)?;
    if journal.is_empty() || journal.len() > MAX_JOURNAL_V2_BYTES {
        return Err(ErrorCodecIntentRetiroV2::Malformed);
    }

    let identities = [
        encode_identity(&record.identidad_intent)?,
        encode_identity(&record.identidad_journal)?,
        encode_identity(&record.identidad_contexto)?,
    ];
    let total = identities.iter().try_fold(
        FIXED_BYTES
            .checked_add(journal.len())
            .ok_or(ErrorCodecIntentRetiroV2::Malformed)?,
        |length, identity| {
            length
                .checked_add(identity.len())
                .ok_or(ErrorCodecIntentRetiroV2::Malformed)
        },
    )?;
    if total > MAX_RETIRE_V2_BYTES {
        return Err(ErrorCodecIntentRetiroV2::TooLarge);
    }

    let journal_length =
        u16::try_from(journal.len()).map_err(|_| ErrorCodecIntentRetiroV2::Malformed)?;
    let identity_lengths = [
        u16::try_from(identities[0].len()).map_err(|_| ErrorCodecIntentRetiroV2::Malformed)?,
        u16::try_from(identities[1].len()).map_err(|_| ErrorCodecIntentRetiroV2::Malformed)?,
        u16::try_from(identities[2].len()).map_err(|_| ErrorCodecIntentRetiroV2::Malformed)?,
    ];
    let mut frame = Vec::new();
    frame
        .try_reserve_exact(total)
        .map_err(|_| ErrorCodecIntentRetiroV2::Allocation)?;
    frame.extend_from_slice(RETIRE_V2_MAGIC);
    frame.extend_from_slice(&RETIRE_V2_VERSION.to_be_bytes());
    frame.extend_from_slice(&journal_length.to_be_bytes());
    frame.extend_from_slice(&journal);
    for (identity, length) in identities.into_iter().zip(identity_lengths) {
        if identity.is_empty() || identity.len() > MAX_PERSISTED_FILE_ID_FRAME_BYTES {
            return Err(ErrorCodecIntentRetiroV2::Malformed);
        }
        frame.extend_from_slice(&length.to_be_bytes());
        frame.extend_from_slice(&identity);
    }
    frame.extend_from_slice(&record.scope_parent.to_be_bytes());
    frame.extend_from_slice(&record.longitud_contexto.to_be_bytes());
    frame.extend_from_slice(&record.digest_contexto);

    if frame.len() != total {
        return Err(ErrorCodecIntentRetiroV2::Malformed);
    }
    if frame.len() > MAX_RETIRE_V2_BYTES {
        return Err(ErrorCodecIntentRetiroV2::TooLarge);
    }
    Ok(frame)
}

struct FrameV2Vista<'a> {
    journal: &'a [u8],
    identidades: [&'a [u8]; 3],
    scope_parent: u64,
    longitud_contexto: u64,
    digest_contexto: [u8; 32],
}

fn parse_frame(frame: &[u8]) -> Result<FrameV2Vista<'_>, ErrorCodecIntentRetiroV2> {
    preflight_frame_length(frame.len(), MAX_RETIRE_V2_BYTES)?;
    let mut offset = 0;
    if take(frame, &mut offset, RETIRE_V2_MAGIC.len())? != RETIRE_V2_MAGIC {
        return Err(ErrorCodecIntentRetiroV2::Malformed);
    }
    if read_u16(frame, &mut offset)? != RETIRE_V2_VERSION {
        return Err(ErrorCodecIntentRetiroV2::Malformed);
    }

    let journal_length = usize::from(read_u16(frame, &mut offset)?);
    if journal_length == 0 || journal_length > MAX_JOURNAL_V2_BYTES {
        return Err(ErrorCodecIntentRetiroV2::Malformed);
    }
    let journal = take(frame, &mut offset, journal_length)?;
    let identidades = [
        take_identity(frame, &mut offset)?,
        take_identity(frame, &mut offset)?,
        take_identity(frame, &mut offset)?,
    ];
    let scope_parent = read_u64(frame, &mut offset)?;
    let longitud_contexto = read_u64(frame, &mut offset)?;
    let digest_contexto = take(frame, &mut offset, CONTEXT_DIGEST_BYTES)?
        .try_into()
        .map_err(|_| ErrorCodecIntentRetiroV2::Malformed)?;
    if offset != frame.len() {
        return Err(ErrorCodecIntentRetiroV2::Malformed);
    }

    Ok(FrameV2Vista {
        journal,
        identidades,
        scope_parent,
        longitud_contexto,
        digest_contexto,
    })
}

fn preflight_frame_length(
    frame_length: usize,
    maximum: usize,
) -> Result<(), ErrorCodecIntentRetiroV2> {
    if frame_length > maximum {
        return Err(ErrorCodecIntentRetiroV2::TooLarge);
    }
    Ok(())
}

fn take_identity<'a>(
    frame: &'a [u8],
    offset: &mut usize,
) -> Result<&'a [u8], ErrorCodecIntentRetiroV2> {
    let length = usize::from(read_u16(frame, offset)?);
    if length == 0 || length > MAX_PERSISTED_FILE_ID_FRAME_BYTES {
        return Err(ErrorCodecIntentRetiroV2::Malformed);
    }
    take(frame, offset, length)
}

fn read_u16(frame: &[u8], offset: &mut usize) -> Result<u16, ErrorCodecIntentRetiroV2> {
    take(frame, offset, 2)?
        .try_into()
        .map(u16::from_be_bytes)
        .map_err(|_| ErrorCodecIntentRetiroV2::Malformed)
}

fn read_u64(frame: &[u8], offset: &mut usize) -> Result<u64, ErrorCodecIntentRetiroV2> {
    take(frame, offset, 8)?
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| ErrorCodecIntentRetiroV2::Malformed)
}

fn take<'a>(
    frame: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], ErrorCodecIntentRetiroV2> {
    let end = offset
        .checked_add(length)
        .filter(|end| *end <= frame.len())
        .ok_or(ErrorCodecIntentRetiroV2::Malformed)?;
    let bytes = &frame[*offset..end];
    *offset = end;
    Ok(bytes)
}

fn encode_identity(
    identity: &crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida,
) -> Result<Vec<u8>, ErrorCodecIntentRetiroV2> {
    let frame = identity
        .encode()
        .map_err(|_| ErrorCodecIntentRetiroV2::Malformed)?;
    if frame.is_empty() || frame.len() > MAX_PERSISTED_FILE_ID_FRAME_BYTES {
        return Err(ErrorCodecIntentRetiroV2::Malformed);
    }
    Ok(frame)
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
        ErrorCodecIntentRetiroV2, FIXED_BYTES, IntentRetiroPersistidoV2, MAX_JOURNAL_V2_BYTES,
        MAX_PERSISTED_FILE_ID_FRAME_BYTES, MAX_RETIRE_V2_BYTES, RETIRE_V2_MAGIC, RETIRE_V2_VERSION,
        parse_frame, preflight_frame_length,
    };

    const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
    const SCOPE: u64 = 0x0102_0304_0506_0708;
    const CONTEXT_LENGTH: u64 = 123;
    const CONTEXT_DIGEST: [u8; 32] = [0xD4; 32];

    fn file_identity(value: u64) -> IdentidadArchivoPersistida {
        #[cfg(unix)]
        let payload = format!(r#"{{"platform":"unix-v1","device":17,"inode":{value}}}"#);
        #[cfg(windows)]
        let payload =
            format!(r#"{{"platform":"windows-v1","volume":17,"file_id":"{value:032x}"}}"#);

        let mut frame = b"FELIX-FILEID\0".to_vec();
        frame.extend_from_slice(&1_u16.to_be_bytes());
        frame.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
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

    fn committed_journal() -> JournalV2 {
        let mut journal = JournalV2::new(
            TRANSACTION_ID,
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
        for state in [
            JournalState::BackupReady,
            JournalState::Installed,
            JournalState::Committed,
        ] {
            journal.advance(state).unwrap();
        }
        journal
    }

    fn record() -> IntentRetiroPersistidoV2 {
        IntentRetiroPersistidoV2::desde_datos_persistidos(
            committed_journal(),
            file_identity(41),
            file_identity(42),
            file_identity(43),
            SCOPE,
            CONTEXT_LENGTH,
            CONTEXT_DIGEST,
        )
        .unwrap()
    }

    fn read_u16(frame: &[u8], offset: &mut usize) -> u16 {
        let end = *offset + 2;
        let value = u16::from_be_bytes(frame[*offset..end].try_into().unwrap());
        *offset = end;
        value
    }

    fn take<'a>(frame: &'a [u8], offset: &mut usize, length: usize) -> &'a [u8] {
        let end = *offset + length;
        let value = &frame[*offset..end];
        *offset = end;
        value
    }

    fn identity_length_offsets(frame: &[u8]) -> [usize; 3] {
        let mut offset = RETIRE_V2_MAGIC.len() + 2;
        let journal_length = usize::from(read_u16(frame, &mut offset));
        offset += journal_length;
        let mut lengths = [0; 3];
        for length_offset in &mut lengths {
            *length_offset = offset;
            let length = usize::from(read_u16(frame, &mut offset));
            offset += length;
        }
        lengths
    }

    fn truncate_last_payload_byte(frame: &[u8], length_offset: usize) -> Vec<u8> {
        let declared = usize::from(u16::from_be_bytes(
            frame[length_offset..length_offset + 2].try_into().unwrap(),
        ));
        assert!(declared > 0);

        let mut truncated = frame.to_vec();
        let final_payload_byte = length_offset + 2 + declared - 1;
        truncated.remove(final_payload_byte);
        truncated
    }

    #[test]
    fn encoder_emits_canonical_v2_fields_in_contract_order() {
        let record = record();
        assert_eq!(record.journal().state(), JournalState::Committed);
        assert_eq!(record.journal().kind(), JournalKind::ContextRequired);

        let frame = record.encode().unwrap();
        assert!(frame.len() <= MAX_RETIRE_V2_BYTES);

        let mut offset = 0;
        assert_eq!(
            take(&frame, &mut offset, RETIRE_V2_MAGIC.len()),
            RETIRE_V2_MAGIC
        );
        assert_eq!(read_u16(&frame, &mut offset), RETIRE_V2_VERSION);
        let journal_length = usize::from(read_u16(&frame, &mut offset));
        let journal = take(&frame, &mut offset, journal_length);
        assert_eq!(journal, record.journal.encode().unwrap());

        for identity in [
            &record.identidad_intent,
            &record.identidad_journal,
            &record.identidad_contexto,
        ] {
            let length = usize::from(read_u16(&frame, &mut offset));
            assert!((1..=MAX_PERSISTED_FILE_ID_FRAME_BYTES).contains(&length));
            assert_eq!(
                take(&frame, &mut offset, length),
                identity.encode().unwrap()
            );
        }

        assert_eq!(
            take(&frame, &mut offset, 8),
            record.scope_parent.to_be_bytes().as_slice()
        );
        assert_eq!(
            take(&frame, &mut offset, 8),
            record.longitud_contexto.to_be_bytes().as_slice()
        );
        assert_eq!(take(&frame, &mut offset, 32), record.digest_contexto);
        assert_eq!(offset, frame.len());
    }

    #[test]
    fn v2_frame_quota_is_derived_not_a_sample_record_measurement() {
        assert_eq!(FIXED_BYTES, 71);
        assert_eq!(MAX_RETIRE_V2_BYTES, 71 + 1024 + 3 * 147);
        assert_eq!(MAX_RETIRE_V2_BYTES, 1536);
        assert!(record().encode().unwrap().len() < MAX_RETIRE_V2_BYTES);
    }

    #[test]
    fn frame_preflight_borrows_sections_in_wire_order() {
        let record = record();
        let frame = record.encode().unwrap();
        let parsed = parse_frame(&frame).unwrap();

        assert_eq!(parsed.journal, record.journal.encode().unwrap());
        for (bytes, identity) in parsed.identidades.iter().zip([
            &record.identidad_intent,
            &record.identidad_journal,
            &record.identidad_contexto,
        ]) {
            assert_eq!(*bytes, identity.encode().unwrap());
        }
        assert_eq!(parsed.scope_parent, SCOPE);
        assert_eq!(parsed.longitud_contexto, CONTEXT_LENGTH);
        assert_eq!(parsed.digest_contexto, CONTEXT_DIGEST);
        assert!(std::ptr::eq(
            parsed.journal.as_ptr(),
            frame[RETIRE_V2_MAGIC.len() + 4..].as_ptr()
        ));
    }

    #[test]
    fn frame_preflight_rejects_headers_and_nested_lengths_outside_bounds() {
        let frame = record().encode().unwrap();
        let mut wrong_magic = frame.clone();
        wrong_magic[0] ^= 1;
        assert!(parse_frame(&wrong_magic).is_err());

        let mut wrong_version = frame.clone();
        wrong_version[RETIRE_V2_MAGIC.len()..RETIRE_V2_MAGIC.len() + 2]
            .copy_from_slice(&(RETIRE_V2_VERSION + 1).to_be_bytes());
        assert!(parse_frame(&wrong_version).is_err());

        let journal_length_offset = RETIRE_V2_MAGIC.len() + 2;
        for length in [0, MAX_JOURNAL_V2_BYTES + 1] {
            let mut invalid = frame.clone();
            invalid[journal_length_offset..journal_length_offset + 2]
                .copy_from_slice(&u16::try_from(length).unwrap().to_be_bytes());
            assert!(parse_frame(&invalid).is_err());
        }

        for length_offset in identity_length_offsets(&frame) {
            for length in [0, MAX_PERSISTED_FILE_ID_FRAME_BYTES + 1] {
                let mut invalid = frame.clone();
                invalid[length_offset..length_offset + 2]
                    .copy_from_slice(&u16::try_from(length).unwrap().to_be_bytes());
                assert!(parse_frame(&invalid).is_err());
            }
        }
    }

    #[test]
    fn frame_preflight_checks_quota_before_header_and_requires_exact_eof() {
        assert_eq!(preflight_frame_length(8, 8), Ok(()));
        assert_eq!(
            preflight_frame_length(9, 8),
            Err(ErrorCodecIntentRetiroV2::TooLarge)
        );
        assert_eq!(MAX_RETIRE_V2_BYTES, 1536);
        assert!(matches!(
            parse_frame(&vec![0; MAX_RETIRE_V2_BYTES]),
            Err(ErrorCodecIntentRetiroV2::Malformed)
        ));
        assert!(matches!(
            parse_frame(&vec![0; MAX_RETIRE_V2_BYTES + 1]),
            Err(ErrorCodecIntentRetiroV2::TooLarge)
        ));

        let frame = record().encode().unwrap();
        assert!(parse_frame(&frame[..frame.len() - 1]).is_err());
        let mut trailing = frame;
        trailing.push(0);
        assert!(parse_frame(&trailing).is_err());
    }

    #[test]
    fn frame_preflight_rejects_nested_truncation_with_original_lengths() {
        let frame = record().encode().unwrap();
        let mut length_offsets = vec![RETIRE_V2_MAGIC.len() + 2];
        length_offsets.extend(identity_length_offsets(&frame));

        for length_offset in length_offsets {
            let truncated = truncate_last_payload_byte(&frame, length_offset);
            assert!(parse_frame(&truncated).is_err());
        }
    }

    #[test]
    fn frame_preflight_rejects_offset_addition_overflow() {
        let mut offset = usize::MAX;
        assert!(matches!(
            super::take(&[], &mut offset, 1),
            Err(ErrorCodecIntentRetiroV2::Malformed)
        ));
    }

    #[test]
    fn codec_error_debug_and_display_are_fixed_and_redacted() {
        let error = ErrorCodecIntentRetiroV2::Malformed;
        assert_eq!(format!("{error:?}"), "ErrorCodecIntentRetiroV2([redacted])");
        assert_eq!(error.to_string(), "invalid_retirement_envelope_v2");
        assert!(!format!("{error:?}").contains(TRANSACTION_ID));
    }
}
