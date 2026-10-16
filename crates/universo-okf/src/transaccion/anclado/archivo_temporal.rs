//! Escritura exclusiva de un archivo sibling desde un parent ya abierto.
//!
//! Un fallo posterior a `create_new` deja el archivo presente; este helper no
//! elimina temporales ni conserva autoridad sobre los nombres.

use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use fs_at::{OpenOptions as OpenOptionsAt, OpenOptionsWriteMode};
use sha2::{Digest, Sha256};

use super::{IdentidadArchivo, ScopeFilesystem, identidad_archivo, metadata_directorio};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorArchivoTemporalAnclado {
    NameInvalid,
    EntryRejected,
    ParentUnverified,
    ScopeUnverified,
    Exists,
    Io,
}

impl ErrorArchivoTemporalAnclado {
    pub(super) const fn codigo(self) -> &'static str {
        match self {
            Self::NameInvalid => "anchored_temporal_name_invalid",
            Self::EntryRejected => "anchored_temporal_entry_rejected",
            Self::ParentUnverified => "anchored_temporal_parent_unverified",
            Self::ScopeUnverified => "anchored_temporal_scope_unverified",
            Self::Exists => "anchored_temporal_exists",
            Self::Io => "anchored_temporal_io",
        }
    }
}

impl fmt::Debug for ErrorArchivoTemporalAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorArchivoTemporalAnclado")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorArchivoTemporalAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorArchivoTemporalAnclado {}

/// Datos del mismo handle y bytes, sin autoridad de lifecycle.
pub(super) struct DatosArchivoTemporal {
    pub(super) identidad_archivo: IdentidadArchivo,
    pub(super) longitud: usize,
    pub(super) digest: [u8; 32],
}

impl fmt::Debug for DatosArchivoTemporal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatosArchivoTemporal { .. }")
    }
}

pub(super) fn crear(
    parent: &File,
    nombre: &OsStr,
    bytes: &[u8],
) -> Result<DatosArchivoTemporal, ErrorArchivoTemporalAnclado> {
    crear_con(
        parent,
        nombre,
        bytes,
        |archivo, bytes| {
            archivo
                .write_all(bytes)
                .map_err(|_| ErrorArchivoTemporalAnclado::Io)
        },
        |archivo| {
            archivo
                .sync_all()
                .map_err(|_| ErrorArchivoTemporalAnclado::Io)
        },
    )
}

pub(super) fn crear_con<W, S>(
    parent: &File,
    nombre: &OsStr,
    bytes: &[u8],
    escribir: W,
    sincronizar: S,
) -> Result<DatosArchivoTemporal, ErrorArchivoTemporalAnclado>
where
    W: FnOnce(&mut File, &[u8]) -> Result<(), ErrorArchivoTemporalAnclado>,
    S: FnOnce(&File) -> Result<(), ErrorArchivoTemporalAnclado>,
{
    super::validar_componente(nombre).map_err(|_| ErrorArchivoTemporalAnclado::NameInvalid)?;
    let identidad_parent =
        metadata_directorio(parent).map_err(|_| ErrorArchivoTemporalAnclado::ParentUnverified)?;
    let scope_parent = capturar_scope(parent)?;
    let longitud = bytes.len();
    let digest: [u8; 32] = Sha256::digest(bytes).into();

    let mut archivo = abrir_temporal(parent, nombre)?;
    validar_archivo_regular(&archivo)?;
    let identidad_archivo =
        identidad_archivo(&archivo).map_err(|_| ErrorArchivoTemporalAnclado::Io)?;
    revalidar_parent(parent, identidad_parent, scope_parent)?;
    validar_scope(&archivo, scope_parent)?;

    escribir(&mut archivo, bytes)?;
    sincronizar(&archivo)?;
    drop(archivo);

    Ok(DatosArchivoTemporal {
        identidad_archivo,
        longitud,
        digest,
    })
}

fn capturar_scope(parent: &File) -> Result<ScopeFilesystem, ErrorArchivoTemporalAnclado> {
    super::scope_filesystem(parent).map_err(|_| ErrorArchivoTemporalAnclado::ScopeUnverified)
}

fn revalidar_parent(
    parent: &File,
    identidad_esperada: super::IdentidadDirectorio,
    scope_esperado: ScopeFilesystem,
) -> Result<(), ErrorArchivoTemporalAnclado> {
    let identidad =
        metadata_directorio(parent).map_err(|_| ErrorArchivoTemporalAnclado::ParentUnverified)?;
    if identidad != identidad_esperada {
        return Err(ErrorArchivoTemporalAnclado::ParentUnverified);
    }
    validar_scope(parent, scope_esperado)
}

fn validar_scope(
    archivo: &File,
    esperado: ScopeFilesystem,
) -> Result<(), ErrorArchivoTemporalAnclado> {
    let observado = super::scope_filesystem(archivo)
        .map_err(|_| ErrorArchivoTemporalAnclado::ScopeUnverified)?;
    if observado == esperado {
        Ok(())
    } else {
        Err(ErrorArchivoTemporalAnclado::ScopeUnverified)
    }
}

fn abrir_temporal(parent: &File, nombre: &OsStr) -> Result<File, ErrorArchivoTemporalAnclado> {
    let mut opciones = OpenOptionsAt::default();
    opciones
        .write(OpenOptionsWriteMode::Write)
        .create_new(true)
        .follow(false);
    #[cfg(unix)]
    {
        use fs_at::os::unix::OpenOptionsExt;
        opciones.mode(0o600);
    }
    #[cfg(windows)]
    {
        use fs_at::os::windows::OpenOptionsExt;
        opciones.desired_access(acceso_temporal_creacion());
    }
    opciones
        .open_at(parent, Path::new(nombre))
        .map_err(mapear_error_io)
}

fn validar_archivo_regular(archivo: &File) -> Result<(), ErrorArchivoTemporalAnclado> {
    let metadata = archivo
        .metadata()
        .map_err(|_| ErrorArchivoTemporalAnclado::Io)?;
    if metadata.file_type().is_symlink()
        || super::metadata_es_reparse(&metadata)
        || !metadata.is_file()
    {
        return Err(ErrorArchivoTemporalAnclado::EntryRejected);
    }
    Ok(())
}

fn mapear_error_io(error: io::Error) -> ErrorArchivoTemporalAnclado {
    if error.kind() == io::ErrorKind::AlreadyExists {
        ErrorArchivoTemporalAnclado::Exists
    } else {
        ErrorArchivoTemporalAnclado::Io
    }
}

#[cfg(windows)]
fn acceso_temporal_creacion() -> u32 {
    use windows_sys::Win32::Foundation::GENERIC_WRITE;
    use windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;

    FILE_READ_ATTRIBUTES | GENERIC_WRITE
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "archivo_temporal_tests.rs"]
mod tests;
