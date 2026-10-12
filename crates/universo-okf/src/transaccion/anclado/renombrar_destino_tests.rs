use super::renombrar::ErrorRenombrarAnclado;
use super::renombrar_test_support::{rename_backup_name, rename_identity, saltar_sin_file_id};
use super::{DirectorioAnclado, UbicacionRaiz};
use std::ffi::OsStr;
use tempfile::tempdir;

#[test]
fn reabrir_backup_y_restaurar_root_ausente_preserva_identidad_y_contenido() {
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("old.txt"), b"old bundle canary").unwrap();
    let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
    let backup_path = ubicacion.backup().as_path().to_path_buf();
    let backup_name = rename_backup_name(&root);
    let parent_identity = rename_identity(temp.path());
    let old_identity = rename_identity(&root);
    std::fs::rename(&root, &backup_path).unwrap();

    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut espacio = lock.espacio_transaccional_anclado().unwrap();
    let mut old = espacio
        .reabrir_backup_sibling(
            |identidad| identidad == parent_identity,
            |identidad| identidad == old_identity,
        )
        .unwrap();
    assert_eq!(old.nombre(), backup_name.as_os_str());

    espacio
        .renombrar_directorio(&mut old, OsStr::new("bundle"))
        .unwrap();

    assert_eq!(old.nombre(), OsStr::new("bundle"));
    assert_eq!(old.identidad(), old_identity);
    assert_eq!(rename_identity(&root), old_identity);
    assert_eq!(
        std::fs::read(root.join("old.txt")).unwrap(),
        b"old bundle canary"
    );
    assert!(!backup_path.exists());
}

#[test]
fn reabrir_backup_rechaza_predicados_parent_y_old_falsos() {
    use super::renombrar::ErrorRenombrarAnclado;
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    let backup_name = rename_backup_name(&root);
    let backup_path = temp.path().join(&backup_name);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("root.txt"), b"root canary").unwrap();
    std::fs::create_dir(&backup_path).unwrap();
    std::fs::write(backup_path.join("old.txt"), b"backup canary").unwrap();
    let parent_identity = rename_identity(temp.path());

    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let espacio = lock.espacio_transaccional_anclado().unwrap();
    let error_parent = espacio
        .reabrir_backup_sibling(|_| false, |_| true)
        .map(|_| ())
        .unwrap_err();
    let error_old = espacio
        .reabrir_backup_sibling(|identidad| identidad == parent_identity, |_| false)
        .map(|_| ())
        .unwrap_err();

    assert_eq!(error_parent, ErrorRenombrarAnclado::IdentidadNoCoincide);
    assert_eq!(error_old, ErrorRenombrarAnclado::IdentidadNoCoincide);
    assert_eq!(
        std::fs::read(root.join("root.txt")).unwrap(),
        b"root canary"
    );
    assert_eq!(
        std::fs::read(backup_path.join("old.txt")).unwrap(),
        b"backup canary"
    );
}

#[test]
fn reabrir_backup_rechaza_ausencia_y_archivo_preservando_canarios() {
    use super::renombrar::ErrorRenombrarAnclado;
    use crate::transaccion::lock::LockEscritura;

    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    let backup_name = rename_backup_name(&root);
    let backup_path = temp.path().join(&backup_name);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("root.txt"), b"root canary").unwrap();

    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let espacio = lock.espacio_transaccional_anclado().unwrap();
    let ausente = espacio
        .reabrir_backup_sibling(|_| true, |_| true)
        .map(|_| ())
        .unwrap_err();
    std::fs::write(&backup_path, b"file canary").unwrap();
    let archivo = espacio
        .reabrir_backup_sibling(|_| true, |_| true)
        .map(|_| ())
        .unwrap_err();

    assert_eq!(ausente, ErrorRenombrarAnclado::FuenteAusente);
    assert_eq!(archivo, ErrorRenombrarAnclado::TipoNoAdmitido);
    assert_eq!(
        std::fs::read(root.join("root.txt")).unwrap(),
        b"root canary"
    );
    assert_eq!(std::fs::read(&backup_path).unwrap(), b"file canary");
}

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
