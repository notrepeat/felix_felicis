use super::*;

use crate::transaccion::anclado::IdentidadArchivo;
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const ENV_MODO: &str = "FELIX_REOPEN_CANONICAL_TEST_MODE";
const ENV_ROOT: &str = "FELIX_REOPEN_CANONICAL_TEST_ROOT";
const ENV_PID_PADRE: &str = "FELIX_REOPEN_CANONICAL_TEST_PARENT_PID";
const NOMBRE_TEST: &str = "reabre_observaciones_a_b_c_en_otro_proceso";

struct ArchivoEsperado {
    bytes: Vec<u8>,
    identidad: IdentidadArchivo,
}

struct ResultadoHijo {
    status: ExitStatus,
    stdout: String,
    stderr: String,
    excedio_limite: bool,
}

#[derive(Clone, Copy)]
enum ModoReapertura {
    A,
    B,
    C,
}

impl ModoReapertura {
    fn variable(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
        }
    }

    fn marcador(self) -> &'static str {
        match self {
            Self::A => "REOPEN_OK_A",
            Self::B => "REOPEN_OK_B",
            Self::C => "REOPEN_OK_C",
        }
    }
}

#[test]
fn reabre_observaciones_a_b_c_en_otro_proceso() {
    if let Some((modo, root, pid_padre)) = variables_hijo() {
        ejecutar_rama_hija(&modo, root, pid_padre);
        return;
    }

    ejecutar_rama_padre();
}

fn ejecutar_rama_padre() {
    let filtro = filtro_test_exacta();
    for modo in [ModoReapertura::A, ModoReapertura::B, ModoReapertura::C] {
        let temp = tempdir().unwrap();
        let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
        let ubicacion = &fixture.publicada.ubicacion;
        let (intent_path, intent_tmp) = intent_paths(&fixture);
        let journal_location = ubicacion.journal();
        let journal_path = journal_location.as_path().to_path_buf();
        let context_path = contexto_path(ubicacion);
        let parent = ubicacion.ruta().parent().unwrap();
        let journal_tmp = parent.join(ubicacion.nombre_journal_temporal());
        let context_tmp =
            parent.join(nombre_temporal_sibling(context_path.file_name().unwrap()).unwrap());

        match modo {
            ModoReapertura::A => {}
            ModoReapertura::B => fs::remove_file(&journal_path).unwrap(),
            ModoReapertura::C => {
                fs::remove_file(&journal_path).unwrap();
                fs::remove_file(&context_path).unwrap();
            }
        }

        let expected_intent = capturar_archivo(&intent_path).unwrap();
        let expected_journal = capturar_archivo(&journal_path);
        let expected_context = capturar_archivo(&context_path);
        let root = fixture.publicada.root.clone();
        let root_identity = identidad_directorio(&root);
        let parent_pid = std::process::id().to_string();

        // El fixture no retiene handles. La captura de arriba cierra cada lectura;
        // el owner y el guard del publicador también se soltaron al crear el fixture.
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                filtro.as_str(),
                "--nocapture",
                "--test-threads=1",
                "--color=never",
            ])
            .env(ENV_MODO, modo.variable())
            .env(ENV_ROOT, &root)
            .env(ENV_PID_PADRE, &parent_pid);
        let output = ejecutar_con_timeout(command);
        let transcript = format!("{}{}", output.stdout, output.stderr);
        assert!(
            !output.excedio_limite,
            "child exceeded 30 seconds\n{transcript}"
        );
        assert!(output.status.success(), "{transcript}");
        assert!(transcript.contains("running 1 test"), "{transcript}");
        assert!(
            transcript.contains(&format!("test {filtro} ...")),
            "{transcript}"
        );
        assert!(transcript.contains("1 passed; 0 failed"), "{transcript}");
        assert!(!transcript.contains("0 tests"), "{transcript}");
        assert!(
            transcript.lines().any(|line| line == modo.marcador()),
            "{transcript}"
        );

        assert_archivo_igual(&intent_path, &expected_intent);
        assert_archivo_opcional_igual(&journal_path, expected_journal.as_ref());
        assert_archivo_opcional_igual(&context_path, expected_context.as_ref());
        for path in [&intent_tmp, &journal_tmp, &context_tmp] {
            assert_ausente(path);
        }
        assert!(identidad_directorio(&root) == root_identity);
        verificar_arbol(&root, &fixture.publicada.entradas);
    }
}

