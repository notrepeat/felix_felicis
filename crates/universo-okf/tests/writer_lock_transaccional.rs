use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use universo_okf::transaccion::lock::{ErrorLockEscritura, LockEscritura};

const CHILD_ROOT_ENV: &str = "FELIX_WRITER_LOCK_CHILD_ROOT";
const RUNTIME_ROOT_ENV: &str = "FELIX_WRITER_LOCK_RUNTIME_ROOT";
const RUNTIME_READY_DIR_ENV: &str = "FELIX_WRITER_LOCK_RUNTIME_READY_DIR";
const RUNTIME_START_ENV: &str = "FELIX_WRITER_LOCK_RUNTIME_START";
const RUNTIME_TRACE_ENV: &str = "FELIX_WRITER_LOCK_RUNTIME_TRACE";
const RUNTIME_WORKER_ENV: &str = "FELIX_WRITER_LOCK_RUNTIME_WORKER";
const RUNTIME_WRITERS_ENV: &str = "FELIX_WRITER_LOCK_RUNTIME_WRITERS";
const RUNTIME_CRITICAL_MS_ENV: &str = "FELIX_WRITER_LOCK_RUNTIME_CRITICAL_MS";
const RUNTIME_MAX_ATTEMPTS: usize = 512;
const RUNTIME_CRITICAL_MS: u64 = 40;

#[test]
fn child_runtime_contiende_por_writer_lock() {
    let Some(root) = std::env::var_os(RUNTIME_ROOT_ENV) else {
        return;
    };
    let ready_dir = std::env::var_os(RUNTIME_READY_DIR_ENV)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("runtime readiness configuration missing"));
    let start = std::env::var_os(RUNTIME_START_ENV)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("runtime start configuration missing"));
    let trace = std::env::var_os(RUNTIME_TRACE_ENV)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("runtime trace configuration missing"));
    let worker = runtime_env_usize(RUNTIME_WORKER_ENV);
    let writers = runtime_env_usize(RUNTIME_WRITERS_ENV);
    let critical_ms = runtime_env_usize(RUNTIME_CRITICAL_MS_ENV) as u64;
    let mut trace = std::fs::OpenOptions::new()
        .append(true)
        .open(trace)
        .unwrap_or_else(|_| panic!("runtime trace setup failed"));

    std::fs::write(ready_dir.join(format!("ready-{worker}")), b"ready")
        .unwrap_or_else(|_| panic!("runtime readiness signal failed"));
    esperar_archivo(&start, Duration::from_secs(30));

    let espera_inicio = Instant::now();
    let mut busy = 0;
    for intento in 1..=RUNTIME_MAX_ATTEMPTS {
        match LockEscritura::adquirir(&root) {
            Ok(guard) => {
                let espera_us = espera_inicio.elapsed().as_micros();
                escribir_evento_runtime(&mut trace, worker as u8);
                std::thread::sleep(Duration::from_millis(critical_ms));
                escribir_evento_runtime(&mut trace, (writers + worker) as u8);
                drop(guard);
                println!(
                    "LOCK_RUNTIME_METRIC worker={worker} wait_us={espera_us} busy={busy} attempts={intento} acquisitions=1"
                );
                return;
            }
            Err(ErrorLockEscritura::Busy) => {
                busy += 1;
                let retroceso_ms = (1_u64 << busy.min(4)) + ((worker + busy) % 3) as u64;
                std::thread::sleep(Duration::from_millis(retroceso_ms));
            }
            Err(error) => panic!("runtime lock acquisition failed: {error}"),
        }
    }

    panic!("runtime lock retry limit exhausted");
}

#[test]
#[ignore = "harness de runtime multiproceso; ejecutar en release con --ignored --nocapture"]
fn runtime_lock_mide_contencion_multiproceso() {
    for writers in [2, 8] {
        runtime_lock_scenario(writers);
    }
}

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

fn runtime_env_usize(name: &str) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("runtime worker configuration invalid"))
}

