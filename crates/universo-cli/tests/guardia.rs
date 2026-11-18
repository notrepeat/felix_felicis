//! `seal` y `guard`: detección de cambios hechos fuera de Felix.
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    manifest: PathBuf,
    invariants: PathBuf,
}

fn fixture(regla: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("PRIVATE_ROOT");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::write(
        root.join("n/PRIVATE_NODE.md"),
        "---\ntype: a\ntitle: PRIVATE_TITLE\n---\nPRIVATE_BODY\n",
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
    Fixture {
        _dir: dir,
        root,
        manifest,
        invariants,
    }
}

fn run(command: &str, f: &Fixture) -> (i32, Value, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .args([
            command,
            f.root.to_str().unwrap(),
            "--manifiesto",
            f.manifest.to_str().unwrap(),
            "--invariantes",
            f.invariants.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(!stdout.contains("PRIVATE"), "{stdout}");
    (
        out.status.code().unwrap(),
        serde_json::from_str(&stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn guard_falla_sin_sello_pasa_tras_seal_y_falla_tras_editar_a_mano() {
    let f = fixture("(Type @n/PRIVATE_NODE a)\n");
    let (code, value, _) = run("guard", &f);
    assert_eq!(code, 1);
    assert_eq!(value["command"], "guard");
    assert_eq!(value["status"], "fail");
    assert_eq!(value["seal"], "absent");
    assert_eq!(value["invariants"], "admit");
    assert_eq!(value["transaction"], "clean");

    let (code, value, err) = run("seal", &f);
    assert_eq!(code, 0, "{err}");
    assert_eq!(value["status"], "ok");
    let (code, value, _) = run("guard", &f);
    assert_eq!(code, 0);
    assert_eq!(value["status"], "pass");
    assert_eq!(value["seal"], "match");

    fs::write(
        f.root.join("n/PRIVATE_NODE.md"),
        "---\ntype: a\ntitle: PRIVATE_EDIT\n---\nPRIVATE_BODY\n",
    )
    .unwrap();
    let (code, value, _) = run("guard", &f);
    assert_eq!(code, 1);
    assert_eq!(value["seal"], "mismatch");
}

#[test]
fn seal_rechaza_un_estado_que_viola_las_invariantes() {
    let f = fixture("(Type @n/PRIVATE_NODE b)\n");
    let (code, value, _) = run("seal", &f);
    assert_eq!(code, 1);
    assert_eq!(value["errors"][0]["code"], "seal_invariants_rejected");
    let (code, value, _) = run("guard", &f);
    assert_eq!(code, 1);
    assert_eq!(value["invariants"], "reject");
}

#[test]
fn guard_y_seal_exigen_manifiesto_e_invariantes() {
    for command in ["guard", "seal"] {
        let out = Command::new(env!("CARGO_BIN_EXE_universo"))
            .args([command, "raiz"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{command}");
    }
}
