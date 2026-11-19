use super::super::ErrorRenombrarAnclado;
use super::{
    DiagnosticoRenombrarWindows, EtapaRenombrarWindows, limpiar_diagnostico_renombrar_windows,
    mapear_error_noreplace, renombrar_plataforma, tomar_diagnostico_renombrar_windows,
};
use crate::transaccion::anclado::renombrar_noreplace::renombrar_windows_con;
use fs_at::OpenOptions as OpenOptionsAt;
use fs_at::os::windows::OpenOptionsExt as FsAtOpenOptionsExt;
use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use tempfile::tempdir;
use windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION;
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_TRAVERSE,
};

fn abrir_directorio(
    ruta: &Path,
    access_mode: Option<u32>,
    share_mode: Option<u32>,
) -> std::fs::File {
    let mut opciones = OpenOptions::new();
    opciones
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    if let Some(access_mode) = access_mode {
        opciones.access_mode(access_mode);
    }
    if let Some(share_mode) = share_mode {
        opciones.share_mode(share_mode);
    }
    opciones.open(ruta).expect("open test directory handle")
}

#[test]
fn share_delete_denial_records_source_open_and_success_clears_the_previous_diagnostic() {
    let temp = tempdir().unwrap();
    if crate::transaccion::anclado::renombrar_test_support::saltar_sin_file_id(temp.path()) {
        return;
    }

    let source_path = temp.path().join("bundle");
    let destination_path = temp.path().join("backup");
    std::fs::create_dir(&source_path).unwrap();
    std::fs::write(source_path.join("canary.txt"), b"source canary").unwrap();

    let parent_access = FILE_READ_ATTRIBUTES
        | FILE_LIST_DIRECTORY
        | FILE_TRAVERSE
        | FILE_ADD_FILE
        | FILE_ADD_SUBDIRECTORY;
    let parent = abrir_directorio(temp.path(), Some(parent_access), None);
    let root_known = abrir_directorio(&source_path, None, None);
    let identity = super::super::metadata_directorio(&root_known).unwrap();
    let external = abrir_directorio(&source_path, None, Some(FILE_SHARE_READ | FILE_SHARE_WRITE));

    assert_eq!(
        renombrar_plataforma(
            &parent,
            OsStr::new("bundle"),
            OsStr::new("backup"),
            &root_known,
            identity,
        ),
        Err(ErrorRenombrarAnclado::Io)
    );
    assert_eq!(
        tomar_diagnostico_renombrar_windows(),
        Some(DiagnosticoRenombrarWindows {
            etapa: EtapaRenombrarWindows::AperturaFuente,
            raw_os_error: Some(ERROR_SHARING_VIOLATION as i32),
            ntstatus: None,
            io_status: None,
        })
    );
    assert_eq!(
        std::fs::read(source_path.join("canary.txt")).unwrap(),
        b"source canary"
    );
    assert!(!destination_path.exists());

    drop(external);
    let retry = renombrar_plataforma(
        &parent,
        OsStr::new("bundle"),
        OsStr::new("backup"),
        &root_known,
        identity,
    );
    let retry_diagnostic = tomar_diagnostico_renombrar_windows();
    assert_eq!(retry, Ok(()), "retry diagnostic: {retry_diagnostic:?}");
    assert_eq!(retry_diagnostic, None);
    assert!(!source_path.exists());
    assert_eq!(
        std::fs::read(destination_path.join("canary.txt")).unwrap(),
        b"source canary"
    );
}

#[test]
fn nt_set_information_failure_diagnostic_preserves_both_native_status_values() {
    limpiar_diagnostico_renombrar_windows();
    let ntstatus = 0xC0000043_u32 as i32;
    let io_status = 0xC0000022_u32 as i32;
    let temp = tempdir().unwrap();
    let source_path = temp.path().join("source.bin");
    std::fs::write(&source_path, b"source canary").unwrap();
    let parent_access = FILE_READ_ATTRIBUTES
        | FILE_LIST_DIRECTORY
        | FILE_TRAVERSE
        | FILE_ADD_FILE
        | FILE_ADD_SUBDIRECTORY;
    let parent = abrir_directorio(temp.path(), Some(parent_access), None);
    let mut opciones = OpenOptionsAt::default();
    opciones
        .desired_access(FILE_READ_ATTRIBUTES | DELETE)
        .follow(false);
    let source = opciones
        .open_at(&parent, Path::new("source.bin"))
        .expect("open test source for rename");

    let error = renombrar_windows_con(
        &parent,
        &source,
        OsStr::new("destination.bin"),
        |_, status, _, _| {
            status.Anonymous.Status = io_status;
            ntstatus
        },
    )
    .expect_err("the injected native failure is retained internally");
    assert_eq!(
        mapear_error_noreplace(error),
        ErrorRenombrarAnclado::OperacionAmbigua
    );
    assert_eq!(
        tomar_diagnostico_renombrar_windows(),
        Some(DiagnosticoRenombrarWindows {
            etapa: EtapaRenombrarWindows::NtSetInformationFile,
            raw_os_error: None,
            ntstatus: Some(ntstatus),
            io_status: Some(io_status),
        })
    );
}
