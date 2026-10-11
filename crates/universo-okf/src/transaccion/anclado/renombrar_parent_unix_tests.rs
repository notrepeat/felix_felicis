use super::renombrar_test_support::rename_backup_name;
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
