//! `propose` con `felix-delta-v2`: simula la creación de notas; el apply la
//! rechaza hasta que el recovery sepa reproducirla.
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    manifest: PathBuf,
    invariants: PathBuf,
    delta: PathBuf,
}

fn fixture(regla: &str, delta: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("PRIVATE_ROOT");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::write(
        root.join("n/existente.md"),
        "---\ntype: a\ntitle: Existente\n---\n",
    )
    .unwrap();
    let manifest = dir.path().join("manifest.toml");
    fs::write(
        &manifest,
        "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n",
    )
    .unwrap();
    let invariants = dir.path().join("invariants.sx");
    fs::write(&invariants, regla).unwrap();
    let delta_path = dir.path().join("delta.json");
    fs::write(&delta_path, delta).unwrap();
    Fixture {
        _dir: dir,
        root,
        manifest,
        invariants,
        delta: delta_path,
    }
}

fn propose(f: &Fixture, extra: &[&str]) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .arg("propose")
        .arg(&f.root)
        .arg("--manifiesto")
        .arg(&f.manifest)
        .arg("--invariantes")
        .arg(&f.invariants)
        .arg("--delta")
        .arg(&f.delta)
        .args(["--format", "json"])
        .args(extra)
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    (
        out.status.code().unwrap(),
        serde_json::from_str(&stdout).unwrap(),
    )
}

const CREAR_B: &str = r#"{"schema":"felix-delta-v2","create":[{"path":"n/nueva","type":"b","title":"Nueva","body":"Texto.\n"}],"add":[],"remove":[]}"#;

#[test]
fn simular_creacion_evalua_las_invariantes_con_la_nota() {
    let admite = fixture("(Type @n/existente a)\n", CREAR_B);
    let (code, value) = propose(&admite, &[]);
    assert_eq!(
        (code, &value["status"]),
        (0, &Value::from("admit")),
        "{value}"
    );
    let rechaza = fixture("(forall ((x N)) (Type x a))\n", CREAR_B);
    let (code, value) = propose(&rechaza, &[]);
    assert_eq!(code, 1);
    assert_eq!(value["status"], "reject");
    assert_eq!(value["summary"]["failed_invariants"], 1);
}

#[test]
fn crear_una_nota_existente_es_error_tecnico() {
    let f = fixture(
        "(Type @n/existente a)\n",
        r#"{"schema":"felix-delta-v2","create":[{"path":"n/existente","type":"a","title":"Otra"}],"add":[],"remove":[]}"#,
    );
    let (code, value) = propose(&f, &[]);
    assert_eq!(code, 1);
    assert_eq!(value["status"], "error");
    assert_eq!(value["errors"][0]["code"], "note_exists");
}

#[test]
fn el_apply_con_notas_se_rechaza_sin_tocar_el_bundle() {
    let f = fixture("(Type @n/existente a)\n", CREAR_B);
    let (_, value) = propose(&f, &["--emit-snapshot-token"]);
    let token = value["snapshot_token"].as_str().unwrap().to_string();
    let (code, value) = propose(&f, &["--apply", "--expect-snapshot", &token]);
    assert_eq!(code, 1);
    assert_eq!(value["errors"][0]["code"], "apply_notes_unsupported");
    assert!(!f.root.join("n/nueva.md").exists());
}
