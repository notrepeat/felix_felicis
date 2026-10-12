use std::ffi::OsStr;
use std::fs::File;

use fs_at::{OpenOptions as OpenOptionsAt, OpenOptionsWriteMode};

use super::abrir::abrir_entrada_relativa;
use super::{
    DirectorioAnclado, EntradaLecturaAnclada, ErrorDirectorioAnclado, TipoEntradaAnclada,
    clasificar_entrada_sin_seguir, clonar_handle, mapear_error_creacion, mapear_io,
    metadata_directorio, metadata_es_reparse, validar_componente,
};
use crate::transaccion::raiz::UbicacionRaiz;

impl DirectorioAnclado {
    /// Opens one allowlisted root/backup/stage directory from the anchored
    /// parent without write-capable handles. Only an anchored `NotFound`
    /// result is represented as `None`; every other opening failure is kept.
    pub(crate) fn abrir_artifacto_sibling_directorio_lectura(
        &self,
        ubicacion: &UbicacionRaiz,
        componente: &OsStr,
    ) -> Result<Option<DirectorioAnclado>, ErrorDirectorioAnclado> {
        validar_componente(componente)?;
        let basename = ubicacion
            .ruta()
            .file_name()
            .ok_or(ErrorDirectorioAnclado::ComponenteInvalido)?;
        if self.basename != basename || !ubicacion.permite_rename_sibling(componente) {
            return Err(ErrorDirectorioAnclado::ComponenteInvalido);
        }

        let identidad_parent = metadata_directorio(&self.parent)?;
        if identidad_parent != self.identidad_parent {
            return Err(ErrorDirectorioAnclado::Io);
        }

        let raiz = match super::abrir_directorio_hijo_lectura(&self.parent, componente) {
            Ok(raiz) => raiz,
            Err(ErrorDirectorioAnclado::EntradaAusente) => return Ok(None),
            Err(error) => return Err(error),
        };
        let identidad_raiz = metadata_directorio(&raiz)?;

        Ok(Some(DirectorioAnclado {
            parent: clonar_handle(&self.parent)?,
            basename: componente.to_os_string(),
            raiz: Some(raiz),
            identidad_parent,
            identidad_raiz: Some(identidad_raiz),
            permite_escritura: false,
            puede_crear_stage_sibling: false,
        }))
    }

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

#[cfg(test)]
mod sibling_read_only_tests {
    use super::*;
    use crate::transaccion::raiz::UbicacionRaiz;
    use std::fs;
    use tempfile::tempdir;

    fn parent(location: &UbicacionRaiz) -> DirectorioAnclado {
        DirectorioAnclado::parent_solo_lectura_desde_ubicacion(location).unwrap()
    }

    fn open(
        parent: &DirectorioAnclado,
        location: &UbicacionRaiz,
        name: &OsStr,
    ) -> Result<Option<DirectorioAnclado>, ErrorDirectorioAnclado> {
        parent.abrir_artifacto_sibling_directorio_lectura(location, name)
    }

    #[test]
    fn abre_solo_root_backup_y_stage_desde_el_parent_anclado() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        fs::create_dir(&root).unwrap();
        let location = UbicacionRaiz::nueva(&root).unwrap();
        let backup_name = location.backup().as_path().file_name().unwrap().to_owned();
        let stage_name = location
            .stage("0123456789abcdef0123456789abcdef")
            .unwrap()
            .as_path()
            .file_name()
            .unwrap()
            .to_owned();
        fs::create_dir(temp.path().join(&backup_name)).unwrap();
        fs::create_dir(temp.path().join(&stage_name)).unwrap();
        let root_name = location.ruta().file_name().unwrap().to_owned();
        let parent = parent(&location);

        let root_opened = open(&parent, &location, &root_name).unwrap().unwrap();
        let backup_opened = open(&parent, &location, &backup_name).unwrap().unwrap();
        let stage_opened = open(&parent, &location, &stage_name).unwrap().unwrap();

        assert!(root_opened.identidad_raiz().is_some());
        assert!(backup_opened.identidad_raiz().is_some());
        assert!(stage_opened.identidad_raiz().is_some());
        assert!(!root_opened.permite_escritura);
        assert!(!backup_opened.permite_escritura);
        assert!(!stage_opened.permite_escritura);
    }

    #[test]
    fn solo_el_open_anclado_con_not_found_demuestra_ausencia() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        let location = UbicacionRaiz::nueva(&root).unwrap();
        let parent = parent(&location);
        let root_name = location.ruta().file_name().unwrap();

        assert!(open(&parent, &location, root_name).unwrap().is_none());

        fs::write(&root, b"regular-file-canary").unwrap();
        assert!(matches!(
            open(&parent, &location, root_name),
            Err(ErrorDirectorioAnclado::TipoNoAdmitido)
        ));
        assert_eq!(fs::read(root).unwrap(), b"regular-file-canary");
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_sibling_symlink_sin_seguirlo() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        let target = temp.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("canary"), b"untouched").unwrap();
        let location = UbicacionRaiz::nueva(&root).unwrap();
        let backup = location.backup();
        let backup_name = backup.as_path().file_name().unwrap();
        std::os::unix::fs::symlink(&target, backup.as_path()).unwrap();
        let parent = parent(&location);

        assert!(open(&parent, &location, backup_name).is_err());
        assert_eq!(fs::read(target.join("canary")).unwrap(), b"untouched");
    }

    #[cfg(windows)]
    #[test]
    fn rechaza_sibling_junction_sin_seguirla() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        let target = temp.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("canary"), b"untouched").unwrap();
        let location = UbicacionRaiz::nueva(&root).unwrap();
        let backup = location.backup();
        let backup_name = backup.as_path().file_name().unwrap();
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(backup.as_path())
            .arg(&target)
            .output()
            .expect("cmd crea junctions");
        assert!(
            output.status.success(),
            "no se pudo crear junction: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let parent = parent(&location);

        assert!(open(&parent, &location, backup_name).is_err());
        assert_eq!(fs::read(target.join("canary")).unwrap(), b"untouched");
    }

    #[test]
    fn rechaza_nombres_fuera_de_la_allowlist() {
        let temp = tempdir().unwrap();
        let location = UbicacionRaiz::nueva(&temp.path().join("bundle")).unwrap();
        let parent = parent(&location);

        assert!(matches!(
            open(&parent, &location, OsStr::new("unrelated")),
            Err(ErrorDirectorioAnclado::ComponenteInvalido)
        ));
    }
}
