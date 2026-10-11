use std::ffi::OsStr;
use std::fs::File;

use fs_at::{OpenOptions as OpenOptionsAt, OpenOptionsWriteMode};

use super::{
    DirectorioAnclado, ErrorDirectorioAnclado, abrir_directorio_hijo, clonar_handle, mapear_io,
    metadata_directorio, metadata_es_reparse, validar_componente,
};

impl DirectorioAnclado {
    pub(crate) fn read_dir(&mut self) -> Result<Vec<String>, ErrorDirectorioAnclado> {
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
            validar_componente(nombre)?;
            nombres.push(
                nombre
                    .to_str()
                    .ok_or(ErrorDirectorioAnclado::ComponenteInvalido)?
                    .to_owned(),
            );
        }
        Ok(nombres)
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
        let directorio = abrir_directorio_hijo(raiz, componente)?;
        let identidad_parent = metadata_directorio(raiz)?;
        let identidad_raiz = metadata_directorio(&directorio)?;
        Ok(DirectorioAnclado {
            parent: clonar_handle(raiz)?,
            basename: componente.to_os_string(),
            raiz: Some(directorio),
            identidad_parent,
            identidad_raiz: Some(identidad_raiz),
        })
    }

    pub(crate) fn crear_directorio(
        &self,
        componente: &str,
    ) -> Result<DirectorioAnclado, ErrorDirectorioAnclado> {
        let componente = validar_componente(OsStr::new(componente))?;
        let raiz = self
            .raiz
            .as_ref()
            .ok_or(ErrorDirectorioAnclado::RaizAusente)?;
        let opciones = OpenOptionsAt::default();
        let creado = opciones.mkdir_at(raiz, componente).map_err(mapear_io)?;
        drop(creado);
        let directorio = abrir_directorio_hijo(raiz, componente)?;
        let identidad_parent = metadata_directorio(raiz)?;
        let identidad_raiz = metadata_directorio(&directorio)?;
        Ok(DirectorioAnclado {
            parent: clonar_handle(raiz)?,
            basename: componente.to_os_string(),
            raiz: Some(directorio),
            identidad_parent,
            identidad_raiz: Some(identidad_raiz),
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
        let mut opciones = OpenOptionsAt::default();
        opciones.read(true).follow(false);
        #[cfg(unix)]
        {
            use fs_at::os::unix::OpenOptionsExt;
            opciones.custom_flags(libc::O_NONBLOCK);
        }
        let archivo = opciones.open_at(raiz, componente).map_err(mapear_io)?;
        metadata_archivo_regular(&archivo)?;
        Ok(archivo)
    }

    pub(crate) fn crear_nuevo(&self, componente: &str) -> Result<File, ErrorDirectorioAnclado> {
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
