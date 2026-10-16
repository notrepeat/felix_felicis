//! Probe de un nombre sibling reservado desde el parent anclado.
//!
//! Solo informa ausencia o presencia de un archivo regular cuyo handle coincide
//! con el scope esperado. No lee payload ni concede autoridad de transacción.

use std::ffi::OsStr;
use std::fs::File;

use super::super::abrir::abrir_entrada_relativa;
use super::super::{
    ErrorDirectorioAnclado, ScopeFilesystem, TipoEntradaAnclada, identidad_archivo,
    scope_filesystem,
};
use super::ErrorDuenioTransaccionAnclado;

/// Hecho observado para un único nombre sibling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PresenciaSiblingNuevoFlujo {
    Ausente,
    Presente,
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
