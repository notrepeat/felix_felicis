//! Creación exclusiva del sibling temporal, sin publicación ni cleanup.

use std::ffi::{OsStr, OsString};
use std::fmt;

use crate::transaccion::admision::envelope::v2::{
    EnvelopeContextoAdmisionV2, ErrorCodecEnvelopeContextoAdmisionV2,
};
use crate::transaccion::admision::envelope::{
    EnvelopeContextoAdmision, codec::ErrorCodecEnvelopeAdmision,
};

use super::super::archivo_temporal::{self, DatosArchivoTemporal, ErrorArchivoTemporalAnclado};
use super::super::{TipoEntradaAnclada, journal_temporal};
use super::{
    ContextoAdmisionAnclado, ErrorContextoAdmisionAnclado, abrir_entrada_relativa,
    identidad_archivo, metadata_directorio, validar_componente, validar_scope,
};

/// Recibo opaco que retiene el lease y los datos del temporal sincronizado.
///
/// No autentica el envelope, no habilita recovery y no concede cleanup. Si una
/// escritura falla después de crear el sibling, este tipo no se devuelve y el
/// archivo permanece para bloquear una escritura exclusiva posterior.
pub(crate) struct ReciboContextoTemporal<'lock> {
    pub(super) _lease: ContextoAdmisionAnclado<'lock>,
    pub(super) datos: DatosArchivoTemporal,
}

impl fmt::Debug for ReciboContextoTemporal<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ReciboContextoTemporal { .. }")
    }
}

impl<'lock> ReciboContextoTemporal<'lock> {
    /// Reabre y verifica el mismo temporal bajo el lock, sin publicarlo.
    ///
    /// El handle resultante conserva el lease y los derechos necesarios para
    /// una publicación futura; un error nunca borra ni renombra el sibling.
    pub(crate) fn verificar(
        self,
    ) -> Result<super::verificacion::ContextoTemporalVerificado<'lock>, ErrorContextoAdmisionAnclado>
    {
        self._lease.revalidar_parent_y_lock()?;
        let nombre_temporal = nombre_temporal(&self._lease.nombre)?;
        let archivo =
            super::verificacion::verificar_archivo(&self._lease, &nombre_temporal, &self.datos)?;
        Ok(super::verificacion::ContextoTemporalVerificado {
            _lease: self._lease,
            archivo,
            nombre_temporal,
            datos: self.datos,
        })
    }
}

