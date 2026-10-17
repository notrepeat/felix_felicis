//! Preflight prestado del frame ADMENV v2; no interpreta ni copia payloads.

use crate::transaccion::journal::v2::MAX_JOURNAL_V2_BYTES;

use super::super::ErrorCodecEnvelopeContextoAdmisionV2;
use super::{
    CONTEXT_LENGTH_BYTES, HEADER_BYTES, JOURNAL_LENGTH_BYTES, MAGIC, MAX_ADMENV_V2_BYTES,
    MAX_CONTEXT_ARTIFACT_V2_BYTES, VERSION,
};

pub(super) fn preflight(
    bytes: &[u8],
) -> Result<(&[u8], &[u8]), ErrorCodecEnvelopeContextoAdmisionV2> {
    preflight_with_limits(bytes, MAX_ADMENV_V2_BYTES, MAX_CONTEXT_ARTIFACT_V2_BYTES)
}

fn preflight_with_limits(
    bytes: &[u8],
    frame_limit: usize,
    context_limit: usize,
) -> Result<(&[u8], &[u8]), ErrorCodecEnvelopeContextoAdmisionV2> {
    let frame_limit = frame_limit.min(MAX_ADMENV_V2_BYTES);
    let context_limit = context_limit.min(MAX_CONTEXT_ARTIFACT_V2_BYTES);
    if bytes.len() > frame_limit {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::TotalExceedsLimit);
    }

    let mut cursor = Cursor::new(bytes);
    if cursor.take(MAGIC.len())? != MAGIC {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid);
    }
    if cursor.u16()? != VERSION {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::VersionUnsupported);
    }

    let declared_total = cursor.u64()?;
    let frame_limit_u64 = u64::try_from(frame_limit)
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    if declared_total > frame_limit_u64 {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::TotalExceedsLimit);
    }
    let total = usize::try_from(declared_total)
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    if total != bytes.len() {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid);
    }

    let journal_len = usize::try_from(cursor.u32()?)
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    if journal_len == 0 || journal_len > MAX_JOURNAL_V2_BYTES {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid);
    }
    let journal = cursor.take(journal_len)?;

    let declared_context_len = cursor.u64()?;
    let context_limit_u64 = u64::try_from(context_limit)
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    if declared_context_len > context_limit_u64 {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::ContextExceedsLimit);
    }
    let context_len = usize::try_from(declared_context_len)
        .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    let context_end = cursor
        .position()
        .checked_add(context_len)
        .ok_or(ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
    if context_end > total {
        return Err(ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid);
    }
    let context = cursor.take(context_len)?;
    cursor.finish()?;

    debug_assert_eq!(
        HEADER_BYTES + JOURNAL_LENGTH_BYTES + journal.len() + CONTEXT_LENGTH_BYTES + context.len(),
        total
    );
    Ok((journal, context))
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn position(&self) -> usize {
        self.position
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ErrorCodecEnvelopeContextoAdmisionV2> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid)?;
        self.position = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, ErrorCodecEnvelopeContextoAdmisionV2> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid)?;
        Ok(u16::from_be_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, ErrorCodecEnvelopeContextoAdmisionV2> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid)?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, ErrorCodecEnvelopeContextoAdmisionV2> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid)?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn finish(&self) -> Result<(), ErrorCodecEnvelopeContextoAdmisionV2> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(journal: &[u8], context: &[u8]) -> Vec<u8> {
        let total = HEADER_BYTES
            + JOURNAL_LENGTH_BYTES
            + journal.len()
            + CONTEXT_LENGTH_BYTES
            + context.len();
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_be_bytes());
        bytes.extend_from_slice(&u64::try_from(total).unwrap().to_be_bytes());
        bytes.extend_from_slice(&u32::try_from(journal.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(journal);
        bytes.extend_from_slice(&u64::try_from(context.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(context);
        bytes
    }

    fn frame_header_with_journal_length(journal_length: u32) -> Vec<u8> {
        let total = HEADER_BYTES + JOURNAL_LENGTH_BYTES;
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_be_bytes());
        bytes.extend_from_slice(&u64::try_from(total).unwrap().to_be_bytes());
        bytes.extend_from_slice(&journal_length.to_be_bytes());
        bytes
    }

    fn context_length_offset(bytes: &[u8]) -> usize {
        let journal_start = HEADER_BYTES + JOURNAL_LENGTH_BYTES;
        journal_start
            + usize::try_from(u32::from_be_bytes(
                bytes[HEADER_BYTES..journal_start].try_into().unwrap(),
            ))
            .unwrap()
    }

    fn set_total(bytes: &mut [u8], total: u64) {
        bytes[MAGIC.len() + 2..HEADER_BYTES].copy_from_slice(&total.to_be_bytes());
    }

    fn set_context_length(bytes: &mut [u8], length: u64) {
        let start = context_length_offset(bytes);
        bytes[start..start + CONTEXT_LENGTH_BYTES].copy_from_slice(&length.to_be_bytes());
    }

    #[test]
    fn preflight_retorna_slices_del_frame_y_admite_context_vacio() {
        let bytes = frame(b"journal-v2", b"context");
        let (journal, context) = preflight(&bytes).unwrap();
        assert_eq!(journal, b"journal-v2");
        assert_eq!(context, b"context");

        let start = bytes.as_ptr() as usize;
        let end = start + bytes.len();
        for slice in [journal, context] {
            let slice_start = slice.as_ptr() as usize;
            let slice_end = slice_start + slice.len();
            assert!(slice_start >= start && slice_end <= end);
        }

        let empty_context = frame(b"journal-v2", b"");
        assert_eq!(
            preflight(&empty_context),
            Ok((&b"journal-v2"[..], &b""[..]))
        );
    }

    #[test]
    fn preflight_rechaza_magic_y_versiones_distintas_de_v2() {
        let original = frame(b"journal-v2", b"ctx");
        let mut wrong_magic = original.clone();
        wrong_magic[0] ^= 1;
        assert_eq!(
            preflight(&wrong_magic),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid)
        );

        for version in [1, 3, u16::MAX] {
            let mut wrong_version = original.clone();
            wrong_version[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&version.to_be_bytes());
            assert_eq!(
                preflight(&wrong_version),
                Err(ErrorCodecEnvelopeContextoAdmisionV2::VersionUnsupported)
            );
        }
    }

    #[test]
    fn preflight_rechaza_todas_las_truncaciones_trailing_y_total_desigual() {
        let original = frame(b"journal-v2", b"context");
        for end in 0..original.len() {
            assert!(preflight(&original[..end]).is_err(), "truncation at {end}");
        }

        for end in HEADER_BYTES..original.len() {
            let mut prefix = original[..end].to_vec();
            let prefix_length = u64::try_from(prefix.len()).unwrap();
            set_total(&mut prefix, prefix_length);
            assert!(
                preflight(&prefix).is_err(),
                "truncation after matching total at {end}"
            );
        }

        let mut trailing = original.clone();
        trailing.push(b'x');
        assert_eq!(
            preflight(&trailing),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid)
        );

        let mut wrong_total = original.clone();
        set_total(&mut wrong_total, u64::try_from(original.len() + 1).unwrap());
        assert_eq!(
            preflight(&wrong_total),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::FramingInvalid)
        );
    }

    #[test]
    fn preflight_rechaza_journal_vacio_o_sobre_cuota_antes_de_tomar_slice() {
        let empty_journal = frame_header_with_journal_length(0);
        assert_eq!(
            preflight(&empty_journal),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid)
        );

        let oversized_length = u32::try_from(MAX_JOURNAL_V2_BYTES + 1).unwrap();
        let oversized_journal = frame_header_with_journal_length(oversized_length);
        assert_eq!(
            preflight(&oversized_journal),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::JournalInvalid)
        );

        let exact_journal = vec![b'j'; MAX_JOURNAL_V2_BYTES];
        let exact_frame = frame(&exact_journal, b"");
        assert_eq!(preflight(&exact_frame).unwrap().0, exact_journal);
    }

    #[test]
    fn preflight_aplica_limites_inclusivos_antes_de_tomar_context() {
        let bytes = frame(b"journal-v2", b"ctx");
        assert_eq!(
            preflight_with_limits(&bytes, bytes.len(), 3),
            Ok((&b"journal-v2"[..], &b"ctx"[..]))
        );
        assert_eq!(
            preflight_with_limits(&bytes, bytes.len() - 1, 3),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::TotalExceedsLimit)
        );
        assert_eq!(
            preflight_with_limits(&bytes, bytes.len(), 2),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::ContextExceedsLimit)
        );
    }

    #[test]
    fn preflight_rechaza_u64_max_y_offset_checked() {
        let original = frame(b"journal-v2", b"ctx");
        let mut huge_total = original.clone();
        set_total(&mut huge_total, u64::MAX);
        assert_eq!(
            preflight(&huge_total),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::TotalExceedsLimit)
        );

        let mut huge_context = original;
        set_context_length(&mut huge_context, u64::MAX);
        assert_eq!(
            preflight(&huge_context),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::ContextExceedsLimit)
        );

        let mut cursor = Cursor::new(b"x");
        cursor.position = usize::MAX;
        assert_eq!(
            cursor.take(1),
            Err(ErrorCodecEnvelopeContextoAdmisionV2::LengthExceeded)
        );
    }
}
