use std::ffi::OsStr;
use std::fs::File;

use fs_at::{OpenOptions as OpenOptionsAt, OpenOptionsWriteMode};

use super::abrir::abrir_entrada_relativa;
use super::{
    DirectorioAnclado, EntradaLecturaAnclada, ErrorDirectorioAnclado, TipoEntradaAnclada,
    clasificar_entrada_sin_seguir, clonar_handle, mapear_error_creacion, mapear_io,
    metadata_directorio, metadata_es_reparse, validar_componente,
};

impl DirectorioAnclado {
    pub(crate) fn read_dir(&mut self) -> Result<Vec<std::ffi::OsString>, ErrorDirectorioAnclado> {
        let raiz = self
            .raiz
            .as_mut()
            .ok_or(ErrorDirectorioAnclado::RaizAusente)?;
        metadata_directorio(raiz)?;
        let mut entradas = fs_at::read_dir(raiz).map_err(mapear_io)?;
        let mut nombres = Vec::new();
        for entrada in &mut entradas {
            let entrada = entrada.map_err(mapear_io)?;
            let nombre = entrada.name();
            if nombre == OsStr::new(".") || nombre == OsStr::new("..") {
                continue;
            }
            nombres.push(nombre.to_os_string());
        }
        Ok(nombres)
    }

    /// Abre y clasifica una sola entrada relativa al handle del directorio.
    /// El handle abierto también es el que se recorre o se lee.
    pub(crate) fn abrir_entrada_inventario(
        &self,
        componente: &str,
    ) -> Result<EntradaLecturaAnclada, ErrorDirectorioAnclado> {
        let componente = validar_componente_lectura(componente)?;
        let raiz = self
            .raiz
            .as_ref()
            .ok_or(ErrorDirectorioAnclado::RaizAusente)?;
        match clasificar_entrada_sin_seguir(raiz, componente)? {
            TipoEntradaAnclada::Directorio => {
                let entrada =
                    abrir_entrada_relativa(raiz, componente, TipoEntradaAnclada::Directorio)?;
                let identidad_parent = metadata_directorio(raiz)?;
                let identidad_raiz = metadata_directorio(&entrada)?;
                Ok(EntradaLecturaAnclada::Directorio(Self {
                    parent: clonar_handle(raiz)?,
                    basename: componente.to_os_string(),
                    raiz: Some(entrada),
                    identidad_parent,
                    identidad_raiz: Some(identidad_raiz),
                    permite_escritura: false,
                    puede_crear_stage_sibling: false,
                }))
            }
            TipoEntradaAnclada::Archivo => {
                let entrada =
                    abrir_entrada_relativa(raiz, componente, TipoEntradaAnclada::Archivo)?;
                metadata_archivo_regular(&entrada)?;
                Ok(EntradaLecturaAnclada::Archivo(entrada))
            }
        }
    }

    pub(crate) fn abrir_directorio(
        &self,
        componente: &str,
    ) -> Result<DirectorioAnclado, ErrorDirectorioAnclado> {
        let componente = validar_componente(OsStr::new(componente))?;
        let raiz = self
            .raiz
            .as_ref()
            .ok_or(ErrorDirectorioAnclado::RaizAusente)?;
        let directorio = super::abrir_directorio_hijo(raiz, componente)?;
        let identidad_parent = metadata_directorio(raiz)?;
        let identidad_raiz = metadata_directorio(&directorio)?;
        Ok(DirectorioAnclado {
            parent: clonar_handle(raiz)?,
            basename: componente.to_os_string(),
            raiz: Some(directorio),
            identidad_parent,
            identidad_raiz: Some(identidad_raiz),
            permite_escritura: false,
            puede_crear_stage_sibling: false,
        })
    }

