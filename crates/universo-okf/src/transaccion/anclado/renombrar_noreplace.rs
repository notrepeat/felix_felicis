//! Syscall sibling atómico que nunca reemplaza el destino.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CausaRenombrarNoReemplazo {
    Io,
    DestinoExistente,
    OperacionAmbigua,
}

/// Error interno redacted; no conserva rutas ni nombres de componentes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ErrorRenombrarNoReemplazo {
    causa: CausaRenombrarNoReemplazo,
}

impl ErrorRenombrarNoReemplazo {
    const fn nuevo(causa: CausaRenombrarNoReemplazo) -> Self {
        Self { causa }
    }

    pub(super) const fn causa(self) -> CausaRenombrarNoReemplazo {
        self.causa
    }
}

impl fmt::Debug for ErrorRenombrarNoReemplazo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ErrorRenombrarNoReemplazo { .. }")
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) fn renombrar_unix(
    parent: &std::fs::File,
    fuente: &std::ffi::CStr,
    destino: &std::ffi::CStr,
) -> Result<(), ErrorRenombrarNoReemplazo> {
    renombrar_unix_con(parent, fuente, destino, invocar_renameat2)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) fn renombrar_unix_con<I>(
    parent: &std::fs::File,
    fuente: &std::ffi::CStr,
    destino: &std::ffi::CStr,
    invocar: I,
) -> Result<(), ErrorRenombrarNoReemplazo>
where
    I: FnOnce(&std::fs::File, &std::ffi::CStr, &std::ffi::CStr) -> (libc::c_long, Option<i32>),
{
    let (resultado, error) = invocar(parent, fuente, destino);
    if resultado == 0 {
        return Ok(());
    }

    Err(mapear_error_renameat2(error))
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) fn invocar_renameat2(
    parent: &std::fs::File,
    fuente: &std::ffi::CStr,
    destino: &std::ffi::CStr,
) -> (libc::c_long, Option<i32>) {
    use std::os::fd::AsRawFd;

    // SAFETY: both NUL-terminated names are validated sibling components and
    // the retained parent descriptor anchors lookup. The kernel performs the
    // no-replace check atomically; there is deliberately no renameat fallback.
    let resultado = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            parent.as_raw_fd(),
            fuente.as_ptr(),
            parent.as_raw_fd(),
            destino.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    let error = (resultado != 0)
        .then(std::io::Error::last_os_error)
        .and_then(|error| error.raw_os_error());
    (resultado, error)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn mapear_error_renameat2(error: Option<i32>) -> ErrorRenombrarNoReemplazo {
    use CausaRenombrarNoReemplazo as Causa;

    let causa = match error {
        Some(libc::EEXIST) => Causa::DestinoExistente,
        // An unavailable syscall or unsupported flag fails closed. In
        // particular, neither condition may fall back to replace-capable renameat.
        Some(libc::ENOSYS | libc::EINVAL) => Causa::Io,
        Some(_) | None => Causa::OperacionAmbigua,
    };
    ErrorRenombrarNoReemplazo::nuevo(causa)
}

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
#[path = "renombrar_noreplace_tests.rs"]
mod tests;
