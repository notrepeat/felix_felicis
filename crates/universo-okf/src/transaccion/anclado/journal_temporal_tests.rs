use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn crear_root(temp: &Path) -> PathBuf {
    let root = temp.join("bundle");
    fs::create_dir(&root).unwrap();
    root
}

fn nombre_temporal(temp: &Path) -> PathBuf {
    temp.join(".bundle.felix.journal.tmp")
}

#[test]
fn crea_cierra_lee_desde_parent_anclado_y_conserva_identidad_del_handle() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let identidad_escrita = capability.crear_nuevo(b"journal-temp-canary").unwrap();

    let (identidad_leida, bytes) = capability.abrir_y_leer().unwrap();
    assert_eq!(identidad_escrita, identidad_leida);
    assert_eq!(bytes, b"journal-temp-canary");
    assert_eq!(fs::read(nombre_temporal(temp.path())).unwrap(), bytes);
    assert!(!format!("{identidad_leida:?}").contains("bundle"));
}

#[test]
fn acepta_exactamente_max_journal_bytes_al_escribir_y_leer() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let bytes = vec![b'x'; MAX_JOURNAL_BYTES];

    let identidad_escrita = capability.crear_nuevo(&bytes).unwrap();
    let (identidad_leida, read) = capability.abrir_y_leer().unwrap();
    assert_eq!(identidad_escrita, identidad_leida);
    assert_eq!(read, bytes);
}

#[test]
fn rechaza_max_mas_uno_antes_de_crear_y_al_leer() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let bytes = vec![b'x'; MAX_JOURNAL_BYTES + 1];
    let nombre = nombre_temporal(temp.path());

    assert_eq!(
        capability.crear_nuevo(&bytes).unwrap_err(),
        ErrorJournalTemporal::TooLarge
    );
    assert!(!nombre.exists());

    fs::write(&nombre, &bytes).unwrap();
    assert_eq!(
        capability.abrir_y_leer().unwrap_err(),
        ErrorJournalTemporal::TooLarge
    );
    assert_eq!(
        fs::metadata(nombre).unwrap().len(),
        (MAX_JOURNAL_BYTES + 1) as u64
    );
}

#[cfg(windows)]
#[test]
fn raiz_parent_windows_deriva_drive_y_unc_sin_probar_acceso_real() {
    assert_eq!(
        super::super::raiz_parent_windows(Path::new(r"C:\")).unwrap(),
        Path::new(r"C:\")
    );
    assert_eq!(
        super::super::raiz_parent_windows(Path::new(r"\\server\share")).unwrap(),
        Path::new(r"\\server\share\")
    );
}
