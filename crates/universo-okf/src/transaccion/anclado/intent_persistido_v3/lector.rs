//! Lectura acotada del frame; no abre nombres ni concede autoridad filesystem.
use std::io::{self, Read};

use super::{ErrorCodecIntentRetiroV3, IntentRetiroPersistidoV3};
use crate::transaccion::anclado::intent_persistido_v3_preflight::{
    ErrorPreflightIntentRetiroV3, HEADER_BYTES, HeaderPreflightIntentRetiroV3,
};

#[derive(Debug, PartialEq, Eq)]
pub(in crate::transaccion::anclado) enum ErrorLecturaIntentRetiroV3 {
    Io,
    Codec(ErrorCodecIntentRetiroV3),
}

pub(super) fn leer<R: Read>(
    reader: &mut R,
) -> Result<IntentRetiroPersistidoV3, ErrorLecturaIntentRetiroV3> {
    let mut header = [0; HEADER_BYTES];
    reader.read_exact(&mut header).map_err(map_io)?;
    let parsed = HeaderPreflightIntentRetiroV3::parse(&header).map_err(|error| {
        ErrorLecturaIntentRetiroV3::Codec(match error {
            ErrorPreflightIntentRetiroV3::Malformed => ErrorCodecIntentRetiroV3::Malformed,
            ErrorPreflightIntentRetiroV3::TooLarge => ErrorCodecIntentRetiroV3::TooLarge,
        })
    })?;
    let mut frame = Vec::new();
    frame
        .try_reserve_exact(parsed.total_bytes())
        .map_err(|_| ErrorLecturaIntentRetiroV3::Codec(ErrorCodecIntentRetiroV3::Allocation))?;
    frame.extend_from_slice(&header);
    frame.resize(parsed.total_bytes(), 0);
    reader
        .read_exact(&mut frame[HEADER_BYTES..])
        .map_err(map_io)?;
    let mut sentinel = [0];
    loop {
        match reader.read(&mut sentinel) {
            Ok(0) => break,
            Ok(_) => {
                return Err(ErrorLecturaIntentRetiroV3::Codec(
                    ErrorCodecIntentRetiroV3::Malformed,
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(map_io(error)),
        }
    }
    IntentRetiroPersistidoV3::decode(frame).map_err(ErrorLecturaIntentRetiroV3::Codec)
}

fn map_io(error: io::Error) -> ErrorLecturaIntentRetiroV3 {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        ErrorLecturaIntentRetiroV3::Codec(ErrorCodecIntentRetiroV3::Malformed)
    } else {
        ErrorLecturaIntentRetiroV3::Io
    }
}
