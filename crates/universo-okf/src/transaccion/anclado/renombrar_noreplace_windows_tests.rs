use super::{CausaRenombrarNoReemplazo, renombrar_windows};
use fs_at::OpenOptions as OpenOptionsAt;
use fs_at::os::windows::OpenOptionsExt;
use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt as StdOpenOptionsExt;
use std::path::Path;
use tempfile::tempdir;
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ADD_FILE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_TRAVERSE,
};

fn abrir_parent(ruta: &Path) -> File {
    let mut opciones = OpenOptions::new();
    opciones
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .access_mode(FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_ADD_FILE);
    opciones.open(ruta).expect("open test parent")
}

fn abrir_fuente(parent: &File, nombre: &str) -> File {
    let mut opciones = OpenOptionsAt::default();
    opciones
        .desired_access(FILE_READ_ATTRIBUTES | DELETE)
        .follow(false);
    opciones
        .open_at(parent, Path::new(nombre))
        .expect("open test source for rename")
}

#[test]
fn archivo_regular_no_reemplaza_destino_y_luego_puede_moverse() {
    let temp = tempdir().unwrap();
    let source_path = temp.path().join("source.bin");
    let destination_path = temp.path().join("destination.bin");
    std::fs::write(&source_path, b"source canary").unwrap();
    std::fs::write(&destination_path, b"destination canary").unwrap();
    let parent = abrir_parent(temp.path());
    let source = abrir_fuente(&parent, "source.bin");

    let error = renombrar_windows(&parent, &source, std::ffi::OsStr::new("destination.bin"))
        .expect_err("the no-replace syscall must reject an existing file");
    assert!(matches!(
        error.causa(),
        CausaRenombrarNoReemplazo::OperacionAmbigua
    ));
    assert_eq!(
        format!("{error:?}"),
        "ErrorRenombrarNoReemplazo { .. }",
        "native statuses stay redacted from Debug"
    );
    assert_eq!(std::fs::read(&source_path).unwrap(), b"source canary");
    assert_eq!(
        std::fs::read(&destination_path).unwrap(),
        b"destination canary"
    );

    drop(source);
    std::fs::remove_file(&destination_path).unwrap();
    let source = abrir_fuente(&parent, "source.bin");
    renombrar_windows(&parent, &source, std::ffi::OsStr::new("destination.bin"))
        .expect("rename into an absent sibling");
    drop(source);
    assert!(!source_path.exists());
    assert_eq!(std::fs::read(&destination_path).unwrap(), b"source canary");
}
