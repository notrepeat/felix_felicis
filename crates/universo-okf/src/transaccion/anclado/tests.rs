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

#[cfg(windows)]
#[test]
fn derechos_parent_transaccional_incluyen_permisos_de_creacion_del_temporal() {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
        FILE_TRAVERSE,
    };

    assert_eq!(
        derechos_parent_transaccional(),
        FILE_READ_ATTRIBUTES
            | FILE_LIST_DIRECTORY
            | FILE_TRAVERSE
            | FILE_ADD_FILE
            | FILE_ADD_SUBDIRECTORY
    );
}

#[test]
fn file_id_hex_conserva_el_orden_literal_de_bytes_en_minusculas() {
    assert_eq!(
        file_id_hex(&[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ]),
        "000102030405060708090a0b0c0d0e0f"
    );
}

#[test]
fn file_id_no_soportado_usa_fallback_y_otros_errores_fallan_cerrado() {
    for codigo in [1, 50, 87] {
        assert_eq!(
            identidad_opcional::<()>(
                Err(std::io::Error::from_raw_os_error(codigo)),
                filesystem_sin_file_id,
            )
            .unwrap(),
            None
        );
    }

    let error = identidad_opcional::<()>(
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private",
        )),
        filesystem_sin_file_id,
    )
    .unwrap_err();
    assert_eq!(error, ErrorDirectorioAnclado::Io);
}

#[test]
fn file_id_windows_rechaza_sentinels_y_conserva_un_id_valido() {
    for file_id in [[0; 16], [u8::MAX; 16]] {
        let error = validar_file_id_windows(file_id).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(!filesystem_sin_file_id(&error));
        assert_eq!(
            identidad_opcional::<()>(Err(error), filesystem_sin_file_id).unwrap_err(),
            ErrorDirectorioAnclado::TipoNoAdmitido
        );
    }

    let file_id = [0x2a; 16];
    assert_eq!(validar_file_id_windows(file_id).unwrap(), file_id);
}

#[cfg(unix)]
#[test]
fn identidad_unix_se_convierte_a_device_e_inode_del_journal() {
    let identidad = IdentidadDirectorio {
        dispositivo: 17,
        inode: 29,
    };
    let convertida = identidad.journal_identity().unwrap();
    assert!(convertida == crate::transaccion::journal::DirectoryIdentity::unix(17, 29));
}

#[cfg(windows)]
#[test]
fn identidad_windows_se_convierte_a_volume_y_file_id_literal() {
    let identidad = IdentidadDirectorio {
        volumen: 17,
        id_archivo: [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ],
    };
    let convertida = identidad.journal_identity().unwrap();
    assert!(matches!(
        convertida,
        crate::transaccion::journal::DirectoryIdentity::Windows { volume: 17, file_id }
            if file_id == "000102030405060708090a0b0c0d0e0f"
    ));
}

