/*! Structural preflight for the self-contained RETIRE v3 frame.

The header parser runs before payload allocation or reading. It validates
fixed framing fields and computes checked offsets for both payloads.
*/

use std::fmt;
use std::ops::Range;

use crate::transaccion::anclado::intent_persistido_v2::MAX_RETIRE_V2_BYTES;

pub(super) const RETIRE_V3_MAGIC: &[u8; 13] = b"FELIX-RETIRE\0";
pub(super) const RETIRE_V3_VERSION: u16 = 3;
pub(super) const KIND_CONTEXT_V2: u8 = 1;
pub(super) const MAX_CONTEXT_BYTES: u64 = 512 * 1024 * 1024;
pub(super) const MAX_METADATA_TOTAL_BYTES: usize = 4096;
pub(super) const HEADER_BYTES: usize = RETIRE_V3_MAGIC.len() + 2 + 1 + 2 + 8;
pub(super) const MAX_FRAME_BYTES: usize = 536_875_008;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorPreflightIntentRetiroV3 {
    TooLarge,
    Malformed,
}

impl ErrorPreflightIntentRetiroV3 {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::TooLarge => "retirement_v3_too_large",
            Self::Malformed => "retirement_v3_malformed",
        }
    }
}

impl fmt::Debug for ErrorPreflightIntentRetiroV3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl fmt::Display for ErrorPreflightIntentRetiroV3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ErrorPreflightIntentRetiroV3 {}

/// Checked byte offsets derived from the fixed header, before payload I/O.
#[derive(PartialEq, Eq)]
pub(super) struct HeaderPreflightIntentRetiroV3 {
    metadata: Range<usize>,
    contexto: Range<usize>,
    total_bytes: usize,
}

impl fmt::Debug for HeaderPreflightIntentRetiroV3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HeaderPreflightIntentRetiroV3([redacted])")
    }
}

impl HeaderPreflightIntentRetiroV3 {
    pub(super) fn parse(header: &[u8]) -> Result<Self, ErrorPreflightIntentRetiroV3> {
        if header.len() != HEADER_BYTES || &header[..RETIRE_V3_MAGIC.len()] != RETIRE_V3_MAGIC {
            return Err(ErrorPreflightIntentRetiroV3::Malformed);
        }

        let mut offset = RETIRE_V3_MAGIC.len();
        if read_u16(header, &mut offset)? != RETIRE_V3_VERSION
            || read_u8(header, &mut offset)? != KIND_CONTEXT_V2
        {
            return Err(ErrorPreflightIntentRetiroV3::Malformed);
        }
        let metadata_len = usize::from(read_u16(header, &mut offset)?);
        let contexto_len = read_u64(header, &mut offset)?;
        let total_bytes = total_frame_bytes(metadata_len, contexto_len)?;
        let metadata_end = HEADER_BYTES
            .checked_add(metadata_len)
            .filter(|end| *end <= total_bytes)
            .ok_or(ErrorPreflightIntentRetiroV3::Malformed)?;
        Ok(Self {
            metadata: HEADER_BYTES..metadata_end,
            contexto: metadata_end..total_bytes,
            total_bytes,
        })
    }

    pub(super) fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub(super) fn payloads<'a>(
        &self,
        frame: &'a [u8],
    ) -> Result<(&'a [u8], &'a [u8]), ErrorPreflightIntentRetiroV3> {
        if frame.len() != self.total_bytes || frame.len() < HEADER_BYTES {
            return Err(ErrorPreflightIntentRetiroV3::Malformed);
        }
        let actual_header = Self::parse(&frame[..HEADER_BYTES])?;
        if actual_header != *self {
            return Err(ErrorPreflightIntentRetiroV3::Malformed);
        }
        Ok((&frame[self.metadata.clone()], &frame[self.contexto.clone()]))
    }
}

