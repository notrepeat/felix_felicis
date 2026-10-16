//! Reapertura y verificación de un temporal ya creado, sin publicarlo.

use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use sha2::{Digest, Sha256};

use super::super::archivo_temporal::DatosArchivoTemporal;
use super::{ContextoAdmisionAnclado, ErrorContextoAdmisionAnclado};

/// Handle verificado para una publicación futura; no concede recovery ni cleanup.
#[must_use = "el handle queda anclado al lease y no publica el archivo por sí solo"]
pub(crate) struct ContextoTemporalVerificado<'lock> {
    pub(super) _lease: ContextoAdmisionAnclado<'lock>,
    pub(super) archivo: File,
    pub(super) nombre_temporal: std::ffi::OsString,
    pub(super) datos: DatosArchivoTemporal,
}

impl fmt::Debug for ContextoTemporalVerificado<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ContextoTemporalVerificado { .. }")
    }
}

pub(super) fn verificar_archivo(
    lease: &ContextoAdmisionAnclado<'_>,
    nombre: &OsStr,
    datos: &DatosArchivoTemporal,
) -> Result<File, ErrorContextoAdmisionAnclado> {
    let mut archivo = abrir_temporal(&lease.parent, nombre)?;
    validar_archivo_regular(&archivo)?;
    super::validar_scope(&archivo, lease.scope_parent)?;
    let identidad = super::identidad_archivo(&archivo)
        .map_err(|_| ErrorContextoAdmisionAnclado::TemporaryUnverified)?;
    if identidad != datos.identidad_archivo {
        return Err(ErrorContextoAdmisionAnclado::TemporaryUnverified);
    }
    verificar_contenido(&mut archivo, datos)?;
    Ok(archivo)
}

#[cfg(windows)]
fn abrir_temporal(parent: &File, nombre: &OsStr) -> Result<File, ErrorContextoAdmisionAnclado> {
    use fs_at::OpenOptions as OpenOptionsAt;
    use fs_at::os::windows::OpenOptionsExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_READ_ATTRIBUTES, FILE_READ_DATA};

    let mut opciones = OpenOptionsAt::default();
    opciones
        .desired_access(FILE_READ_DATA | FILE_READ_ATTRIBUTES | DELETE)
        .follow(false);
    opciones
        .open_at(parent, Path::new(nombre))
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ErrorContextoAdmisionAnclado::TemporaryUnverified
            } else {
                ErrorContextoAdmisionAnclado::Io
            }
        })
}

#[cfg(not(windows))]
fn abrir_temporal(parent: &File, nombre: &OsStr) -> Result<File, ErrorContextoAdmisionAnclado> {
    super::abrir_entrada_relativa(parent, nombre, super::TipoEntradaAnclada::Archivo).map_err(
        |error| match error {
            super::ErrorDirectorioAnclado::EntradaAusente => {
                ErrorContextoAdmisionAnclado::TemporaryUnverified
            }
            super::ErrorDirectorioAnclado::EnlaceNoPermitido
            | super::ErrorDirectorioAnclado::TipoNoAdmitido => {
                ErrorContextoAdmisionAnclado::EntryRejected
            }
            super::ErrorDirectorioAnclado::ComponenteInvalido => {
                ErrorContextoAdmisionAnclado::NameInvalid
            }
            _ => ErrorContextoAdmisionAnclado::Io,
        },
    )
}

fn validar_archivo_regular(archivo: &File) -> Result<(), ErrorContextoAdmisionAnclado> {
    let metadata = archivo
        .metadata()
        .map_err(|_| ErrorContextoAdmisionAnclado::Io)?;
    if metadata.file_type().is_symlink()
        || super::super::metadata_es_reparse(&metadata)
        || !metadata.is_file()
    {
        return Err(ErrorContextoAdmisionAnclado::EntryRejected);
    }
    Ok(())
}

fn verificar_contenido(
    archivo: &mut File,
    datos: &DatosArchivoTemporal,
) -> Result<(), ErrorContextoAdmisionAnclado> {
    let metadata = archivo
        .metadata()
        .map_err(|_| ErrorContextoAdmisionAnclado::Io)?;
    let longitud = u64::try_from(datos.longitud)
        .map_err(|_| ErrorContextoAdmisionAnclado::TemporaryUnverified)?;
    if metadata.len() != longitud {
        return Err(ErrorContextoAdmisionAnclado::TemporaryUnverified);
    }
    loop {
        match archivo.seek(SeekFrom::Start(0)) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorContextoAdmisionAnclado::Io),
        }
    }

    let mut digest = Sha256::new();
    let mut restante = datos.longitud;
    let mut buffer = [0u8; 8192];
    while restante > 0 {
        let por_leer = restante.min(buffer.len());
        match archivo.read(&mut buffer[..por_leer]) {
            Ok(0) => return Err(ErrorContextoAdmisionAnclado::TemporaryUnverified),
            Ok(leidos) => {
                digest.update(&buffer[..leidos]);
                restante -= leidos;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorContextoAdmisionAnclado::Io),
        }
    }

    loop {
        let mut extra = [0u8; 1];
        match archivo.read(&mut extra) {
            Ok(0) => break,
            Ok(_) => return Err(ErrorContextoAdmisionAnclado::TemporaryUnverified),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorContextoAdmisionAnclado::Io),
        }
    }

    let digest_observado: [u8; 32] = digest.finalize().into();
    if digest_observado != datos.digest {
        return Err(ErrorContextoAdmisionAnclado::TemporaryUnverified);
    }
    Ok(())
}
