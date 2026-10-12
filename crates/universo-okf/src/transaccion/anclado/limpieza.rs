use std::ffi::{OsStr, OsString};
use std::fs::File;

use super::super::inventario::LimitesInventario;
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
    LimiteEstructuralExcedido,
}

enum EntradaLimpieza {
    Archivo(OsString),
    Directorio {
        nombre: OsString,
        identidad: IdentidadDirectorio,
        hijos: Vec<EntradaLimpieza>,
    },
}

#[derive(Clone, Copy)]
struct PresupuestoLimpieza {
    limites: LimitesInventario,
    entradas: usize,
    bytes_rutas: usize,
}

impl PresupuestoLimpieza {
    fn nuevo(limites: LimitesInventario) -> Self {
        Self {
            limites,
            entradas: 0,
            bytes_rutas: 0,
        }
    }

    fn reservar(
        &mut self,
        bytes_ruta: usize,
        profundidad: usize,
    ) -> Result<(), ErrorLimpiezaAnclado> {
        if profundidad > self.limites.profundidad {
            return Err(ErrorLimpiezaAnclado::LimiteEstructuralExcedido);
        }
        let entradas = self
            .entradas
            .checked_add(1)
            .filter(|total| *total <= self.limites.entradas)
            .ok_or(ErrorLimpiezaAnclado::LimiteEstructuralExcedido)?;
        let bytes_rutas = self
            .bytes_rutas
            .checked_add(bytes_ruta)
            .filter(|total| *total <= self.limites.bytes_rutas)
            .ok_or(ErrorLimpiezaAnclado::LimiteEstructuralExcedido)?;
        self.entradas = entradas;
        self.bytes_rutas = bytes_rutas;
        Ok(())
    }
}

struct NombreLimpieza {
    nombre: OsString,
    profundidad: usize,
    bytes_ruta: usize,
}

pub(crate) fn limpiar_stage_anclado<F>(
    stage: &StageAnclado,
    eliminar: &mut F,
) -> Result<(), ErrorLimpiezaAnclado>
where
    F: FnMut(&File, &OsStr, bool) -> Result<(), ErrorDirectorioAnclado>,
{
    limpiar_stage_anclado_con_limites(stage, LimitesInventario::CONTRATO, eliminar)
}

pub(in crate::transaccion) fn limpiar_stage_anclado_con_limites<F>(
    stage: &StageAnclado,
    limites: LimitesInventario,
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
    let mut presupuesto = PresupuestoLimpieza::nuevo(limites);
    let entradas = preflight_directorio(&mut raiz, 0, 0, &mut presupuesto)?;
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
    profundidad: usize,
    bytes_ruta: usize,
    presupuesto: &mut PresupuestoLimpieza,
) -> Result<Vec<EntradaLimpieza>, ErrorLimpiezaAnclado> {
    let nombres = nombres_hijos(directorio, profundidad, bytes_ruta, presupuesto)?;
    let mut entradas = Vec::new();
    for hijo in nombres {
        let NombreLimpieza {
            nombre,
            profundidad,
            bytes_ruta,
        } = hijo;
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
                let hijos =
                    preflight_directorio(&mut entrada, profundidad, bytes_ruta, presupuesto)?;
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

fn nombres_hijos(
    directorio: &mut File,
    profundidad: usize,
    bytes_ruta: usize,
    presupuesto: &mut PresupuestoLimpieza,
) -> Result<Vec<NombreLimpieza>, ErrorLimpiezaAnclado> {
    let mut entradas = fs_at::read_dir(directorio)
        .map_err(mapear_io)
        .map_err(ErrorLimpiezaAnclado::Directorio)?;
    let mut nombres: Vec<NombreLimpieza> = Vec::new();
    for entrada in &mut entradas {
        let entrada = entrada
            .map_err(mapear_io)
            .map_err(ErrorLimpiezaAnclado::Directorio)?;
        let nombre = entrada.name();
        if nombre == OsStr::new(".") || nombre == OsStr::new("..") {
            continue;
        }
        validar_componente(nombre).map_err(ErrorLimpiezaAnclado::Directorio)?;
        let profundidad_hijo = profundidad
            .checked_add(1)
            .ok_or(ErrorLimpiezaAnclado::LimiteEstructuralExcedido)?;
        let bytes_ruta_hijo = bytes_ruta
            .checked_add(usize::from(bytes_ruta > 0))
            .and_then(|bytes| bytes.checked_add(nombre.len()))
            .ok_or(ErrorLimpiezaAnclado::LimiteEstructuralExcedido)?;
        presupuesto.reservar(bytes_ruta_hijo, profundidad_hijo)?;
        nombres.push(NombreLimpieza {
            nombre: nombre.to_os_string(),
            profundidad: profundidad_hijo,
            bytes_ruta: bytes_ruta_hijo,
        });
    }
    nombres.sort_unstable_by(|a, b| a.nombre.cmp(&b.nombre));
    if nombres.windows(2).any(|par| par[0].nombre == par[1].nombre) {
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

#[cfg(test)]
mod tests {
    use super::{ErrorLimpiezaAnclado, PresupuestoLimpieza};
    use crate::transaccion::inventario::LimitesInventario;

    #[test]
    fn presupuesto_limpieza_respeta_limites_del_contrato_sin_archivos() {
        let contrato = LimitesInventario::CONTRATO;

        let mut entradas = PresupuestoLimpieza::nuevo(contrato);
        for _ in 0..contrato.entradas {
            entradas.reservar(0, 1).unwrap();
        }
        assert_eq!(entradas.entradas, 100_000);
        assert_eq!(
            entradas.reservar(0, 1),
            Err(ErrorLimpiezaAnclado::LimiteEstructuralExcedido)
        );
        assert_eq!(entradas.entradas, contrato.entradas);

        let mut profundidad = PresupuestoLimpieza::nuevo(contrato);
        profundidad.reservar(0, contrato.profundidad).unwrap();
        assert_eq!(
            profundidad.reservar(0, contrato.profundidad + 1),
            Err(ErrorLimpiezaAnclado::LimiteEstructuralExcedido)
        );
        assert_eq!(profundidad.entradas, 1);

        let mut rutas = PresupuestoLimpieza::nuevo(contrato);
        rutas.reservar(contrato.bytes_rutas, 1).unwrap();
        assert_eq!(rutas.bytes_rutas, 16 * 1024 * 1024);
        assert_eq!(
            rutas.reservar(1, 1),
            Err(ErrorLimpiezaAnclado::LimiteEstructuralExcedido)
        );
        assert_eq!(rutas.bytes_rutas, contrato.bytes_rutas);
    }
}
