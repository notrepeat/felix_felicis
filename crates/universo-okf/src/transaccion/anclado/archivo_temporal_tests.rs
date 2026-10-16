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
