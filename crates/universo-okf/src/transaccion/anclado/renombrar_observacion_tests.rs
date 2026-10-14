use super::renombrar::ErrorRenombrarAnclado;
use super::{DirectorioAnclado, IdentidadDirectorio, UbicacionRaiz};
use crate::transaccion::lock::LockEscritura;
use std::path::Path;
use tempfile::tempdir;

fn identidad_de_ruta(ruta: &Path) -> IdentidadDirectorio {
    DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(ruta).unwrap())
        .unwrap()
        .identidad_raiz()
        .unwrap()
}

#[cfg(windows)]
fn saltar_sin_file_id(ruta: &Path) -> bool {
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
fn saltar_sin_file_id(_ruta: &Path) -> bool {
    false
}

#[test]
fn rename_observacion_rechaza_raices_artifacto_con_identidad_ajena() {
    let temp = tempdir().unwrap();
    if saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = temp.path().join("bundle");
    let stage_collision = temp.path().join(".bundle.felix.stage.txn-1");
    let backup_collision = temp.path().join(".bundle.felix.backup");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&stage_collision).unwrap();
    std::fs::create_dir(&backup_collision).unwrap();
    std::fs::write(root.join("canary.txt"), b"authorized root").unwrap();
    std::fs::write(stage_collision.join("canary.txt"), b"stage-shaped root").unwrap();
    std::fs::write(backup_collision.join("canary.txt"), b"backup-shaped root").unwrap();

    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let espacio = lock.espacio_transaccional_anclado().unwrap();
    let root_identity = identidad_de_ruta(&root);
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    let (stage, _) = ancla.crear_stage_sibling("authorized-stage").unwrap();
    let stage_identity = stage.identidad_raiz().unwrap();
    let stage_collision_identity = identidad_de_ruta(&stage_collision);
    let backup_collision_identity = identidad_de_ruta(&backup_collision);
    assert_ne!(root_identity, stage_collision_identity);
    assert_ne!(stage_identity, stage_collision_identity);
    assert_ne!(root_identity, backup_collision_identity);

    let observed_root = espacio
        .observar_directorio_sibling(root.file_name().unwrap(), root_identity)
        .unwrap();
    assert_eq!(observed_root.identidad(), root_identity);

    assert!(matches!(
        espacio.observar_directorio_sibling(stage_collision.file_name().unwrap(), stage_identity),
        Err(ErrorRenombrarAnclado::IdentidadNoCoincide)
    ));
    assert!(matches!(
        espacio.observar_directorio_sibling(backup_collision.file_name().unwrap(), root_identity),
        Err(ErrorRenombrarAnclado::IdentidadNoCoincide)
    ));

    assert_eq!(identidad_de_ruta(&root), root_identity);
    assert_eq!(
        identidad_de_ruta(&stage_collision),
        stage_collision_identity
    );
    assert_eq!(
        identidad_de_ruta(&backup_collision),
        backup_collision_identity
    );
    assert_eq!(
        std::fs::read(root.join("canary.txt")).unwrap(),
        b"authorized root"
    );
    assert_eq!(
        std::fs::read(stage_collision.join("canary.txt")).unwrap(),
        b"stage-shaped root"
    );
    assert_eq!(
        std::fs::read(backup_collision.join("canary.txt")).unwrap(),
        b"backup-shaped root"
    );
}
