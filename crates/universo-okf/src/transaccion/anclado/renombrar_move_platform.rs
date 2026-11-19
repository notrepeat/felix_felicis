use super::super::super::metadata_directorio;
use super::super::{ErrorRenombrarAnclado, IdentidadDirectorio, mapear_error_directorio};
#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
use crate::transaccion::anclado::renombrar_noreplace::invocar_renameat2;
#[cfg(any(target_os = "linux", target_os = "android", windows))]
use crate::transaccion::anclado::renombrar_noreplace::{
    CausaRenombrarNoReemplazo, ErrorRenombrarNoReemplazo,
};
use std::ffi::OsStr;
use std::fs::File;

#[cfg(unix)]
pub(super) fn renombrar_plataforma(
    parent: &File,
    fuente: &OsStr,
    destino: &OsStr,
    raiz_conocida: &File,
    identidad_esperada: IdentidadDirectorio,
) -> Result<(), ErrorRenombrarAnclado> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let operar = |parent: &File, fuente: &std::ffi::CStr, destino: &std::ffi::CStr| {
        crate::transaccion::anclado::renombrar_noreplace::renombrar_unix(parent, fuente, destino)
            .map_err(mapear_error_noreplace)
    };
    #[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
    let operar = |_: &File, _: &std::ffi::CStr, _: &std::ffi::CStr| {
        // Other Unix platforms have no verified no-replace primitive in this
        // work unit. Never emulate it with the racy, replace-capable renameat.
        Err(ErrorRenombrarAnclado::Io)
    };

    renombrar_plataforma_con_operacion(
        parent,
        fuente,
        destino,
        raiz_conocida,
        identidad_esperada,
        operar,
    )
}