impl<'lock> ContextoAdmisionAnclado<'lock> {
    /// Codifica y sincroniza un temporal exclusivo bajo el parent retenido.
    ///
    /// El resultado es solo un recibo de storage. No publica el canonical ni
    /// ordena la escritura del journal, los renames o el cleanup.
    pub(crate) fn crear_temporal(
        self,
        envelope: &EnvelopeContextoAdmision,
    ) -> Result<ReciboContextoTemporal<'lock>, ErrorContextoAdmisionAnclado> {
        self.crear_temporal_con(
            envelope,
            crate::transaccion::admision::envelope::codec::codificar,
        )
    }

    pub(super) fn crear_temporal_con<C>(
        self,
        envelope: &EnvelopeContextoAdmision,
        codificar: C,
    ) -> Result<ReciboContextoTemporal<'lock>, ErrorContextoAdmisionAnclado>
    where
        C: FnOnce(&EnvelopeContextoAdmision) -> Result<Vec<u8>, ErrorCodecEnvelopeAdmision>,
    {
        let bytes = codificar(envelope).map_err(ErrorContextoAdmisionAnclado::Codec)?;
        self.crear_temporal_desde_bytes(bytes)
    }

    /// Codifica y sincroniza un temporal exclusivo V2 bajo el parent retenido.
    ///
    /// Es un recibo de storage, no publica el canonical ni habilita recovery.
    pub(in crate::transaccion::anclado) fn crear_temporal_v2(
        self,
        envelope: &EnvelopeContextoAdmisionV2,
    ) -> Result<ReciboContextoTemporal<'lock>, ErrorContextoAdmisionAnclado> {
        self.crear_temporal_v2_con(envelope, EnvelopeContextoAdmisionV2::encode)
    }

    pub(super) fn crear_temporal_v2_con<C>(
        self,
        envelope: &EnvelopeContextoAdmisionV2,
        codificar: C,
    ) -> Result<ReciboContextoTemporal<'lock>, ErrorContextoAdmisionAnclado>
    where
        C: FnOnce(
            &EnvelopeContextoAdmisionV2,
        ) -> Result<Vec<u8>, ErrorCodecEnvelopeContextoAdmisionV2>,
    {
        let bytes = codificar(envelope).map_err(ErrorContextoAdmisionAnclado::CodecV2)?;
        self.crear_temporal_desde_bytes(bytes)
    }

    fn crear_temporal_desde_bytes(
        self,
        bytes: Vec<u8>,
    ) -> Result<ReciboContextoTemporal<'lock>, ErrorContextoAdmisionAnclado> {
        if journal_temporal::raiz_es_remota(self._lock.raiz_canonica()) {
            return Err(ErrorContextoAdmisionAnclado::RemoteUnsupported);
        }

        let nombre = nombre_temporal(&self.nombre)?;
        self.revalidar_parent_y_lock()?;
        let datos = archivo_temporal::crear(&self.parent, &nombre, &bytes)
            .map_err(mapear_error_temporal)?;

        Ok(ReciboContextoTemporal {
            _lease: self,
            datos,
        })
    }

    pub(super) fn revalidar_parent_y_lock(&self) -> Result<(), ErrorContextoAdmisionAnclado> {
        let identidad = metadata_directorio(&self.parent)
            .map_err(|_| ErrorContextoAdmisionAnclado::ParentUnverified)?;
        if identidad != self.identidad_parent {
            return Err(ErrorContextoAdmisionAnclado::ParentUnverified);
        }
        validar_scope(&self.parent, self.scope_parent)?;

        let ruta_lock = self._lock.ubicacion().lock();
        let nombre_lock = ruta_lock
            .as_path()
            .file_name()
            .ok_or(ErrorContextoAdmisionAnclado::OwnerUnverified)?
            .to_os_string();
        validar_componente(&nombre_lock)
            .map_err(|_| ErrorContextoAdmisionAnclado::OwnerUnverified)?;
        let visible =
            abrir_entrada_relativa(&self.parent, &nombre_lock, TipoEntradaAnclada::Archivo)
                .map_err(|_| ErrorContextoAdmisionAnclado::OwnerUnverified)?;
        let identidad_visible = identidad_archivo(&visible)
            .map_err(|_| ErrorContextoAdmisionAnclado::OwnerUnverified)?;
        let identidad_guard = self
            ._lock
            .identidad_archivo_lock()
            .map_err(|_| ErrorContextoAdmisionAnclado::OwnerUnverified)?;
        if identidad_visible != identidad_guard {
            return Err(ErrorContextoAdmisionAnclado::OwnerUnverified);
        }
        Ok(())
    }
}

pub(super) fn nombre_temporal(
    nombre_canonico: &OsStr,
) -> Result<OsString, ErrorContextoAdmisionAnclado> {
    let mut nombre = nombre_canonico.to_os_string();
    nombre.push(".tmp");
    validar_componente(&nombre).map_err(|_| ErrorContextoAdmisionAnclado::NameInvalid)?;
    Ok(nombre)
}

fn mapear_error_temporal(error: ErrorArchivoTemporalAnclado) -> ErrorContextoAdmisionAnclado {
    match error {
        ErrorArchivoTemporalAnclado::NameInvalid => ErrorContextoAdmisionAnclado::NameInvalid,
        ErrorArchivoTemporalAnclado::EntryRejected => ErrorContextoAdmisionAnclado::EntryRejected,
        ErrorArchivoTemporalAnclado::ParentUnverified => {
            ErrorContextoAdmisionAnclado::ParentUnverified
        }
        ErrorArchivoTemporalAnclado::ScopeUnverified => {
            ErrorContextoAdmisionAnclado::ScopeUnverified
        }
        ErrorArchivoTemporalAnclado::Exists => ErrorContextoAdmisionAnclado::TemporaryExists,
        ErrorArchivoTemporalAnclado::Io => ErrorContextoAdmisionAnclado::Io,
    }
}
