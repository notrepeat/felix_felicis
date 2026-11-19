use super::{DirectorioAnclado, IdentidadDirectorio, UbicacionRaiz};
use crate::transaccion::lock::LockEscritura;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use tempfile::tempdir;

fn identidad_de_ruta(ruta: &Path) -> IdentidadDirectorio {
    DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(ruta).unwrap())
        .unwrap()
        .identidad_raiz()
        .unwrap()
}

fn backup_name(root: &Path) -> OsString {
    UbicacionRaiz::nueva(root)
        .unwrap()
        .backup()
        .as_path()
        .file_name()
        .unwrap()
        .to_os_string()
}

#[test]
fn rename_sibling_success_preserva_identidad_y_bytes() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    let backup = backup_name(&root);
    let backup_path = temp.path().join(&backup);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("payload"), b"same directory, same bytes").unwrap();
    let identity = identidad_de_ruta(&root);
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let mut source = espacio
        .observar_directorio_sibling(OsStr::new("bundle"), identity)
        .unwrap();

    espacio.renombrar_directorio(&mut source, &backup).unwrap();

    assert_eq!(source.nombre(), backup);
    assert_eq!(source.identidad(), identity);
    assert!(!root.exists());
    assert_eq!(identidad_de_ruta(&backup_path), identity);
    assert_eq!(
        std::fs::read(backup_path.join("payload")).unwrap(),
        b"same directory, same bytes"
    );
}
