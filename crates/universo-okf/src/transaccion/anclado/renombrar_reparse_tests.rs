use super::renombrar::ErrorRenombrarAnclado;
use super::renombrar_test_support::{rename_backup_name, saltar_sin_file_id};
use super::{DirectorioAnclado, UbicacionRaiz};
use tempfile::tempdir;

#[cfg(windows)]
fn crear_rename_junction(link: &std::path::Path, target: &std::path::Path) {
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

#[cfg(unix)]
fn crear_rename_junction(link: &std::path::Path, target: &std::path::Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[test]
fn rename_sibling_rechaza_reparse_source_sin_seguirlo() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("canary.txt"), b"outside canary").unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    let (stage_anchor, component) = ancla.crear_stage_sibling("reparse-source").unwrap();
    let source_path = temp.path().join(component);
    let source_identity = stage_anchor.identidad_raiz().unwrap();
    let mut source = espacio
        .observar_directorio_sibling(source_path.file_name().unwrap(), source_identity)
        .unwrap();
    let saved_path = temp.path().join("saved-source");
    std::fs::write(source_path.join("source.txt"), b"source canary").unwrap();
    std::fs::rename(&source_path, &saved_path).unwrap();
    crear_rename_junction(&source_path, &outside);

    let backup_name = rename_backup_name(&root);
    let error = espacio
        .renombrar_directorio(&mut source, &backup_name)
        .unwrap_err();

    assert_eq!(error, ErrorRenombrarAnclado::EnlaceNoPermitido);
    assert_eq!(source.nombre(), source_path.file_name().unwrap());
    assert!(
        std::fs::symlink_metadata(&source_path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read(saved_path.join("source.txt")).unwrap(),
        b"source canary"
    );
    assert_eq!(
        std::fs::read(outside.join("canary.txt")).unwrap(),
        b"outside canary"
    );
    assert!(!temp.path().join(&backup_name).exists());
}

#[test]
fn reabrir_backup_rechaza_reparse_y_preserva_canary_externo() {
    use super::renombrar::ErrorRenombrarAnclado;
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    let outside = temp.path().join("outside");
    let backup_name = rename_backup_name(&root);
    let backup_path = temp.path().join(&backup_name);
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("canary.txt"), b"outside canary").unwrap();
    crear_rename_junction(&backup_path, &outside);

    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let espacio = lock.espacio_transaccional_anclado().unwrap();
    let error = espacio
        .reabrir_backup_sibling(|_| true, |_| true)
        .map(|_| ())
        .unwrap_err();

    #[cfg(unix)]
    assert_eq!(error, ErrorRenombrarAnclado::TipoNoAdmitido);
    #[cfg(windows)]
    assert_eq!(error, ErrorRenombrarAnclado::EnlaceNoPermitido);
    assert_eq!(
        std::fs::read(outside.join("canary.txt")).unwrap(),
        b"outside canary"
    );
    let metadata = std::fs::symlink_metadata(&backup_path).unwrap();
    #[cfg(unix)]
    assert!(metadata.file_type().is_symlink());
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

        assert_ne!(metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT, 0);
    }
    assert!(!root.join("canary.txt").exists());
}
