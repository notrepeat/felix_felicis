use super::renombrar_test_support::{rename_backup_name, saltar_sin_file_id};
use super::renombrar::ErrorRenombrarAnclado;
use super::{DirectorioAnclado, IdentidadDirectorio, UbicacionRaiz};
use std::path::Path;
use tempfile::tempdir;

fn identidad_de_ruta_windows(ruta: &Path) -> IdentidadDirectorio {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };

    let mut opciones = std::fs::OpenOptions::new();
    opciones
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let directorio = opciones.open(ruta).unwrap();
    super::identidad_windows(&directorio).unwrap()
}

fn crear_rename_junction(link: &Path, target: &Path) {
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .expect("cmd crea junctions");
    assert!(
        output.status.success(),
        "no se pudo crear junction: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn rename_sibling_parent_junction_no_desvia_la_operacion_anclada() {
    use crate::transaccion::lock::LockEscritura;
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let parent = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let outside = temp.path().join("outside");
    let root = parent.join("bundle");
    std::fs::create_dir(&parent).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::create_dir(&root).unwrap();
    let backup_name = rename_backup_name(&root);
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    let (stage_anchor, component) = ancla.crear_stage_sibling("parent-junction").unwrap();
    let mut source = espacio
        .observar_directorio_sibling(&component, stage_anchor.identidad_raiz().unwrap())
        .unwrap();
    let source_name = component.to_string_lossy().into_owned();
    let root_identity = ancla.identidad_raiz().unwrap();
    let source_identity = source.identidad();
    std::fs::write(root.join("canary.txt"), b"root canary").unwrap();
    std::fs::write(
        parent.join(&source_name).join("canary.txt"),
        b"stage canary",
    )
    .unwrap();
    let decoy = outside.join(&source_name);
    std::fs::create_dir(&decoy).unwrap();
    std::fs::write(decoy.join("canary.txt"), b"outside decoy").unwrap();
    if let Err(error) = std::fs::rename(&parent, &moved_parent) {
        assert!(
            matches!(error.raw_os_error(), Some(5) | Some(32)),
            "{error}"
        );
        let parent_metadata = std::fs::symlink_metadata(&parent).unwrap();
        assert_eq!(
            parent_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT,
            0,
            "parent remained a real directory after the blocked move"
        );
        assert_eq!(identidad_de_ruta_windows(&root), root_identity);
        assert_eq!(
            identidad_de_ruta_windows(&parent.join(&source_name)),
            source_identity
        );
        assert_eq!(
            std::fs::read(root.join("canary.txt")).unwrap(),
            b"root canary"
        );
        assert_eq!(
            std::fs::read(parent.join(&source_name).join("canary.txt")).unwrap(),
            b"stage canary"
        );
        assert_eq!(
            std::fs::read(decoy.join("canary.txt")).unwrap(),
            b"outside decoy"
        );
        assert!(!moved_parent.exists());
        assert!(!outside.join(&backup_name).exists());
        eprintln!(
            "SKIP explícito: Windows bloqueó el renombre del parent con error {}",
            error.raw_os_error().unwrap_or_default()
        );
        return;
    }
    crear_rename_junction(&parent, &outside);

    espacio
        .renombrar_directorio(&mut source, &backup_name)
        .unwrap();

    assert_eq!(source.nombre(), backup_name.as_os_str());
    assert!(moved_parent.join(&backup_name).is_dir());
    assert!(moved_parent.join("bundle").is_dir());
    assert_eq!(
        identidad_de_ruta_windows(&moved_parent.join("bundle")),
        root_identity
    );
    assert_eq!(
        identidad_de_ruta_windows(&moved_parent.join(&backup_name)),
        source_identity
    );
    assert_eq!(
        std::fs::read(moved_parent.join("bundle/canary.txt")).unwrap(),
        b"root canary"
    );
    assert_eq!(
        std::fs::read(moved_parent.join(&backup_name).join("canary.txt")).unwrap(),
        b"stage canary"
    );
    assert!(decoy.is_dir());
    assert_eq!(
        std::fs::read(decoy.join("canary.txt")).unwrap(),
        b"outside decoy"
    );
    assert!(!outside.join(&backup_name).exists());
}

#[test]
fn rename_sibling_handle_externo_que_nega_share_delete_bloquea_y_luego_permite() {
    use crate::transaccion::lock::LockEscritura;
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    std::fs::create_dir(&root).unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    let (stage_anchor, component) = ancla.crear_stage_sibling("share-delete").unwrap();
    let source_path = temp.path().join(&component);
    let mut source = espacio
        .observar_directorio_sibling(&component, stage_anchor.identidad_raiz().unwrap())
        .unwrap();

    let mut opciones = std::fs::OpenOptions::new();
    opciones
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let externo = opciones.open(&source_path).unwrap();
    let backup_name = rename_backup_name(&root);
    let error = espacio
        .renombrar_directorio(&mut source, &backup_name)
        .unwrap_err();
    assert_eq!(error, ErrorRenombrarAnclado::Io);
    assert_eq!(source.nombre(), component.as_os_str());
    assert!(source_path.is_dir());
    drop(externo);

    espacio
        .renombrar_directorio(&mut source, &backup_name)
        .unwrap();
    assert_eq!(source.nombre(), backup_name.as_os_str());
    assert!(temp.path().join(&backup_name).is_dir());
}
