//! `universo show <raiz> <n>`: el historial guarda el delta aplicado y lo muestra.
use serde_json::Value;
use std::{path::Path, process::Command};

fn universo(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

fn aplicar(kb: &Path, delta: &str, motivo: &str) {
    let archivo = kb.join("delta.json");
    std::fs::write(&archivo, delta).unwrap();
    let base = [
        "propose".to_string(),
        kb.join("bundle").display().to_string(),
        format!("--manifiesto={}", kb.join("manifest.toml").display()),
        format!("--invariantes={}", kb.join("invariants.sx").display()),
        format!("--delta={}", archivo.display()),
        "--format=json".to_string(),
    ];
    let args: Vec<&str> = base.iter().map(String::as_str).collect();
    let mut sim = args.clone();
    sim.push("--emit-snapshot-token");
    let (_, salida) = universo(&sim);
    let token = serde_json::from_str::<Value>(&salida).unwrap()["snapshot_token"]
        .as_str()
        .unwrap()
        .to_string();
    let esperado = format!("--expect-snapshot={token}");
    let razon = format!("--reason={motivo}");
    let mut apply = args.clone();
    apply.extend(["--apply", &esperado, "--author=ana", &razon]);
    let (codigo, salida) = universo(&apply);
    assert_eq!(codigo, 0, "{salida}");
}

#[test]
fn show_muestra_el_delta_de_cada_cambio_y_log_no() {
    let dir = tempfile::tempdir().unwrap();
    let kb = dir.path().join("kb");
    assert_eq!(universo(&["init", kb.to_str().unwrap()]).0, 0);
    aplicar(
        &kb,
        r#"{"schema":"felix-delta-v1","add":["Estado(notas/bienvenida, vigente)"],"remove":["Estado(notas/bienvenida, borrador)"]}"#,
        "activar",
    );
    aplicar(
        &kb,
        r#"{"schema":"felix-delta-v2","create":[{"path":"notas/nueva","type":"fuente","title":"Nueva"}],"update":[],"add":[],"remove":[]}"#,
        "crear nota",
    );
    let raiz = kb.join("bundle");
    let raiz = raiz.to_str().unwrap();

    let (codigo, texto) = universo(&["show", raiz, "1"]);
    assert_eq!(codigo, 0);
    assert!(texto.contains("show: entry 1 of 2\n"), "{texto}");
    assert!(texto.contains("reason: activar\n"));
    assert!(texto.contains("- Estado(notas/bienvenida, borrador)\n"));
    assert!(texto.contains("+ Estado(notas/bienvenida, vigente)\n"));

    let (_, texto) = universo(&["show", raiz, "2"]);
    assert!(
        texto.contains("create notas/nueva type=fuente title=Nueva\n"),
        "{texto}"
    );

    let (codigo, json) = universo(&["show", raiz, "1", "--format=json"]);
    assert_eq!(codigo, 0);
    let json: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json["entry"], 1);
    assert_eq!(json["author"], "ana");
    assert_eq!(
        json["delta"]["add"],
        serde_json::json!(["Estado(notas/bienvenida, vigente)"])
    );

    // `log` sigue sin mostrar datos del bundle.
    let (_, log) = universo(&["log", raiz, "--format=json"]);
    assert!(!log.contains("Estado(") && !log.contains("delta"), "{log}");

    assert_eq!(universo(&["show", raiz, "3"]).0, 1);
    assert!(
        universo(&["show", raiz, "3", "--format=json"])
            .1
            .contains("entry_not_found")
    );
    assert_eq!(universo(&["show", raiz, "0"]).0, 2);
    assert_eq!(universo(&["show", raiz]).0, 2);
}

#[test]
fn una_entrada_sin_delta_se_muestra_igual() {
    let dir = tempfile::tempdir().unwrap();
    let kb = dir.path().join("kb");
    assert_eq!(universo(&["init", kb.to_str().unwrap()]).0, 0);
    // Una línea escrita antes de que el historial guardara el delta.
    let log = kb.join(".bundle.felix.log");
    std::fs::write(&log, "{\"schema\":\"felix-log-v1\",\"time\":\"2026-11-17T10:00:00Z\",\"author\":null,\"reason\":null,\"sealed\":true,\"state\":null}\n").unwrap();
    let (codigo, texto) = universo(&["show", kb.join("bundle").to_str().unwrap(), "1"]);
    assert_eq!(codigo, 0);
    assert!(texto.contains("delta: not recorded\n"), "{texto}");
}

#[test]
fn show_escapa_controles_y_distingue_el_delta_omitido() {
    let dir = tempfile::tempdir().unwrap();
    let kb = dir.path().join("kb");
    assert_eq!(universo(&["init", kb.to_str().unwrap()]).0, 0);
    aplicar(
        &kb,
        "{\"schema\":\"felix-delta-v2\",\"create\":[{\"path\":\"notas/x\",\"type\":\"fuente\",\"title\":\"Hola\\u001b[31mROJO\"}],\"update\":[],\"add\":[],\"remove\":[]}",
        "escape",
    );
    let raiz = kb.join("bundle");
    let (_, texto) = universo(&["show", raiz.to_str().unwrap(), "1"]);
    assert!(!texto.contains('\u{1b}'), "{texto:?}");
    assert!(texto.contains("title=Hola\\u{1b}[31mROJO"), "{texto}");
    let log = kb.join(".bundle.felix.log");
    std::fs::write(&log, "{\"schema\":\"felix-log-v1\",\"time\":\"2026-11-17T10:00:00Z\",\"author\":null,\"reason\":null,\"sealed\":true,\"state\":null,\"delta\":null,\"delta_omitted\":true}\n").unwrap();
    let (_, texto) = universo(&["show", raiz.to_str().unwrap(), "1"]);
    assert!(texto.contains("delta: omitted (too large)\n"), "{texto}");
}