#[cfg(unix)]
// Production passes the real no-replace operation; tests can insert a sibling
// after these prechecks and before that operation without global hooks.
fn renombrar_plataforma_con_operacion<R>(
    parent: &File,
    fuente: &OsStr,
    destino: &OsStr,
    raiz_conocida: &File,
    identidad_esperada: IdentidadDirectorio,
    operar: R,
) -> Result<(), ErrorRenombrarAnclado>
where
    R: FnOnce(&File, &std::ffi::CStr, &std::ffi::CStr) -> Result<(), ErrorRenombrarAnclado>,
{
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let fuente =
        CString::new(fuente.as_bytes()).map_err(|_| ErrorRenombrarAnclado::ComponenteInvalido)?;
    let destino =
        CString::new(destino.as_bytes()).map_err(|_| ErrorRenombrarAnclado::ComponenteInvalido)?;
    let identidad_padre = metadata_directorio(parent).map_err(mapear_error_directorio)?;

    let mut estado_destino = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: both names are validated NUL-terminated components and the live
    // parent descriptor anchors the lookup. No-follow detects any destination.
    let resultado = unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            destino.as_ptr(),
            estado_destino.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if resultado == 0 {
        return Err(ErrorRenombrarAnclado::DestinoExistente);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() != Some(libc::ENOENT) {
        return Err(mapear_error_io(error));
    }

    let identidad_nombrada = identidad_de_nombre(parent, &fuente)?;
    if identidad_nombrada != identidad_esperada
        || metadata_directorio(raiz_conocida).map_err(mapear_error_directorio)?
            != identidad_esperada
    {
        return Err(ErrorRenombrarAnclado::IdentidadNoCoincide);
    }
    if metadata_directorio(parent).map_err(mapear_error_directorio)? != identidad_padre {
        return Err(ErrorRenombrarAnclado::FuenteNoHermana);
    }

    operar(parent, &fuente, &destino)
}

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
fn renombrar_noreplace<I>(
    parent: &File,
    fuente: &std::ffi::CStr,
    destino: &std::ffi::CStr,
    invocar: I,
) -> Result<(), ErrorRenombrarAnclado>
where
    I: FnOnce(&File, &std::ffi::CStr, &std::ffi::CStr) -> (libc::c_long, Option<i32>),
{
    crate::transaccion::anclado::renombrar_noreplace::renombrar_unix_con(
        parent, fuente, destino, invocar,
    )
    .map_err(mapear_error_noreplace)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn mapear_error_noreplace(error: ErrorRenombrarNoReemplazo) -> ErrorRenombrarAnclado {
    match error.causa() {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        CausaRenombrarNoReemplazo::Io => ErrorRenombrarAnclado::Io,
        #[cfg(any(target_os = "linux", target_os = "android"))]
        CausaRenombrarNoReemplazo::DestinoExistente => ErrorRenombrarAnclado::DestinoExistente,
        CausaRenombrarNoReemplazo::OperacionAmbigua => ErrorRenombrarAnclado::OperacionAmbigua,
    }
}

#[cfg(windows)]
fn mapear_error_noreplace(error: ErrorRenombrarNoReemplazo) -> ErrorRenombrarAnclado {
    #[cfg(test)]
    if let Some((ntstatus, io_status)) = error.diagnostico_ntstatus() {
        registrar_diagnostico_renombrar_windows(DiagnosticoRenombrarWindows {
            etapa: EtapaRenombrarWindows::NtSetInformationFile,
            raw_os_error: None,
            ntstatus: Some(ntstatus),
            io_status: Some(io_status),
        });
    }

    match error.causa() {
        CausaRenombrarNoReemplazo::OperacionAmbigua => ErrorRenombrarAnclado::OperacionAmbigua,
        CausaRenombrarNoReemplazo::ComponenteInvalido => ErrorRenombrarAnclado::ComponenteInvalido,
    }
}

#[cfg(unix)]
fn identidad_de_nombre(
    parent: &File,
    nombre: &std::ffi::CStr,
) -> Result<IdentidadDirectorio, ErrorRenombrarAnclado> {
    use std::os::fd::{AsRawFd, FromRawFd};

    let mut estado = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `nombre` is a validated C string and fstatat writes `estado`.
    let resultado = unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            nombre.as_ptr(),
            estado.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if resultado != 0 {
        return Err(mapear_error_io(std::io::Error::last_os_error()));
    }
    // SAFETY: fstatat returned success and initialized the structure.
    let estado = unsafe { estado.assume_init() };
    match estado.st_mode & libc::S_IFMT {
        libc::S_IFLNK => return Err(ErrorRenombrarAnclado::EnlaceNoPermitido),
        libc::S_IFDIR => {}
        _ => return Err(ErrorRenombrarAnclado::TipoNoAdmitido),
    }
    // SAFETY: O_NOFOLLOW and O_DIRECTORY force a real directory open; the
    // resulting descriptor is uniquely owned by this File.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            nombre.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(mapear_error_io(std::io::Error::last_os_error()));
    }
    // SAFETY: successful openat returned a fresh owned descriptor.
    let abierto = unsafe { File::from_raw_fd(fd) };
    metadata_directorio(&abierto).map_err(mapear_error_directorio)
}

#[cfg(windows)]
pub(super) fn renombrar_plataforma(
    parent: &File,
    fuente: &OsStr,
    destino: &OsStr,
    raiz_conocida: &File,
    identidad_esperada: IdentidadDirectorio,
) -> Result<(), ErrorRenombrarAnclado> {
    use fs_at::OpenOptions as OpenOptionsAt;
    use fs_at::os::windows::OpenOptionsExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_READ_ATTRIBUTES};

    #[cfg(test)]
    limpiar_diagnostico_renombrar_windows();

    // A relative no-follow open is enough to detect any existing target,
    // including files and reparse points; it is never opened for writing.
    let mut opciones_destino = OpenOptionsAt::default();
    opciones_destino
        .desired_access(FILE_READ_ATTRIBUTES)
        .follow(false);
    match opciones_destino.open_at(parent, Path::new(destino)) {
        Ok(_) => return Err(ErrorRenombrarAnclado::DestinoExistente),
        Err(error) if windows_not_found(&error) => {}
        Err(error) => {
            #[cfg(test)]
            registrar_diagnostico_renombrar_windows(DiagnosticoRenombrarWindows {
                etapa: EtapaRenombrarWindows::ProbeDestino,
                raw_os_error: error.raw_os_error(),
                ntstatus: None,
                io_status: None,
            });
            return Err(mapear_error_io(error));
        }
    }

    // Request only the attributes needed to verify the object and DELETE to
    // rename it. FILE_OPEN_REPARSE_POINT is set by fs_at when follow(false).
    let mut opciones_fuente = OpenOptionsAt::default();
    opciones_fuente
        .desired_access(FILE_READ_ATTRIBUTES | DELETE)
        .follow(false);
    let fuente_renombrable = opciones_fuente
        .open_dir_at(parent, Path::new(fuente))
        .map_err(|error| {
            #[cfg(test)]
            registrar_diagnostico_renombrar_windows(DiagnosticoRenombrarWindows {
                etapa: EtapaRenombrarWindows::AperturaFuente,
                raw_os_error: error.raw_os_error(),
                ntstatus: None,
                io_status: None,
            });
            mapear_error_io(error)
        })?;
    let identidad = metadata_directorio(&fuente_renombrable).map_err(|error| {
        #[cfg(test)]
        registrar_diagnostico_renombrar_windows(DiagnosticoRenombrarWindows {
            etapa: EtapaRenombrarWindows::MetadataFuente,
            raw_os_error: None,
            ntstatus: None,
            io_status: None,
        });
        mapear_error_directorio(error)
    })?;
    if identidad != identidad_esperada {
        return Err(ErrorRenombrarAnclado::IdentidadNoCoincide);
    }
    let identidad_raiz_conocida = metadata_directorio(raiz_conocida).map_err(|error| {
        #[cfg(test)]
        registrar_diagnostico_renombrar_windows(DiagnosticoRenombrarWindows {
            etapa: EtapaRenombrarWindows::MetadataRaiz,
            raw_os_error: None,
            ntstatus: None,
            io_status: None,
        });
        mapear_error_directorio(error)
    })?;
    if identidad_raiz_conocida != identidad_esperada {
        return Err(ErrorRenombrarAnclado::IdentidadNoCoincide);
    }

    crate::transaccion::anclado::renombrar_noreplace::renombrar_windows(
        parent,
        &fuente_renombrable,
        destino,
    )
    .map_err(mapear_error_noreplace)
}

