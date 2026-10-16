//! Probe de un nombre sibling reservado desde el parent anclado.
//!
//! Solo informa ausencia o presencia de un archivo regular cuyo handle coincide
//! con el scope esperado. No lee payload ni concede autoridad de transacción.

use std::ffi::OsStr;
use std::fs::File;

use super::super::abrir::abrir_entrada_relativa;
use super::super::{
    ErrorDirectorioAnclado, ScopeFilesystem, TipoEntradaAnclada, identidad_archivo,
    scope_filesystem, validar_componente,
};
use super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado};

/// Hecho observado para un único nombre sibling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PresenciaSiblingNuevoFlujo {
    Ausente,
    Presente,
}

/// Hechos de presencia de los cuatro siblings reservados para el flujo nuevo.
/// No son permiso, dispatch ni autoridad de cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ObservacionSiblingsNuevoFlujo {
    journal: PresenciaSiblingNuevoFlujo,
    journal_temporal: PresenciaSiblingNuevoFlujo,
    contexto: PresenciaSiblingNuevoFlujo,
    contexto_temporal: PresenciaSiblingNuevoFlujo,
}

impl ObservacionSiblingsNuevoFlujo {
    pub(crate) fn journal(&self) -> PresenciaSiblingNuevoFlujo {
        self.journal
    }

    pub(crate) fn journal_temporal(&self) -> PresenciaSiblingNuevoFlujo {
        self.journal_temporal
    }

    pub(crate) fn contexto(&self) -> PresenciaSiblingNuevoFlujo {
        self.contexto
    }

    pub(crate) fn contexto_temporal(&self) -> PresenciaSiblingNuevoFlujo {
        self.contexto_temporal
    }
}

impl DuenioTransaccionAnclado<'_> {
    /// Observa los cuatro nombres sibling internos desde el parent retenido.
    ///
    /// Solo una ausencia reportada por la primitiva de apertura se registra
    /// como `Ausente`; cualquier error de nombre, tipo, scope o identidad falla
    /// cerrado. Las observaciones son secuenciales y no equivalen a una
    /// garantía global contra cambios externos.
    pub(crate) fn observar_siblings_nuevo_flujo(
        &mut self,
    ) -> Result<ObservacionSiblingsNuevoFlujo, ErrorDuenioTransaccionAnclado> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let ubicacion = self._lock.ubicacion();
        let basename = ubicacion
            .ruta()
            .file_name()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        validar_componente(basename).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

        let ruta_journal = ubicacion.journal();
        let nombre_journal = ruta_journal
            .as_path()
            .file_name()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?
            .to_os_string();
        validar_componente(&nombre_journal)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let nombre_journal_temporal = ubicacion.nombre_journal_temporal();
        validar_componente(&nombre_journal_temporal)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let nombre_contexto = super::super::contexto_admision::nombre_canonico(basename)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let nombre_contexto_temporal =
            super::super::contexto_admision::nombre_temporal_sibling(&nombre_contexto)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

        let identidad_scope = self.scope_parent;
        Ok(ObservacionSiblingsNuevoFlujo {
            journal: observar_sibling(&parent, &nombre_journal, identidad_scope)?,
            journal_temporal: observar_sibling(&parent, &nombre_journal_temporal, identidad_scope)?,
            contexto: observar_sibling(&parent, &nombre_contexto, identidad_scope)?,
            contexto_temporal: observar_sibling(
                &parent,
                &nombre_contexto_temporal,
                identidad_scope,
            )?,
        })
    }
}

/// Abre un sibling no-follow y valida el scope y la identidad del handle.
///
/// Solo `EntradaAusente` produce `Ausente`; cualquier otro fallo se redacta
/// como error cerrado. El contenido del archivo nunca se lee.
pub(super) fn observar_sibling(
    parent: &File,
    nombre: &OsStr,
    scope_esperado: ScopeFilesystem,
) -> Result<PresenciaSiblingNuevoFlujo, ErrorDuenioTransaccionAnclado> {
    observar_sibling_con_scope(parent, nombre, scope_esperado, scope_filesystem)
}

fn observar_sibling_con_scope<S>(
    parent: &File,
    nombre: &OsStr,
    scope_esperado: ScopeFilesystem,
    obtener_scope: S,
) -> Result<PresenciaSiblingNuevoFlujo, ErrorDuenioTransaccionAnclado>
where
    S: FnOnce(&File) -> Result<ScopeFilesystem, ErrorDirectorioAnclado>,
{
    let archivo = match abrir_entrada_relativa(parent, nombre, TipoEntradaAnclada::Archivo) {
        Ok(archivo) => archivo,
        Err(ErrorDirectorioAnclado::EntradaAusente) => {
            return Ok(PresenciaSiblingNuevoFlujo::Ausente);
        }
        Err(_) => return Err(ErrorDuenioTransaccionAnclado::NoVerificable),
    };
    if obtener_scope(&archivo).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?
        != scope_esperado
    {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }
    identidad_archivo(&archivo).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    Ok(PresenciaSiblingNuevoFlujo::Presente)
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "observar_siblings_nuevo_flujo_tests.rs"]
mod tests;
