//! Renombre de directorios sibling dentro del parent transaccional anclado.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::marker::PhantomData;

use super::{DirectorioAnclado, ErrorDirectorioAnclado, IdentidadDirectorio, metadata_directorio};
use crate::transaccion::lock::LockEscritura;

/// Error estable y redactado del renombre sibling.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorRenombrarAnclado {
    Io,
    ComponenteInvalido,
    EnlaceNoPermitido,
    TipoNoAdmitido,
    FuenteAusente,
    DestinoExistente,
    IdentidadNoCoincide,
    FuenteNoHermana,
}

impl ErrorRenombrarAnclado {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::Io => "anchored_rename_io",
            Self::ComponenteInvalido => "anchored_rename_invalid_component",
            Self::EnlaceNoPermitido => "anchored_rename_reparse_not_allowed",
            Self::TipoNoAdmitido => "anchored_rename_directory_required",
            Self::FuenteAusente => "anchored_rename_source_missing",
            Self::DestinoExistente => "anchored_rename_destination_exists",
            Self::IdentidadNoCoincide => "anchored_rename_identity_mismatch",
            Self::FuenteNoHermana => "anchored_rename_source_not_sibling",
        }
    }
}

impl fmt::Debug for ErrorRenombrarAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorRenombrarAnclado")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorRenombrarAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorRenombrarAnclado {}

/// Única capability que puede observar y renombrar siblings transaccionales.
///
/// El borrow mutable de `LockEscritura` serializa las operaciones entre
/// escritores cooperantes. No excluye ni promete seguridad frente a procesos
/// externos que muten el filesystem durante la validación y el rename.
pub(crate) struct EspacioTransaccionalAnclado<'lock> {
    _lock: &'lock mut LockEscritura,
    parent: File,
    identidad_parent: IdentidadDirectorio,
}

/// Objeto sibling observado desde una capability transaccional.
///
/// No se puede construir desde una raíz o child genérico ni clonar. El nombre
/// solo cambia mediante `renombrar_directorio`, así que nunca queda expuesto
/// como una ancla al basename viejo después de un éxito.
pub(crate) struct DirectorioSiblingAnclado<'lock> {
    nombre: OsString,
    raiz: File,
    identidad_raiz: IdentidadDirectorio,
    _lock: PhantomData<&'lock LockEscritura>,
}

impl DirectorioSiblingAnclado<'_> {
    pub(crate) fn nombre(&self) -> &OsStr {
        &self.nombre
    }

    pub(crate) fn identidad(&self) -> IdentidadDirectorio {
        self.identidad_raiz
    }
}

impl<'lock> EspacioTransaccionalAnclado<'lock> {
    pub(crate) fn desde_lock(
        lock: &'lock mut LockEscritura,
    ) -> Result<EspacioTransaccionalAnclado<'lock>, ErrorRenombrarAnclado> {
        let ancla = DirectorioAnclado::desde_ubicacion(lock.ubicacion())
            .map_err(mapear_error_directorio)?;
        let identidad_parent = ancla.identidad_parent;
        Ok(EspacioTransaccionalAnclado {
            _lock: lock,
            parent: ancla.parent,
            identidad_parent,
        })
    }

    /// Abre un directorio sibling por nombre relativo al parent retenido y
    /// compara su identidad con la identidad esperada antes de acuñar una
    /// capability. Esa identidad debe venir de una capability, recibo o
    /// journal validado de esta transacción; no se debe obtener enumerando el
    /// sibling que se intenta autorizar. Enlaces y no-directorios se rechazan
    /// sin seguirlos.
    pub(crate) fn observar_directorio_sibling(
        &self,
        nombre: &OsStr,
        identidad_esperada: IdentidadDirectorio,
    ) -> Result<DirectorioSiblingAnclado<'lock>, ErrorRenombrarAnclado> {
        super::validar_componente(nombre).map_err(mapear_error_directorio)?;
        if !self._lock.ubicacion().permite_rename_sibling(nombre) {
            return Err(ErrorRenombrarAnclado::FuenteNoHermana);
        }
        self.validar_parent()?;
        let raiz = super::abrir_directorio_hijo_lectura(&self.parent, nombre)
            .map_err(mapear_error_directorio)?;
        let identidad_raiz = metadata_directorio(&raiz).map_err(mapear_error_directorio)?;
        if identidad_raiz != identidad_esperada {
            return Err(ErrorRenombrarAnclado::IdentidadNoCoincide);
        }
        Ok(DirectorioSiblingAnclado {
            nombre: nombre.to_os_string(),
            raiz,
            identidad_raiz,
            _lock: PhantomData,
        })
    }

    fn validar_parent(&self) -> Result<(), ErrorRenombrarAnclado> {
        if metadata_directorio(&self.parent).map_err(mapear_error_directorio)?
            != self.identidad_parent
        {
            return Err(ErrorRenombrarAnclado::FuenteNoHermana);
        }
        Ok(())
    }
}

fn mapear_error_directorio(error: ErrorDirectorioAnclado) -> ErrorRenombrarAnclado {
    match error {
        ErrorDirectorioAnclado::Io => ErrorRenombrarAnclado::Io,
        ErrorDirectorioAnclado::ComponenteInvalido => ErrorRenombrarAnclado::ComponenteInvalido,
        ErrorDirectorioAnclado::EnlaceNoPermitido => ErrorRenombrarAnclado::EnlaceNoPermitido,
        ErrorDirectorioAnclado::TipoNoAdmitido => ErrorRenombrarAnclado::TipoNoAdmitido,
        ErrorDirectorioAnclado::RaizAusente | ErrorDirectorioAnclado::EntradaAusente => {
            ErrorRenombrarAnclado::FuenteAusente
        }
        ErrorDirectorioAnclado::EntradaExistente => ErrorRenombrarAnclado::DestinoExistente,
    }
}
