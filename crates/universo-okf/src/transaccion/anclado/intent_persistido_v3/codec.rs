use std::ops::Range;

use crate::transaccion::anclado::intent_persistido_v2::IntentRetiroPersistidoV2;
use crate::transaccion::anclado::intent_persistido_v3_preflight::{
    self as preflight, ErrorPreflightIntentRetiroV3, HeaderPreflightIntentRetiroV3,
    total_frame_bytes,
};

use super::ErrorCodecIntentRetiroV3;

pub(super) fn codificar(
    metadata: &IntentRetiroPersistidoV2,
    contexto_original: &[u8],
) -> Result<Vec<u8>, ErrorCodecIntentRetiroV3> {
    let metadata = metadata
        .encode()
        .map_err(|_| ErrorCodecIntentRetiroV3::Malformed)?;
    let context_len =
        u64::try_from(contexto_original.len()).map_err(|_| ErrorCodecIntentRetiroV3::TooLarge)?;
    let total = total_frame_bytes(metadata.len(), context_len).map_err(map_preflight_error)?;
    let metadata_len =
        u16::try_from(metadata.len()).map_err(|_| ErrorCodecIntentRetiroV3::Malformed)?;

    let mut frame = Vec::new();
    frame
        .try_reserve_exact(total)
        .map_err(|_| ErrorCodecIntentRetiroV3::Allocation)?;
    frame.extend_from_slice(preflight::RETIRE_V3_MAGIC);
    frame.extend_from_slice(&preflight::RETIRE_V3_VERSION.to_be_bytes());
    frame.push(preflight::KIND_CONTEXT_V2);
    frame.extend_from_slice(&metadata_len.to_be_bytes());
    frame.extend_from_slice(&context_len.to_be_bytes());
    frame.extend_from_slice(&metadata);
    frame.extend_from_slice(contexto_original);
    if frame.len() != total {
        return Err(ErrorCodecIntentRetiroV3::Malformed);
    }
    Ok(frame)
}

pub(super) fn rangos(
    frame: &[u8],
) -> Result<(Range<usize>, Range<usize>), ErrorCodecIntentRetiroV3> {
    let header = frame
        .get(..preflight::HEADER_BYTES)
        .ok_or(ErrorCodecIntentRetiroV3::Malformed)?;
    let preflight = HeaderPreflightIntentRetiroV3::parse(header).map_err(map_preflight_error)?;
    let (metadata, context) = preflight.payloads(frame).map_err(map_preflight_error)?;
    let metadata_end = preflight::HEADER_BYTES
        .checked_add(metadata.len())
        .filter(|end| *end <= frame.len())
        .ok_or(ErrorCodecIntentRetiroV3::Malformed)?;
    let context_start = frame
        .len()
        .checked_sub(context.len())
        .ok_or(ErrorCodecIntentRetiroV3::Malformed)?;
    Ok((
        preflight::HEADER_BYTES..metadata_end,
        context_start..frame.len(),
    ))
}

fn map_preflight_error(error: ErrorPreflightIntentRetiroV3) -> ErrorCodecIntentRetiroV3 {
    match error {
        ErrorPreflightIntentRetiroV3::TooLarge => ErrorCodecIntentRetiroV3::TooLarge,
        ErrorPreflightIntentRetiroV3::Malformed => ErrorCodecIntentRetiroV3::Malformed,
    }
}