#[cfg(all(test, windows))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EtapaRenombrarWindows {
    ProbeDestino,
    AperturaFuente,
    MetadataFuente,
    MetadataRaiz,
    NtSetInformationFile,
}

#[cfg(all(test, windows))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DiagnosticoRenombrarWindows {
    etapa: EtapaRenombrarWindows,
    raw_os_error: Option<i32>,
    ntstatus: Option<i32>,
    io_status: Option<i32>,
}

#[cfg(all(test, windows))]
thread_local! {
    static ULTIMO_DIAGNOSTICO_RENOMBRAR_WINDOWS: std::cell::Cell<Option<DiagnosticoRenombrarWindows>> = const { std::cell::Cell::new(None) };
}

#[cfg(all(test, windows))]
fn limpiar_diagnostico_renombrar_windows() {
    ULTIMO_DIAGNOSTICO_RENOMBRAR_WINDOWS.with(|ultimo| ultimo.set(None));
}

#[cfg(all(test, windows))]
fn registrar_diagnostico_renombrar_windows(diagnostico: DiagnosticoRenombrarWindows) {
    eprintln!(
        "windows rename diagnostic: etapa={:?}, raw_os_error={:?}, ntstatus={:?}, io_status={:?}",
        diagnostico.etapa, diagnostico.raw_os_error, diagnostico.ntstatus, diagnostico.io_status,
    );
    ULTIMO_DIAGNOSTICO_RENOMBRAR_WINDOWS.with(|ultimo| ultimo.set(Some(diagnostico)));
}

#[cfg(all(test, windows))]
fn tomar_diagnostico_renombrar_windows() -> Option<DiagnosticoRenombrarWindows> {
    ULTIMO_DIAGNOSTICO_RENOMBRAR_WINDOWS.with(|ultimo| ultimo.take())
}

#[cfg(all(test, windows))]
#[path = "renombrar_move_platform_windows_tests.rs"]
mod windows_diagnostics_tests;

#[cfg(not(any(unix, windows)))]
pub(super) fn renombrar_plataforma(
    _parent: &File,
    _fuente: &OsStr,
    _destino: &OsStr,
    _raiz_conocida: &File,
    _identidad_esperada: IdentidadDirectorio,
) -> Result<(), ErrorRenombrarAnclado> {
    Err(ErrorRenombrarAnclado::TipoNoAdmitido)
}

#[cfg(unix)]
fn mapear_error_io(error: std::io::Error) -> ErrorRenombrarAnclado {
    match error.raw_os_error() {
        Some(libc::ELOOP) => ErrorRenombrarAnclado::EnlaceNoPermitido,
        Some(libc::ENOENT) => ErrorRenombrarAnclado::FuenteAusente,
        Some(libc::EEXIST | libc::ENOTEMPTY) => ErrorRenombrarAnclado::DestinoExistente,
        _ => ErrorRenombrarAnclado::Io,
    }
}

#[cfg(windows)]
fn mapear_error_io(error: std::io::Error) -> ErrorRenombrarAnclado {
    use windows_sys::Win32::Foundation::{
        ERROR_ALREADY_EXISTS, ERROR_CANT_RESOLVE_FILENAME, ERROR_DIRECTORY, ERROR_FILE_EXISTS,
        ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND,
    };

    match error.raw_os_error().map(|code| code as u32) {
        Some(ERROR_ALREADY_EXISTS) | Some(ERROR_FILE_EXISTS) => {
            ErrorRenombrarAnclado::DestinoExistente
        }
        Some(ERROR_FILE_NOT_FOUND) | Some(ERROR_PATH_NOT_FOUND) => {
            ErrorRenombrarAnclado::FuenteAusente
        }
        Some(ERROR_CANT_RESOLVE_FILENAME) => ErrorRenombrarAnclado::EnlaceNoPermitido,
        Some(ERROR_DIRECTORY) => ErrorRenombrarAnclado::TipoNoAdmitido,
        _ => ErrorRenombrarAnclado::Io,
    }
}

