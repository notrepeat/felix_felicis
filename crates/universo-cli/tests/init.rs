//! `universo init`: repositorio de conocimiento mínimo, válido y sellado.
use serde_json::Value;
use std::{fs, path::Path, process::Command};

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

fn guard_args(dir: &Path) -> Vec<String> {
    vec![
        "guard".into(),
        dir.join("bundle").to_string_lossy().into_owned(),
        "--manifiesto".into(),
        dir.join("manifest.toml").to_string_lossy().into_owned(),
        "--invariantes".into(),
        dir.join("invariants.sx").to_string_lossy().into_owned(),
        "--format".into(),
        "json".into(),
    ]
}

#[test]
fn init_crea_un_repositorio_que_pasa_la_guardia() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("criterio");
    let (code, out) = universo(&["init", dir.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(
        !out.contains(tmp.path().to_str().unwrap()),
        "no imprime rutas: {out}"
    );
    for archivo in [
        "manifest.toml",
        "invariants.sx",
        "bundle/notas/bienvenida.md",
    ] {
        assert!(dir.join(archivo).is_file(), "{archivo}");
    }
    let args = guard_args(&dir);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, out) = universo(&args);
    assert_eq!(code, 0, "{out}");
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["status"], "pass");
    assert_eq!(value["seal"], "match");
}

#[test]
fn init_no_toca_un_directorio_con_contenido() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("algo.txt"), b"x").unwrap();
    let (code, out) = universo(&["init", tmp.path().to_str().unwrap(), "--format", "json"]);
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["errors"][0]["code"], "init_target_not_empty");
    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
}

#[test]
fn init_exige_un_directorio() {
    assert_eq!(universo(&["init"]).0, 2);
    assert_eq!(universo(&["init", "a", "b"]).0, 2);
}
