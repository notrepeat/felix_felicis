use super::*;

use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

const TEMPORAL: &str = ".bundle.felix.admctx.tmp";
const CONTENT: &[u8] = b"bounded synthetic temporal bytes";

fn abrir_parent(path: &Path) -> File {
    fs::create_dir_all(path).unwrap();
    super::super::abrir_directorio_absoluto(path, true).unwrap()
}

fn temporal(parent: &Path) -> PathBuf {
    parent.join(TEMPORAL)
}

#[test]
fn escribe_exclusivo_y_devuelve_solo_datos_del_mismo_archivo() {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let parent = abrir_parent(&parent_path);

    let datos = crear(&parent, OsStr::new(TEMPORAL), CONTENT).unwrap();
    let digest_esperado: [u8; 32] = Sha256::digest(CONTENT).into();

    assert_eq!(fs::read(temporal(&parent_path)).unwrap(), CONTENT);
    assert_eq!(datos.longitud, CONTENT.len());
    assert_eq!(datos.digest, digest_esperado);
    assert_eq!(
        datos.identidad_archivo,
        identidad_archivo(&File::open(temporal(&parent_path)).unwrap()).unwrap()
    );
    assert_eq!(format!("{datos:?}"), "DatosArchivoTemporal { .. }");
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(temporal(&parent_path))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn colision_exclusiva_conserva_bytes_existentes() {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let parent = abrir_parent(&parent_path);
    let path = temporal(&parent_path);
    let canary = b"existing file must remain unchanged";
    fs::write(&path, canary).unwrap();

    let error = crear(&parent, OsStr::new(TEMPORAL), CONTENT).unwrap_err();

    assert_eq!(error, ErrorArchivoTemporalAnclado::Exists);
    assert_eq!(fs::read(path).unwrap(), canary);
}

fn falla_despues_de_crear(sync_falla: bool) {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let parent = abrir_parent(&parent_path);
    let path = temporal(&parent_path);

    let resultado = if sync_falla {
        crear_con(
            &parent,
            OsStr::new(TEMPORAL),
            CONTENT,
            |archivo, bytes| {
                archivo
                    .write_all(bytes)
                    .map_err(|_| ErrorArchivoTemporalAnclado::Io)
            },
            |_| Err(ErrorArchivoTemporalAnclado::Io),
        )
    } else {
        crear_con(
            &parent,
            OsStr::new(TEMPORAL),
            CONTENT,
            |archivo, bytes| {
                let prefix = &bytes[..bytes.len() / 2];
                archivo
                    .write_all(prefix)
                    .map_err(|_| ErrorArchivoTemporalAnclado::Io)?;
                Err(ErrorArchivoTemporalAnclado::Io)
            },
            |_| panic!("sync no debe correr tras un fallo de escritura"),
        )
    };

    assert_eq!(resultado.unwrap_err(), ErrorArchivoTemporalAnclado::Io);
    let preserved = fs::read(&path).unwrap();
    if sync_falla {
        assert_eq!(preserved, CONTENT);
    } else {
        assert_eq!(preserved.as_slice(), &CONTENT[..CONTENT.len() / 2]);
    }
    assert_eq!(
        crear(&parent, OsStr::new(TEMPORAL), CONTENT).unwrap_err(),
        ErrorArchivoTemporalAnclado::Exists
    );
    assert_eq!(fs::read(path).unwrap(), preserved);
}

#[test]
fn fallo_de_escritura_conserva_prefijo_y_bloquea_reintento() {
    falla_despues_de_crear(false);
}

#[test]
fn fallo_de_sync_conserva_archivo_completo_y_bloquea_reintento() {
    falla_despues_de_crear(true);
}

#[test]
fn handle_parent_mantiene_namespace_tras_mover_parent_y_preserva_decoy() {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let parent = abrir_parent(&parent_path);

    if let Err(error) = fs::rename(&parent_path, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32)),
                "{error}"
            );
            assert!(parent_path.is_dir());
            assert!(!moved_parent.exists());
            eprintln!("Windows bloqueó el rename del parent abierto; no se ejercitó el decoy.");
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent sintético: {error}");
    }

    fs::create_dir(&parent_path).unwrap();
    let path_decoy = temporal(&parent_path);
    let decoy = b"decoy namespace must remain unchanged";
    fs::write(&path_decoy, decoy).unwrap();

    let datos = crear(&parent, OsStr::new(TEMPORAL), CONTENT).unwrap();

    assert_eq!(fs::read(temporal(&moved_parent)).unwrap(), CONTENT);
    assert_eq!(fs::read(path_decoy).unwrap(), decoy);
    assert_eq!(datos.longitud, CONTENT.len());
}

