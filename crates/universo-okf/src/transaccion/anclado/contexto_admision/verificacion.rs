//! Reapertura y verificación de un temporal ya creado, sin publicarlo.

use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use sha2::{Digest, Sha256};

use crate::transaccion::admision::envelope::codec::MAX_ENVELOPE_FRAME_BYTES;

use super::super::archivo_temporal::DatosArchivoTemporal;
use super::{ContextoAdmisionAnclado, ErrorContextoAdmisionAnclado, EvidenciaLecturaContexto};

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
    let mut archivo = abrir_sibling(
        &lease.parent,
        nombre,
        ErrorContextoAdmisionAnclado::TemporaryUnverified,
    )?;
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

/// Contrasta el canónico raw en el mismo handle sin decodificarlo.
pub(super) fn verificar_canonico(
    lease: &ContextoAdmisionAnclado<'_>,
    evidencia: &EvidenciaLecturaContexto,
) -> Result<File, ErrorContextoAdmisionAnclado> {
    let mut archivo = abrir_sibling(
        &lease.parent,
        &lease.nombre,
        ErrorContextoAdmisionAnclado::CanonicalUnverified,
    )?;
    validar_archivo_regular(&archivo)?;
    super::validar_scope(&archivo, lease.scope_parent)?;
    let identidad =
        super::identidad_archivo(&archivo).map_err(|_| ErrorContextoAdmisionAnclado::Io)?;
    if identidad != evidencia.identidad_archivo {
        return Err(ErrorContextoAdmisionAnclado::CanonicalUnverified);
    }
    verificar_evidencia_raw(&mut archivo, evidencia)?;
    Ok(archivo)
}

#[cfg(windows)]
fn abrir_sibling(
    parent: &File,
    nombre: &OsStr,
    ausencia: ErrorContextoAdmisionAnclado,
) -> Result<File, ErrorContextoAdmisionAnclado> {
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
                ausencia
            } else {
                ErrorContextoAdmisionAnclado::Io
            }
        })
}

#[cfg(not(windows))]
fn abrir_sibling(
    parent: &File,
    nombre: &OsStr,
    ausencia: ErrorContextoAdmisionAnclado,
) -> Result<File, ErrorContextoAdmisionAnclado> {
    super::abrir_entrada_relativa(parent, nombre, super::TipoEntradaAnclada::Archivo).map_err(
        |error| match error {
            super::ErrorDirectorioAnclado::EntradaAusente => ausencia,
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

fn verificar_evidencia_raw(
    archivo: &mut File,
    evidencia: &EvidenciaLecturaContexto,
) -> Result<(), ErrorContextoAdmisionAnclado> {
    let metadata = archivo
        .metadata()
        .map_err(|_| ErrorContextoAdmisionAnclado::Io)?;
    validar_longitud(metadata.len(), evidencia.longitud)?;
    loop {
        match archivo.seek(SeekFrom::Start(0)) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorContextoAdmisionAnclado::Io),
        }
    }

    let mut digest = Sha256::new();
    let mut restante = evidencia.longitud;
    let mut buffer = [0u8; 8192];
    let tamano_buffer =
        u64::try_from(buffer.len()).map_err(|_| ErrorContextoAdmisionAnclado::Io)?;
    while restante > 0 {
        let por_leer = usize::try_from(restante.min(tamano_buffer))
            .map_err(|_| ErrorContextoAdmisionAnclado::Io)?;
        match archivo.read(&mut buffer[..por_leer]) {
            Ok(0) => return Err(ErrorContextoAdmisionAnclado::CanonicalUnverified),
            Ok(leidos) => {
                digest.update(&buffer[..leidos]);
                let leidos = u64::try_from(leidos).map_err(|_| ErrorContextoAdmisionAnclado::Io)?;
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
            Ok(_) => return Err(ErrorContextoAdmisionAnclado::CanonicalUnverified),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorContextoAdmisionAnclado::Io),
        }
    }

    let digest_observado: [u8; 32] = digest.finalize().into();
    if digest_observado != evidencia.sha256 {
        return Err(ErrorContextoAdmisionAnclado::CanonicalUnverified);
    }
    Ok(())
}

fn validar_longitud(
    longitud_archivo: u64,
    longitud_evidencia: u64,
) -> Result<(), ErrorContextoAdmisionAnclado> {
    let limite =
        u64::try_from(MAX_ENVELOPE_FRAME_BYTES).map_err(|_| ErrorContextoAdmisionAnclado::Io)?;
    if longitud_archivo > limite || longitud_archivo != longitud_evidencia {
        return Err(ErrorContextoAdmisionAnclado::CanonicalUnverified);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limite_del_frame_se_valida_sin_reservar_un_buffer_de_payload() {
        let limite = u64::try_from(MAX_ENVELOPE_FRAME_BYTES).unwrap();
        assert_eq!(validar_longitud(limite, limite), Ok(()));
        assert_eq!(
            validar_longitud(limite + 1, limite + 1),
            Err(ErrorContextoAdmisionAnclado::CanonicalUnverified)
        );
        assert_eq!(
            validar_longitud(8, 9),
            Err(ErrorContextoAdmisionAnclado::CanonicalUnverified)
        );
    }
}
