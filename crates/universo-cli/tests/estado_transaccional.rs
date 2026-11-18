//! `status`, `recover` y aviso de `check` ante estado transaccional pendiente.
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};

struct Fixture {
    dir: tempfile::TempDir,
    root: PathBuf,
    manifest: PathBuf,
    invariants: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("PRIVATE_ROOT");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::write(
        root.join("n/PRIVATE_NODE.md"),
        "---\ntype: b\ntitle: T\n---\nPRIVATE_BODY\n",
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
    Fixture {
        dir,
        root,
        manifest,
        invariants,
    }
}

fn orphan_stage(f: &Fixture) -> PathBuf {
    let stage = f.dir.path().join(".PRIVATE_ROOT.felix.stage.00ff");
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("canary"), b"x").unwrap();
    stage
}

fn universo(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn root(f: &Fixture) -> &str {
    f.root.to_str().unwrap()
}

#[test]
fn status_limpio_sale_con_0() {
    let f = fixture();
    let (code, out, err) = universo(&["status", root(&f), "--format", "json"]);
    assert_eq!(code, 0, "{err}");
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"], "status");
    assert_eq!(value["status"], "clean");
    assert_eq!(value["orphan_stage"], false);
}

#[test]
fn status_con_stage_huerfano_lo_informa_redactado_y_sale_con_1() {
    let f = fixture();
    orphan_stage(&f);
    let (code, out, _) = universo(&["status", root(&f), "--format", "json"]);
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["status"], "pending");
    assert_eq!(value["stages"], json!(1));
    assert_eq!(value["orphan_stage"], true);
    assert_eq!(value["journal"], false);
    let (code, text, _) = universo(&["status", root(&f)]);
    assert_eq!(code, 1);
    assert!(text.contains("orphan stage"), "{text}");
    for output in [&out, &text] {
        assert!(!output.contains("PRIVATE"), "{output}");
    }
}

#[test]
fn recover_limpio_sale_con_0_y_con_stage_huerfano_falla_sin_borrarlo() {
    let f = fixture();
    let (code, _, err) = universo(&["recover", root(&f)]);
    assert_eq!(code, 0, "{err}");
    let stage = orphan_stage(&f);
    let (code, out, _) = universo(&["recover", root(&f), "--format", "json"]);
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["command"], "recover");
    assert_eq!(value["errors"][0]["code"], "recovery_pending_unverified");
    assert_eq!(fs::read(stage.join("canary")).unwrap(), b"x");
    assert!(!out.contains("PRIVATE"));
}

#[test]
fn check_avisa_por_stderr_sin_cambiar_stdout_ni_codigo() {
    let f = fixture();
    let args = [
        "check",
        root(&f),
        "--manifiesto",
        f.manifest.to_str().unwrap(),
        "--invariantes",
        f.invariants.to_str().unwrap(),
    ];
    let (code_limpio, out_limpio, err_limpio) = universo(&args);
    assert_eq!(err_limpio, "");
    orphan_stage(&f);
    let (code, out, err) = universo(&args);
    assert_eq!((code, out), (code_limpio, out_limpio));
    assert!(err.contains("universo status"), "{err}");
    assert!(!err.contains("PRIVATE"), "{err}");
}

#[test]
fn status_y_recover_rechazan_argumentos_invalidos() {
    for args in [
        vec!["status"],
        vec!["recover"],
        vec!["status", "a", "b"],
        vec!["status", "a", "--format", "xml"],
    ] {
        assert_eq!(universo(&args).0, 2, "{args:?}");
    }
}

#[test]
fn status_y_recover_de_root_inexistente_no_escriben_nada() {
    let dir = tempfile::tempdir().unwrap();
    let nada = dir.path().join("PRIVATE_MISSING");
    let nada = nada.to_str().unwrap();
    let (code, out, _) = universo(&["status", nada, "--format", "json"]);
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["status"], "missing");
    let (code, out, _) = universo(&["recover", nada, "--format", "json"]);
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["errors"][0]["code"], "recover_root_absent");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}