#[cfg(windows)]
fn windows_not_found(error: &std::io::Error) -> bool {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};

    matches!(
        error.raw_os_error().map(|code| code as u32),
        Some(ERROR_FILE_NOT_FOUND) | Some(ERROR_PATH_NOT_FOUND)
    ) || error.kind() == std::io::ErrorKind::NotFound
}

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
mod tests {
    use super::{
        ErrorRenombrarAnclado, invocar_renameat2, renombrar_noreplace,
        renombrar_plataforma_con_operacion,
    };
    use std::cell::Cell;
    use std::ffi::{CStr, CString, OsStr};
    use std::fs::File;

    #[test]
    fn renameat2_noreplace_preserva_fuente_y_destino_existente() {
        let temp = tempfile::tempdir().unwrap();
        let fuente = temp.path().join("source");
        let destino = temp.path().join("destination");
        std::fs::create_dir(&fuente).unwrap();
        std::fs::create_dir(&destino).unwrap();
        std::fs::write(fuente.join("marker"), b"source remains").unwrap();
        std::fs::write(destino.join("marker"), b"destination remains").unwrap();
        let parent = File::open(temp.path()).unwrap();

        let resultado = renombrar_noreplace(
            &parent,
            &CString::new("source").unwrap(),
            &CString::new("destination").unwrap(),
            invocar_renameat2,
        );

        assert_eq!(resultado, Err(ErrorRenombrarAnclado::DestinoExistente));
        assert_eq!(
            std::fs::read(fuente.join("marker")).unwrap(),
            b"source remains"
        );
        assert_eq!(
            std::fs::read(destino.join("marker")).unwrap(),
            b"destination remains"
        );
    }

    #[test]
    fn destino_aparecido_despues_de_prechecks_no_es_reemplazado() {
        let temp = tempfile::tempdir().unwrap();
        let fuente = temp.path().join("source");
        let destino = temp.path().join("destination");
        std::fs::create_dir(&fuente).unwrap();
        std::fs::write(fuente.join("marker"), b"source remains").unwrap();
        let parent = File::open(temp.path()).unwrap();
        let raiz = File::open(&fuente).unwrap();
        let identidad = super::metadata_directorio(&raiz).unwrap();
        let llamadas = Cell::new(0);
        let operar = |parent: &File, fuente: &CStr, destino_nombre: &CStr| {
            llamadas.set(llamadas.get() + 1);
            std::fs::create_dir(&destino).unwrap();
            std::fs::write(destino.join("marker"), b"destination remains").unwrap();
            renombrar_noreplace(parent, fuente, destino_nombre, invocar_renameat2)
        };

        let resultado = renombrar_plataforma_con_operacion(
            &parent,
            OsStr::new("source"),
            OsStr::new("destination"),
            &raiz,
            identidad,
            operar,
        );

        assert_eq!(resultado, Err(ErrorRenombrarAnclado::DestinoExistente));
        assert_eq!(llamadas.get(), 1);
        assert_eq!(
            std::fs::read(fuente.join("marker")).unwrap(),
            b"source remains"
        );
        assert_eq!(
            std::fs::read(destino.join("marker")).unwrap(),
            b"destination remains"
        );
    }

    #[test]
    fn renameat2_no_soportado_falla_cerrado_sin_reintento_ni_fallback() {
        for (errno, esperado) in [
            (libc::ENOSYS, ErrorRenombrarAnclado::Io),
            (libc::EINVAL, ErrorRenombrarAnclado::Io),
            (libc::EIO, ErrorRenombrarAnclado::OperacionAmbigua),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let fuente = temp.path().join("source");
            let destino = temp.path().join("destination");
            std::fs::create_dir(&fuente).unwrap();
            std::fs::write(fuente.join("marker"), b"source remains").unwrap();
            let parent = File::open(temp.path()).unwrap();
            let raiz = File::open(&fuente).unwrap();
            let identidad = super::metadata_directorio(&raiz).unwrap();
            let llamadas = Cell::new(0);
            let operar = |parent: &File, fuente: &CStr, destino: &CStr| {
                renombrar_noreplace(parent, fuente, destino, |_, _, _| {
                    llamadas.set(llamadas.get() + 1);
                    (-1, Some(errno))
                })
            };

            assert_eq!(
                renombrar_plataforma_con_operacion(
                    &parent,
                    OsStr::new("source"),
                    OsStr::new("destination"),
                    &raiz,
                    identidad,
                    operar,
                ),
                Err(esperado)
            );
            assert_eq!(llamadas.get(), 1);
            assert_eq!(
                std::fs::read(fuente.join("marker")).unwrap(),
                b"source remains"
            );
            assert!(!destino.exists());
        }
    }
}
