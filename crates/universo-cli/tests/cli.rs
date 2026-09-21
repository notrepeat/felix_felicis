use std::process::Command;
use universo_testkit::bundle_sintetico::Constructor;

fn manifiesto_toml(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("m.toml");
    std::fs::write(&p, "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patrón\"]\n[[aristas]]\nclave = \"rel\"\nsimetrica = true\n[layout]\ndirectorios = [\"n\"]\n").unwrap();
    p
}
fn universo(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn atomos_y_verificar() {
    let d = Constructor::nuevo()
        .archivo("n/x.md", "---\ntype: a\ntitle: X\nrel: [\"[[y]]\"]\n---\n")
        .archivo("n/y.md", "---\ntype: a\ntitle: Y\n---\n")
        .escribir();
    let m = manifiesto_toml(d.path());
    let out = universo(&[
        "atomos",
        d.path().to_str().unwrap(),
        "--manifiesto",
        m.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "Edge(n/x→n/y#rel#0, n/x, n/y, rel)\nHasLab(n/x, \"X\")\nHasLab(n/x→n/y#rel#0, \"\")\nHasLab(n/y, \"Y\")\nType(n/x, a)\nType(n/y, a)\n"
    );
    let out = universo(&[
        "verificar",
        d.path().to_str().unwrap(),
        &format!("--manifiesto={}", m.to_str().unwrap()),
    ]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "OK: 2 nodos, 1 aristas\n"
    );
}

#[test]
fn verificar_reporta_errores_y_sale_con_1() {
    let d = Constructor::nuevo()
        .archivo(
            "n/x.md",
            "---\ntype: a\ntitle: X\nrel: [\"[[nadie]]\"]\n---\n",
        )
        .escribir();
    let m = manifiesto_toml(d.path());
    let out = universo(&[
        "verificar",
        d.path().to_str().unwrap(),
        "--manifiesto",
        m.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("n/x:4: TargetColgante"), "{err}");
    assert!(out.stdout.is_empty());
}

#[test]
fn manifiesto_invalido_sale_con_1() {
    let d = Constructor::nuevo().escribir();
    let m = d.path().join("malo.toml");
    std::fs::write(&m, "esto = [").unwrap();
    let out = universo(&[
        "atomos",
        d.path().to_str().unwrap(),
        "--manifiesto",
        m.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!String::from_utf8(out.stderr).unwrap().is_empty());
}

#[cfg(windows)]
#[test]
fn argumento_no_unicode_sale_con_2() {
    use std::os::windows::ffi::OsStringExt;
    let raro = std::ffi::OsString::from_wide(&[0xD800]); // surrogate suelto: no es Unicode válido
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .arg("atomos")
        .arg(&raro)
        .arg("--manifiesto")
        .arg("m")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("Uso:"));
}
#[cfg(unix)]
#[test]
fn argumento_no_unicode_sale_con_2() {
    use std::os::unix::ffi::OsStringExt;
    let raro = std::ffi::OsString::from_vec(vec![0xff]);
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .arg("atomos")
        .arg(&raro)
        .arg("--manifiesto")
        .arg("m")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Uso:"));
}

#[test]
fn uso_incorrecto_sale_con_2() {
    for args in [
        vec![],
        vec!["atomos"],
        vec!["nada", "x", "--manifiesto", "m"],
        vec!["atomos", "x"],
        vec!["atomos", "x", "--manifiesto"],
    ] {
        let out = universo(&args);
        assert_eq!(out.status.code(), Some(2), "args {args:?}");
        assert!(
            String::from_utf8(out.stderr).unwrap().contains("Uso:"),
            "args {args:?}"
        );
    }
}
