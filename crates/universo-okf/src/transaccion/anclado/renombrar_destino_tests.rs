use super::renombrar_test_support::{
    rename_backup_name, rename_identity, saltar_sin_file_id,
};
use super::renombrar::ErrorRenombrarAnclado;
use super::{DirectorioAnclado, UbicacionRaiz};
use std::ffi::OsStr;
use tempfile::tempdir;

#[test]
fn rename_sibling_mueve_root_y_stage_preservando_identidad_y_bytes() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("old.txt"), b"old bundle").unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    let backup_name = rename_backup_name(&root);
    let (stage_anchor, stage_name) = ancla.crear_stage_sibling("rename-success").unwrap();
    let stage_path = temp.path().join(&stage_name);
    std::fs::write(stage_path.join("new.txt"), b"new bundle").unwrap();
    let old_id = ancla.identidad_raiz().unwrap();
    let stage_id = stage_anchor.identidad_raiz().unwrap();
    let mut old = espacio
        .observar_directorio_sibling(OsStr::new("bundle"), old_id)
        .unwrap();
    let mut stage = espacio
        .observar_directorio_sibling(&stage_name, stage_id)
        .unwrap();
    let new_id = stage.identidad();

    espacio
        .renombrar_directorio(&mut old, &backup_name)
        .unwrap();
    espacio
        .renombrar_directorio(&mut stage, OsStr::new("bundle"))
        .unwrap();

    let installed =
        DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    let backup = temp.path().join(&backup_name);
    assert_eq!(installed.identidad_raiz(), Some(new_id));
    assert_eq!(old.nombre(), backup_name);
    assert_eq!(stage.nombre(), OsStr::new("bundle"));
    assert_eq!(old.identidad(), old_id);
    assert_eq!(
        DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&backup).unwrap())
            .unwrap()
            .identidad_raiz(),
        Some(old_id)
    );
    assert_eq!(std::fs::read(root.join("new.txt")).unwrap(), b"new bundle");
    assert_eq!(
        std::fs::read(backup.join("old.txt")).unwrap(),
        b"old bundle"
    );
}

#[test]
fn rename_sibling_no_reemplaza_destino_existente() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    let backup_name = rename_backup_name(&root);
    let backup = temp.path().join(&backup_name);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("source.txt"), b"source canary").unwrap();
    std::fs::create_dir(&backup).unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let source_identity = rename_identity(&root);
    let destination_identity = rename_identity(&backup);
    let mut source = espacio
        .observar_directorio_sibling(OsStr::new("bundle"), source_identity)
        .unwrap();
    let destination = espacio
        .observar_directorio_sibling(&backup_name, destination_identity)
        .unwrap();

    let error = espacio
        .renombrar_directorio(&mut source, &backup_name)
        .unwrap_err();

    assert_eq!(error, ErrorRenombrarAnclado::DestinoExistente);
    assert_eq!(source.identidad(), source_identity);
    assert_eq!(source.nombre(), OsStr::new("bundle"));
    assert_eq!(
        std::fs::read(root.join("source.txt")).unwrap(),
        b"source canary"
    );
    assert_eq!(destination.identidad(), destination_identity);
    assert!(std::fs::read_dir(&backup).unwrap().next().is_none());
}
