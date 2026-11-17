use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn negocia_version_desconocida_y_eof_cierra_limpiamente() {
    let directory = tempfile::tempdir().unwrap();
    // No tool runs: handshake must not need or inspect a bundle.
    let mut child = Command::new(env!("CARGO_BIN_EXE_universo-mcp"))
        .arg("--root")
        .arg(directory.path())
        .arg("--manifiesto")
        .arg(directory.path().join("missing.toml"))
        .arg("--invariantes")
        .arg(directory.path().join("missing.sx"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    writeln!(stdin, "{}", json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"1900-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1"}
    }})).unwrap();
    stdin.flush().unwrap();
    let reply = rx.recv_timeout(Duration::from_secs(5));
    if reply.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let reply: Value = serde_json::from_str(&reply.unwrap()).unwrap();
    assert_eq!(reply["result"]["protocolVersion"], "2025-11-25");
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    stdin.flush().unwrap();
    drop(stdin);
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("EOF did not terminate the MCP server");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        rx.into_iter()
            .all(|line| serde_json::from_str::<Value>(&line).is_ok())
    );
}