#[cfg(unix)]
#[test]
fn no_sigue_enlace_y_conserva_link_y_destino() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let parent = abrir_parent(&parent_path);
    let target = temp.path().join("target-canary");
    let path = temporal(&parent_path);
    fs::write(&target, b"outside target remains unchanged").unwrap();
    symlink(&target, &path).unwrap();

    let error = crear(&parent, OsStr::new(TEMPORAL), CONTENT).unwrap_err();

    assert_eq!(error, ErrorArchivoTemporalAnclado::Exists);
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(&path).unwrap(), target);
    assert_eq!(
        fs::read(&target).unwrap(),
        b"outside target remains unchanged"
    );
}

#[test]
fn crear_vacio_retorna_id_del_mismo_handle_y_no_escribe_bytes() {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let parent = abrir_parent(&parent_path);
    let path = temporal(&parent_path);

    let archivo = crear_vacio(&parent, OsStr::new(TEMPORAL)).unwrap();
    let identidad_esperada = archivo.identidad();

    assert_eq!(archivo.archivo.metadata().unwrap().len(), 0);
    assert_eq!(
        identidad_esperada,
        identidad_archivo(&archivo.archivo).unwrap()
    );
    assert_eq!(format!("{archivo:?}"), "ArchivoTemporalVacio { .. }");
    drop(archivo);
    assert_eq!(fs::metadata(&path).unwrap().len(), 0);
    let visible = File::open(path).unwrap();
    assert_eq!(identidad_esperada, identidad_archivo(&visible).unwrap());
}

#[test]
fn escribe_despues_de_observar_self_id_y_devuelve_datos_del_mismo_archivo() {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let parent = abrir_parent(&parent_path);
    let path = temporal(&parent_path);
    let vacio = crear_vacio(&parent, OsStr::new(TEMPORAL)).unwrap();
    let identidad_observada = vacio.identidad();
    let digest_esperado: [u8; 32] = Sha256::digest(CONTENT).into();

    let datos = vacio.escribir(CONTENT).unwrap();

    assert_eq!(fs::read(&path).unwrap(), CONTENT);
    assert_eq!(datos.identidad_archivo, identidad_observada);
    assert_eq!(datos.longitud, CONTENT.len());
    assert_eq!(datos.digest, digest_esperado);
}

#[test]
fn temporal_vacio_rechaza_bytes_previos_sin_escribir_y_preserva_id() {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let parent = abrir_parent(&parent_path);
    let path = temporal(&parent_path);
    let prefijo = b"unexpected preexisting prefix";
    let mut vacio = crear_vacio(&parent, OsStr::new(TEMPORAL)).unwrap();
    let identidad_esperada = vacio.identidad();
    vacio.archivo.write_all(prefijo).unwrap();

    let resultado = vacio.escribir_con(
        CONTENT,
        |_, _| panic!("no debe escribir sobre un temporal que ya tiene bytes"),
        |_| panic!("no debe sincronizar tras rechazar bytes previos"),
    );

    assert_eq!(
        resultado.unwrap_err(),
        ErrorArchivoTemporalAnclado::EntryRejected
    );
    assert_eq!(fs::read(&path).unwrap(), prefijo);
    let reopened = File::open(path).unwrap();
    assert_eq!(identidad_esperada, identidad_archivo(&reopened).unwrap());
}

