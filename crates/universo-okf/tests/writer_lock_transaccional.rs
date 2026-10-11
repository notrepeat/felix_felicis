use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use universo_okf::transaccion::lock::{ErrorLockEscritura, LockEscritura};

const CHILD_ROOT_ENV: &str = "FELIX_WRITER_LOCK_CHILD_ROOT";

#[test]
fn child_process_holds_writer_lock_for_parent_test() {
    let Some(root) = std::env::var_os(CHILD_ROOT_ENV) else {
        return;
    };

    let _guard = LockEscritura::adquirir(root).expect("child acquires writer lock");
    println!("LOCK_READY");
    std::io::stdout().flush().unwrap();
    std::thread::sleep(Duration::from_secs(30));
}

#[test]
fn procesos_distintos_reportan_busy_y_la_terminacion_libera_el_lock() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    std::fs::create_dir(&root).unwrap();

    let mut child = ChildGuard::spawn(&root);
    child.readiness_line();
    let error = LockEscritura::adquirir(&root).unwrap_err();
    assert_eq!(error, ErrorLockEscritura::Busy);
    assert_eq!(error.codigo(), "writer_lock_busy");

    child.terminate_within(Duration::from_secs(5));
    let guard = LockEscritura::adquirir(&root).unwrap();
    assert_eq!(guard.raiz_canonica(), std::fs::canonicalize(root).unwrap());
}

#[test]
fn raiz_symlink_se_rechaza_con_error_redactado() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        let alias = temp.path().join("alias");
        std::fs::create_dir(&target).unwrap();
        symlink(&target, &alias).unwrap();

        let error = LockEscritura::adquirir(&alias).unwrap_err();
        assert_eq!(error, ErrorLockEscritura::Io);
        assert_eq!(error.to_string(), "writer_lock_io");
        assert!(!error.to_string().contains(alias.to_string_lossy().as_ref()));
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_dir;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        let alias = temp.path().join("alias");
        std::fs::create_dir(&target).unwrap();
        if let Err(error) = symlink_dir(&target, &alias) {
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314)
            {
                eprintln!(
                    "skip symlink fixture: privilege 1314; junction coverage runs separately"
                );
                return;
            }
            panic!(
                "could not create symlink test fixture: {:?} / {:?}",
                error.kind(),
                error.raw_os_error()
            );
        }

        assert_eq!(
            LockEscritura::adquirir(&alias).unwrap_err(),
            ErrorLockEscritura::Io
        );
    }
}

#[test]
fn alias_parent_converge_en_el_mismo_lock() {
    let temp = tempfile::tempdir().unwrap();
    let detour = temp.path().join("detour");
    let root = temp.path().join("bundle");
    std::fs::create_dir(&detour).unwrap();
    std::fs::create_dir(&root).unwrap();

    let owner = LockEscritura::adquirir(&root).unwrap();
    let alias = detour.join("..").join("bundle");
    let contender = LockEscritura::adquirir(alias).unwrap_err();

    assert_eq!(owner.raiz_canonica(), std::fs::canonicalize(root).unwrap());
    assert_eq!(contender, ErrorLockEscritura::Busy);
}

#[test]
fn raiz_vacia_ausente_o_archivo_devuelve_io_redactado() {
    let temp = tempfile::tempdir().unwrap();
    let absent = temp.path().join("missing-root");
    let file = temp.path().join("not-a-directory");
    std::fs::write(&file, b"not a root").unwrap();

    for path in [Path::new(""), absent.as_path(), file.as_path()] {
        let error = LockEscritura::adquirir(path).unwrap_err();
        assert_eq!(error, ErrorLockEscritura::Io);
        assert_eq!(error.to_string(), "writer_lock_io");
        assert_eq!(
            format!("{error:?}"),
            "ErrorLockEscritura(\"writer_lock_io\")"
        );
        assert!(
            !error
                .to_string()
                .contains(absent.to_string_lossy().as_ref())
        );
        assert!(!format!("{error:?}").contains(file.to_string_lossy().as_ref()));
    }
}

#[test]
fn raices_independientes_pueden_mantener_locks_simultaneos() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();

    let first_guard = LockEscritura::adquirir(&first).unwrap();
    let second_guard = LockEscritura::adquirir(&second).unwrap();

    assert_ne!(first_guard.raiz_canonica(), second_guard.raiz_canonica());
}

#[cfg(unix)]
#[test]
fn symlink_ancestro_antes_de_parent_se_rechaza() {
    use std::os::unix::fs::symlink;

    let (temp, alias) = preparar_alias_ancestro();
    symlink(temp.path().join("target/inside"), &alias).unwrap();
    assert_alias_ancestro_rechazado(&temp, &alias);
}