#[test]
fn reemplazo_de_root_existente_recibe_otra_identidad() {
    let temp = tempdir().unwrap();
    let ruta = temp.path().join("bundle");
    let movida = temp.path().join("bundle-anterior");
    std::fs::create_dir(&ruta).unwrap();

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };

        let mut opciones = std::fs::OpenOptions::new();
        opciones
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
        let directorio = opciones.open(&ruta).unwrap();
        if let Err(error) = identidad_windows(&directorio) {
            if filesystem_sin_file_id(&error) {
                eprintln!(
                    "SKIP explícito: el filesystem no implementa FILE_ID_INFO (error {})",
                    error.raw_os_error().unwrap_or_default()
                );
                return;
            }
            panic!("FILE_ID_INFO falló sin indicar falta de soporte: {error}");
        }
    }

    let ubicacion = UbicacionRaiz::nueva(&ruta).unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();
    let identidad_anterior = ancla.identidad_raiz().unwrap();
    assert_eq!(
        identidad_anterior,
        ancla.abrir_raiz().unwrap().identidad_raiz().unwrap()
    );

    std::fs::rename(&ruta, &movida).unwrap();
    std::fs::create_dir(&ruta).unwrap();
    let reemplazo = ancla.abrir_raiz().unwrap();
    let identidad_nueva = reemplazo.identidad_raiz().unwrap();

    assert_ne!(identidad_anterior, identidad_nueva);
    assert_eq!(
        identidad_nueva,
        ancla.abrir_raiz().unwrap().identidad_raiz().unwrap()
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

    let mut nombres = Vec::new();
    ancla
        .read_dir_con(|nombre| {
            nombres.push(nombre);
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap();
    assert_eq!(nombres, vec![OsString::from("read.txt")]);
    let mut lectura = match ancla.abrir_entrada_inventario("read.txt").unwrap() {
        EntradaLecturaAnclada::Archivo(archivo) => archivo,
        EntradaLecturaAnclada::Directorio(_) => panic!("se esperaba archivo regular"),
    };
    let mut bytes = Vec::new();
    lectura.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"read");
    std::fs::create_dir(ruta.join("readonly")).unwrap();
    let readonly = ancla.abrir_directorio("readonly").unwrap();
    assert!(readonly.crear_nuevo("denied.txt").is_err());
    assert!(readonly.crear_directorio("denied").is_err());
    assert!(readonly.crear_stage_sibling("nested-stage").is_err());
    assert!(!ruta.join(".readonly.felix.stage.nested-stage").exists());
    let (stage, componente_stage) = ancla.crear_stage_sibling("operaciones").unwrap();
    let mut stage_file = stage.crear_nuevo("read.txt").unwrap();
    stage_file.write_all(b"stage read").unwrap();
    let subdirectorio = stage.crear_directorio("subdir").unwrap();
    assert!(matches!(
        stage.abrir_entrada_inventario("subdir").unwrap(),
        EntradaLecturaAnclada::Directorio(_)
    ));
    let mut nuevo = subdirectorio.crear_nuevo("new.txt").unwrap();
    nuevo.write_all(b"new").unwrap();
    assert_eq!(
        std::fs::read(temp.path().join(componente_stage).join("subdir/new.txt")).unwrap(),
        b"new"
    );
    assert!(stage.abrir_archivo_regular("subdir").is_err());
    assert!(stage.abrir_directorio("read.txt").is_err());
}

#[test]
fn fallo_al_reabrir_sibling_conserva_el_handle_creado() {
    let temp = tempdir().unwrap();
    let ruta = temp.path().join("bundle");
    std::fs::create_dir(&ruta).unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&ruta).unwrap()).unwrap();

    let mut error = match ancla
        .crear_stage_sibling_con("reopen-failure", |_, _, _| Err(ErrorDirectorioAnclado::Io))
    {
        Ok(_) => panic!("la falla de apertura debe propagarse"),
        Err(error) => error,
    };

    assert_eq!(error.codigo(), ErrorDirectorioAnclado::Io);
    let (parcial, componente) = error
        .tomar_parcial()
        .unwrap_or_else(|| panic!("el error debe conservar el handle creado"));
    let ruta_stage = temp.path().join(&componente);
    let reabierto =
        DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&ruta_stage).unwrap()).unwrap();

    assert_eq!(parcial.componente(), componente);
    assert_eq!(parcial.identidad_raiz(), reabierto.identidad_raiz());
    assert!(!parcial.permite_escritura);
    assert!(parcial.crear_nuevo("denied.txt").is_err());
    assert!(ruta_stage.is_dir());
    assert!(!ruta_stage.join("denied.txt").exists());
}

