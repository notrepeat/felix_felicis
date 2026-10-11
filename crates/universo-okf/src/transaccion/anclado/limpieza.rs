use std::ffi::{OsStr, OsString};
use std::fs::File;

use super::super::stage::StageAnclado;
use super::abrir::abrir_entrada_relativa;
use super::{
    ErrorDirectorioAnclado, IdentidadDirectorio, TipoEntradaAnclada, clasificar_entrada_sin_seguir,
    mapear_io, metadata_directorio, metadata_es_reparse, validar_componente,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorLimpiezaAnclado {
    Directorio(ErrorDirectorioAnclado),
    IdentidadInconsistente,
}

enum EntradaLimpieza {
    Archivo(OsString),
    Directorio {
        nombre: OsString,
        identidad: IdentidadDirectorio,
        hijos: Vec<EntradaLimpieza>,
    },
}

pub(crate) fn limpiar_stage_anclado<F>(
    stage: &StageAnclado,
    eliminar: &mut F,
) -> Result<(), ErrorLimpiezaAnclado>
where
    F: FnMut(&File, &OsStr, bool) -> Result<(), ErrorDirectorioAnclado>,
{
    let (directorio, componente, identidad_stage) = stage.partes_cleanup();
    let Some(identidad_stage) = identidad_stage else {
        return Err(ErrorLimpiezaAnclado::IdentidadInconsistente);
    };
    let identidad_parent = directorio.identidad_parent();
    validar_componente(componente).map_err(ErrorLimpiezaAnclado::Directorio)?;
    if directorio.basename != componente {
        return Err(ErrorLimpiezaAnclado::IdentidadInconsistente);
    }

    let parent = &directorio.parent;
    validar_identidad(parent, identidad_parent)?;
    let raiz_abierta = directorio
        .raiz
        .as_ref()
        .ok_or(ErrorLimpiezaAnclado::Directorio(
            ErrorDirectorioAnclado::RaizAusente,
        ))?;
    if directorio.identidad_raiz != Some(identidad_stage) {
        return Err(ErrorLimpiezaAnclado::IdentidadInconsistente);
    }
    validar_identidad(raiz_abierta, identidad_stage)?;

    // Reopen the one stage basename from the anchored parent before inspecting
    // or deleting anything. LockEscritura coordinates Felix writers; this
    // check does not make name-based unlink atomic against hostile processes.
    let mut raiz = abrir_directorio_sin_seguir(parent, componente)?;
    validar_identidad(&raiz, identidad_stage)?;

    // Reject the whole tree before the first unlink/rmdir. In particular,
    // links, reparse points, special files, and invalid names leave every
    // staged entry untouched.
    let entradas = preflight_directorio(&mut raiz)?;
    eliminar_entradas(&raiz, &entradas, eliminar)?;

    validar_identidad(parent, identidad_parent)?;
    validar_identidad(raiz_abierta, identidad_stage)?;
    let raiz_final = abrir_directorio_sin_seguir(parent, componente)?;
    validar_identidad(&raiz_final, identidad_stage)?;
    drop(raiz_final);
    drop(raiz);
    eliminar(parent, componente, true).map_err(ErrorLimpiezaAnclado::Directorio)
}

fn preflight_directorio(
    directorio: &mut File,
) -> Result<Vec<EntradaLimpieza>, ErrorLimpiezaAnclado> {
    let nombres = nombres_hijos(directorio)?;
    let mut entradas = Vec::with_capacity(nombres.len());
    for nombre in nombres {
        validar_componente(&nombre).map_err(ErrorLimpiezaAnclado::Directorio)?;
        match clasificar_entrada_sin_seguir(directorio, &nombre)
            .map_err(ErrorLimpiezaAnclado::Directorio)?
        {
            TipoEntradaAnclada::Archivo => {
                let entrada = abrir_entrada_sin_seguir(directorio, &nombre)?;
                let metadata = entrada
                    .metadata()
                    .map_err(mapear_io)
                    .map_err(ErrorLimpiezaAnclado::Directorio)?;
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata_es_reparse(&metadata)
                {
                    return Err(ErrorLimpiezaAnclado::Directorio(
                        ErrorDirectorioAnclado::TipoNoAdmitido,
                    ));
                }
                entradas.push(EntradaLimpieza::Archivo(nombre));
            }
            TipoEntradaAnclada::Directorio => {
                let mut entrada = abrir_directorio_sin_seguir(directorio, &nombre)?;
                let identidad =
                    metadata_directorio(&entrada).map_err(ErrorLimpiezaAnclado::Directorio)?;
                let hijos = preflight_directorio(&mut entrada)?;
                let reabierta = abrir_directorio_sin_seguir(directorio, &nombre)?;
                validar_identidad(&reabierta, identidad)?;
                entradas.push(EntradaLimpieza::Directorio {
                    nombre,
                    identidad,
                    hijos,
                });
            }
        }
    }
    Ok(entradas)
}

fn eliminar_entradas<F>(
    directorio: &File,
    entradas: &[EntradaLimpieza],
    eliminar: &mut F,
) -> Result<(), ErrorLimpiezaAnclado>
where
    F: FnMut(&File, &OsStr, bool) -> Result<(), ErrorDirectorioAnclado>,
{
    for entrada in entradas {
        match entrada {
            EntradaLimpieza::Archivo(nombre) => {
                let archivo = abrir_entrada_sin_seguir(directorio, nombre)?;
                let metadata = archivo
                    .metadata()
                    .map_err(mapear_io)
                    .map_err(ErrorLimpiezaAnclado::Directorio)?;
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata_es_reparse(&metadata)
                {
                    return Err(ErrorLimpiezaAnclado::Directorio(
                        ErrorDirectorioAnclado::TipoNoAdmitido,
                    ));
                }
                drop(archivo);
                eliminar(directorio, nombre, false).map_err(ErrorLimpiezaAnclado::Directorio)?;
            }
            EntradaLimpieza::Directorio {
                nombre,
                identidad,
                hijos,
            } => {
                let hijo = abrir_directorio_sin_seguir(directorio, nombre)?;
                validar_identidad(&hijo, *identidad)?;
                eliminar_entradas(&hijo, hijos, eliminar)?;
                let reabierto = abrir_directorio_sin_seguir(directorio, nombre)?;
                validar_identidad(&reabierto, *identidad)?;
                drop(reabierto);
                drop(hijo);
                eliminar(directorio, nombre, true).map_err(ErrorLimpiezaAnclado::Directorio)?;
            }
        }
    }
    Ok(())
}

fn nombres_hijos(directorio: &mut File) -> Result<Vec<OsString>, ErrorLimpiezaAnclado> {
    let mut entradas = fs_at::read_dir(directorio)
        .map_err(mapear_io)
        .map_err(ErrorLimpiezaAnclado::Directorio)?;
    let mut nombres = Vec::new();
    for entrada in &mut entradas {
        let entrada = entrada
            .map_err(mapear_io)
            .map_err(ErrorLimpiezaAnclado::Directorio)?;
        let nombre = entrada.name();
        if nombre == OsStr::new(".") || nombre == OsStr::new("..") {
            continue;
        }
        nombres.push(nombre.to_os_string());
    }
    nombres.sort_unstable();
    if nombres.windows(2).any(|par| par[0] == par[1]) {
        return Err(ErrorLimpiezaAnclado::Directorio(
            ErrorDirectorioAnclado::ComponenteInvalido,
        ));
    }
    Ok(nombres)
}

fn abrir_directorio_sin_seguir(
    parent: &File,
    componente: &OsStr,
) -> Result<File, ErrorLimpiezaAnclado> {
    validar_componente(componente).map_err(ErrorLimpiezaAnclado::Directorio)?;
    let directorio = abrir_entrada_relativa(parent, componente, TipoEntradaAnclada::Directorio)
        .map_err(ErrorLimpiezaAnclado::Directorio)?;
    metadata_directorio(&directorio).map_err(ErrorLimpiezaAnclado::Directorio)?;
    Ok(directorio)
}

fn abrir_entrada_sin_seguir(
    parent: &File,
    componente: &OsStr,
) -> Result<File, ErrorLimpiezaAnclado> {
    validar_componente(componente).map_err(ErrorLimpiezaAnclado::Directorio)?;
    if clasificar_entrada_sin_seguir(parent, componente)
        .map_err(ErrorLimpiezaAnclado::Directorio)?
        != TipoEntradaAnclada::Archivo
    {
        return Err(ErrorLimpiezaAnclado::Directorio(
            ErrorDirectorioAnclado::TipoNoAdmitido,
        ));
    }
    let entrada = abrir_entrada_relativa(parent, componente, TipoEntradaAnclada::Archivo)
        .map_err(ErrorLimpiezaAnclado::Directorio)?;
    let metadata = entrada
        .metadata()
        .map_err(mapear_io)
        .map_err(ErrorLimpiezaAnclado::Directorio)?;
    if metadata.file_type().is_symlink() || metadata_es_reparse(&metadata) {
        return Err(ErrorLimpiezaAnclado::Directorio(
            ErrorDirectorioAnclado::EnlaceNoPermitido,
        ));
    }
    if !metadata.is_file() {
        return Err(ErrorLimpiezaAnclado::Directorio(
            ErrorDirectorioAnclado::TipoNoAdmitido,
        ));
    }
    Ok(entrada)
}

fn validar_identidad(
    handle: &File,
    esperada: IdentidadDirectorio,
) -> Result<(), ErrorLimpiezaAnclado> {
    if metadata_directorio(handle).map_err(ErrorLimpiezaAnclado::Directorio)? != esperada {
        return Err(ErrorLimpiezaAnclado::IdentidadInconsistente);
    }
    Ok(())
}
