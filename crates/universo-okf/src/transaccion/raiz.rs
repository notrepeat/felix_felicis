//! Identidad canónica validada de una raíz transaccional.
//!
//! Los artefactos sibling se derivan de esta identidad para que distintas
//! rutas al mismo directorio compartan nombres y ubicación.

use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::path::{Component, Path, PathBuf};

/// Fallo de inspección de una raíz, sin conservar detalles sensibles del SO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorRaiz {
    Io,
    EnlaceNoPermitido,
    TipoNoAdmitido,
}

/// Raíz existente, con todos sus componentes inspeccionados sin seguir enlaces
/// y canonizada una vez.
pub(crate) struct RaizValidada {
    ruta: PathBuf,
    parent: PathBuf,
    basename: OsString,
}

impl RaizValidada {
    /// Inspecciona cada componente sin seguir enlaces y conserva la identidad
    /// canónica para derivar todos los artefactos sibling.
    pub(crate) fn nueva(ruta: &Path) -> Result<Self, ErrorRaiz> {
        let absoluta = ruta_absoluta(ruta)?;
        validar_componentes_sin_enlaces(&absoluta)?;

        // Canonizar la ruta original, no una reducción léxica: el sistema de
        // archivos define la semántica de `..`. La inspección rechaza los
        // symlink/reparse observados; no elimina reemplazos concurrentes.
        let canonica = fs::canonicalize(&absoluta).map_err(|_| ErrorRaiz::Io)?;
        validar_directorio_final(&canonica)?;

        let parent = canonica
            .parent()
            .ok_or(ErrorRaiz::TipoNoAdmitido)?
            .to_path_buf();
        let basename = canonica
            .file_name()
            .ok_or(ErrorRaiz::TipoNoAdmitido)?
            .to_os_string();

        Ok(Self {
            ruta: canonica,
            parent,
            basename,
        })
    }

    /// Ruta canónica validada.
    pub(crate) fn ruta(&self) -> &Path {
        &self.ruta
    }

    /// Path sibling estable del lock reutilizable.
    pub(crate) fn ruta_lock(&self) -> PathBuf {
        self.ruta_sibling(".felix.lock")
    }

    fn ruta_sibling(&self, sufijo: &str) -> PathBuf {
        let mut nombre = OsString::from(".");
        nombre.push(&self.basename);
        nombre.push(sufijo);
        self.parent.join(nombre)
    }
}

fn ruta_absoluta(ruta: &Path) -> Result<PathBuf, ErrorRaiz> {
    if ruta.as_os_str().is_empty() {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }

    let ruta = if ruta.is_absolute() {
        ruta.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| ErrorRaiz::Io)?
            .join(ruta)
    };

    if !ruta.is_absolute() {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }

    Ok(ruta)
}

/// Inspecciona cada prefijo antes de resolver el siguiente componente. En
/// particular, un symlink antes de `..` se rechaza antes de que `..` pueda
/// cambiar su significado. El recorrido léxico solo se usa para inspeccionar
/// prefijos ya comprobados; la ruta final se canoniza con semántica del SO.
fn validar_componentes_sin_enlaces(ruta: &Path) -> Result<(), ErrorRaiz> {
    let mut recorrido = PathBuf::new();
    let mut profundidad = 0usize;

    for componente in ruta.components() {
        match componente {
            Component::Prefix(prefijo) => recorrido.push(prefijo.as_os_str()),
            Component::RootDir => {
                recorrido.push(componente.as_os_str());
                validar_directorio_sin_enlace(&recorrido)?;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if profundidad > 0 {
                    recorrido.pop();
                    profundidad -= 1;
                }
            }
            Component::Normal(nombre) => {
                recorrido.push(nombre);
                validar_directorio_sin_enlace(&recorrido)?;
                profundidad += 1;
            }
        }
    }
    validar_directorio_sin_enlace(&recorrido)
}

fn validar_directorio_sin_enlace(ruta: &Path) -> Result<(), ErrorRaiz> {
    let metadata = fs::symlink_metadata(ruta).map_err(|_| ErrorRaiz::Io)?;
    if metadata_es_enlace_o_reparse(&metadata) {
        return Err(ErrorRaiz::EnlaceNoPermitido);
    }
    if !metadata.is_dir() {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }
    Ok(())
}

fn validar_directorio_final(ruta: &Path) -> Result<(), ErrorRaiz> {
    validar_directorio_sin_enlace(ruta)
}

pub(crate) fn metadata_es_enlace_o_reparse(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn canoniza_alias_lexico_y_deriva_lock_sibling_estable() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        fs::create_dir(&raiz).unwrap();

        let directa = RaizValidada::nueva(&raiz).unwrap();
        let alias = RaizValidada::nueva(&raiz.join(".")).unwrap();

        assert_eq!(directa.ruta(), alias.ruta());
        assert_eq!(directa.ruta_lock(), alias.ruta_lock());
        assert_eq!(
            directa.ruta_lock().file_name().unwrap(),
            OsString::from(".bundle.felix.lock")
        );
    }

    #[test]
    fn alias_parent_converge_sin_enlaces() {
        let temp = tempdir().unwrap();
        let desvio = temp.path().join("desvio");
        let raiz = temp.path().join("bundle");
        fs::create_dir(&desvio).unwrap();
        fs::create_dir(&raiz).unwrap();

        let directa = RaizValidada::nueva(&raiz).unwrap();
        let alias = RaizValidada::nueva(&desvio.join("..").join("bundle")).unwrap();

        assert_eq!(directa.ruta(), alias.ruta());
        assert_eq!(directa.ruta_lock(), alias.ruta_lock());
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_raiz_symlink_sin_seguirla() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let destino = temp.path().join("destino");
        let alias = temp.path().join("alias");
        fs::create_dir(&destino).unwrap();
        symlink(&destino, &alias).unwrap();

        assert!(matches!(
            RaizValidada::nueva(&alias),
            Err(ErrorRaiz::EnlaceNoPermitido)
        ));
    }
}
