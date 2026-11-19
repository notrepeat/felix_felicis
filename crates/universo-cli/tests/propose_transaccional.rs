use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture {
    dir: tempfile::TempDir,
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
        "---\ntype: a\ntitle: PRIVATE_LABEL\nlevel: PRIVATE_EXT\nPRIVATE_REL: [\"[[PRIVATE_NODE]]\"]\n---\nPRIVATE_BODY\n",
    )
    .unwrap();
    let manifest = dir.path().join("PRIVATE_MANIFEST.toml");
    fs::write(&manifest,"version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[[aristas]]\nclave = \"PRIVATE_REL\"\n[[predicados]]\nnombre = \"Level\"\nclave = \"level\"\ndominio = [\"PRIVATE_EXT\"]\n[layout]\ndirectorios = [\"n\"]\n").unwrap();
    let invariants = dir.path().join("PRIVATE_FORMULA.sx");
    fs::write(&invariants, "; PRIVATE_COMMENT\n(Type @n/PRIVATE_NODE b)\n").unwrap();
    let delta = dir.path().join("PRIVATE_DELTA.json");
    fs::write(&delta, json!({"schema":"felix-delta-v1","add":["Type(n/PRIVATE_NODE, b)"],"remove":["Type(n/PRIVATE_NODE, a)"]}).to_string()).unwrap();
    Fixture {
        dir,
        root,
        manifest,
        invariants,
        delta,
    }
}
fn run(f: &Fixture, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_universo"))
        .arg("propose")
        .arg(&f.root)
        .arg("--manifiesto")
        .arg(&f.manifest)
        .arg("--invariantes")
        .arg(&f.invariants)
        .arg("--delta")
        .arg(&f.delta)
        .args(extra)
        .output()
        .unwrap()
}
fn tree(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(path: &Path, root: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let p = entry.path();
            if p.is_dir() {
                result.insert(p.strip_prefix(root).unwrap().to_path_buf(), vec![]);
                walk(&p, root, result);
            } else {
                result.insert(
                    p.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(p).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(path, path, &mut result);
    result
}
fn json_output(output: &Output, status: &str, code: i32) -> Value {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(text.lines().count(), 1);
    assert!(!text.contains("PRIVATE"));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "propose");
    assert_eq!(value["status"], status);
    value
}
#[test]
fn simulate_admit_reject_and_error_preserve_every_file_and_omit_token() {
    let f = fixture();
    for (invariants, status, code) in [
        ("(Type @n/PRIVATE_NODE b)", "admit", 0),
        ("(Type @n/PRIVATE_NODE a)", "reject", 1),
        ("PRIVATE_INVALID(", "error", 1),
    ] {
        fs::write(&f.invariants, invariants).unwrap();
        let before = tree(f.dir.path());
        let result = json_output(&run(&f, &["--format", "json"]), status, code);
        assert!(result.get("snapshot_token").is_none());
        assert_eq!(tree(f.dir.path()), before);
    }
}
#[test]
fn token_is_opt_in_deterministic_and_changes_with_raw_bytes() {
    let f = fixture();
    let before = tree(f.dir.path());
    let first = json_output(
        &run(&f, &["--format=json", "--emit-snapshot-token"]),
        "admit",
        0,
    );
    let token = first["snapshot_token"].as_str().unwrap();
    assert_eq!(token.len(), 64);
    assert!(
        token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let again = json_output(
        &run(&f, &["--format=json", "--emit-snapshot-token"]),
        "admit",
        0,
    );
    assert_eq!(first, again);
    assert_eq!(tree(f.dir.path()), before);
    fs::write(f.root.join("auxiliary.bin"), b"new raw bytes").unwrap();
    let changed = json_output(
        &run(&f, &["--format=json", "--emit-snapshot-token"]),
        "admit",
        0,
    );
    assert_ne!(first["snapshot_token"], changed["snapshot_token"]);
    let default = run(&f, &[]);
    assert!(default.status.success());
    assert!(default.stderr.is_empty());
    let text = String::from_utf8_lossy(&default.stdout);
    assert!(!text.contains("PRIVATE"));
    assert!(!text.contains(token));
    assert!(!text.contains("snapshot_token"));
}
#[test]
fn delta_invalid_missing_oversized_and_unknown_options_are_redacted() {
    let f = fixture();
    fs::write(&f.delta, "PRIVATE_DELTA_NOT_JSON").unwrap();
    assert_eq!(
        json_output(&run(&f, &["--format=json"]), "error", 1)["errors"][0]["code"],
        "delta_invalid"
    );
    fs::remove_file(&f.delta).unwrap();
    assert_eq!(
        json_output(&run(&f, &["--format=json"]), "error", 1)["errors"][0]["code"],
        "delta_io"
    );
    fs::File::create(&f.delta)
        .unwrap()
        .set_len((universo_okf::transaccion::simulacion::MAX_DELTA_BYTES + 1) as u64)
        .unwrap();
    assert_eq!(
        json_output(&run(&f, &["--format=json"]), "error", 1)["errors"][0]["code"],
        "delta_resource_limit"
    );
    for args in [
        &["--apply"][..],
        &["--expect-snapshot", "PRIVATE_TOKEN"][..],
        &["--emit-snapshot-token", "--emit-snapshot-token"][..],
        &["--delta", "PRIVATE_DUPLICATE"][..],
    ] {
        let out = run(&f, args);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("PRIVATE"));
    }
}
#[test]
fn functional_conflict_is_reject_and_never_writes() {
    let f = fixture();
    fs::write(
        &f.delta,
        json!({"schema":"felix-delta-v1","add":["Type(n/PRIVATE_NODE, b)"],"remove":[]})
            .to_string(),
    )
    .unwrap();
    let before = tree(f.dir.path());
    let result = json_output(&run(&f, &["--format=json"]), "reject", 1);
    assert_eq!(result["summary"]["conflicts"], 1);
    assert_eq!(tree(f.dir.path()), before);
}

#[test]
#[cfg(target_os = "linux")]
fn delta_fifo_se_rechaza_sin_esperar_un_productor() {
    let f = fixture();
    fs::remove_file(&f.delta).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&f.delta)
            .status()
            .unwrap()
            .success()
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_universo"))
        .arg("propose")
        .arg(&f.root)
        .arg("--manifiesto")
        .arg(&f.manifest)
        .arg("--invariantes")
        .arg(&f.invariants)
        .arg("--delta")
        .arg(&f.delta)
        .arg("--format=json")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("FIFO bloqueó la lectura");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        json_output(&out, "error", 1)["errors"][0]["code"],
        "delta_io"
    );
}