pub(super) fn total_frame_bytes(
    metadata_len: usize,
    contexto_len: u64,
) -> Result<usize, ErrorPreflightIntentRetiroV3> {
    if metadata_len == 0 {
        return Err(ErrorPreflightIntentRetiroV3::Malformed);
    }
    let metadata_total = HEADER_BYTES
        .checked_add(metadata_len)
        .ok_or(ErrorPreflightIntentRetiroV3::Malformed)?;
    if metadata_total > MAX_METADATA_TOTAL_BYTES {
        return Err(ErrorPreflightIntentRetiroV3::TooLarge);
    }
    if metadata_len > MAX_RETIRE_V2_BYTES {
        return Err(ErrorPreflightIntentRetiroV3::Malformed);
    }
    if contexto_len == 0 {
        return Err(ErrorPreflightIntentRetiroV3::Malformed);
    }
    if contexto_len > MAX_CONTEXT_BYTES {
        return Err(ErrorPreflightIntentRetiroV3::TooLarge);
    }
    let contexto_len =
        usize::try_from(contexto_len).map_err(|_| ErrorPreflightIntentRetiroV3::TooLarge)?;
    let total = metadata_total
        .checked_add(contexto_len)
        .ok_or(ErrorPreflightIntentRetiroV3::TooLarge)?;
    if total > MAX_FRAME_BYTES {
        return Err(ErrorPreflightIntentRetiroV3::TooLarge);
    }
    Ok(total)
}

fn read_u8(header: &[u8], offset: &mut usize) -> Result<u8, ErrorPreflightIntentRetiroV3> {
    Ok(take(header, offset, 1)?[0])
}

fn read_u16(header: &[u8], offset: &mut usize) -> Result<u16, ErrorPreflightIntentRetiroV3> {
    take(header, offset, 2)?
        .try_into()
        .map(u16::from_be_bytes)
        .map_err(|_| ErrorPreflightIntentRetiroV3::Malformed)
}

fn read_u64(header: &[u8], offset: &mut usize) -> Result<u64, ErrorPreflightIntentRetiroV3> {
    take(header, offset, 8)?
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| ErrorPreflightIntentRetiroV3::Malformed)
}

