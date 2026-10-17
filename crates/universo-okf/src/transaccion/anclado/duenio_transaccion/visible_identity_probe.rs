//! Comparación final de un sibling con la identidad nativa retenida.
//!
//! Esta primitiva solo confirma que el nombre todavía abre el mismo archivo
//! regular dentro del scope esperado. No conserva el probe ni concede autoridad.

use std::ffi::OsStr;
use std::fs::File;

use super::super::abrir::abrir_entrada_relativa;
use super::super::{
    IdentidadArchivo, ScopeFilesystem, TipoEntradaAnclada, identidad_archivo, scope_filesystem,
    validar_componente,
};
use super::ErrorDuenioTransaccionAnclado;

#[cfg(all(test, any(target_os = "linux", windows)))]
use super::super::ErrorDirectorioAnclado;

/// Reabre un sibling no-follow y compara el scope y la identidad del mismo handle.
///
/// Solo devuelve éxito para el archivo regular esperado. La ausencia y cualquier
/// error de apertura, scope o identidad fallan cerrados; el handle del probe se
/// cierra al salir de esta función.
pub(super) fn confirmar_sibling_presente_con_identidad(
    parent: &File,
    nombre: &OsStr,
    scope_esperado: ScopeFilesystem,
    identidad_esperada: IdentidadArchivo,
) -> Result<(), ErrorDuenioTransaccionAnclado> {
    let probe = abrir_probe(parent, nombre)?;
    let scope_observado =
        scope_filesystem(&probe).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    validar_handle_observado(&probe, scope_esperado, identidad_esperada, scope_observado)
}

fn abrir_probe(parent: &File, nombre: &OsStr) -> Result<File, ErrorDuenioTransaccionAnclado> {
    validar_componente(nombre).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    abrir_entrada_relativa(parent, nombre, TipoEntradaAnclada::Archivo)
        .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)
}

fn validar_handle_observado(
    probe: &File,
    scope_esperado: ScopeFilesystem,
    identidad_esperada: IdentidadArchivo,
    scope_observado: ScopeFilesystem,
) -> Result<(), ErrorDuenioTransaccionAnclado> {
    if scope_observado != scope_esperado {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }
    let identidad_observada =
        identidad_archivo(probe).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    if identidad_observada != identidad_esperada {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }
    Ok(())
}

#[cfg(all(test, any(target_os = "linux", windows)))]
fn confirmar_sibling_con_scope_de_prueba<S>(
    parent: &File,
    nombre: &OsStr,
    scope_esperado: ScopeFilesystem,
    identidad_esperada: IdentidadArchivo,
    obtener_scope: S,
) -> Result<(), ErrorDuenioTransaccionAnclado>
where
    S: FnOnce(&File) -> Result<ScopeFilesystem, ErrorDirectorioAnclado>,
{
    let probe = abrir_probe(parent, nombre)?;
    let scope_observado =
        obtener_scope(&probe).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    validar_handle_observado(&probe, scope_esperado, identidad_esperada, scope_observado)
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "visible_identity_probe_tests.rs"]
mod tests;
