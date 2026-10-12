use super::renombrar_test_support::{rename_backup_name, rename_identity};
use super::{DirectorioAnclado, UbicacionRaiz};
use std::os::unix::fs::symlink;
use tempfile::tempdir;

#[test]
fn rename_sibling_usa_parent_anclado_tras_reemplazo_por_symlink() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let outside = temp.path().join("outside");
    let root = parent.join("bundle");
    std::fs::create_dir(&parent).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::create_dir(&root).unwrap();
    let backup_name = rename_backup_name(&root);
    std::fs::write(outside.join("canary.txt"), b"outside canary").unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    let (stage_anchor, component) = ancla.crear_stage_sibling("parent-replaced").unwrap();
    let mut source = espacio
        .observar_directorio_sibling(&component, stage_anchor.identidad_raiz().unwrap())
        .unwrap();
    let source_name = component.to_string_lossy().into_owned();
    std::fs::create_dir(outside.join(&source_name)).unwrap();
    std::fs::write(
        outside.join(&source_name).join("canary.txt"),
        b"source-name canary",
    )
    .unwrap();
    std::fs::rename(&parent, &moved_parent).unwrap();
    symlink(&outside, &parent).unwrap();

    espacio
        .renombrar_directorio(&mut source, &backup_name)
        .unwrap();

    assert!(moved_parent.join(&backup_name).is_dir());
    assert!(moved_parent.join("bundle").is_dir());
    assert!(!outside.join(&backup_name).exists());
    assert_eq!(
        std::fs::read(outside.join(&source_name).join("canary.txt")).unwrap(),
        b"source-name canary"
    );
    assert!(!moved_parent.join(&source_name).exists());
}

#[test]
fn reabrir_backup_sibling_usa_parent_anclado_tras_reemplazo_por_symlink() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let outside = temp.path().join("outside");
    let root = parent.join("bundle");
    std::fs::create_dir(&parent).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("old.txt"), b"original backup canary").unwrap();

    let backup_name = rename_backup_name(&root);
    let backup = parent.join(&backup_name);
    let parent_identity = rename_identity(&parent);
    let old_identity = rename_identity(&root);
    std::fs::rename(&root, &backup).unwrap();

    let decoy = outside.join(&backup_name);
    std::fs::create_dir(&decoy).unwrap();
    std::fs::write(decoy.join("old.txt"), b"external backup decoy").unwrap();
    let decoy_identity = rename_identity(&decoy);
    std::fs::write(outside.join("canary.txt"), b"outside canary").unwrap();

    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    std::fs::rename(&parent, &moved_parent).unwrap();
    symlink(&outside, &parent).unwrap();

    let mut old = espacio
        .reabrir_backup_sibling(
            |identidad| identidad == parent_identity,
            |identidad| identidad == old_identity,
        )
        .unwrap();
    espacio
        .renombrar_directorio(&mut old, std::ffi::OsStr::new("bundle"))
        .unwrap();

    let restored = moved_parent.join("bundle");
    assert_eq!(old.nombre(), std::ffi::OsStr::new("bundle"));
    assert_eq!(rename_identity(&restored), old_identity);
    assert_eq!(
        std::fs::read(restored.join("old.txt")).unwrap(),
        b"original backup canary"
    );
    assert!(!moved_parent.join(&backup_name).exists());
    assert_eq!(rename_identity(&decoy), decoy_identity);
    assert_eq!(
        std::fs::read(decoy.join("old.txt")).unwrap(),
        b"external backup decoy"
    );
    assert_eq!(
        std::fs::read(outside.join("canary.txt")).unwrap(),
        b"outside canary"
    );
    assert!(!outside.join("bundle").exists());
}