    pub(crate) fn abrir_directorio_stage(
        &self,
        componente: &str,
    ) -> Result<DirectorioAnclado, ErrorDirectorioAnclado> {
        if !self.permite_escritura {
            return Err(ErrorDirectorioAnclado::Io);
        }
        let componente = validar_componente(OsStr::new(componente))?;
        let raiz = self
            .raiz
            .as_ref()
            .ok_or(ErrorDirectorioAnclado::RaizAusente)?;
        let directorio = super::abrir_directorio_hijo_stage_existente(raiz, componente)?;
        let identidad_parent = metadata_directorio(raiz)?;
        let identidad_raiz = metadata_directorio(&directorio)?;
        Ok(DirectorioAnclado {
            parent: clonar_handle(raiz)?,
            basename: componente.to_os_string(),
            raiz: Some(directorio),
            identidad_parent,
            identidad_raiz: Some(identidad_raiz),
            permite_escritura: true,
            puede_crear_stage_sibling: false,
        })
    }

    pub(crate) fn crear_directorio(
        &self,
        componente: &str,
    ) -> Result<DirectorioAnclado, ErrorDirectorioAnclado> {
        if !self.permite_escritura {
            return Err(ErrorDirectorioAnclado::Io);
        }
        let componente = validar_componente(OsStr::new(componente))?;
        let raiz = self
            .raiz
            .as_ref()
            .ok_or(ErrorDirectorioAnclado::RaizAusente)?;
        let mut opciones = OpenOptionsAt::default();
        opciones.create_new(true);
        let creado = opciones
            .mkdir_at(raiz, componente)
            .map_err(mapear_error_creacion)?;
        let directorio = super::abrir_directorio_hijo_stage(raiz, componente, &creado)?;
        let identidad_parent = metadata_directorio(raiz)?;
        let identidad_raiz = metadata_directorio(&directorio)?;
        Ok(DirectorioAnclado {
            parent: clonar_handle(raiz)?,
            basename: componente.to_os_string(),
            raiz: Some(directorio),
            identidad_parent,
            identidad_raiz: Some(identidad_raiz),
            permite_escritura: true,
            puede_crear_stage_sibling: false,
        })
    }

    pub(crate) fn abrir_archivo_regular(
        &self,
        componente: &str,
    ) -> Result<File, ErrorDirectorioAnclado> {
        let componente = validar_componente(OsStr::new(componente))?;
        let raiz = self
            .raiz
            .as_ref()
            .ok_or(ErrorDirectorioAnclado::RaizAusente)?;
        if clasificar_entrada_sin_seguir(raiz, componente)? != TipoEntradaAnclada::Archivo {
            return Err(ErrorDirectorioAnclado::TipoNoAdmitido);
        }
        let archivo = abrir_entrada_relativa(raiz, componente, TipoEntradaAnclada::Archivo)?;
        metadata_archivo_regular(&archivo)?;
        Ok(archivo)
    }

    pub(crate) fn crear_nuevo(&self, componente: &str) -> Result<File, ErrorDirectorioAnclado> {
        if !self.permite_escritura {
            return Err(ErrorDirectorioAnclado::Io);
        }
        let componente = validar_componente(OsStr::new(componente))?;
        let raiz = self
            .raiz
            .as_ref()
            .ok_or(ErrorDirectorioAnclado::RaizAusente)?;
        let mut opciones = OpenOptionsAt::default();
        opciones
            .read(true)
            .write(OpenOptionsWriteMode::Write)
            .create_new(true)
            .follow(false);
        let archivo = opciones.open_at(raiz, componente).map_err(mapear_io)?;
        metadata_archivo_regular(&archivo)?;
        Ok(archivo)
    }
}

fn metadata_archivo_regular(handle: &File) -> Result<(), ErrorDirectorioAnclado> {
    let metadata = handle.metadata().map_err(mapear_io)?;
    if metadata.file_type().is_symlink() || metadata_es_reparse(&metadata) {
        return Err(ErrorDirectorioAnclado::EnlaceNoPermitido);
    }
    if !metadata.is_file() {
        return Err(ErrorDirectorioAnclado::TipoNoAdmitido);
    }
    Ok(())
}

fn validar_componente_lectura(componente: &str) -> Result<&OsStr, ErrorDirectorioAnclado> {
    if componente.is_empty()
        || matches!(componente, "." | "..")
        || componente.contains('/')
        || componente.contains('\\')
        || componente.contains('\0')
    {
        return Err(ErrorDirectorioAnclado::ComponenteInvalido);
    }
    Ok(OsStr::new(componente))
}
