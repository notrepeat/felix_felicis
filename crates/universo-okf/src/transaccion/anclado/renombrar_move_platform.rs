use super::super::super::metadata_directorio;
use super::super::{ErrorRenombrarAnclado, IdentidadDirectorio, mapear_error_directorio};
use std::ffi::OsStr;
use std::fs::File;

#[cfg(unix)]
pub(super) fn renombrar_plataforma(
    parent: &File,
    fuente: &OsStr,
    destino: &OsStr,
    raiz_conocida: &File,
    identidad_esperada: IdentidadDirectorio,
) -> Result<(), ErrorRenombrarAnclado> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let fuente =
        CString::new(fuente.as_bytes()).map_err(|_| ErrorRenombrarAnclado::ComponenteInvalido)?;
    let destino =
        CString::new(destino.as_bytes()).map_err(|_| ErrorRenombrarAnclado::ComponenteInvalido)?;
    let identidad_padre = metadata_directorio(parent).map_err(mapear_error_directorio)?;

    let mut estado_destino = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: both names are validated NUL-terminated components and the live
    // parent descriptor anchors the lookup. No-follow detects any destination.
    let resultado = unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            destino.as_ptr(),
            estado_destino.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if resultado == 0 {
        return Err(ErrorRenombrarAnclado::DestinoExistente);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() != Some(libc::ENOENT) {
        return Err(mapear_error_io(error));
    }

    let identidad_nombrada = identidad_de_nombre(parent, &fuente)?;
    if identidad_nombrada != identidad_esperada
        || metadata_directorio(raiz_conocida).map_err(mapear_error_directorio)?
            != identidad_esperada
    {
        return Err(ErrorRenombrarAnclado::IdentidadNoCoincide);
    }
    if metadata_directorio(parent).map_err(mapear_error_directorio)? != identidad_padre {
        return Err(ErrorRenombrarAnclado::FuenteNoHermana);
    }

    renombrar_noreplace(parent, &fuente, &destino)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn renombrar_noreplace(
    parent: &File,
    fuente: &std::ffi::CStr,
    destino: &std::ffi::CStr,
) -> Result<(), ErrorRenombrarAnclado> {
    use std::os::fd::AsRawFd;

    // SAFETY: both NUL-terminated names are single components relative to the
    // retained parent descriptor. The kernel performs the no-replace check and
    // rename atomically; there is deliberately no renameat fallback.
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
    if resultado == 0 {
        return Ok(());
    }

    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::EEXIST) => Err(ErrorRenombrarAnclado::DestinoExistente),
        // An unavailable syscall or unsupported flag fails closed. In
        // particular, neither condition may fall back to replace-capable renameat.
        Some(libc::ENOSYS | libc::EINVAL) => Err(ErrorRenombrarAnclado::Io),
        // Preserve conservative recovery for errors whose effect is uncertain.
        _ => Err(ErrorRenombrarAnclado::OperacionAmbigua),
    }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
fn renombrar_noreplace(
    _parent: &File,
    _fuente: &std::ffi::CStr,
    _destino: &std::ffi::CStr,
) -> Result<(), ErrorRenombrarAnclado> {
    // Other Unix platforms have no verified no-replace primitive in this
    // work unit. Never emulate it with the racy, replace-capable renameat.
    Err(ErrorRenombrarAnclado::Io)
}

#[cfg(unix)]
fn identidad_de_nombre(
    parent: &File,
    nombre: &std::ffi::CStr,
) -> Result<IdentidadDirectorio, ErrorRenombrarAnclado> {
    use std::os::fd::{AsRawFd, FromRawFd};

    let mut estado = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `nombre` is a validated C string and fstatat writes `estado`.
    let resultado = unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            nombre.as_ptr(),
            estado.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if resultado != 0 {
        return Err(mapear_error_io(std::io::Error::last_os_error()));
    }
    // SAFETY: fstatat returned success and initialized the structure.
    let estado = unsafe { estado.assume_init() };
    match estado.st_mode & libc::S_IFMT {
        libc::S_IFLNK => return Err(ErrorRenombrarAnclado::EnlaceNoPermitido),
        libc::S_IFDIR => {}
        _ => return Err(ErrorRenombrarAnclado::TipoNoAdmitido),
    }
    // SAFETY: O_NOFOLLOW and O_DIRECTORY force a real directory open; the
    // resulting descriptor is uniquely owned by this File.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            nombre.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(mapear_error_io(std::io::Error::last_os_error()));
    }
    // SAFETY: successful openat returned a fresh owned descriptor.
    let abierto = unsafe { File::from_raw_fd(fd) };
    metadata_directorio(&abierto).map_err(mapear_error_directorio)
}