fn esperar_archivo(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        if Instant::now() >= deadline {
            panic!("runtime start barrier timed out");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn escribir_evento_runtime(trace: &mut std::fs::File, event: u8) {
    // Un byte por llamada append deja un rastro ordenable entre procesos.
    trace
        .write_all(&[event])
        .unwrap_or_else(|_| panic!("runtime event trace write failed"));
}

fn runtime_lock_scenario(writers: usize) {
    let temp = tempfile::tempdir().unwrap_or_else(|_| panic!("runtime fixture setup failed"));
    let root = temp.path().join("bundle");
    let ready_dir = temp.path().join("ready");
    let start = temp.path().join("start");
    let trace_path = temp.path().join("events");
    std::fs::create_dir(&root).unwrap_or_else(|_| panic!("runtime fixture setup failed"));
    std::fs::create_dir(&ready_dir).unwrap_or_else(|_| panic!("runtime fixture setup failed"));
    std::fs::File::create(&trace_path).unwrap_or_else(|_| panic!("runtime fixture setup failed"));
    drop(LockEscritura::adquirir(&root).unwrap_or_else(|_| panic!("runtime lock warmup failed")));

    let mut children = (0..writers)
        .map(|worker| RuntimeChild::spawn(worker, writers, &root, &ready_dir, &start, &trace_path))
        .collect::<Vec<_>>();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !(0..writers).all(|worker| ready_dir.join(format!("ready-{worker}")).is_file()) {
        if Instant::now() >= deadline {
            panic!("runtime workers did not reach the start barrier");
        }
        std::thread::sleep(Duration::from_millis(2));
    }

    // El spawn y el calentamiento del lock quedan fuera de la ventana medida.
    let total_inicio = Instant::now();
    std::fs::write(&start, b"go").unwrap_or_else(|_| panic!("runtime start signal failed"));
    let mut workers = vec![None; writers];
    for child in &mut children {
        let output = child.finish(Duration::from_secs(30));
        assert!(
            output.status.success(),
            "runtime worker failed; details redacted"
        );
        let metric = parse_runtime_metric(&output);
        assert!(metric.worker < writers, "runtime worker id out of range");
        assert_eq!(metric.acquisitions, 1, "worker must acquire exactly once");
        assert_eq!(
            metric.attempts,
            metric.busy + 1,
            "attempt counters disagree"
        );
        assert!(
            workers[metric.worker].replace(metric).is_none(),
            "runtime worker emitted duplicate metrics"
        );
    }
    let total = total_inicio.elapsed();
    let workers = workers
        .into_iter()
        .map(|metric| metric.unwrap_or_else(|| panic!("runtime worker metrics missing")))
        .collect::<Vec<_>>();
    let busy_attempts = workers.iter().map(|metric| metric.busy).sum::<usize>();
    let attempts = workers.iter().map(|metric| metric.attempts).sum::<usize>();
    let acquisitions = workers
        .iter()
        .map(|metric| metric.acquisitions)
        .sum::<usize>();
    let wait_total_us = workers.iter().map(|metric| metric.wait_us).sum::<u128>();
    let wait_max_us = workers
        .iter()
        .map(|metric| metric.wait_us)
        .max()
        .unwrap_or_default();
    let events =
        std::fs::read(&trace_path).unwrap_or_else(|_| panic!("runtime event trace read failed"));
    let (max_simultaneous, traced_acquisitions, trace_valid) =
        resumir_eventos_runtime(&events, writers);
    let busy_rate = busy_attempts as f64 * 100.0 / attempts as f64;

    println!(
        "Runtime lock: writers={writers}, total_ms={:.3}, wait_sum_ms={:.3}, wait_avg_ms={:.3}, wait_max_ms={:.3}, attempts={attempts}, busy_attempts={busy_attempts}, busy_rate={busy_rate:.1}%, acquisitions={acquisitions}, max_simultaneous={max_simultaneous}, critical_ms={RUNTIME_CRITICAL_MS} (release, orientativo, sin SLA)",
        total.as_secs_f64() * 1_000.0,
        wait_total_us as f64 / 1_000.0,
        wait_total_us as f64 / writers as f64 / 1_000.0,
        wait_max_us as f64 / 1_000.0,
    );
    assert_eq!(acquisitions, writers, "each process must acquire once");
    assert_eq!(
        traced_acquisitions, writers,
        "event trace must cover each writer"
    );
    assert!(trace_valid, "runtime event trace is incomplete");
    assert_eq!(max_simultaneous, 1, "writer critical sections overlapped");
}

fn parse_runtime_metric(output: &Output) -> RuntimeWorkerMetric {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|line| line.starts_with("LOCK_RUNTIME_METRIC "))
        .unwrap_or_else(|| panic!("runtime worker metrics missing"));
    RuntimeWorkerMetric {
        worker: runtime_metric_value(line, "worker"),
        wait_us: runtime_metric_value(line, "wait_us"),
        busy: runtime_metric_value(line, "busy"),
        attempts: runtime_metric_value(line, "attempts"),
        acquisitions: runtime_metric_value(line, "acquisitions"),
    }
}

fn runtime_metric_value<T: std::str::FromStr>(line: &str, key: &str) -> T {
    line.split_whitespace()
        .filter_map(|field| field.split_once('='))
        .find_map(|(name, value)| (name == key).then_some(value))
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("runtime worker metric invalid"))
}

