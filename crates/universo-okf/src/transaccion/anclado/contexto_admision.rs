//! Lectura canónica del contexto desde el parent que retiene el dueño.
//! No contrasta el binding con un journal observado ni reproduce admisión.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::sync::Arc;

use crate::transaccion::admision::envelope::{
    EnvelopeContextoAdmision, codec::ErrorCodecEnvelopeAdmision,
};
use crate::transaccion::lock::LockEscritura;

use super::abrir::abrir_entrada_relativa;
use super::{
    ErrorDirectorioAnclado, IdentidadArchivo, IdentidadDirectorio, ScopeFilesystem,
    TipoEntradaAnclada, identidad_archivo, metadata_directorio, scope_filesystem,
    validar_componente,
};

/// Error fijo y redactado al leer el contexto canónico bajo el lock.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorContextoAdmisionAnclado {
    OwnerUnverified,
    ParentUnverified,
    ScopeUnverified,
    NameInvalid,
    EntryRejected,
    CanonicalUnverified,
    TemporaryExists,
    TemporaryUnverified,
    PublicationUnverified,
    RemovalUnverified,
    RemoteUnsupported,
    Io,
    Codec(ErrorCodecEnvelopeAdmision),
}

impl ErrorContextoAdmisionAnclado {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::OwnerUnverified => "admission_context_owner_unverified",
            Self::ParentUnverified => "admission_context_parent_unverified",
            Self::ScopeUnverified => "admission_context_scope_unverified",
            Self::NameInvalid => "admission_context_name_invalid",
            Self::EntryRejected => "admission_context_entry_rejected",
            Self::CanonicalUnverified => "admission_context_canonical_unverified",
            Self::TemporaryExists => "admission_context_temporary_exists",
            Self::TemporaryUnverified => "admission_context_temporary_unverified",
            Self::PublicationUnverified => "admission_context_publication_unverified",
            Self::RemovalUnverified => "admission_context_removal_unverified",
            Self::RemoteUnsupported => "admission_context_remote_unsupported",
            Self::Io => "admission_context_io",
            Self::Codec(error) => error.codigo(),
        }
    }
}

impl fmt::Debug for ErrorContextoAdmisionAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorContextoAdmisionAnclado")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorContextoAdmisionAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorContextoAdmisionAnclado {}

#[must_use]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ResultadoEliminacionContextoAnclado {
    Ausente,
    Eliminado,
    Conflicto(ErrorContextoAdmisionAnclado),
    Indeterminado(ErrorContextoAdmisionAnclado),
}

impl fmt::Debug for ResultadoEliminacionContextoAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ausente => formatter.write_str("Ausente"),
            Self::Eliminado => formatter.write_str("Eliminado"),
            Self::Conflicto(error) => formatter.debug_tuple("Conflicto").field(error).finish(),
            Self::Indeterminado(error) => {
                formatter.debug_tuple("Indeterminado").field(error).finish()
            }
        }
    }
}

pub(crate) type RegistroContextoAdmisionLeido = (EnvelopeContextoAdmision, IdentidadArchivo);

/// Lease de lectura del sibling canónico; no concede autoridad de recovery.
///
/// Conserva el lock y el parent abierto durante una única lectura secuencial.
/// La exclusión solo cubre writers Felix cooperantes, no hardlinks ni carreras
/// de procesos externos; el scope no demuestra una frontera general de mounts.
pub(crate) struct ContextoAdmisionAnclado<'lock> {
    _lock: &'lock mut LockEscritura,
    parent: File,
    identidad_parent: IdentidadDirectorio,
    scope_parent: ScopeFilesystem,
    nombre: OsString,
    _autoridad: Arc<()>,
}

#[path = "contexto_admision/eliminar.rs"]
mod eliminar;
#[path = "contexto_admision/evidencia.rs"]
mod evidencia;
#[path = "contexto_admision/publicar.rs"]
mod publicar;
pub(super) use publicar::renombrar_no_reemplazar;
#[path = "contexto_admision/temporal.rs"]
mod temporal;
#[path = "contexto_admision/verificacion.rs"]
mod verificacion;

pub(crate) use evidencia::EvidenciaLecturaContexto;

