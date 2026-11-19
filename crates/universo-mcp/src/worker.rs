//! La cuota temporal termina el proceso del motor, no sólo abandona un future.
use crate::{
    MAX_FRAME_BYTES, MAX_RESPONSE_BYTES, TOOL_DEADLINE_MS,
    dominio::{self, ServerConfig},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Peticion {
    pub nombre: String,
    pub argumentos: Value,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Respuesta {
    pub resultado: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct ErrorWorker;

pub fn ejecutar_worker(config: &ServerConfig) -> Result<(), ErrorWorker> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_FRAME_BYTES as u64 + 1)
        .read_to_end(&mut input)
        .map_err(|_| ErrorWorker)?;
    if input.len() > MAX_FRAME_BYTES {
        return Err(ErrorWorker);
    }
    let request: Peticion = serde_json::from_slice(&input).map_err(|_| ErrorWorker)?;
    let response = match dominio::ejecutar(&request.nombre, request.argumentos, config) {
        Ok(result) => Respuesta {
            resultado: Some(result),
            error: None,
        },
        Err(error) => Respuesta {
            resultado: None,
            error: Some(error.codigo.into()),
        },
    };
    let output = serde_json::to_vec(&response).map_err(|_| ErrorWorker)?;
    if output.len() > MAX_RESPONSE_BYTES {
        return Err(ErrorWorker);
    }
    std::io::stdout()
        .write_all(&output)
        .map_err(|_| ErrorWorker)?;
    Ok(())
}

pub(crate) fn consultar(
    config: ServerConfig,
    request: Peticion,
) -> Result<Respuesta, &'static str> {
    let mut command = Command::new(std::env::current_exe().map_err(|_| "worker_unavailable")?);
    command
        .arg("--worker")
        .arg("--root")
        .arg(config.root)
        .arg("--manifiesto")
        .arg(config.manifiesto)
        .arg("--invariantes")
        .arg(config.invariantes);
    // Una escritura no se corta a mitad de un apply: matar el worker dejaría
    // la transacción pendiente (con el lock) y al agente con un error por una
    // escritura que sí ocurrió. Las consultas conservan su plazo corto.
    let plazo = if crate::dominio::ESCRITURA.contains(&request.nombre.as_str()) {
        crate::WRITE_DEADLINE_MS
    } else {
        TOOL_DEADLINE_MS
    };
    supervisar(
        command,
        &serde_json::to_vec(&request).map_err(|_| "worker_request_invalid")?,
        Duration::from_millis(plazo),
    )
}
fn supervisar(
    mut command: Command,
    input: &[u8],
    timeout: Duration,
) -> Result<Respuesta, &'static str> {
    if input.len() > MAX_FRAME_BYTES {
        return Err("worker_request_limit");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "worker_unavailable")?;
    let mut stdin = child.stdin.take().ok_or("worker_unavailable")?;
    let stdout = child.stdout.take().ok_or("worker_unavailable")?;
    let child = Arc::new(Mutex::new(child));
    let (finished, wait_finished) = mpsc::channel();
    let watched = Arc::clone(&child);
    let watchdog = std::thread::spawn(move || {
        if wait_finished.recv_timeout(timeout).is_err() {
            if let Ok(mut child) = watched.lock() {
                let _ = child.kill();
            }
            true
        } else {
            false
        }
    });
    let sent = stdin.write_all(input);
    drop(stdin);
    let mut output = Vec::new();
    let read = stdout
        .take(MAX_RESPONSE_BYTES as u64 + 1)
        .read_to_end(&mut output);
    if sent.is_err() || read.is_err() || output.len() > MAX_RESPONSE_BYTES {
        if let Ok(mut process) = child.lock() {
            let _ = process.kill();
        }
    }
    // Nunca retener el mutex durante wait: incluso un worker que cierre stdout
    // y siga calculando debe seguir siendo terminable por el watchdog.
    let status = loop {
        match child.lock().map_err(|_| "worker_failed")?.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(1)),
            Err(error) => break Err(error),
        }
    };
    let _ = finished.send(());
    let expired = watchdog.join().unwrap_or(true);
    if expired {
        return Err("tool_deadline_exceeded");
    }
    if sent.is_err()
        || read.is_err()
        || output.len() > MAX_RESPONSE_BYTES
        || !status.is_ok_and(|status| status.success())
    {
        return Err("worker_failed");
    }
    serde_json::from_slice(&output).map_err(|_| "worker_failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn child(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--ignored",
                "--exact",
                "worker::tests::worker_hostil",
                "--nocapture",
            ])
            .env("FELIX_MCP_WORKER_TEST", mode);
        command
    }

    #[test]
    #[ignore = "subproceso adversarial invocado sólo por el supervisor"]
    fn worker_hostil() {
        match std::env::var("FELIX_MCP_WORKER_TEST").unwrap().as_str() {
            "timeout" => loop {
                std::thread::sleep(Duration::from_secs(1));
            },
            "flood" => {
                let block = vec![b'x'; 65536];
                loop {
                    if std::io::stdout().write_all(&block).is_err() {
                        break;
                    }
                }
            }
            _ => panic!("invalid test mode"),
        }
    }

    #[test]
    fn deadline_mata_y_recolecta_proceso_real() {
        let start = Instant::now();
        let error = supervisar(child("timeout"), b"{}", Duration::from_millis(150)).err();
        assert_eq!(error, Some("tool_deadline_exceeded"));
        // supervisar sólo retorna después de try_wait(Some(status)): hijo recolectado.
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn salida_hostil_acotada_termina_y_recolecta_proceso() {
        let start = Instant::now();
        let error = supervisar(child("flood"), b"{}", Duration::from_secs(3)).err();
        assert_eq!(error, Some("worker_failed"));
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
