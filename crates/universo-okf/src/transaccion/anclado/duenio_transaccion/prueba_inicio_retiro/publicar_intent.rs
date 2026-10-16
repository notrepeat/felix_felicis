//! Publicación puntual del intent bajo el owner que revalidó la fila.

use std::ffi::{OsStr, OsString};
use std::fs::File;

use super::{
    ErrorPruebaReplayRootCoincidente, IntentPublicado, IntentTemporalEscrito,
    IntentTemporalVerificado, ObservacionIntentTemporalCompleta,
};
use crate::transaccion::anclado::duenio_transaccion::observar_siblings_nuevo_flujo::{
    PresenciaSiblingNuevoFlujo, observar_sibling,
};

pub(super) fn publicar<'owner, 'lock>(
    observacion: ObservacionIntentTemporalCompleta<'owner, 'lock>,
) -> Result<IntentPublicado<'owner, 'lock>, ErrorPruebaReplayRootCoincidente> {
    publicar_impl(
        observacion,
        crate::transaccion::anclado::contexto_admision::renombrar_no_reemplazar,
    )
}

#[cfg(test)]
pub(super) fn publicar_con<'owner, 'lock, R>(
    observacion: ObservacionIntentTemporalCompleta<'owner, 'lock>,
    renombrar: R,
) -> Result<IntentPublicado<'owner, 'lock>, ErrorPruebaReplayRootCoincidente>
where
    R: FnOnce(&File, &File, &OsStr, &OsStr) -> Result<(), ()>,
{
    publicar_impl(observacion, renombrar)
}

fn publicar_impl<'owner, 'lock, R>(
    observacion: ObservacionIntentTemporalCompleta<'owner, 'lock>,
    renombrar: R,
) -> Result<IntentPublicado<'owner, 'lock>, ErrorPruebaReplayRootCoincidente>
where
    R: FnOnce(&File, &File, &OsStr, &OsStr) -> Result<(), ()>,
{
    let ObservacionIntentTemporalCompleta { _verificado } = observacion;
    let ObservacionIntentTemporalCompleta { _verificado } = _verificado.revalidar_completo()?;
    let IntentTemporalVerificado { _escrito, _archivo } = _verificado;
    drop(_archivo);

    let (nombre_temporal, nombre_canonico) = nombres_intent(&_escrito)?;
    let prueba = &_escrito._inicio._prueba;
    let parent = prueba
        ._owner
        .parent_revalidado()
        .map_err(|_| ErrorPruebaReplayRootCoincidente)?;

    let fuente = abrir_temporal_renombrable(&parent, &nombre_temporal)?;
    let fuente = _escrito.verificar_handle_temporal(fuente)?;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    verificar_nombre_fuente(&_escrito, &parent, &nombre_temporal)?;

    let _resultado_syscall = renombrar(&parent, &fuente, &nombre_temporal, &nombre_canonico);
    // Windows reabre el destino readonly durante la reconciliación. Cierra
    // antes el handle que conserva DELETE; también simplifica el ciclo Unix.
    drop(fuente);

    if !confirmar_publicacion(&_escrito, &nombre_canonico, &nombre_temporal) {
        return Err(ErrorPruebaReplayRootCoincidente);
    }
    drop(parent);

    Ok(IntentPublicado { _escrito })
}

fn nombres_intent(
    escrito: &IntentTemporalEscrito<'_, '_>,
) -> Result<(OsString, OsString), ErrorPruebaReplayRootCoincidente> {
    let ubicacion = escrito._inicio._prueba._owner._lock.ubicacion();
    let basename = ubicacion
        .ruta()
        .file_name()
        .ok_or(ErrorPruebaReplayRootCoincidente)?;
    let canonico =
        super::super::super::observar_siblings_nuevo_flujo::nombre_intent_retiro(basename)
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
    let temporal =
        super::super::super::super::contexto_admision::nombre_temporal_sibling(&canonico)
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
    Ok((temporal, canonico))
}

#[cfg(windows)]
fn abrir_temporal_renombrable(
    parent: &File,
    nombre: &OsStr,
) -> Result<File, ErrorPruebaReplayRootCoincidente> {
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
        .map_err(|_| ErrorPruebaReplayRootCoincidente)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn abrir_temporal_renombrable(
    parent: &File,
    nombre: &OsStr,
) -> Result<File, ErrorPruebaReplayRootCoincidente> {
    super::super::super::super::abrir::abrir_entrada_relativa(
        parent,
        nombre,
        super::super::super::super::TipoEntradaAnclada::Archivo,
    )
    .map_err(|_| ErrorPruebaReplayRootCoincidente)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
fn abrir_temporal_renombrable(
    _parent: &File,
    _nombre: &OsStr,
) -> Result<File, ErrorPruebaReplayRootCoincidente> {
    Err(ErrorPruebaReplayRootCoincidente)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn verificar_nombre_fuente(
    escrito: &IntentTemporalEscrito<'_, '_>,
    parent: &File,
    nombre: &OsStr,
) -> Result<(), ErrorPruebaReplayRootCoincidente> {
    let nombre_visible = super::super::super::super::abrir::abrir_entrada_relativa(
        parent,
        nombre,
        super::super::super::super::TipoEntradaAnclada::Archivo,
    )
    .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
    drop(escrito.verificar_handle_temporal(nombre_visible)?);
    Ok(())
}

fn confirmar_publicacion(
    escrito: &IntentTemporalEscrito<'_, '_>,
    nombre_canonico: &OsStr,
    nombre_temporal: &OsStr,
) -> bool {
    let prueba = &escrito._inicio._prueba;
    let parent = match prueba._owner.parent_revalidado() {
        Ok(parent) => parent,
        Err(_) => return false,
    };
    let canonico = match super::super::super::super::abrir::abrir_entrada_relativa(
        &parent,
        nombre_canonico,
        super::super::super::super::TipoEntradaAnclada::Archivo,
    ) {
        Ok(canonico) => canonico,
        Err(_) => return false,
    };
    if escrito.verificar_handle_temporal(canonico).is_err() {
        return false;
    }
    matches!(
        observar_sibling(&parent, nombre_temporal, prueba.scope_parent),
        Ok(PresenciaSiblingNuevoFlujo::Ausente)
    )
}
