//! Pruebas del caller público y medición opt-in, con procesos escritores reales.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use universo_okf::transaccion::{
    aplicar::{EstadoAplicacion, aplicar_archivo},
    lock::LockEscritura,
    simulacion::simular_archivo,
    snapshot::SnapshotToken,
};

struct Fixture {
    dir: tempfile::TempDir,
    root: PathBuf,
    manifest: PathBuf,
    invariants: PathBuf,
    delta: PathBuf,
}
fn fixture(bytes: usize) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::create_dir_all(root.join("empty/extra")).unwrap();
    fs::write(
        root.join("n/node.md"),
        "---\ntype: a\ntitle: Node\n---\nbody\n",
    )
    .unwrap();
    fs::write(root.join("payload.bin"), vec![0xA5; bytes]).unwrap();
    let manifest = dir.path().join("manifest.toml");
    fs::write(&manifest, "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n").unwrap();
    let invariants = dir.path().join("invariants.sx");
    fs::write(&invariants, "(Type @n/node b)\n").unwrap();
    let delta = dir.path().join("delta.json");
    fs::write(
        &delta,
        r#"{"schema":"felix-delta-v1","add":["Type(n/node, b)"],"remove":["Type(n/node, a)"]}"#,
    )
    .unwrap();
    Fixture {
        dir,
        root,
        manifest,
        invariants,
        delta,
    }
}
fn token(f: &Fixture) -> SnapshotToken {
    simular_archivo(&f.root, &f.manifest, &f.invariants, &f.delta)
        .unwrap()
        .snapshot_token()
}
fn apply(
    f: &Fixture,
    token: SnapshotToken,
) -> Result<
    universo_okf::transaccion::aplicar::ResultadoAplicacion,
    universo_okf::transaccion::aplicar::ErrorAplicacion,
