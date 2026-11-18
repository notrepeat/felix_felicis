//! `propose --witness`: testigo localizado opt-in de la primera regla incumplida.
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

struct Fixture {
    _dir: tempfile::TempDir,
    args: Vec<String>,
}

fn fixture(invariants: &str, delta: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    fs::create_dir_all(root.join("n")).unwrap();
    for (nodo, tipo) in [("x", "a"), ("y", "a")] {
        fs::write(
            root.join(format!("n/{nodo}.md")),
            format!("---\ntype: {tipo}\ntitle: {nodo}\n---\n"),
        )
        .unwrap();
    }
    let manifest = dir.path().join("manifest.toml");
    fs::write(
        &manifest,
        "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n",
    )
    .unwrap();
    let inv: PathBuf = dir.path().join("invariants.sx");
    fs::write(&inv, invariants).unwrap();
    let d = dir.path().join("delta.json");
    fs::write(&d, delta).unwrap();
    let args = vec![
        "propose".into(),
        root.to_string_lossy().into(),
        format!("--manifiesto={}", manifest.display()),
        format!("--invariantes={}", inv.display()),
        format!("--delta={}", d.display()),
    ];
    Fixture { _dir: dir, args }
}

fn run(f: &Fixture, extra: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(&f.args)
        .args(extra)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

const REGLAS: &str = "(Type @n/x a)\n(regla :id \"todos-a\" :mensaje \"Todo nodo es a.\" (forall ((n N)) (Type n a)))\n";
const A_B: &str = r#"{"schema":"felix-delta-v1","add":["Type(n/y, b)"],"remove":["Type(n/y, a)"]}"#;

#[test]
fn el_rechazo_trae_la_regla_y_la_asignacion_que_la_incumple() {
    let f = fixture(REGLAS, A_B);
    let (code, out) = run(&f, &["--format=json", "--witness"]);
    assert_eq!(code, 1);
    let json: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["status"], "reject");
    let w = &json["witness"];
    assert_eq!(w["source_index"], 1);
    assert_eq!(w["rule_id"], "todos-a");
    assert_eq!(w["rule_message"], "Todo nodo es a.");
    assert_eq!(
        w["bindings"],
        serde_json::json!([{"variable": "n", "kind": "node", "value": "n/y"}])
    );
    let (_, texto) = run(&f, &["--witness"]);
    assert!(texto.contains("witness: todos-a: n=n/y\n"), "{texto}");
}

#[test]
fn sin_witness_o_admitido_no_hay_rutas_en_la_salida() {
    let f = fixture(REGLAS, A_B);
    let (_, out) = run(&f, &["--format=json"]);
    assert!(!out.contains("witness") && !out.contains("n/y"), "{out}");
    let admitido = fixture(
        REGLAS,
        r#"{"schema":"felix-delta-v1","add":[],"remove":[]}"#,
    );
    let (code, out) = run(&admitido, &["--format=json", "--witness"]);
    assert_eq!(code, 0);
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap()["witness"],
        Value::Null
    );
}

#[test]
fn witness_es_solo_de_simulacion() {
    let f = fixture(REGLAS, A_B);
    let (code, _) = run(&f, &["--witness", "--witness"]);
    assert_eq!(code, 2);
    let (code, _) = run(&f, &["--apply", "--expect-snapshot", "00", "--witness"]);
    assert_eq!(code, 2);
}
