use std::ffi::OsStr;
use std::fs::File;
use std::io;

use super::{ErrorDirectorioAnclado, TipoEntradaAnclada, mapear_io, metadata_es_reparse};

/// Abre un componente único relativo al handle del parent, sin seguir enlaces.
///
/// Unix usa `O_NONBLOCK` además del prefilter de tipo: así, si el nombre cambia
/// de archivo regular a FIFO antes de `openat`, no queda esperando a un writer.
/// La metadata del handle abierto vuelve a validar el tipo esperado. Ninguno de
/// estos checks es una defensa frente a un proceso externo que sustituya
/// nombres concurrentemente.
pub(super) fn abrir_entrada_relativa(
    parent: &File,
    componente: &OsStr,
    tipo_esperado: TipoEntradaAnclada,
) -> Result<File, ErrorDirectorioAnclado> {
    let handle = abrir_plataforma(parent, componente, tipo_esperado)?;
    validar_tipo_abierto(&handle, tipo_esperado)?;
    Ok(handle)
}

#[cfg(unix)]
fn abrir_plataforma(
    parent: &File,
    componente: &OsStr,
    tipo_esperado: TipoEntradaAnclada,
) -> Result<File, ErrorDirectorioAnclado> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;

    let nombre = CString::new(componente.as_bytes())
        .map_err(|_| ErrorDirectorioAnclado::ComponenteInvalido)?;
    let flags = flags_unix(tipo_esperado);
    // SAFETY: `nombre` is NUL-terminated, `parent` is a live directory
    // handle, and no creation flag is passed, so no mode argument is needed.
    let descriptor = unsafe { libc::openat(parent.as_raw_fd(), nombre.as_ptr(), flags) };
    if descriptor < 0 {
        return Err(mapear_apertura(io::Error::last_os_error()));
    }
    // SAFETY: on success openat returns a fresh descriptor owned by this call.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

#[cfg(unix)]
fn flags_unix(tipo_esperado: TipoEntradaAnclada) -> libc::c_int {
    let mut flags =
        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC;
    if tipo_esperado == TipoEntradaAnclada::Directorio {
        flags |= libc::O_DIRECTORY;
    }
    flags
}

#[cfg(windows)]
fn abrir_plataforma(
    parent: &File,
    componente: &OsStr,
    tipo_esperado: TipoEntradaAnclada,
) -> Result<File, ErrorDirectorioAnclado> {
    use std::path::Path;

    use fs_at::OpenOptions as OpenOptionsAt;

    let mut opciones = OpenOptionsAt::default();
    opciones.read(true).follow(false);
    let resultado = match tipo_esperado {
        TipoEntradaAnclada::Archivo => opciones.open_at(parent, Path::new(componente)),
        TipoEntradaAnclada::Directorio => opciones.open_dir_at(parent, Path::new(componente)),
    };
    resultado.map_err(mapear_apertura)
}

#[cfg(not(any(unix, windows)))]
fn abrir_plataforma(
    _parent: &File,
    _componente: &OsStr,
    _tipo_esperado: TipoEntradaAnclada,
) -> Result<File, ErrorDirectorioAnclado> {
    Err(ErrorDirectorioAnclado::TipoNoAdmitido)
}

fn validar_tipo_abierto(
    handle: &File,
    tipo_esperado: TipoEntradaAnclada,
) -> Result<(), ErrorDirectorioAnclado> {
    let metadata = handle.metadata().map_err(mapear_io)?;
    if metadata.file_type().is_symlink() || metadata_es_reparse(&metadata) {
        return Err(ErrorDirectorioAnclado::EnlaceNoPermitido);
    }
    let tipo_correcto = match tipo_esperado {
        TipoEntradaAnclada::Archivo => metadata.is_file(),
        TipoEntradaAnclada::Directorio => metadata.is_dir(),
    };
    if !tipo_correcto {
        return Err(ErrorDirectorioAnclado::TipoNoAdmitido);
    }
    Ok(())
}

fn mapear_apertura(error: io::Error) -> ErrorDirectorioAnclado {
    #[cfg(unix)]
    if error.raw_os_error() == Some(libc::ELOOP) {
        return ErrorDirectorioAnclado::EnlaceNoPermitido;
    }
    #[cfg(windows)]
    if error.raw_os_error() == Some(1921) {
        return ErrorDirectorioAnclado::EnlaceNoPermitido;
    }
    mapear_io(error)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use tempfile::tempdir;

    #[test]
    fn openat_no_bloquea_si_el_nombre_es_fifo() {
        let flags_archivo = flags_unix(TipoEntradaAnclada::Archivo);
        assert_ne!(flags_archivo & libc::O_NONBLOCK, 0);
        assert_ne!(flags_archivo & libc::O_NOFOLLOW, 0);
        assert_ne!(flags_archivo & libc::O_NOCTTY, 0);
        assert_ne!(flags_archivo & libc::O_CLOEXEC, 0);
        assert_eq!(flags_archivo & libc::O_DIRECTORY, 0);
        let flags_directorio = flags_unix(TipoEntradaAnclada::Directorio);
        assert_ne!(flags_directorio & libc::O_DIRECTORY, 0);

        let temp = tempdir().unwrap();
        let fifo = temp.path().join("entrada");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: the nul-terminated path is valid for this synchronous call.
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        let parent = File::open(temp.path()).unwrap();

        let resultado =
            abrir_entrada_relativa(&parent, OsStr::new("entrada"), TipoEntradaAnclada::Archivo);
        assert_eq!(
            resultado.unwrap_err(),
            ErrorDirectorioAnclado::TipoNoAdmitido
        );
    }
}