fn variables_hijo() -> Option<(OsString, OsString, OsString)> {
    let modo = std::env::var_os(ENV_MODO);
    let root = std::env::var_os(ENV_ROOT);
    let pid_padre = std::env::var_os(ENV_PID_PADRE);
    match (modo, root, pid_padre) {
        (None, None, None) => None,
        (Some(modo), Some(root), Some(pid_padre)) => Some((modo, root, pid_padre)),
        _ => panic!("child mode, root, and parent PID must be set together"),
    }
}

fn ejecutar_rama_hija(modo: &OsStr, root: OsString, pid_padre: OsString) {
    let Some(modo) = modo.to_str() else {
        panic!("invalid child mode");
    };
    let modo = match modo {
        "A" => ModoReapertura::A,
        "B" => ModoReapertura::B,
        "C" => ModoReapertura::C,
        _ => panic!("invalid child mode"),
    };
    let root = PathBuf::from(root);
    let parent_pid: u32 = pid_padre
        .to_str()
        .expect("parent PID must be UTF-8")
        .parse()
        .expect("parent PID must be numeric");
    assert_ne!(std::process::id(), parent_pid);

    let ubicacion = UbicacionRaiz::nueva(Path::new(&root)).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    match modo {
        ModoReapertura::A => {
            let observation = owner
                .observar_intent_canonico()
                .unwrap()
                .observar_reconciliacion_a()
                .unwrap();
            drop(observation);
        }
        ModoReapertura::B => {
            let observation = owner
                .observar_intent_canonico()
                .unwrap()
                .observar_reconciliacion_b_sin_journal()
                .unwrap();
            drop(observation);
        }
        ModoReapertura::C => {
            let observation = owner
                .observar_intent_canonico()
                .unwrap()
                .observar_reconciliacion_c_sin_sidecars()
                .unwrap();
            drop(observation);
        }
    }
    drop(owner);
    drop(lock);
    println!("\n{}", modo.marcador());
}

fn ejecutar_con_timeout(mut command: Command) -> ResultadoHijo {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });

    let deadline = Instant::now() + Duration::from_secs(30);
    let (status, excedio_limite) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (status, false),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let status = child.wait().unwrap();
                break (status, true);
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let stdout = stdout_reader.join().unwrap();
                let stderr = stderr_reader.join().unwrap();
                panic!(
                    "cannot wait for child: {error}\n{}\n{}",
                    String::from_utf8_lossy(&stdout),
                    String::from_utf8_lossy(&stderr)
                );
            }
        }
    };
    let stdout = stdout_reader.join().unwrap();
    let stderr = stderr_reader.join().unwrap();

    ResultadoHijo {
        status,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        excedio_limite,
    }
}

fn filtro_test_exacta() -> String {
    let module = module_path!();
    let prefix = concat!(env!("CARGO_CRATE_NAME"), "::");
    let module = module.strip_prefix(prefix).unwrap_or(module);
    format!("{module}::{NOMBRE_TEST}")
}

fn capturar_archivo(path: &Path) -> Option<ArchivoEsperado> {
    match fs::symlink_metadata(path) {
        Ok(_) => Some(ArchivoEsperado {
            bytes: fs::read(path).unwrap(),
            identidad: id_archivo(path),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("cannot inspect fixture file: {error}"),
    }
}

fn assert_archivo_igual(path: &Path, expected: &ArchivoEsperado) {
    assert_eq!(fs::read(path).unwrap(), expected.bytes);
    assert!(id_archivo(path) == expected.identidad);
}

fn assert_archivo_opcional_igual(path: &Path, expected: Option<&ArchivoEsperado>) {
    match expected {
        Some(expected) => assert_archivo_igual(path, expected),
        None => assert_ausente(path),
    }
}

fn assert_ausente(path: &Path) {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => panic!("expected absent path: {}", path.display()),
        Err(error) => panic!("cannot inspect absent path: {error}"),
    }
}