#[cfg(windows)]
pub(super) fn renombrar_plataforma(
    parent: &File,
    fuente: &OsStr,
    destino: &OsStr,
    raiz_conocida: &File,
    identidad_esperada: IdentidadDirectorio,
) -> Result<(), ErrorRenombrarAnclado> {
    use fs_at::OpenOptions as OpenOptionsAt;
    use fs_at::os::windows::OpenOptionsExt;
    use std::mem::{offset_of, size_of};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;
    use windows_sys::Wdk::Storage::FileSystem::{
        FILE_RENAME_INFORMATION, FileRenameInformation, NtSetInformationFile,
    };
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_READ_ATTRIBUTES};
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    // A relative no-follow open is enough to detect any existing target,
    // including files and reparse points; it is never opened for writing.
    let mut opciones_destino = OpenOptionsAt::default();
    opciones_destino
        .desired_access(FILE_READ_ATTRIBUTES)
        .follow(false);
    match opciones_destino.open_at(parent, Path::new(destino)) {
        Ok(_) => return Err(ErrorRenombrarAnclado::DestinoExistente),
        Err(error) if windows_not_found(&error) => {}
        Err(error) => return Err(mapear_error_io(error)),
    }

    // Request only the attributes needed to verify the object and DELETE to
    // rename it. FILE_OPEN_REPARSE_POINT is set by fs_at when follow(false).
    let mut opciones_fuente = OpenOptionsAt::default();
    opciones_fuente
        .desired_access(FILE_READ_ATTRIBUTES | DELETE)
        .follow(false);
    let fuente_renombrable = opciones_fuente
        .open_dir_at(parent, Path::new(fuente))
        .map_err(mapear_error_io)?;
    let identidad = metadata_directorio(&fuente_renombrable).map_err(mapear_error_directorio)?;
    if identidad != identidad_esperada
        || metadata_directorio(raiz_conocida).map_err(mapear_error_directorio)?
            != identidad_esperada
    {
        return Err(ErrorRenombrarAnclado::IdentidadNoCoincide);
    }

    let utf16: Vec<u16> = destino.encode_wide().collect();
    let offset = offset_of!(FILE_RENAME_INFORMATION, FileName);
    let nombre_bytes = utf16
        .len()
        .checked_mul(size_of::<u16>())
        .ok_or(ErrorRenombrarAnclado::ComponenteInvalido)?;
    let byte_len = size_of::<FILE_RENAME_INFORMATION>()
        .checked_add(nombre_bytes)
        .ok_or(ErrorRenombrarAnclado::ComponenteInvalido)?;
    let bytes =
        u32::try_from(nombre_bytes).map_err(|_| ErrorRenombrarAnclado::ComponenteInvalido)?;
    let words = byte_len.div_ceil(size_of::<usize>());
    let mut storage = vec![0usize; words];
    let rename_info = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    // SAFETY: `storage` is usize-aligned and sized to contain the fixed header
    // plus the encoded name. The pointers are valid for these writes.
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
    // SAFETY: the source handle has DELETE access, RootDirectory is the retained
    // parent, ReplaceIfExists=false, and this aligned buffer contains a relative
    // UTF-16 component matching FILE_RENAME_INFORMATION. The handle is opened
    // synchronously by fs_at, so the operation completes before this buffer ends.
    let mut io_status = IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtSetInformationFile(
            fuente_renombrable.as_raw_handle() as HANDLE,
            &mut io_status,
            rename_info.cast(),
            u32::try_from(byte_len).map_err(|_| ErrorRenombrarAnclado::ComponenteInvalido)?,
            FileRenameInformation,
        )
    };
    if status < 0 {
        // NTSTATUS never leaves the primitive. A failed rename syscall can
        // still have moved the directory, so recovery must reconcile it.
        return Err(ErrorRenombrarAnclado::OperacionAmbigua);
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(super) fn renombrar_plataforma(
    _parent: &File,
    _fuente: &OsStr,
    _destino: &OsStr,
    _raiz_conocida: &File,
    _identidad_esperada: IdentidadDirectorio,
) -> Result<(), ErrorRenombrarAnclado> {
    Err(ErrorRenombrarAnclado::TipoNoAdmitido)
}

#[cfg(unix)]
fn mapear_error_io(error: std::io::Error) -> ErrorRenombrarAnclado {
    match error.raw_os_error() {
        Some(libc::ELOOP) => ErrorRenombrarAnclado::EnlaceNoPermitido,
        Some(libc::ENOENT) => ErrorRenombrarAnclado::FuenteAusente,
        Some(libc::EEXIST | libc::ENOTEMPTY) => ErrorRenombrarAnclado::DestinoExistente,
        _ => ErrorRenombrarAnclado::Io,
    }
}

#[cfg(windows)]
fn mapear_error_io(error: std::io::Error) -> ErrorRenombrarAnclado {
    use windows_sys::Win32::Foundation::{
        ERROR_ALREADY_EXISTS, ERROR_CANT_RESOLVE_FILENAME, ERROR_DIRECTORY, ERROR_FILE_EXISTS,
        ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND,
    };

    match error.raw_os_error().map(|code| code as u32) {
        Some(ERROR_ALREADY_EXISTS) | Some(ERROR_FILE_EXISTS) => {
            ErrorRenombrarAnclado::DestinoExistente
        }
        Some(ERROR_FILE_NOT_FOUND) | Some(ERROR_PATH_NOT_FOUND) => {
            ErrorRenombrarAnclado::FuenteAusente
        }
        Some(ERROR_CANT_RESOLVE_FILENAME) => ErrorRenombrarAnclado::EnlaceNoPermitido,
        Some(ERROR_DIRECTORY) => ErrorRenombrarAnclado::TipoNoAdmitido,
        _ => ErrorRenombrarAnclado::Io,
    }
}

#[cfg(windows)]
fn windows_not_found(error: &std::io::Error) -> bool {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};

    matches!(
        error.raw_os_error().map(|code| code as u32),
        Some(ERROR_FILE_NOT_FOUND) | Some(ERROR_PATH_NOT_FOUND)
    ) || error.kind() == std::io::ErrorKind::NotFound
}

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
mod tests {
    use super::{ErrorRenombrarAnclado, renombrar_noreplace};
    use std::ffi::CString;
    use std::fs::File;

    #[test]
    fn renameat2_noreplace_preserva_fuente_y_destino_existente() {
        let temp = tempfile::tempdir().unwrap();
        let fuente = temp.path().join("source");
        let destino = temp.path().join("destination");
        std::fs::create_dir(&fuente).unwrap();
        std::fs::create_dir(&destino).unwrap();
        std::fs::write(fuente.join("marker"), b"source remains").unwrap();
        std::fs::write(destino.join("marker"), b"destination remains").unwrap();
        let parent = File::open(temp.path()).unwrap();

        let resultado = renombrar_noreplace(
            &parent,
            &CString::new("source").unwrap(),
            &CString::new("destination").unwrap(),
        );

        assert_eq!(resultado, Err(ErrorRenombrarAnclado::DestinoExistente));
        assert_eq!(
            std::fs::read(fuente.join("marker")).unwrap(),
            b"source remains"
        );
        assert_eq!(
            std::fs::read(destino.join("marker")).unwrap(),
            b"destination remains"
        );
    }
}
