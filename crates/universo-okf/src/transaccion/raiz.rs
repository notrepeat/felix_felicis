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

/// Raíz existente, no enlazada en su componente final y canonizada una vez.
pub(crate) struct RaizValidada {
    ruta: PathBuf,
    parent: PathBuf,
    basename: OsString,
}

impl RaizValidada {
    /// Inspecciona la entrada sin seguir el componente final y conserva su
    /// identidad canónica para derivar todos los artefactos sibling.
    pub(crate) fn nueva(ruta: &Path) -> Result<Self, ErrorRaiz> {
        let absoluta = ruta_absoluta_lexica(ruta)?;
        validar_directorio_final(&absoluta)?;

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

fn ruta_absoluta_lexica(ruta: &Path) -> Result<PathBuf, ErrorRaiz> {
    let ruta = if ruta.is_absolute() {
        ruta.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| ErrorRaiz::Io)?
            .join(ruta)
    };

    let mut normalizada = PathBuf::new();
    for componente in ruta.components() {
        match componente {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalizada.pop() {
                    return Err(ErrorRaiz::TipoNoAdmitido);
                }
            }
            otro => normalizada.push(otro.as_os_str()),
        }
    }
    Ok(normalizada)
}

fn validar_directorio_final(ruta: &Path) -> Result<(), ErrorRaiz> {
    let metadata = fs::symlink_metadata(ruta).map_err(|_| ErrorRaiz::Io)?;
    if metadata_es_enlace_o_reparse(&metadata) {
        return Err(ErrorRaiz::EnlaceNoPermitido);
    }
    if !metadata.is_dir() {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }
    Ok(())
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
