//! Dueño en memoria del lock y el parent de una transacción.
//!
//! Esta base todavía no crea leases ni persiste evidencia. Solo fija una vez
//! el parent y comprueba que el lock visible sea el mismo archivo que conserva
//! el guard, bajo el modelo de escritores Felix cooperantes. El token de scope
//! disponible no demuestra una frontera general de mounts.

use std::fmt;
use std::fs::File;
use std::sync::Arc;

use crate::transaccion::lock::LockEscritura;

use super::abrir::abrir_entrada_relativa;
use super::{
    ErrorDirectorioAnclado, IdentidadDirectorio, ScopeFilesystem, TipoEntradaAnclada,
    identidad_archivo, metadata_directorio, scope_filesystem, validar_componente,
};

/// Error único y redactado cuando no se puede demostrar el ownership anclado.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorDuenioTransaccionAnclado {
    NoVerificable,
}

impl ErrorDuenioTransaccionAnclado {
    pub(crate) const fn codigo(self) -> &'static str {
        "transaction_owner_unverified"
    }
}

impl fmt::Debug for ErrorDuenioTransaccionAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorDuenioTransaccionAnclado")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorDuenioTransaccionAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorDuenioTransaccionAnclado {}

/// Retiene el guard, el parent abierto y los tokens para leases futuros.
///
/// Sus campos son privados y el parent se conserva aunque cambie su nombre en
/// el namespace. La capability solo coordina escritores cooperantes; no cubre
/// hardlinks ni carreras de procesos externos.
#[must_use = "el dueño mantiene el lock y el parent anclado durante la operación"]
pub(crate) struct DuenioTransaccionAnclado<'lock> {
    _lock: &'lock mut LockEscritura,
    parent: File,
    identidad_parent: IdentidadDirectorio,
    scope_parent: ScopeFilesystem,
    _autoridad: Arc<()>,
}

impl<'lock> DuenioTransaccionAnclado<'lock> {
    /// Fija el parent de la ubicación del lock sin abrir ni inspeccionar root.
    pub(crate) fn desde_lock(
        lock: &'lock mut LockEscritura,
    ) -> Result<Self, ErrorDuenioTransaccionAnclado> {
        let ubicacion = lock.ubicacion();
        let parent_path = ubicacion
            .ruta()
            .parent()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let nombre_lock = ubicacion
            .lock()
            .as_path()
            .file_name()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?
            .to_os_string();
        validar_componente(&nombre_lock).map_err(mapear_error)?;

        let parent = super::abrir_directorio_absoluto(parent_path, true).map_err(mapear_error)?;
        let identidad_parent = metadata_directorio(&parent).map_err(mapear_error)?;
        let scope_parent = scope_filesystem(&parent).map_err(mapear_error)?;
        let lock_visible =
            abrir_entrada_relativa(&parent, &nombre_lock, TipoEntradaAnclada::Archivo)
                .map_err(mapear_error)?;
        let identidad_visible = identidad_archivo(&lock_visible)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let identidad_guard = lock
            .identidad_archivo_lock()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        if identidad_visible != identidad_guard {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        let autoridad = lock.autoridad();
        Ok(Self {
            _lock: lock,
            parent,
            identidad_parent,
            scope_parent,
            _autoridad: autoridad,
        })
    }
}

impl fmt::Debug for DuenioTransaccionAnclado<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DuenioTransaccionAnclado { .. }")
    }
}

fn mapear_error(_error: ErrorDirectorioAnclado) -> ErrorDuenioTransaccionAnclado {
    ErrorDuenioTransaccionAnclado::NoVerificable
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "duenio_transaccion_tests.rs"]
mod tests;
