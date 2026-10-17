//! Lectura bounded de un contexto exterior v1/v2 bajo el lease del parent.

use std::fs::File;
use std::io::{Seek, SeekFrom};

use crate::transaccion::admision::envelope::selector;

use super::{
    ContextoAdmisionAnclado, ErrorContextoAdmisionAnclado, ErrorLecturaContextoVersionada,
    ObservacionContextoVersionada, TipoEntradaAnclada, abrir_entrada_relativa, identidad_archivo,
    metadata_directorio, validar_scope,
};

/// Lee un handle ya abierto por el caller privado; captura su identidad desde ese handle.
pub(super) fn leer_archivo_versionado(
    lease: &ContextoAdmisionAnclado<'_>,
    archivo: File,
) -> Result<ObservacionContextoVersionada, ErrorLecturaContextoVersionada> {
    leer_con_limite_y_hook(lease, archivo, selector::cuota_maxima_frame_bytes(), || {})
}

#[cfg(test)]
pub(super) fn leer_archivo_con_limite_y_hook<F>(
    lease: &ContextoAdmisionAnclado<'_>,
    archivo: File,
    cap: usize,
    despues_metadata: F,
) -> Result<ObservacionContextoVersionada, ErrorLecturaContextoVersionada>
where
    F: FnOnce(),
{
    leer_con_limite_y_hook(lease, archivo, cap, despues_metadata)
}

fn leer_con_limite_y_hook<F>(
    lease: &ContextoAdmisionAnclado<'_>,
    mut archivo: File,
    limite_solicitado: usize,
    despues_metadata: F,
) -> Result<ObservacionContextoVersionada, ErrorLecturaContextoVersionada>
where
    F: FnOnce(),
{
    lease
        .revalidar_parent_y_lock()
        .map_err(ErrorLecturaContextoVersionada::Lease)?;

    let limite = limite_solicitado.min(selector::cuota_maxima_frame_bytes());
    let limite_u64 =
        u64::try_from(limite).map_err(|_| ErrorLecturaContextoVersionada::LengthOverflow)?;
    validar_scope(&archivo, lease.scope_parent).map_err(ErrorLecturaContextoVersionada::Lease)?;
    let identidad = identidad_archivo(&archivo).map_err(|_| {
        ErrorLecturaContextoVersionada::Lease(ErrorContextoAdmisionAnclado::CanonicalUnverified)
    })?;
    let metadata = archivo
        .metadata()
        .map_err(|_| ErrorLecturaContextoVersionada::ReadIo)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || super::super::metadata_es_reparse(&metadata)
    {
        return Err(ErrorLecturaContextoVersionada::Lease(
            ErrorContextoAdmisionAnclado::EntryRejected,
        ));
    }
    if metadata.len() > limite_u64 {
        return Err(ErrorLecturaContextoVersionada::FrameTooLarge);
    }

    despues_metadata();
    seek_inicio(&mut archivo)?;
    let frame = super::recolector::reunir(&mut archivo, limite).map_err(mapear_recoleccion)?;
    let envelope =
        selector::decodificar(&frame.bytes).map_err(ErrorLecturaContextoVersionada::Selector)?;

    let parent = metadata_directorio(&lease.parent)
        .map_err(|_| {
            ErrorLecturaContextoVersionada::Lease(ErrorContextoAdmisionAnclado::ParentUnverified)
        })?
        .journal_identity()
        .ok_or(ErrorLecturaContextoVersionada::Lease(
            ErrorContextoAdmisionAnclado::ParentUnverified,
        ))?;
    if !envelope.coincide_parent(&parent) {
        return Err(ErrorLecturaContextoVersionada::ParentBindingMismatch);
    }

    lease
        .revalidar_parent_y_lock()
        .map_err(ErrorLecturaContextoVersionada::Lease)?;
    let probe = abrir_entrada_relativa(&lease.parent, &lease.nombre, TipoEntradaAnclada::Archivo)
        .map_err(mapear_apertura)?;
    validar_scope(&probe, lease.scope_parent).map_err(ErrorLecturaContextoVersionada::Lease)?;
    let identidad_visible = identidad_archivo(&probe).map_err(|_| {
        ErrorLecturaContextoVersionada::Lease(ErrorContextoAdmisionAnclado::CanonicalUnverified)
    })?;
    if identidad_visible != identidad {
        return Err(ErrorLecturaContextoVersionada::SourceIdentityMismatch);
    }
    drop(probe);

    Ok(ObservacionContextoVersionada {
        envelope,
        evidencia: super::EvidenciaLecturaContexto {
            identidad_archivo: identidad,
            longitud: frame.longitud,
            sha256: frame.sha256,
        },
        archivo_original: archivo,
    })
}

fn seek_inicio(archivo: &mut File) -> Result<(), ErrorLecturaContextoVersionada> {
    loop {
        match archivo.seek(SeekFrom::Start(0)) {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorLecturaContextoVersionada::ReadIo),
        }
    }
}

fn mapear_apertura(error: super::ErrorDirectorioAnclado) -> ErrorLecturaContextoVersionada {
    use super::ErrorDirectorioAnclado;

    match error {
        ErrorDirectorioAnclado::EntradaAusente => {
            ErrorLecturaContextoVersionada::Lease(ErrorContextoAdmisionAnclado::CanonicalUnverified)
        }
        ErrorDirectorioAnclado::EnlaceNoPermitido | ErrorDirectorioAnclado::TipoNoAdmitido => {
            ErrorLecturaContextoVersionada::Lease(ErrorContextoAdmisionAnclado::EntryRejected)
        }
        ErrorDirectorioAnclado::ComponenteInvalido => {
            ErrorLecturaContextoVersionada::Lease(ErrorContextoAdmisionAnclado::NameInvalid)
        }
        _ => ErrorLecturaContextoVersionada::ReadIo,
    }
}

fn mapear_recoleccion(
    error: super::recolector::ErrorRecoleccionFrame,
) -> ErrorLecturaContextoVersionada {
    use super::recolector::ErrorRecoleccionFrame;

    match error {
        ErrorRecoleccionFrame::TooLarge => ErrorLecturaContextoVersionada::FrameTooLarge,
        ErrorRecoleccionFrame::Overflow => ErrorLecturaContextoVersionada::LengthOverflow,
        ErrorRecoleccionFrame::Reserve => ErrorLecturaContextoVersionada::ReservationFailed,
        ErrorRecoleccionFrame::Io => ErrorLecturaContextoVersionada::ReadIo,
    }
}
