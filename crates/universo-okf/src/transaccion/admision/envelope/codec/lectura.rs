//! Preflight prestado del framing exterior; no asigna ni decodifica payloads.

use crate::transaccion::journal::MAX_JOURNAL_BYTES;

use super::{
    CONTEXT_LENGTH_BYTES, ErrorCodecEnvelopeAdmision, HEADER_BYTES, JOURNAL_LENGTH_BYTES, MAGIC,
    MAX_CODEC_BYTES, VERSION,
};

pub(super) struct WireEnvelope<'a> {
    pub(super) journal: &'a [u8],
    pub(super) context: &'a [u8],
}

pub(super) fn preflight(bytes: &[u8]) -> Result<WireEnvelope<'_>, ErrorCodecEnvelopeAdmision> {
    if bytes.len() > MAX_CODEC_BYTES {
        return Err(ErrorCodecEnvelopeAdmision::TotalExcedeLimite);
    }

    let mut cursor = Cursor::new(bytes);
    if cursor.take(MAGIC.len())? != MAGIC {
        return Err(ErrorCodecEnvelopeAdmision::FramingInvalido);
    }
    if cursor.u16()? != VERSION {
        return Err(ErrorCodecEnvelopeAdmision::VersionNoSoportada);
    }

    let total =
        usize::try_from(cursor.u64()?).map_err(|_| ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    if total > MAX_CODEC_BYTES {
        return Err(ErrorCodecEnvelopeAdmision::TotalExcedeLimite);
    }
    if total != bytes.len() {
        return Err(ErrorCodecEnvelopeAdmision::FramingInvalido);
    }

    let journal_len =
        usize::try_from(cursor.u32()?).map_err(|_| ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    if journal_len > MAX_JOURNAL_BYTES {
        return Err(ErrorCodecEnvelopeAdmision::JournalInvalido);
    }
    let journal = cursor.take(journal_len)?;

    let context_len =
        usize::try_from(cursor.u64()?).map_err(|_| ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    let context_end = cursor
        .position()
        .checked_add(context_len)
        .ok_or(ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
    if context_end > total {
        return Err(ErrorCodecEnvelopeAdmision::FramingInvalido);
    }
    let context = cursor.take(context_len)?;
    cursor.finish()?;

    debug_assert_eq!(
        HEADER_BYTES + JOURNAL_LENGTH_BYTES + journal.len() + CONTEXT_LENGTH_BYTES + context.len(),
        total
    );
    Ok(WireEnvelope { journal, context })
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

    fn take(&mut self, len: usize) -> Result<&'a [u8], ErrorCodecEnvelopeAdmision> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(ErrorCodecEnvelopeAdmision::LongitudExcedida)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(ErrorCodecEnvelopeAdmision::FramingInvalido)?;
        self.position = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, ErrorCodecEnvelopeAdmision> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| ErrorCodecEnvelopeAdmision::FramingInvalido)?;
        Ok(u16::from_be_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, ErrorCodecEnvelopeAdmision> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| ErrorCodecEnvelopeAdmision::FramingInvalido)?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, ErrorCodecEnvelopeAdmision> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| ErrorCodecEnvelopeAdmision::FramingInvalido)?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn finish(&self) -> Result<(), ErrorCodecEnvelopeAdmision> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(ErrorCodecEnvelopeAdmision::FramingInvalido)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(journal: &[u8], context: &[u8]) -> Vec<u8> {
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

    fn error(bytes: &[u8]) -> ErrorCodecEnvelopeAdmision {
        match preflight(bytes) {
            Ok(_) => panic!("se esperaba rechazo de framing"),
            Err(error) => error,
        }
    }

    fn cambiar_total(bytes: &mut [u8], total: u64) {
        bytes[MAGIC.len() + 2..HEADER_BYTES].copy_from_slice(&total.to_be_bytes());
    }

    fn offset_longitud_contexto(bytes: &[u8]) -> usize {
        let inicio = HEADER_BYTES;
        let fin = inicio + JOURNAL_LENGTH_BYTES;
        let journal_len =
            usize::try_from(u32::from_be_bytes(bytes[inicio..fin].try_into().unwrap())).unwrap();
        fin + journal_len
    }

    #[test]
    fn devuelve_payloads_prestados_y_acepta_longitudes_cero_en_framing() {
        let bytes = wire(b"journal", b"context");
        let parsed = preflight(&bytes).unwrap();
        let journal_start = HEADER_BYTES + JOURNAL_LENGTH_BYTES;
        let context_start = offset_longitud_contexto(&bytes) + CONTEXT_LENGTH_BYTES;
        assert_eq!(parsed.journal, b"journal");
        assert_eq!(parsed.context, b"context");
        assert_eq!(parsed.journal.as_ptr(), bytes[journal_start..].as_ptr());
        assert_eq!(parsed.context.as_ptr(), bytes[context_start..].as_ptr());

        let empty = wire(&[], &[]);
        let parsed_empty = preflight(&empty).unwrap();
        assert!(parsed_empty.journal.is_empty());
        assert!(parsed_empty.context.is_empty());
    }

    #[test]
    fn rechaza_todas_las_truncaciones_y_bytes_sobrantes() {
        let bytes = wire(b"j", b"c");
        for end in 0..bytes.len() {
            assert_eq!(
                error(&bytes[..end]),
                ErrorCodecEnvelopeAdmision::FramingInvalido,
                "prefijo de {end} bytes"
            );
        }

        let mut trailing = bytes.clone();
        trailing.push(0);
        let trailing_len = u64::try_from(trailing.len()).unwrap();
        cambiar_total(&mut trailing, trailing_len);
        assert_eq!(
            error(&trailing),
            ErrorCodecEnvelopeAdmision::FramingInvalido
        );
    }

    #[test]
    fn clasifica_magic_version_total_y_exceso_de_cuota() {
        let original = wire(b"j", b"c");
        let mut bad_magic = original.clone();
        bad_magic[0] ^= 1;
        assert_eq!(
            error(&bad_magic),
            ErrorCodecEnvelopeAdmision::FramingInvalido
        );

        let mut bad_version = original.clone();
        bad_version[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&(VERSION + 1).to_be_bytes());
        assert_eq!(
            error(&bad_version),
            ErrorCodecEnvelopeAdmision::VersionNoSoportada
        );

        let mut wrong_total = original.clone();
        cambiar_total(&mut wrong_total, u64::try_from(original.len() + 1).unwrap());
        assert_eq!(
            error(&wrong_total),
            ErrorCodecEnvelopeAdmision::FramingInvalido
        );

        // A declared over-limit size is rejected from the fixed header; no payload allocation is needed.
        let mut over_limit = original;
        cambiar_total(&mut over_limit, u64::try_from(MAX_CODEC_BYTES).unwrap() + 1);
        assert_eq!(
            error(&over_limit),
            ErrorCodecEnvelopeAdmision::TotalExcedeLimite
        );
    }

    #[test]
    fn limita_journal_y_detecta_longitudes_de_contexto_invalidas() {
        let mut oversized_journal = wire(&[], &[]);
        let journal_len_end = HEADER_BYTES + JOURNAL_LENGTH_BYTES;
        oversized_journal[HEADER_BYTES..journal_len_end]
            .copy_from_slice(&u32::try_from(MAX_JOURNAL_BYTES + 1).unwrap().to_be_bytes());
        assert_eq!(
            error(&oversized_journal),
            ErrorCodecEnvelopeAdmision::JournalInvalido
        );

        let mut overflow_context = wire(&[], &[]);
        let context_offset = offset_longitud_contexto(&overflow_context);
        overflow_context[context_offset..context_offset + CONTEXT_LENGTH_BYTES]
            .copy_from_slice(&u64::MAX.to_be_bytes());
        assert_eq!(
            error(&overflow_context),
            ErrorCodecEnvelopeAdmision::LongitudExcedida
        );

        let mut truncated_context = wire(&[], &[]);
        let context_offset = offset_longitud_contexto(&truncated_context);
        truncated_context[context_offset..context_offset + CONTEXT_LENGTH_BYTES]
            .copy_from_slice(&1_u64.to_be_bytes());
        assert_eq!(
            error(&truncated_context),
            ErrorCodecEnvelopeAdmision::FramingInvalido
        );
    }
}