#[test]
#[cfg(unix)]
fn delta_symlink_y_root_symlink_se_rechazan_sin_modificar_destino() {
    let f = fixture();
    let target = f.dir.path().join("target_delta");
    fs::rename(&f.delta, &target).unwrap();
    let original = fs::read(&target).unwrap();
    std::os::unix::fs::symlink(&target, &f.delta).unwrap();
    assert_eq!(
        json_output(&run(&f, &["--format=json"]), "error", 1)["errors"][0]["code"],
        "delta_io"
    );
    assert_eq!(fs::read(&target).unwrap(), original);
    fs::remove_file(&f.delta).unwrap();
    fs::rename(&target, &f.delta).unwrap();
    let renamed = f.dir.path().join("realroot");
    fs::rename(&f.root, &renamed).unwrap();
    std::os::unix::fs::symlink(&renamed, &f.root).unwrap();
    assert_eq!(
        json_output(&run(&f, &["--format=json"]), "error", 1)["errors"][0]["code"],
        "snapshot_link_not_allowed"
    );
}

fn token_apply(f: &Fixture, status: &str, code: i32) -> String {
    json_output(
        &run(f, &["--format=json", "--emit-snapshot-token"]),
        status,
        code,
    )["snapshot_token"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn apply_json(f: &Fixture, token: &str, status: &str, code: i32) -> Value {
    let output = run(f, &["--format=json", "--apply", "--expect-snapshot", token]);
    assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
    let value = json_output(&output, status, code);
    assert_eq!(value["mode"], "apply");
    assert!(value.get("snapshot_token").is_none());
    value
}
#[test]
fn apply_admit_redacta_canarios_y_token_y_retira_siblings() {
    let f = fixture();
    fs::write(f.root.join("payload.bin"), b"PRIVATE_AUXILIARY").unwrap();
    let token = token_apply(&f, "admit", 0);
    let value = apply_json(&f, &token, "admit", 0);
    assert_eq!(value["sealed"], true);
    let node = fs::read_to_string(f.root.join("n/PRIVATE_NODE.md")).unwrap();
    assert!(node.contains("type: b"));
    assert!(node.contains("PRIVATE_BODY"));
    assert_eq!(
        fs::read(f.root.join("payload.bin")).unwrap(),
        b"PRIVATE_AUXILIARY"
    );
    let transaction_files = fs::read_dir(f.dir.path())
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().to_string_lossy().to_string();
            name.starts_with(".PRIVATE_ROOT.felix.").then_some(name)
        })
        .collect::<Vec<_>>();
    let mut transaction_files = transaction_files;
    transaction_files.sort();
    // Lockfile reutilizable, historial y sello del último estado admitido.
    assert_eq!(
        transaction_files,
        vec![
            ".PRIVATE_ROOT.felix.lock",
            ".PRIVATE_ROOT.felix.log",
            ".PRIVATE_ROOT.felix.seal"
        ]
    );
}
#[test]
fn apply_reject_y_stale_preservan_root_y_redactan_texto_json() {
    let f = fixture();
    fs::write(
        &f.delta,
        r#"{"schema":"felix-delta-v1","add":[],"remove":[]}"#,
    )
    .unwrap();
    let token = token_apply(&f, "reject", 1);
    let before = tree(&f.root);
    assert_eq!(
        apply_json(&f, &token, "reject", 1)["summary"]["failed_invariants"],
        1
    );
    let text = run(&f, &["--apply", "--expect-snapshot", &token]);
    assert_eq!(text.status.code(), Some(1));
    assert!(text.stderr.is_empty());
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("propose apply: reject"));
    assert!(!text.contains("PRIVATE") && !text.contains(&token));
    assert_eq!(tree(&f.root), before);
    fs::write(&f.invariants, "PRIVATE_INVALID(").unwrap();
    assert_eq!(
        apply_json(&f, &token, "error", 1)["errors"][0]["code"],
        "stale"
    );
    assert_eq!(tree(&f.root), before);
}
#[test]
fn apply_entrada_token_invalida_y_combinaciones_no_se_reflejan() {
    let f = fixture();
    let before = tree(f.dir.path());
    assert_eq!(
        apply_json(&f, "PRIVATE_INPUT_TOKEN", "error", 1)["errors"][0]["code"],
        "snapshot_token_invalid"
    );
    assert_eq!(tree(f.dir.path()), before);
    for args in [
        vec!["--apply"],
        vec!["--expect-snapshot", "PRIVATE_INPUT_TOKEN"],
        vec![
            "--apply",
            "--expect-snapshot",
            "PRIVATE_INPUT_TOKEN",
            "--emit-snapshot-token",
        ],
        vec![
            "--apply",
            "--apply",
            "--expect-snapshot",
            "PRIVATE_INPUT_TOKEN",
        ],
    ] {
        let out = run(&f, &args);
        assert_eq!(out.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&out.stderr).contains("PRIVATE"));
        assert!(out.stdout.is_empty());
    }
    assert_eq!(tree(f.dir.path()), before);
}
