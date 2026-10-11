use std::ffi::OsStr;
use std::fs::File;

use fs_at::{OpenOptions as OpenOptionsAt, OpenOptionsWriteMode};

use super::{
    DirectorioAnclado, EntradaLecturaAnclada, ErrorDirectorioAnclado, abrir_directorio_hijo,
    clonar_handle, mapear_io, metadata_directorio, metadata_es_reparse, validar_componente,
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
        let mut opciones = OpenOptionsAt::default();
        opciones.read(true).follow(false);
        #[cfg(unix)]
        {
            use fs_at::os::unix::OpenOptionsExt;
            opciones.custom_flags(libc::O_NONBLOCK);
        }
        let entrada = opciones
            .open_at(raiz, componente)
            .map_err(mapear_apertura_sin_seguir)?;
        let metadata = entrada.metadata().map_err(mapear_io)?;
        if metadata.file_type().is_symlink() || metadata_es_reparse(&metadata) {
            return Err(ErrorDirectorioAnclado::EnlaceNoPermitido);
        }
        if metadata.is_dir() {
            let identidad_parent = metadata_directorio(raiz)?;
            let identidad_raiz = metadata_directorio(&entrada)?;
            Ok(EntradaLecturaAnclada::Directorio(Self {
                parent: clonar_handle(raiz)?,
                basename: componente.to_os_string(),
                raiz: Some(entrada),
                identidad_parent,
                identidad_raiz: Some(identidad_raiz),
            }))
        } else if metadata.is_file() {
            Ok(EntradaLecturaAnclada::Archivo(entrada))
        } else {
            Err(ErrorDirectorioAnclado::TipoNoAdmitido)
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

fn mapear_apertura_sin_seguir(error: std::io::Error) -> ErrorDirectorioAnclado {
    #[cfg(unix)]
    if error.raw_os_error() == Some(libc::ELOOP) {
        return ErrorDirectorioAnclado::EnlaceNoPermitido;
    }
    #[cfg(windows)]
    // fs_at reports a reparse point rejected by follow(false) as Win32 1921.
    if error.raw_os_error() == Some(1921) {
        return ErrorDirectorioAnclado::EnlaceNoPermitido;
    }
    mapear_io(error)
}