#[test]
fn colisiones_de_create_new_y_mkdir_preservan_contenido_existente() {
    let temp = tempdir().unwrap();
    let ruta = temp.path().join("bundle");
    std::fs::create_dir(&ruta).unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&ruta).unwrap()).unwrap();
    let (stage, componente_stage) = ancla.crear_stage_sibling("collision-probe").unwrap();
    let stage_path = temp.path().join(componente_stage);
    let directorio_existente = stage_path.join("directorio-existente");
    std::fs::create_dir(&directorio_existente).unwrap();
    std::fs::write(stage_path.join("archivo-existente.txt"), b"no modificar").unwrap();
    std::fs::write(
        directorio_existente.join("sentinel.txt"),
        b"directorio intacto",
    )
    .unwrap();

    assert!(stage.crear_nuevo("archivo-existente.txt").is_err());
    assert!(stage.crear_directorio("directorio-existente").is_err());
    assert_eq!(
        std::fs::read(stage_path.join("archivo-existente.txt")).unwrap(),
        b"no modificar"
    );
    assert_eq!(
        std::fs::read(stage_path.join("directorio-existente/sentinel.txt")).unwrap(),
        b"directorio intacto"
    );
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
        "cOn",
        "Con.TXT",
        "nUl.txt",
        "NuL.",
        "cOm1.log",
        "lPt9",
        "CLOCK$.txt",
        "conin$",
        "CONOUT$",
    ] {
        assert!(validar_componente(OsStr::new(nombre)).is_err(), "{nombre}");
    }
    #[cfg(windows)]
    for nombre in ["normal.", "normal ", "bundle~1", "BUNDLE~23", "MiXeD~2.txt"] {
        assert!(validar_componente(OsStr::new(nombre)).is_err(), "{nombre}");
    }
    for nombre in ["bundle-safe", "readme.md"] {
        assert!(validar_componente(OsStr::new(nombre)).is_ok(), "{nombre}");
    }
}

#[cfg(unix)]
#[test]
fn operaciones_rechazan_fifo_y_socket_sin_bloquear() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::net::UnixListener;

    let temp = tempdir().unwrap();
    let ruta = temp.path().join("bundle");
    std::fs::create_dir(&ruta).unwrap();
    let fifo = ruta.join("fifo");
    let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: `fifo_c` is a valid nul-terminated path, and mkfifo does not
    // retain the pointer after the call.
    let status = unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) };
    assert_eq!(
        status,
        0,
        "mkfifo failed: {}",
        std::io::Error::last_os_error()
    );
    let socket_path = ruta.join("socket");
    let _socket = UnixListener::bind(&socket_path).unwrap();
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&ruta).unwrap()).unwrap();

    assert!(ancla.abrir_archivo_regular("fifo").is_err());
    assert!(ancla.abrir_archivo_regular("socket").is_err());
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
    let (stage, componente_stage) = raiz.crear_stage_sibling("parent-swap").unwrap();
    let mut created = stage.crear_nuevo("created.txt").unwrap();
    created.write_all(b"inside only").unwrap();
    let stage_path = moved.join(&componente_stage);
    assert!(!replaced_root.join(&componente_stage).exists());
    assert_eq!(
        std::fs::read(replaced_root.join("sentinel.txt")).unwrap(),
        b"external"
    );
    assert_eq!(
        std::fs::read(stage_path.join("created.txt")).unwrap(),
        b"inside only"
    );
}

#[cfg(windows)]
#[test]
fn rechaza_ancestro_junction_y_archivos_a_traves_de_junction() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let parent_movido = temp.path().join("parent-movido");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&parent).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let ubicacion = UbicacionRaiz::nueva(&parent.join("bundle")).unwrap();
    std::fs::rename(&parent, &parent_movido).unwrap();
    crear_junction(&parent, &outside);
    assert!(DirectorioAnclado::desde_ubicacion(&ubicacion).is_err());

    let temp = tempdir().unwrap();
    let root = temp.path().join("bundle");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"untouched").unwrap();

    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&root).unwrap()).unwrap();
    let (stage, componente_stage) = ancla.crear_stage_sibling("junction-probe").unwrap();
    let stage_path = temp.path().join(componente_stage);
    crear_junction(&stage_path.join("junction"), &outside);
    assert!(stage.abrir_directorio("junction").is_err());
    assert!(stage.crear_directorio("junction").is_err());
    assert!(stage.abrir_archivo_regular("junction").is_err());
    assert!(
        stage
            .abrir_archivo_regular("junction\\sentinel.txt")
            .is_err()
    );
    assert!(stage.crear_nuevo("junction\\created.txt").is_err());
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
