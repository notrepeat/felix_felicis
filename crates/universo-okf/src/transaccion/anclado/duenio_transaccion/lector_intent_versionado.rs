//! Recolecta un frame RETIRE acotado desde el mismo handle ya abierto.

use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use crate::transaccion::anclado::intent_persistido::{
    self, ErrorSelectorIntentRetiro, IntentRetiroPersistidoVersionado,
};

use super::super::{
    IdentidadArchivo, ScopeFilesystem, identidad_archivo, metadata_es_reparse, scope_filesystem,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorLecturaFrameIntent {
    EntryRejected,
    ScopeUnverified,
    ScopeMismatch,
    FrameTooLarge,
    LengthOverflow,
    ReservationFailed,
    ReadIo,
}

impl ErrorLecturaFrameIntent {
    pub(super) const fn codigo(self) -> &'static str {
        match self {
            Self::EntryRejected => "retirement_intent_entry_rejected",
            Self::ScopeUnverified => "retirement_intent_scope_unverified",
            Self::ScopeMismatch => "retirement_intent_scope_mismatch",
            Self::FrameTooLarge => "retirement_intent_frame_too_large",
            Self::LengthOverflow => "retirement_intent_frame_length_overflow",
            Self::ReservationFailed => "retirement_intent_frame_reservation_failed",
            Self::ReadIo => "retirement_intent_read_io",
        }
    }
}

impl fmt::Debug for ErrorLecturaFrameIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorLecturaFrameIntent")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorLecturaFrameIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorLecturaFrameIntent {}

#[cfg(test)]
#[path = "../duenio_transaccion_tests/lector_intent_versionado_tests.rs"]
mod tests;

/// Bytes y observaciones del handle; no incluye owner, ruta ni capacidad.
pub(super) struct FrameIntentLeido {
    pub(super) archivo_original: File,
    pub(super) bytes: Vec<u8>,
    pub(super) identidad_archivo: IdentidadArchivo,
    pub(super) scope: ScopeFilesystem,
}

impl fmt::Debug for FrameIntentLeido {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FrameIntentLeido { .. }")
    }
}

/// Candidato decodificado antes de comparar vínculos persistidos y nativos.
pub(super) struct CandidatoIntentDecodificadoSinVinculo {
    envelope: IntentRetiroPersistidoVersionado,
    archivo_original: File,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorCandidatoIntentSinVinculo {
    Frame(ErrorLecturaFrameIntent),
    Selector(ErrorSelectorIntentRetiro),
}

impl ErrorCandidatoIntentSinVinculo {
    pub(super) const fn codigo(self) -> &'static str {
        match self {
            Self::Frame(error) => error.codigo(),
            Self::Selector(error) => error.codigo(),
        }
    }
}

impl fmt::Debug for ErrorCandidatoIntentSinVinculo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl fmt::Display for ErrorCandidatoIntentSinVinculo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorCandidatoIntentSinVinculo {}

/// Lee un frame acotado y decodifica su versión sin comprobar vínculos.
pub(super) fn leer_candidato_intent_versionado(
    archivo: File,
    scope_esperado: ScopeFilesystem,
) -> Result<CandidatoIntentDecodificadoSinVinculo, ErrorCandidatoIntentSinVinculo> {
    leer_candidato_con_hook(
        archivo,
        scope_esperado,
        intent_persistido::cuota_maxima_frame_bytes(),
        || {},
    )
}

#[cfg(test)]
pub(super) fn leer_candidato_intent_versionado_con_cap_y_hook<F>(
    archivo: File,
    scope_esperado: ScopeFilesystem,
    cap_prueba: usize,
    despues_metadata: F,
) -> Result<CandidatoIntentDecodificadoSinVinculo, ErrorCandidatoIntentSinVinculo>
where
    F: FnOnce(),
{
    leer_candidato_con_hook(archivo, scope_esperado, cap_prueba, despues_metadata)
}

