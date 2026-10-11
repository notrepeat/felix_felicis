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

#[test]
fn colision_con_archivo_preserva_canary_y_rechaza_segundo_create() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let canary = b"first-create-canary";
    capability.crear_nuevo(canary).unwrap();

    assert_eq!(
        capability.crear_nuevo(b"replacement").unwrap_err(),
        ErrorJournalTemporal::Exists
    );
    let (_, read) = capability.abrir_y_leer().unwrap();
    assert_eq!(read, canary);
    assert_eq!(fs::read(nombre_temporal(temp.path())).unwrap(), canary);
}

#[test]
fn colision_con_directorio_no_lo_reemplaza_ni_sigue_y_preserva_canary() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let nombre = nombre_temporal(temp.path());
    fs::create_dir(&nombre).unwrap();
    fs::write(nombre.join("canary"), b"directory-canary").unwrap();

    assert_eq!(
        capability.crear_nuevo(b"replacement").unwrap_err(),
        ErrorJournalTemporal::Exists
    );
    assert_eq!(
        capability.abrir_y_leer().unwrap_err(),
        ErrorJournalTemporal::RegularFileRequired
    );
    assert_eq!(
        fs::read(nombre.join("canary")).unwrap(),
        b"directory-canary"
    );
}

#[cfg(unix)]
#[test]
fn symlink_existente_no_se_reemplaza_ni_se_sigue_y_preserva_canary() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let target = temp.path().join("outside-canary");
    fs::write(&target, b"outside-canary").unwrap();
    let nombre = nombre_temporal(temp.path());
    symlink(&target, &nombre).unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();

    assert_eq!(
        capability.crear_nuevo(b"replacement").unwrap_err(),
        ErrorJournalTemporal::Exists
    );
    assert_eq!(
        capability.abrir_y_leer().unwrap_err(),
        ErrorJournalTemporal::ReparseNotAllowed
    );
    assert_eq!(fs::read(&target).unwrap(), b"outside-canary");
}

#[cfg(windows)]
#[test]
fn junction_existente_no_se_reemplaza_ni_se_sigue_y_preserva_canary() {
    let temp = tempdir().unwrap();
    if crate::transaccion::anclado::renombrar_test_support::saltar_sin_file_id(temp.path()) {
        return;
    }
    let root = crear_root(temp.path());
    let target = temp.path().join("outside-canary");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("canary"), b"outside-canary").unwrap();
    let nombre = nombre_temporal(temp.path());
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&nombre)
        .arg(&target)
        .output()
        .expect("cmd crea junctions");
    if !output.status.success() {
        let detalle = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if detalle.to_ascii_lowercase().contains("privilege") {
            eprintln!("SKIP explícito: Windows no concedió privilegio para la junction");
            return;
        }
        panic!("no se pudo crear junction: {detalle}");
    }
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();

    assert_eq!(
        capability.crear_nuevo(b"replacement").unwrap_err(),
        ErrorJournalTemporal::Exists
    );
    assert!(matches!(
        capability.abrir_y_leer().unwrap_err(),
        ErrorJournalTemporal::ReparseNotAllowed | ErrorJournalTemporal::RegularFileRequired
    ));
    assert_eq!(fs::read(target.join("canary")).unwrap(), b"outside-canary");
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