> {
    aplicar_archivo(&f.root, &f.manifest, &f.invariants, &f.delta, token)
}
fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let p = entry.path();
            let rel = p.strip_prefix(root).unwrap().to_path_buf();
            if entry.file_type().unwrap().is_dir() {
                out.insert(rel, None);
                walk(root, &p, out);
            } else {
                out.insert(rel, Some(fs::read(p).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
#[test]
fn caller_aplica_preserva_auxiliares_y_rechaza_stale_sin_nueva_publicacion() {
    let f = fixture(4096);
    let before = tree(&f.root);
    let expected = token(&f);
    assert_eq!(
        apply(&f, expected).unwrap().estado,
        EstadoAplicacion::Aplicada
    );
    let after = tree(&f.root);
    for (path, data) in &before {
        if path != Path::new("n/node.md") {
            assert_eq!(after.get(path), Some(data));
        }
    }
    let error = apply(&f, expected).unwrap_err();
    assert_eq!(error.codigo(), "stale");
    assert_eq!(tree(&f.root), after);
    assert!(
        fs::read_to_string(f.root.join("n/node.md"))
            .unwrap()
            .contains("type: b")
    );
    let residual: Vec<_> = fs::read_dir(f.dir.path())
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name();
            name.to_string_lossy().contains(".felix.").then_some(name)
        })
        .collect();
    assert_eq!(residual.len(), 1); // Sólo el lockfile reutilizable.
}
#[test]
fn caller_reject_conserva_bytes_y_nombres_del_bundle() {
    let f = fixture(1024);
    fs::write(&f.invariants, "(Type @n/node a)\n").unwrap();
    let before = tree(&f.root);
    let expected = token(&f);
    assert_eq!(
        apply(&f, expected).unwrap().estado,
        EstadoAplicacion::Rechazada
    );
    assert_eq!(tree(&f.root), before);
}
#[test]
fn caller_busy_no_cambia_archivos_y_guard_sigue_vigente() {
    let f = fixture(1024);
    let expected = token(&f);
    let before = tree(&f.root);
    let _guard = LockEscritura::adquirir(&f.root).unwrap();
    let child = worker(&f, expected, 0, None);
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(f.dir.path().join("result-0.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "writer_lock_busy");
    assert_eq!(tree(&f.root), before);
    assert!(LockEscritura::adquirir(&f.root).is_err());
}
#[cfg(any(unix, windows))]
#[test]
fn caller_no_sigue_symlink_o_junction_y_preserva_destino() {
    let f = fixture(1024);
    let expected = token(&f);
    let target = f.dir.path().join("outside");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("canary"), b"preserve").unwrap();
    let link = f.root.join("link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(windows)]
    {
        let output = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    assert!(apply(&f, expected).is_err());
    assert_eq!(fs::read(target.join("canary")).unwrap(), b"preserve");
    assert!(fs::symlink_metadata(&link).is_ok());
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
}
fn worker(
    f: &Fixture,
    expected: SnapshotToken,
    id: usize,
    start: Option<&Path>,
) -> std::process::Child {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        "proceso_apply_publico",
        "--ignored",
        "--nocapture",
    ])
    .env("FELIX_APPLY_TEST_ROOT", &f.root)
    .env("FELIX_APPLY_TEST_MANIFEST", &f.manifest)
    .env("FELIX_APPLY_TEST_INV", &f.invariants)
    .env("FELIX_APPLY_TEST_DELTA", &f.delta)
    .env("FELIX_APPLY_TEST_TOKEN", expected.to_hex())
    .env(
        "FELIX_APPLY_TEST_RESULT",
        f.dir.path().join(format!("result-{id}.json")),
    )
    .env(
        "FELIX_APPLY_TEST_READY",
        f.dir.path().join(format!("ready-{id}")),
    )
    .stdout(Stdio::null())
    .stderr(Stdio::piped());
    if let Some(start) = start {
        cmd.env("FELIX_APPLY_TEST_START", start);
    }
    cmd.spawn().unwrap()
}
fn wait_file(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(start.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
#[ignore = "child process entry point"]
fn proceso_apply_publico() {
    fn path(key: &str) -> PathBuf {
        std::env::var_os(key).unwrap().into()
    }
    let root = path("FELIX_APPLY_TEST_ROOT");
    let m = path("FELIX_APPLY_TEST_MANIFEST");
    let i = path("FELIX_APPLY_TEST_INV");
    let d = path("FELIX_APPLY_TEST_DELTA");
    let expected =
        SnapshotToken::from_hex(&std::env::var("FELIX_APPLY_TEST_TOKEN").unwrap()).unwrap();
    fs::write(path("FELIX_APPLY_TEST_READY"), b"ready").unwrap();
    if let Some(start) = std::env::var_os("FELIX_APPLY_TEST_START") {
        wait_file(Path::new(&start));
    }
    let start = Instant::now();
    let result = aplicar_archivo(&root, &m, &i, &d, expected);
    let elapsed = start.elapsed().as_micros();
    let status = match result {
        Ok(result) if result.estado == EstadoAplicacion::Aplicada => "applied",
        Ok(_) => "rejected",
        Err(ref error) => error.codigo(),
    };
    let report = serde_json::json!({"status":status,"elapsed_us":elapsed});
    fs::write(path("FELIX_APPLY_TEST_RESULT"), report.to_string()).unwrap();
}
fn contention(bytes: usize) -> (u128, Vec<serde_json::Value>) {
    let f = fixture(bytes);
    let expected = token(&f);
    let start_file = f.dir.path().join("start");
    let mut children = vec![
        worker(&f, expected, 0, Some(&start_file)),
        worker(&f, expected, 1, Some(&start_file)),
    ];
    for id in 0..2 {
        wait_file(&f.dir.path().join(format!("ready-{id}")));
    }
    let start = Instant::now();
    fs::write(&start_file, b"start").unwrap();
    for child in children.drain(..) {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let elapsed = start.elapsed().as_micros();
    let reports: Vec<serde_json::Value> = (0..2)
        .map(|id| {
            serde_json::from_slice(
                &fs::read(f.dir.path().join(format!("result-{id}.json"))).unwrap(),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(
        reports.iter().filter(|r| r["status"] == "applied").count(),
        1,
        "{reports:?}"
    );
    assert_eq!(
        reports
            .iter()
            .filter(|r| r["status"] == "writer_lock_busy" || r["status"] == "stale")
            .count(),
        1,
        "{reports:?}"
    );
    assert_eq!(
        fs::read(f.root.join("payload.bin")).unwrap(),
        vec![0xA5; bytes]
    );
    assert!(
        fs::read_to_string(f.root.join("n/node.md"))
            .unwrap()
            .contains("type: b")
    );
    (elapsed, reports)
}
#[test]
fn dos_callers_un_solo_token_no_publican_ambos() {
    contention(64 * 1024);
}
#[test]
#[ignore = "release benchmark: five samples per size, two real writers"]
fn runtime_apply_dos_writers_1_y_10_mib() {
    for mib in [1, 10] {
        for sample in 1..=5 {
            let (elapsed, reports) = contention(mib * 1024 * 1024);
            println!(
                "APPLY_RUNTIME mib={mib} sample={sample} writers=2 wall_us={elapsed} reports={reports:?}"
            );
        }
    }
}

#[test]
fn caller_stale_precede_parseo_invalido_de_bundle_y_configuracion() {
    for changed in 0..3 {
        let f = fixture(32);
        let expected = token(&f);
        let path = match changed {
            0 => f.root.join("n/node.md"),
            1 => f.manifest.clone(),
            _ => f.invariants.clone(),
        };
        fs::write(&path, b"PRIVATE_INVALID_PARSE[[{\0").unwrap();
        let before = tree(&f.root);
        let error = apply(&f, expected).unwrap_err();
        assert_eq!(error.codigo(), "stale");
        assert!(!format!("{error:?}").contains("PRIVATE"));
        assert_eq!(tree(&f.root), before);
        assert_eq!(fs::read(path).unwrap(), b"PRIVATE_INVALID_PARSE[[{\0");
    }
}
