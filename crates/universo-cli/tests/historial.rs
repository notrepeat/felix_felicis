//! `propose --apply --author/--reason` y `universo log`.
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    manifest: PathBuf,
    invariants: PathBuf,
    delta: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("PRIVATE_ROOT");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::write(
        root.join("n/PRIVATE_NODE.md"),
        "---\ntype: a\ntitle: PRIVATE_LABEL\n---\nPRIVATE_BODY\n",
    )
    .unwrap();
    let manifest = dir.path().join("manifest.toml");
    fs::write(
        &manifest,
        "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n",
    )
    .unwrap();
    let invariants = dir.path().join("invariants.sx");
    fs::write(&invariants, "(Type @n/PRIVATE_NODE b)\n").unwrap();
    let delta = dir.path().join("delta.json");
    fs::write(
        &delta,
        r#"{"schema":"felix-delta-v1","add":["Type(n/PRIVATE_NODE, b)"],"remove":["Type(n/PRIVATE_NODE, a)"]}"#,
    )
    .unwrap();
    Fixture {
        _dir: dir,
        root,
        manifest,
        invariants,
        delta,
    }
}

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

fn propose<'a>(f: &'a Fixture, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "propose",
        f.root.to_str().unwrap(),
        "--manifiesto",
        f.manifest.to_str().unwrap(),
        "--invariantes",
        f.invariants.to_str().unwrap(),
        "--delta",
        f.delta.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    args
}

#[test]
fn apply_con_autor_y_motivo_queda_en_el_log() {
    let f = fixture();
    let (_, out) = universo(&propose(&f, &["--format", "json", "--emit-snapshot-token"]));
    let token: Value = serde_json::from_str(&out).unwrap();
    let token = token["snapshot_token"].as_str().unwrap().to_string();
    let (code, out) = universo(&propose(
        &f,
        &[
            "--format",
            "json",
            "--apply",
            "--expect-snapshot",
            &token,
            "--author",
            "Romeo",
            "--reason",
            "Corrige el tipo",
        ],
    ));
    assert_eq!(code, 0, "{out}");
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["logged"], true);
    let (code, out) = universo(&["log", f.root.to_str().unwrap(), "--format", "json"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains(&token), "el log no muestra el token");
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["command"], "log");
    let entries = value["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["author"], "Romeo");
    assert_eq!(entries[0]["reason"], "Corrige el tipo");
    assert_eq!(entries[0]["sealed"], true);
    assert!(entries[0]["time"].as_str().unwrap().ends_with('Z'));
    let (code, text) = universo(&["log", f.root.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(
        text.contains("Romeo") && text.contains("Corrige el tipo"),
        "{text}"
    );
}

#[test]
fn autor_y_motivo_solo_con_apply_y_log_vacio_sale_con_0() {
    let f = fixture();
    assert_eq!(universo(&propose(&f, &["--author", "Romeo"])).0, 2);
    assert_eq!(universo(&propose(&f, &["--reason", "x"])).0, 2);
    let (code, out) = universo(&["log", f.root.to_str().unwrap(), "--format", "json"]);
    assert_eq!(code, 0);
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["entries"], serde_json::json!([]));
    assert_eq!(universo(&["log"]).0, 2);
}