fn leer_candidato_con_hook<F>(
    archivo: File,
    scope_esperado: ScopeFilesystem,
    cap_prueba: usize,
    despues_metadata: F,
) -> Result<CandidatoIntentDecodificadoSinVinculo, ErrorCandidatoIntentSinVinculo>
where
    F: FnOnce(),
{
    let cap = cap_prueba.min(intent_persistido::cuota_maxima_frame_bytes());
    let frame = leer_con_hook(archivo, scope_esperado, cap, despues_metadata)
        .map_err(ErrorCandidatoIntentSinVinculo::Frame)?;
    let envelope = intent_persistido::decodificar_versionado(&frame.bytes)
        .map_err(ErrorCandidatoIntentSinVinculo::Selector)?;

    Ok(CandidatoIntentDecodificadoSinVinculo {
        envelope,
        archivo_original: frame.archivo_original,
    })
}

pub(super) fn leer_frame_intent(
    archivo: File,
    scope_esperado: ScopeFilesystem,
    cap: usize,
) -> Result<FrameIntentLeido, ErrorLecturaFrameIntent> {
    leer_con_hook(archivo, scope_esperado, cap, || {})
}

#[cfg(test)]
pub(super) fn leer_frame_intent_con_hook<F>(
    archivo: File,
    scope_esperado: ScopeFilesystem,
    cap: usize,
    despues_metadata: F,
) -> Result<FrameIntentLeido, ErrorLecturaFrameIntent>
where
    F: FnOnce(),
{
    leer_con_hook(archivo, scope_esperado, cap, despues_metadata)
}

fn leer_con_hook<F>(
    mut archivo: File,
    scope_esperado: ScopeFilesystem,
    cap: usize,
    despues_metadata: F,
) -> Result<FrameIntentLeido, ErrorLecturaFrameIntent>
where
    F: FnOnce(),
{
    let cap_mas_uno = cap
        .checked_add(1)
        .ok_or(ErrorLecturaFrameIntent::LengthOverflow)?;
    let cap_mas_uno_u64 =
        u64::try_from(cap_mas_uno).map_err(|_| ErrorLecturaFrameIntent::LengthOverflow)?;
    let cap_u64 = u64::try_from(cap).map_err(|_| ErrorLecturaFrameIntent::LengthOverflow)?;

    let metadata = archivo
        .metadata()
        .map_err(|_| ErrorLecturaFrameIntent::ReadIo)?;
    if metadata.file_type().is_symlink() || metadata_es_reparse(&metadata) || !metadata.is_file() {
        return Err(ErrorLecturaFrameIntent::EntryRejected);
    }
    if metadata.len() > cap_u64 {
        return Err(ErrorLecturaFrameIntent::FrameTooLarge);
    }

    let identidad_archivo =
        identidad_archivo(&archivo).map_err(|_| ErrorLecturaFrameIntent::ReadIo)?;
    let scope = scope_filesystem(&archivo).map_err(|_| ErrorLecturaFrameIntent::ScopeUnverified)?;
    if scope != scope_esperado {
        return Err(ErrorLecturaFrameIntent::ScopeMismatch);
    }

    despues_metadata();
    seek_inicio(&mut archivo)?;
    let bytes = leer_acotado(&mut archivo, cap, cap_mas_uno_u64)?;

    Ok(FrameIntentLeido {
        archivo_original: archivo,
        bytes,
        identidad_archivo,
        scope,
    })
}

fn seek_inicio(archivo: &mut File) -> Result<(), ErrorLecturaFrameIntent> {
    loop {
        match archivo.seek(SeekFrom::Start(0)) {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorLecturaFrameIntent::ReadIo),
        }
    }
}

fn leer_acotado<R: Read>(
    reader: &mut R,
    cap: usize,
    cap_mas_uno_u64: u64,
) -> Result<Vec<u8>, ErrorLecturaFrameIntent> {
    let mut bounded = reader.take(cap_mas_uno_u64);
    let mut bytes = Vec::new();
    let mut fragmento = [0_u8; 512];

    loop {
        let leidos = match bounded.read(&mut fragmento) {
            Ok(0) => break,
            Ok(leidos) => leidos,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorLecturaFrameIntent::ReadIo),
        };
        let largo_siguiente = bytes
            .len()
            .checked_add(leidos)
            .ok_or(ErrorLecturaFrameIntent::LengthOverflow)?;
        if largo_siguiente > cap {
            return Err(ErrorLecturaFrameIntent::FrameTooLarge);
        }
        bytes
            .try_reserve(leidos)
            .map_err(|_| ErrorLecturaFrameIntent::ReservationFailed)?;
        bytes.extend_from_slice(&fragmento[..leidos]);
    }
    Ok(bytes)
}
