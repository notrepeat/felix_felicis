use super::{DirectorioAnclado, IdentidadDirectorio, UbicacionRaiz};
use std::ffi::OsString;
use std::path::Path;

pub(super) fn rename_backup_name(root: &Path) -> OsString {
    UbicacionRaiz::nueva(root)
        .unwrap()
        .backup()
        .as_path()
        .file_name()
        .unwrap()
        .to_os_string()
}

pub(super) fn rename_identity(ruta: &Path) -> IdentidadDirectorio {
    DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(ruta).unwrap())
        .unwrap()
        .identidad_raiz()
        .unwrap()
}

#[cfg(windows)]
pub(super) fn saltar_sin_file_id(ruta: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };

    let mut opciones = std::fs::OpenOptions::new();
    opciones
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let directorio = opciones
        .open(ruta)
        .expect("abrir parent para preflight FileId");
    match super::identidad_windows(&directorio) {
        Ok(_) => false,
        Err(error) if super::filesystem_sin_file_id(&error) => {
            eprintln!(
                "SKIP explícito: el filesystem no implementa FILE_ID_INFO (error {})",
                error.raw_os_error().unwrap_or_default()
            );
            true
        }
        Err(error) if super::es_sentinel_file_id(&error) => {
            eprintln!("SKIP explícito: FILE_ID_INFO devolvió un FileId sentinel");
            true
        }
        Err(error) => panic!("FILE_ID_INFO falló sin indicar falta de soporte: {error}"),
    }
}

#[cfg(unix)]
pub(super) fn saltar_sin_file_id(_ruta: &Path) -> bool {
    false
}