#[test]
fn crear_vacio_colision_preserva_canary() {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let parent = abrir_parent(&parent_path);
    let path = temporal(&parent_path);
    let canary = b"existing intent temporary must remain unchanged";
    fs::write(&path, canary).unwrap();

    let error = crear_vacio(&parent, OsStr::new(TEMPORAL)).unwrap_err();

    assert_eq!(error, ErrorArchivoTemporalAnclado::Exists);
    assert_eq!(fs::read(path).unwrap(), canary);
}

fn fallo_escritura_vacio(sync_falla: bool) {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let parent = abrir_parent(&parent_path);
    let path = temporal(&parent_path);
    let contenido = b"self-identified intent frame";
    let vacio = crear_vacio(&parent, OsStr::new(TEMPORAL)).unwrap();

    let resultado = if sync_falla {
        vacio.escribir_con(
            contenido,
            |archivo, bytes| {
                archivo
                    .write_all(bytes)
                    .map_err(|_| ErrorArchivoTemporalAnclado::Io)
            },
            |_| Err(ErrorArchivoTemporalAnclado::Io),
        )
    } else {
        vacio.escribir_con(
            contenido,
            |archivo, bytes| {
                archivo
                    .write_all(&bytes[..bytes.len() / 2])
                    .map_err(|_| ErrorArchivoTemporalAnclado::Io)?;
                Err(ErrorArchivoTemporalAnclado::Io)
            },
            |_| panic!("sync no debe correr tras un fallo de escritura"),
        )
    };

    assert_eq!(resultado.unwrap_err(), ErrorArchivoTemporalAnclado::Io);
    let preservado = fs::read(&path).unwrap();
    if sync_falla {
        assert_eq!(preservado, contenido);
    } else {
        assert_eq!(preservado.as_slice(), &contenido[..contenido.len() / 2]);
    }
    assert_eq!(
        crear_vacio(&parent, OsStr::new(TEMPORAL)).unwrap_err(),
        ErrorArchivoTemporalAnclado::Exists
    );
    assert_eq!(fs::read(path).unwrap(), preservado);
}

#[test]
fn fallo_de_escritura_despues_de_self_id_preserva_temporal_y_bloquea_retry() {
    fallo_escritura_vacio(false);
}

#[test]
fn fallo_de_sync_despues_de_self_id_preserva_temporal_y_bloquea_retry() {
    fallo_escritura_vacio(true);
}

#[test]
fn escritura_despues_de_mover_parent_sigue_handle_original_y_preserva_decoy() {
    let temp = tempdir().unwrap();
    let parent_path = temp.path().join("parent");
    let moved_parent = temp.path().join("parent-moved");
    let parent = abrir_parent(&parent_path);
    let vacio = crear_vacio(&parent, OsStr::new(TEMPORAL)).unwrap();
    let identidad_esperada = vacio.identidad();

    if let Err(error) = fs::rename(&parent_path, &moved_parent) {
        #[cfg(windows)]
        {
            assert!(
                matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33)),
                "{error}"
            );
            assert!(parent_path.is_dir());
            assert!(!moved_parent.exists());
            drop(vacio);
            assert_eq!(fs::metadata(temporal(&parent_path)).unwrap().len(), 0);
            let visible = File::open(temporal(&parent_path)).unwrap();
            assert_eq!(identidad_esperada, identidad_archivo(&visible).unwrap());
            eprintln!(
                "Windows bloqueó el rename entre creación y escritura; no se ejercitó el decoy."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mover el parent sintético: {error}");
    }

    fs::create_dir(&parent_path).unwrap();
    let decoy_path = temporal(&parent_path);
    let decoy = b"moved-parent decoy remains unchanged";
    fs::write(&decoy_path, decoy).unwrap();
    let datos = vacio.escribir(CONTENT).unwrap();

    let reopened_moved = File::open(temporal(&moved_parent)).unwrap();
    assert_eq!(fs::read(temporal(&moved_parent)).unwrap(), CONTENT);
    assert_eq!(fs::read(decoy_path).unwrap(), decoy);
    assert_eq!(datos.identidad_archivo, identidad_esperada);
    assert_eq!(
        datos.identidad_archivo,
        identidad_archivo(&reopened_moved).unwrap()
    );
}