fn resumir_eventos_runtime(events: &[u8], writers: usize) -> (usize, usize, bool) {
    let mut active = vec![false; writers];
    let mut enters = vec![0; writers];
    let mut exits = vec![0; writers];
    let mut maximum = 0;
    let mut valid = true;

    for event in events {
        let event = usize::from(*event);
        let (worker, entering) = if event < writers {
            (event, true)
        } else if event < writers * 2 {
            (event - writers, false)
        } else {
            valid = false;
            continue;
        };
        if entering {
            enters[worker] += 1;
            valid &= !active[worker];
            active[worker] = true;
        } else {
            exits[worker] += 1;
            valid &= active[worker];
            active[worker] = false;
        }
        maximum = maximum.max(active.iter().filter(|inside| **inside).count());
    }

    valid &= active.iter().all(|inside| !inside);
    valid &= enters.iter().all(|count| *count == 1);
    valid &= exits.iter().all(|count| *count == 1);
    (maximum, enters.iter().sum(), valid)
}

#[derive(Clone, Copy)]
struct RuntimeWorkerMetric {
    worker: usize,
    wait_us: u128,
    busy: usize,
    attempts: usize,
    acquisitions: usize,
}

struct RuntimeChild {
    child: Option<Child>,
}

impl RuntimeChild {
    fn spawn(
        worker: usize,
        writers: usize,
        root: &Path,
        ready_dir: &Path,
        start: &Path,
        trace: &Path,
    ) -> Self {
        let executable =
            std::env::current_exe().unwrap_or_else(|_| panic!("runtime test executable missing"));
        let child = Command::new(executable)
            .arg("--exact")
            .arg("child_runtime_contiende_por_writer_lock")
            .arg("--nocapture")
            .env(RUNTIME_ROOT_ENV, root)
            .env(RUNTIME_READY_DIR_ENV, ready_dir)
            .env(RUNTIME_START_ENV, start)
            .env(RUNTIME_TRACE_ENV, trace)
            .env(RUNTIME_WORKER_ENV, worker.to_string())
            .env(RUNTIME_WRITERS_ENV, writers.to_string())
            .env(RUNTIME_CRITICAL_MS_ENV, RUNTIME_CRITICAL_MS.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|_| panic!("could not launch runtime worker"));
        Self { child: Some(child) }
    }

    fn finish(&mut self, timeout: Duration) -> Output {
        let deadline = Instant::now() + timeout;
        loop {
            match self
                .child
                .as_mut()
                .unwrap_or_else(|| panic!("runtime worker already collected"))
                .try_wait()
            {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(None) | Err(_) => {
                    if let Some(child) = self.child.as_mut() {
                        let _ = child.kill();
                    }
                    panic!("runtime worker timed out or could not be polled");
                }
            }
        }
        self.child
            .take()
            .unwrap_or_else(|| panic!("runtime worker already collected"))
            .wait_with_output()
            .unwrap_or_else(|_| panic!("runtime worker output unavailable"))
    }
}

impl Drop for RuntimeChild {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
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