#[cfg(windows)]
#[test]
fn junction_ancestro_antes_de_parent_se_rechaza() {
    let (temp, alias) = preparar_alias_ancestro();
    crear_junction(&alias, &temp.path().join("target/inside"));
    assert_alias_ancestro_rechazado(&temp, &alias);
}

fn preparar_alias_ancestro() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("target/inside")).unwrap();
    // El destino correcto según el filesystem existe, al igual que el destino
    // incorrecto de una reducción léxica que ignore el symlink/junction.
    std::fs::create_dir(temp.path().join("target/bundle")).unwrap();
    std::fs::create_dir(temp.path().join("bundle")).unwrap();
    let alias = temp.path().join("alias");
    (temp, alias)
}

fn assert_alias_ancestro_rechazado(temp: &tempfile::TempDir, alias: &Path) {
    let root = alias.join("..").join("bundle");
    let error = LockEscritura::adquirir(&root).unwrap_err();
    assert_eq!(error, ErrorLockEscritura::Io);
    assert_eq!(error.to_string(), "writer_lock_io");
    assert!(!format!("{error:?}").contains(temp.path().to_string_lossy().as_ref()));
}

#[cfg(windows)]
fn crear_junction(link: &Path, target: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    std::fs::create_dir(link).unwrap();
    let target = std::fs::canonicalize(target).unwrap();
    let target = target.to_string_lossy();
    let target = target.strip_prefix("\\\\?\\").unwrap_or(&target);
    let substitute = format!("\\??\\{target}");
    let substitute: Vec<u16> = std::ffi::OsStr::new(&substitute).encode_wide().collect();
    let print_name: Vec<u16> = std::ffi::OsStr::new(target).encode_wide().collect();
    let print_offset = ((substitute.len() + 1) * 2) as u16;
    let path_bytes = (substitute.len() + print_name.len() + 2) * 2;
    let data_length = 8 + path_bytes;
    let mut buffer = Vec::with_capacity(16 + path_bytes);
    buffer.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buffer.extend_from_slice(&(data_length as u16).to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&((substitute.len() * 2) as u16).to_le_bytes());
    buffer.extend_from_slice(&print_offset.to_le_bytes());
    buffer.extend_from_slice(&((print_name.len() * 2) as u16).to_le_bytes());
    for unit in substitute
        .iter()
        .chain(std::iter::once(&0u16))
        .chain(print_name.iter())
        .chain(std::iter::once(&0u16))
    {
        buffer.extend_from_slice(&unit.to_le_bytes());
    }

    let link_wide: Vec<u16> = link
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: the path is nul-terminated; the reparse buffer follows the mount-point ABI.
    let handle = unsafe {
        CreateFileW(
            link_wide.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(
        handle, INVALID_HANDLE_VALUE,
        "open junction for reparse setup"
    );
    let mut bytes_returned = 0;
    // SAFETY: the input buffer is valid for its declared length and output is unused.
    let result = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            buffer.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };
    // SAFETY: handle was returned by CreateFileW above and is closed exactly once.
    unsafe { CloseHandle(handle) };
    assert_ne!(result, 0, "create directory junction reparse point");
}

struct ChildGuard {
    child: Child,
    output: mpsc::Receiver<Result<(), String>>,
}

impl ChildGuard {
    fn spawn(root: &std::path::Path) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("child_process_holds_writer_lock_for_parent_test")
            .arg("--nocapture")
            .env(CHILD_ROOT_ENV, root)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn current integration-test executable");

        let stdout = child.stdout.take().expect("piped child stdout");
        let (sender, output) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        let _ = sender.send(Err("child exited before lock readiness".into()));
                        break;
                    }
                    Ok(_) if line.contains("LOCK_READY") => {
                        let _ = sender.send(Ok(()));
                        break;
                    }
                    Ok(_) => {}
                    Err(_) => {
                        let _ = sender.send(Err("could not read child readiness".into()));
                        break;
                    }
                }
            }
        });
        Self { child, output }
    }

    fn readiness_line(&self) {
        self.output
            .recv_timeout(Duration::from_secs(10))
            .expect("child lock readiness arrives within bounded wait")
            .expect("child writes readiness line");
    }

    fn terminate_within(&mut self, timeout: Duration) {
        self.child.kill().expect("terminate child holding lock");
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait().expect("poll child termination") {
                Some(_) => return,
                None if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                None => panic!("child termination exceeded bounded wait"),
            }
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