fn take<'a>(
    header: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], ErrorPreflightIntentRetiroV3> {
    let end = offset
        .checked_add(length)
        .filter(|end| *end <= header.len())
        .ok_or(ErrorPreflightIntentRetiroV3::Malformed)?;
    let bytes = &header[*offset..end];
    *offset = end;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        ErrorPreflightIntentRetiroV3, HEADER_BYTES, HeaderPreflightIntentRetiroV3, KIND_CONTEXT_V2,
        MAX_CONTEXT_BYTES, MAX_FRAME_BYTES, MAX_METADATA_TOTAL_BYTES, RETIRE_V3_MAGIC,
        RETIRE_V3_VERSION, total_frame_bytes,
    };
    use crate::transaccion::anclado::intent_persistido_v2::MAX_RETIRE_V2_BYTES;

    fn header(metadata_len: u16, contexto_len: u64) -> [u8; HEADER_BYTES] {
        let mut header = [0; HEADER_BYTES];
        header[..RETIRE_V3_MAGIC.len()].copy_from_slice(RETIRE_V3_MAGIC);
        header[13..15].copy_from_slice(&RETIRE_V3_VERSION.to_be_bytes());
        header[15] = KIND_CONTEXT_V2;
        header[16..18].copy_from_slice(&metadata_len.to_be_bytes());
        header[18..26].copy_from_slice(&contexto_len.to_be_bytes());
        header
    }

    #[test]
    fn header_preflight_computes_both_payload_ranges_before_payload_exists() {
        let raw_header = header(80, 120);
        let preflight = HeaderPreflightIntentRetiroV3::parse(&raw_header).unwrap();
        assert_eq!(preflight.metadata, HEADER_BYTES..HEADER_BYTES + 80);
        assert_eq!(preflight.contexto, HEADER_BYTES + 80..HEADER_BYTES + 200);
        assert_eq!(preflight.total_bytes(), HEADER_BYTES + 200);
        assert!(preflight.payloads(&raw_header).is_err());
    }

    #[test]
    fn checked_ranges_require_the_same_exact_header_and_complete_frame() {
        let raw_header = header(80, 120);
        let preflight = HeaderPreflightIntentRetiroV3::parse(&raw_header).unwrap();
        let mut frame = raw_header.to_vec();
        frame.resize(preflight.total_bytes(), 0);
        let (metadata, context) = preflight.payloads(&frame).unwrap();
        assert_eq!(metadata.len(), 80);
        assert_eq!(context.len(), 120);

        let mut truncated = frame.clone();
        truncated.pop();
        assert_eq!(
            preflight.payloads(&truncated),
            Err(ErrorPreflightIntentRetiroV3::Malformed)
        );
        let mut trailing = frame.clone();
        trailing.push(0);
        assert_eq!(
            preflight.payloads(&trailing),
            Err(ErrorPreflightIntentRetiroV3::Malformed)
        );
        let changed_header = header(81, 119);
        frame[..HEADER_BYTES].copy_from_slice(&changed_header);
        assert_eq!(
            preflight.payloads(&frame),
            Err(ErrorPreflightIntentRetiroV3::Malformed)
        );
    }

    #[test]
    fn parser_rejects_wrong_magic_version_kind_and_short_headers() {
        assert_eq!(
            HeaderPreflightIntentRetiroV3::parse(&[0; HEADER_BYTES]),
            Err(ErrorPreflightIntentRetiroV3::Malformed)
        );
        let mut bad_magic = header(80, 120);
        bad_magic[0] ^= 1;
        assert_eq!(
            HeaderPreflightIntentRetiroV3::parse(&bad_magic),
            Err(ErrorPreflightIntentRetiroV3::Malformed)
        );
        for version in [2_u16, 4] {
            let mut bad = header(80, 120);
            bad[13..15].copy_from_slice(&version.to_be_bytes());
            assert!(HeaderPreflightIntentRetiroV3::parse(&bad).is_err());
        }
        for kind in [0, 2] {
            let mut bad = header(80, 120);
            bad[15] = kind;
            assert!(HeaderPreflightIntentRetiroV3::parse(&bad).is_err());
        }
        assert_eq!(
            HeaderPreflightIntentRetiroV3::parse(&header(80, 120)[..HEADER_BYTES - 1]),
            Err(ErrorPreflightIntentRetiroV3::Malformed)
        );
    }

    #[test]
    fn header_lengths_enforce_context_metadata_and_frame_quotas() {
        for metadata_len in [0, u16::MAX] {
            assert!(HeaderPreflightIntentRetiroV3::parse(&header(metadata_len, 1)).is_err());
        }
        for contexto_len in [0, MAX_CONTEXT_BYTES + 1, u64::MAX] {
            assert!(HeaderPreflightIntentRetiroV3::parse(&header(80, contexto_len)).is_err());
        }
        assert_eq!(
            total_frame_bytes(0, 1),
            Err(ErrorPreflightIntentRetiroV3::Malformed)
        );
        assert_eq!(
            total_frame_bytes(MAX_METADATA_TOTAL_BYTES, 1),
            Err(ErrorPreflightIntentRetiroV3::TooLarge)
        );
        assert_eq!(
            total_frame_bytes(usize::MAX, u64::MAX),
            Err(ErrorPreflightIntentRetiroV3::Malformed)
        );
        assert_eq!(
            total_frame_bytes(MAX_RETIRE_V2_BYTES + 1, 1),
            Err(ErrorPreflightIntentRetiroV3::Malformed)
        );
        assert_eq!(
            total_frame_bytes(MAX_RETIRE_V2_BYTES, MAX_CONTEXT_BYTES),
            Ok(HEADER_BYTES + MAX_RETIRE_V2_BYTES + usize::try_from(MAX_CONTEXT_BYTES).unwrap())
        );
        assert!(MAX_FRAME_BYTES >= HEADER_BYTES + usize::try_from(MAX_CONTEXT_BYTES).unwrap());
    }

    #[test]
    fn preflight_errors_are_redacted_and_stable() {
        let malformed = ErrorPreflightIntentRetiroV3::Malformed;
        let too_large = ErrorPreflightIntentRetiroV3::TooLarge;
        assert_eq!(format!("{malformed:?}"), "retirement_v3_malformed");
        assert_eq!(malformed.to_string(), "retirement_v3_malformed");
        assert_eq!(too_large.to_string(), "retirement_v3_too_large");
        assert_eq!(
            format!(
                "{:?}",
                HeaderPreflightIntentRetiroV3::parse(&header(80, 120)).unwrap()
            ),
            "HeaderPreflightIntentRetiroV3([redacted])"
        );
    }
}
