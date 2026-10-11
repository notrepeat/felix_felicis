use super::*;
use std::io::{Read, Write};
use tempfile::tempdir;

#[test]
fn root_ausente_conserva_parent_y_se_abre_despues_de_recovery() {
    let temp = tempdir().unwrap();
    let ruta = temp.path().join("bundle");
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&ruta).unwrap()).unwrap();

    assert!(ancla.identidad_raiz().is_none());
    assert!(ancla.abrir_raiz().is_err());
    std::fs::create_dir(&ruta).unwrap();
    let raiz = ancla.abrir_raiz().unwrap();
    assert!(raiz.identidad_raiz().is_some());
    assert_eq!(
        raiz.identidad_raiz(),
        ancla.abrir_raiz().unwrap().identidad_raiz()
    );
}

#[test]
fn operaciones_relativas_validan_componentes_y_tipos_de_handle() {
    let temp = tempdir().unwrap();
    let ruta = temp.path().join("bundle");
    std::fs::create_dir(&ruta).unwrap();
    std::fs::write(ruta.join("read.txt"), b"read").unwrap();
    let mut ancla =
        DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&ruta).unwrap()).unwrap();

    let nombres = ancla.read_dir().unwrap();
    assert_eq!(nombres, vec!["read.txt".to_owned()]);
    let mut lectura = ancla.abrir_archivo_regular("read.txt").unwrap();
    let mut bytes = Vec::new();
    lectura.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"read");
    let subdirectorio = ancla.crear_directorio("subdir").unwrap();
    let mut nuevo = subdirectorio.crear_nuevo("new.txt").unwrap();
    nuevo.write_all(b"new").unwrap();
    assert_eq!(std::fs::read(ruta.join("subdir/new.txt")).unwrap(), b"new");
    assert!(ancla.abrir_archivo_regular("subdir").is_err());
    assert!(ancla.abrir_directorio("read.txt").is_err());
}

#[test]
fn nombres_de_componente_rechazan_separadores_ads_y_reservados() {
    for nombre in [
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "file:stream",
        "CON",
        "nul.txt",
    ] {
        assert!(validar_componente(OsStr::new(nombre)).is_err(), "{nombre}");
    }
    for nombre in ["bundle-safe", "readme.md"] {
        assert!(validar_componente(OsStr::new(nombre)).is_ok(), "{nombre}");
    }
}

#[test]
fn identidad_parent_sobrevive_renombre_y_reemplazo_de_ancestro() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let moved = temp.path().join("parent-moved");
    let outside = temp.path().join("outside");
    let root = parent.join("bundle");
    let replaced_root = outside.join("bundle");
    std::fs::create_dir(&parent).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::create_dir(&replaced_root).unwrap();
    std::fs::write(replaced_root.join("sentinel.txt"), b"external").unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    let parent_id = ancla.identidad_parent();
    assert!(ancla.identidad_raiz().is_none());

    std::fs::rename(&parent, &moved).unwrap();
    crear_junction(&parent, &outside);
    std::fs::create_dir(moved.join("bundle")).unwrap();
    std::fs::write(moved.join("bundle/sentinel.txt"), b"inside").unwrap();

    assert_eq!(ancla.identidad_parent(), parent_id);
    let raiz = ancla.abrir_raiz().unwrap();
    assert!(raiz.identidad_raiz().is_some());
    assert_eq!(
        raiz.identidad_raiz(),
        ancla.abrir_raiz().unwrap().identidad_raiz()
    );
    let mut sentinel = raiz.abrir_archivo_regular("sentinel.txt").unwrap();
    let mut bytes = Vec::new();
    sentinel.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"inside");
    let mut created = raiz.crear_nuevo("created.txt").unwrap();
    created.write_all(b"inside only").unwrap();
    assert!(!replaced_root.join("created.txt").exists());
    assert_eq!(
        std::fs::read(replaced_root.join("sentinel.txt")).unwrap(),
        b"external"
    );
    assert_eq!(
        std::fs::read(moved.join("bundle/created.txt")).unwrap(),
        b"inside only"
    );
}

#[cfg(windows)]
#[test]
fn symlink_y_junction_se_rechazan_sin_tocar_sentinel_externo() {
    use std::os::windows::fs::{symlink_dir, symlink_file};

    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"untouched").unwrap();
    crear_junction(&root.join("junction"), &outside);
    let symlink = root.join("symlink");
    match symlink_dir(&outside, &symlink) {
        Ok(()) => {}
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314) =>
        {
            eprintln!("SKIP explícito: crear symlink requiere privilegio; junction sí se prueba");
        }
        Err(error) => panic!("no se pudo crear fixture symlink: {error}"),
    }
    let enlace_archivo = root.join("sentinel-link.txt");
    match symlink_file(outside.join("sentinel.txt"), &enlace_archivo) {
        Ok(()) => {}
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314) =>
        {
            eprintln!("SKIP explícito: crear symlink de archivo requiere privilegio");
        }
        Err(error) => panic!("no se pudo crear fixture symlink de archivo: {error}"),
    }

    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    assert!(ancla.abrir_directorio("junction").is_err());
    assert!(ancla.crear_directorio("junction").is_err());
    if symlink.exists() {
        assert!(ancla.abrir_directorio("symlink").is_err());
    }
    if enlace_archivo.exists() {
        assert!(ancla.abrir_archivo_regular("sentinel-link.txt").is_err());
    }
    assert_eq!(
        std::fs::read(outside.join("sentinel.txt")).unwrap(),
        b"untouched"
    );
    assert!(!outside.join("created.txt").exists());
}

#[cfg(windows)]
fn crear_junction(link: &std::path::Path, target: &std::path::Path) {
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
fn crear_junction(link: &std::path::Path, target: &std::path::Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}
