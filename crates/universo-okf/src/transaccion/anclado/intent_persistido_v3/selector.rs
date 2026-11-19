//! Selector de lectura independiente: no amplía cuotas ni autoridad del lector legacy.
use std::io::{self, Cursor, Read};

use super::{ErrorLecturaIntentRetiroV3, IntentRetiroPersistidoV3};
use crate::transaccion::anclado::intent_persistido::{
    IntentRetiroPersistido, IntentRetiroPersistidoVersionado, MAX_INTENT_BYTES,
};
use crate::transaccion::anclado::intent_persistido_v2::{
    IntentRetiroPersistidoV2, MAX_RETIRE_V2_BYTES,
};
use crate::transaccion::anclado::intent_persistido_v3_preflight::RETIRE_V3_MAGIC;

#[derive(Debug)]
pub(in crate::transaccion::anclado) enum IntentSeleccionado {
    Legacy(IntentRetiroPersistidoVersionado),
    V3(IntentRetiroPersistidoV3),
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::transaccion::anclado) enum ErrorSeleccion {
    Io,
    Header,
    Version,
    Legacy,
    TooLarge,
    V3(ErrorLecturaIntentRetiroV3),
}

pub(in crate::transaccion::anclado) fn leer<R: Read>(
    reader: &mut R,
) -> Result<IntentSeleccionado, ErrorSeleccion> {
    let mut header = [0; 15];
    reader.read_exact(&mut header).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            ErrorSeleccion::Header
        } else {
            ErrorSeleccion::Io
        }
    })?;
    if &header[..13] != RETIRE_V3_MAGIC {
        return Err(ErrorSeleccion::Header);
    }
    let version = u16::from_be_bytes([header[13], header[14]]);
    let joined = Cursor::new(header).chain(reader);
    if version == 3 {
        return IntentRetiroPersistidoV3::leer(&mut { joined })
            .map(IntentSeleccionado::V3)
            .map_err(ErrorSeleccion::V3);
    }
    let limit = match version {
        1 => MAX_INTENT_BYTES,
        2 => MAX_RETIRE_V2_BYTES,
        _ => return Err(ErrorSeleccion::Version),
    };
    let mut frame = Vec::new();
    joined
        .take((limit + 1) as u64)
        .read_to_end(&mut frame)
        .map_err(|_| ErrorSeleccion::Io)?;
    if frame.len() > limit {
        return Err(ErrorSeleccion::TooLarge);
    }
    let legacy = match version {
        1 => IntentRetiroPersistido::decode(&frame)
            .map(IntentRetiroPersistidoVersionado::V1)
            .map_err(|_| ErrorSeleccion::Legacy)?,
        2 => IntentRetiroPersistidoV2::decode(&frame)
            .map(IntentRetiroPersistidoVersionado::V2)
            .map_err(|_| ErrorSeleccion::Legacy)?,
        _ => unreachable!("version already selected"),
    };
    Ok(IntentSeleccionado::Legacy(legacy))
}

#[cfg(test)]
#[path = "selector_tests.rs"]
mod tests;
