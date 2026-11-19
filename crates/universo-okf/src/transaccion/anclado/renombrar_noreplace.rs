//! Syscall sibling atómico que nunca reemplaza el destino.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CausaRenombrarNoReemplazo {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    Io,
    #[cfg(any(target_os = "linux", target_os = "android"))]
    DestinoExistente,
    OperacionAmbigua,
    #[cfg(windows)]
    ComponenteInvalido,
}

/// Error interno redacted; no conserva rutas ni nombres de componentes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ErrorRenombrarNoReemplazo {
    causa: CausaRenombrarNoReemplazo,
    #[cfg(all(windows, test))]
    diagnostico_ntstatus: Option<(i32, i32)>,
}

impl ErrorRenombrarNoReemplazo {
    const fn nuevo(causa: CausaRenombrarNoReemplazo) -> Self {
        Self {
            causa,
            #[cfg(all(windows, test))]
            diagnostico_ntstatus: None,
        }
    }

    pub(super) const fn causa(self) -> CausaRenombrarNoReemplazo {
        self.causa
    }

    #[cfg(all(windows, test))]
    pub(super) const fn diagnostico_ntstatus(self) -> Option<(i32, i32)> {
        self.diagnostico_ntstatus
    }
}

impl fmt::Debug for ErrorRenombrarNoReemplazo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ErrorRenombrarNoReemplazo { .. }")
    }
}

#[cfg(windows)]
pub(super) fn renombrar_windows(
    parent: &std::fs::File,
    fuente: &std::fs::File,
    destino: &std::ffi::OsStr,
) -> Result<(), ErrorRenombrarNoReemplazo> {
    renombrar_windows_con(parent, fuente, destino, invocar_nt_set_information_file)
}

#[cfg(windows)]
pub(super) fn renombrar_windows_con<I>(
    parent: &std::fs::File,
    fuente: &std::fs::File,
    destino: &std::ffi::OsStr,
    invocar: I,
) -> Result<(), ErrorRenombrarNoReemplazo>
where
    I: FnOnce(
        &std::fs::File,
        &mut windows_sys::Win32::System::IO::IO_STATUS_BLOCK,
        *mut std::ffi::c_void,
        u32,
    ) -> i32,
{
    use std::mem::{offset_of, size_of};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Wdk::Storage::FileSystem::FILE_RENAME_INFORMATION;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    let utf16: Vec<u16> = destino.encode_wide().collect();
    let offset = offset_of!(FILE_RENAME_INFORMATION, FileName);
    let nombre_bytes = utf16.len().checked_mul(size_of::<u16>()).ok_or_else(|| {
        ErrorRenombrarNoReemplazo::nuevo(CausaRenombrarNoReemplazo::ComponenteInvalido)
    })?;
    let byte_len = size_of::<FILE_RENAME_INFORMATION>()
        .checked_add(nombre_bytes)
        .ok_or_else(|| {
            ErrorRenombrarNoReemplazo::nuevo(CausaRenombrarNoReemplazo::ComponenteInvalido)
        })?;
    let bytes = u32::try_from(nombre_bytes).map_err(|_| {
        ErrorRenombrarNoReemplazo::nuevo(CausaRenombrarNoReemplazo::ComponenteInvalido)
    })?;
    let longitud = u32::try_from(byte_len).map_err(|_| {
        ErrorRenombrarNoReemplazo::nuevo(CausaRenombrarNoReemplazo::ComponenteInvalido)
    })?;
    let words = byte_len.div_ceil(size_of::<usize>());
    let mut storage = vec![0usize; words];
    let rename_info = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();

    // SAFETY: `storage` is usize-aligned and sized for the fixed header plus
    // the encoded name. Both handles stay open through the synchronous call.
    unsafe {
        (*rename_info).Anonymous.ReplaceIfExists = false;
        (*rename_info).RootDirectory = parent.as_raw_handle() as HANDLE;
        (*rename_info).FileNameLength = bytes;
        std::ptr::copy_nonoverlapping(
            utf16.as_ptr(),
            storage.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
            utf16.len(),
        );
    }

    let mut io_status = IO_STATUS_BLOCK::default();
    let status = invocar(fuente, &mut io_status, rename_info.cast(), longitud);
    if status < 0 {
        // NT may have applied the rename despite failure; the caller must
        // reconcile sibling state. This helper never retries or cleans up.
        #[cfg(test)]
        let io_status_code = unsafe { io_status.Anonymous.Status };
        #[cfg(test)]
        let error = {
            let mut error =
                ErrorRenombrarNoReemplazo::nuevo(CausaRenombrarNoReemplazo::OperacionAmbigua);
            error.diagnostico_ntstatus = Some((status, io_status_code));
            error
        };
        #[cfg(not(test))]
        let error = ErrorRenombrarNoReemplazo::nuevo(CausaRenombrarNoReemplazo::OperacionAmbigua);
        return Err(error);
    }
    Ok(())
}

#[cfg(windows)]
fn invocar_nt_set_information_file(
    fuente: &std::fs::File,
    io_status: &mut windows_sys::Win32::System::IO::IO_STATUS_BLOCK,
    informacion: *mut std::ffi::c_void,
    longitud: u32,
) -> i32 {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Wdk::Storage::FileSystem::{FileRenameInformation, NtSetInformationFile};
    use windows_sys::Win32::Foundation::HANDLE;

    // SAFETY: the caller prepared an aligned, initialized rename buffer and
    // keeps its source handle alive for this synchronous native call.
    unsafe {
        NtSetInformationFile(
            fuente.as_raw_handle() as HANDLE,
            io_status,
            informacion,
            longitud,
            FileRenameInformation,
        )
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

#[cfg(all(test, windows))]
#[path = "renombrar_noreplace_windows_tests.rs"]
mod windows_tests;