impl ContextoAdmisionAnclado<'_> {
    /// Devuelve ausencia observada o el frame y la identidad real del archivo.
    /// No contrasta su binding con el journal observado ni reproduce admisión.
    pub(crate) fn leer_canonico(
        &self,
    ) -> Result<Option<RegistroContextoAdmisionLeido>, ErrorContextoAdmisionAnclado> {
        let Some((mut archivo, identidad)) = self.abrir_canonico()? else {
            return Ok(None);
        };
        let envelope = crate::transaccion::admision::envelope::codec::leer(&mut archivo)
            .map_err(ErrorContextoAdmisionAnclado::Codec)?;
        Ok(Some((envelope, identidad)))
    }

    /// Lee el mismo frame acotado y devuelve evidencia de sus bytes observados.
    /// El digest y la identidad son datos; no autentican ni conceden autoridad.
    pub(crate) fn leer_canonico_con_evidencia(
        &self,
    ) -> Result<
        Option<(
            EnvelopeContextoAdmision,
            evidencia::EvidenciaLecturaContexto,
        )>,
        ErrorContextoAdmisionAnclado,
    > {
        let Some((mut archivo, identidad)) = self.abrir_canonico()? else {
            return Ok(None);
        };
        evidencia::leer(&mut archivo, identidad)
            .map(Some)
            .map_err(ErrorContextoAdmisionAnclado::Codec)
    }

    /// Reabre y contrasta el frame raw con evidencia medida antes.
    /// Revalida parent y lock antes y después; no decodifica ni concede cleanup.
    #[allow(dead_code)] // El consumidor futuro de Context aún no está integrado.
    pub(super) fn verificar_canonico_con_evidencia(
        &self,
        evidencia: &EvidenciaLecturaContexto,
    ) -> Result<File, ErrorContextoAdmisionAnclado> {
        self.revalidar_parent_y_lock()?;
        let archivo = verificacion::verificar_canonico(self, evidencia)?;
        self.revalidar_parent_y_lock()?;
        Ok(archivo)
    }

    fn abrir_canonico(
        &self,
    ) -> Result<Option<(File, IdentidadArchivo)>, ErrorContextoAdmisionAnclado> {
        let identidad_parent = metadata_directorio(&self.parent)
            .map_err(|_| ErrorContextoAdmisionAnclado::ParentUnverified)?;
        if identidad_parent != self.identidad_parent {
            return Err(ErrorContextoAdmisionAnclado::ParentUnverified);
        }
        validar_scope(&self.parent, self.scope_parent)?;

        let archivo =
            match abrir_entrada_relativa(&self.parent, &self.nombre, TipoEntradaAnclada::Archivo) {
                Ok(archivo) => archivo,
                Err(ErrorDirectorioAnclado::EntradaAusente) => return Ok(None),
                Err(error) => return Err(mapear_error(error)),
            };
        validar_scope(&archivo, self.scope_parent)?;
        let identidad =
            identidad_archivo(&archivo).map_err(|_| ErrorContextoAdmisionAnclado::Io)?;
        Ok(Some((archivo, identidad)))
    }
}

impl fmt::Debug for ContextoAdmisionAnclado<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ContextoAdmisionAnclado { .. }")
    }
}

pub(super) fn desde_parent_retenido<'lock>(
    lock: &'lock mut LockEscritura,
    parent: File,
    identidad_parent: IdentidadDirectorio,
    scope_parent: ScopeFilesystem,
    autoridad: Arc<()>,
) -> Result<ContextoAdmisionAnclado<'lock>, ErrorContextoAdmisionAnclado> {
    let nombre = nombre_canonico(
        lock.ubicacion()
            .ruta()
            .file_name()
            .ok_or(ErrorContextoAdmisionAnclado::NameInvalid)?,
    )?;
    Ok(ContextoAdmisionAnclado {
        _lock: lock,
        parent,
        identidad_parent,
        scope_parent,
        nombre,
        _autoridad: autoridad,
    })
}

pub(super) fn nombre_canonico(basename: &OsStr) -> Result<OsString, ErrorContextoAdmisionAnclado> {
    validar_componente(basename).map_err(|_| ErrorContextoAdmisionAnclado::NameInvalid)?;
    let mut nombre = OsString::from(".");
    nombre.push(basename);
    nombre.push(".felix.admctx");
    validar_componente(&nombre).map_err(|_| ErrorContextoAdmisionAnclado::NameInvalid)?;
    Ok(nombre)
}

pub(super) fn nombre_temporal_sibling(
    nombre_canonico: &OsStr,
) -> Result<OsString, ErrorContextoAdmisionAnclado> {
    temporal::nombre_temporal(nombre_canonico)
}

fn validar_scope(
    handle: &File,
    esperado: ScopeFilesystem,
) -> Result<(), ErrorContextoAdmisionAnclado> {
    let observado =
        scope_filesystem(handle).map_err(|_| ErrorContextoAdmisionAnclado::ScopeUnverified)?;
    if observado != esperado {
        return Err(ErrorContextoAdmisionAnclado::ScopeUnverified);
    }
    Ok(())
}

fn mapear_error(error: ErrorDirectorioAnclado) -> ErrorContextoAdmisionAnclado {
    match error {
        ErrorDirectorioAnclado::EnlaceNoPermitido | ErrorDirectorioAnclado::TipoNoAdmitido => {
            ErrorContextoAdmisionAnclado::EntryRejected
        }
        ErrorDirectorioAnclado::ComponenteInvalido => ErrorContextoAdmisionAnclado::NameInvalid,
        _ => ErrorContextoAdmisionAnclado::Io,
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
pub(crate) fn publicar_temporal_para_fixture(
    verificado: verificacion::ContextoTemporalVerificado<'_>,
) -> Result<(), ErrorContextoAdmisionAnclado> {
    verificado.publicar().map(|_| ())
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "contexto_admision_tests.rs"]
mod tests;
