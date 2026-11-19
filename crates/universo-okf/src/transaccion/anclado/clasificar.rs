use std::ffi::OsStr;
use std::fs::File;
#[cfg(unix)]
use std::io;
#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use fs_at::OpenOptions as OpenOptionsAt;

#[cfg(windows)]
use super::metadata_es_reparse;
use super::{ErrorDirectorioAnclado, mapear_io, validar_componente};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TipoEntradaAnclada {
    Archivo,
    Directorio,
}

/// Classifies one relative entry without following links and before a
/// content-reading open. This is a point-in-time observation: an external
/// process can still replace the name between classification and open.
pub(crate) fn clasificar_entrada_sin_seguir(
    parent: &File,
    componente: &OsStr,
) -> Result<TipoEntradaAnclada, ErrorDirectorioAnclado> {
    validar_componente(componente)?;

    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::mem::MaybeUninit;
        use std::os::fd::AsRawFd;
        use std::os::unix::ffi::OsStrExt;

        let nombre = CString::new(componente.as_bytes())
            .map_err(|_| ErrorDirectorioAnclado::ComponenteInvalido)?;
        let mut metadata = MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `nombre` is NUL-terminated, `parent` is a live directory
        // handle, and `metadata` is writable for the duration of fstatat.
        let resultado = unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                nombre.as_ptr(),
                metadata.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if resultado != 0 {
            return Err(mapear_io(io::Error::last_os_error()));
        }
        // SAFETY: fstatat returned success and initialized the stat structure.
        let tipo = unsafe { metadata.assume_init() }.st_mode & libc::S_IFMT;
        if tipo == libc::S_IFLNK {
            Err(ErrorDirectorioAnclado::EnlaceNoPermitido)
        } else if tipo == libc::S_IFREG {
            Ok(TipoEntradaAnclada::Archivo)
        } else if tipo == libc::S_IFDIR {
            Ok(TipoEntradaAnclada::Directorio)
        } else {
            Err(ErrorDirectorioAnclado::TipoNoAdmitido)
        }
    }

    #[cfg(windows)]
    {
        use fs_at::os::windows::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;

        // Request attributes only; open_path_at preserves reparse points.
        // This still obtains an object handle and is subject to filesystem or
        // driver behavior, but does not read content or follow a reparse point.
        let mut opciones = OpenOptionsAt::default();
        opciones.desired_access(FILE_READ_ATTRIBUTES);
        let entrada = opciones
            .open_path_at(parent, Path::new(componente))
            .map_err(mapear_io)?;
        let metadata = entrada.metadata().map_err(mapear_io)?;
        if metadata.file_type().is_symlink() || metadata_es_reparse(&metadata) {
            Err(ErrorDirectorioAnclado::EnlaceNoPermitido)
        } else if metadata.is_file() {
            Ok(TipoEntradaAnclada::Archivo)
        } else if metadata.is_dir() {
            Ok(TipoEntradaAnclada::Directorio)
        } else {
            Err(ErrorDirectorioAnclado::TipoNoAdmitido)
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (parent, componente);
        Err(ErrorDirectorioAnclado::TipoNoAdmitido)
    }
}
