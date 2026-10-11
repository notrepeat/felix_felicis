//! Lectura raw del árbol del bundle sin seguir enlaces observables.
//!
//! El inventario incluye archivos regulares y directorios, incluidos los
//! vacíos. La identidad de ruta es UTF-8 exacto, separador `/`, sin casefold
//! ni normalización Unicode. Staging añade el preflight bajo lock antes de
//! usar esta API para apply.

use std::fs::{self, File, Metadata, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use super::snapshot::{EntradaSnapshot, ErrorSnapshot};

/// Lee una vez cada archivo y devuelve el inventario ordenado por bytes UTF-8.
pub fn inventariar_root(raiz: &Path) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot> {
    inventariar_root_con_lector(raiz, leer_archivo_regular)
}

fn inventariar_root_con_lector<F>(
    raiz: &Path,
    mut leer: F,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot>
where
    F: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let raiz = ruta_absoluta_lexica(raiz)?;
    validar_directorio_sin_enlaces(&raiz)?;
    let mut pendientes = vec![(raiz, String::new())];
    let mut entradas = Vec::new();

    while let Some((directorio, prefijo)) = pendientes.pop() {
        validar_directorio_sin_enlaces(&directorio)?;
        let leidos = fs::read_dir(&directorio).map_err(|_| ErrorSnapshot::Io)?;
        for hijo in leidos {
            let hijo = hijo.map_err(|_| ErrorSnapshot::Io)?;
            let nombre = hijo
                .file_name()
                .into_string()
                .map_err(|_| ErrorSnapshot::RutaNoUtf8)?;
            if nombre.contains('\\') {
                return Err(ErrorSnapshot::RutaNoRepresentable);
            }
            let ruta = if prefijo.is_empty() {
                nombre
            } else {
                format!("{prefijo}/{nombre}")
            };
            let camino = hijo.path();
            let metadata = fs::symlink_metadata(&camino).map_err(|_| ErrorSnapshot::Io)?;
            if es_enlace_o_reparse(&metadata) {
                return Err(ErrorSnapshot::EnlaceNoPermitido);
            }
            if metadata.is_dir() {
                entradas.push(EntradaSnapshot::directorio(ruta.clone()));
                pendientes.push((camino, ruta));
            } else if metadata.is_file() {
                entradas.push(EntradaSnapshot::archivo(ruta, leer(&camino)?));
            } else {
                return Err(ErrorSnapshot::TipoNoAdmitido);
            }
        }
    }

    entradas.sort_by(|a, b| a.ruta().as_bytes().cmp(b.ruta().as_bytes()));
    Ok(entradas)
}

pub(crate) fn ruta_absoluta_lexica(ruta: &Path) -> Result<PathBuf, ErrorSnapshot> {
    let ruta = if ruta.is_absolute() {
        ruta.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| ErrorSnapshot::Io)?
            .join(ruta)
    };
    let mut normalizada = PathBuf::new();
    for componente in ruta.components() {
        match componente {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalizada.pop() {
                    return Err(ErrorSnapshot::RaizInvalida);
                }
            }
            otro => normalizada.push(otro.as_os_str()),
        }
    }
    Ok(normalizada)
}

pub(crate) fn validar_directorio_sin_enlaces(ruta: &Path) -> Result<(), ErrorSnapshot> {
    let metadata = validar_componentes_sin_enlaces(ruta)?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(ErrorSnapshot::RaizInvalida)
    }
}

fn validar_componentes_sin_enlaces(ruta: &Path) -> Result<Metadata, ErrorSnapshot> {
    let mut ancestros: Vec<_> = ruta.ancestors().collect();
    ancestros.reverse();
    let mut final_metadata = None;
    for componente in ancestros {
        let metadata = fs::symlink_metadata(componente).map_err(|_| ErrorSnapshot::Io)?;
        if es_enlace_o_reparse(&metadata) {
            return Err(ErrorSnapshot::EnlaceNoPermitido);
        }
        if componente != ruta && !metadata.is_dir() {
            return Err(ErrorSnapshot::TipoNoAdmitido);
        }
        if componente == ruta {
            final_metadata = Some(metadata);
        }
    }
    final_metadata.ok_or(ErrorSnapshot::Io)
}

fn es_enlace_o_reparse(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub(crate) fn leer_archivo_regular(ruta: &Path) -> Result<Vec<u8>, ErrorSnapshot> {
    let mut opciones = OpenOptions::new();
    opciones.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opciones.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        opciones.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut archivo: File = opciones.open(ruta).map_err(|_| ErrorSnapshot::Io)?;
    let metadata = archivo.metadata().map_err(|_| ErrorSnapshot::Io)?;
    if es_enlace_o_reparse(&metadata) {
        return Err(ErrorSnapshot::EnlaceNoPermitido);
    }
    if !metadata.is_file() {
        return Err(ErrorSnapshot::TipoNoAdmitido);
    }
    let mut bytes = Vec::new();
    archivo
        .read_to_end(&mut bytes)
        .map_err(|_| ErrorSnapshot::Io)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::snapshot::TipoEntradaSnapshot;
    use std::collections::HashMap;

    #[test]
    fn incluye_archivos_binarios_vacios_y_directorios_vacios_en_orden() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("vacio")).unwrap();
        fs::create_dir(root.path().join("a")).unwrap();
        fs::write(root.path().join("a").join("é.bin"), [0, 255, 1]).unwrap();
        fs::write(root.path().join("0.empty"), []).unwrap();

        let entries = inventariar_root(root.path()).unwrap();
        let paths: Vec<_> = entries.iter().map(EntradaSnapshot::ruta).collect();
        assert_eq!(paths, ["0.empty", "a", "a/é.bin", "vacio"]);
        assert_eq!(entries[0].bytes(), Some([].as_slice()));
        assert_eq!(entries[2].bytes(), Some([0, 255, 1].as_slice()));
        assert_eq!(entries[3].tipo(), TipoEntradaSnapshot::Directorio);
    }

    #[test]
    fn el_lector_recibe_una_vez_cada_archivo_del_root() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("sub")).unwrap();
        fs::write(root.path().join("sub/a.md"), b"a").unwrap();
        fs::write(root.path().join("sub/b.bin"), b"b").unwrap();
        let mut lecturas = HashMap::<PathBuf, usize>::new();

        let entries = inventariar_root_con_lector(root.path(), |path| {
            *lecturas.entry(path.to_path_buf()).or_default() += 1;
            leer_archivo_regular(path)
        })
        .unwrap();

        assert_eq!(entries.iter().filter(|e| e.bytes().is_some()).count(), 2);
        assert_eq!(lecturas.len(), 2);
        assert!(lecturas.values().all(|&count| count == 1));
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_symlink_sin_leer_su_destino() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), b"PRIVATE-CANARY").unwrap();
        symlink(outside.path(), root.path().join("link")).unwrap();

        let error = inventariar_root(root.path()).unwrap_err();
        assert_eq!(error, ErrorSnapshot::EnlaceNoPermitido);
        assert!(!error.to_string().contains("PRIVATE-CANARY"));
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_nombres_no_utf8_sin_normalizarlos_con_replacement() {
        use std::os::unix::ffi::OsStrExt;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(std::ffi::OsStr::from_bytes(b"bad-\xff"));
        fs::write(path, b"raw").unwrap();
        assert_eq!(
            inventariar_root(root.path()).unwrap_err(),
            ErrorSnapshot::RutaNoUtf8
        );
    }
}
