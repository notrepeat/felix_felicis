use std::io::{BufRead, BufReader, Write};
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
