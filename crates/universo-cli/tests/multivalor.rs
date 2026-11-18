//! C3 de punta a punta: apply transaccional sobre un atributo multivalor.
use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn apply_agrega_un_valor_multivalor_y_la_guardia_queda_en_verde() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::write(
        root.join("n/x.md"),
        "---\ntype: a\ntitle: X\ntemas: [red]\n---\n",
    )
    .unwrap();
    let manifest = dir.path().join("manifest.toml");
    fs::write(
        &manifest,
        "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patrón\"]\n[[predicados]]\nnombre = \"Tema\"\nclave = \"temas\"\ndominio = [\"cache\", \"red\"]\nmultivalor = true\n[layout]\ndirectorios = [\"n\"]\n",
    )
    .unwrap();
    let inv = dir.path().join("inv.sx");
    fs::write(&inv, "(Tema @n/x red)\n").unwrap();
    let delta = dir.path().join("d.json");
    fs::write(
        &delta,
        r#"{"schema":"felix-delta-v1","add":["Tema(n/x, cache)"],"remove":[]}"#,
    )
    .unwrap();
    let base = |extra: &[&str]| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_universo"));
        c.args([
            "propose",
            root.to_str().unwrap(),
            "--manifiesto",
            manifest.to_str().unwrap(),
            "--invariantes",
            inv.to_str().unwrap(),
            "--delta",
            delta.to_str().unwrap(),
            "--format",
            "json",
        ]);
        c.args(extra);
        let out = c.output().unwrap();
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    let token = base(&["--emit-snapshot-token"])["snapshot_token"]
        .as_str()
        .unwrap()
        .to_string();
    let aplicado = base(&["--apply", "--expect-snapshot", &token]);
    assert_eq!(aplicado["status"], "admit", "{aplicado}");
    assert_eq!(aplicado["sealed"], true);
    let texto = fs::read_to_string(root.join("n/x.md")).unwrap();
    assert!(texto.contains("temas: [cache, red]\n"), "{texto}");
}
