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

fn bundle_valido() -> (tempfile::TempDir, std::path::PathBuf) {
    let d = Constructor::nuevo()
        .archivo("n/x.md", "---\ntype: a\ntitle: X\nrel: [\"[[y]]\"]\n---\n")
        .archivo("n/y.md", "---\ntype: a\ntitle: Y\n---\n")
        .escribir();
    let m = manifiesto_toml(d.path());
    (d, m)
}

fn validar(
    raiz: &std::path::Path,
    manifiesto: &std::path::Path,
    formato: &str,
) -> std::process::Output {
    universo(&[
        "validate",
        raiz.to_str().unwrap(),
        "--manifiesto",
        manifiesto.to_str().unwrap(),
        "--format",
        formato,
    ])
}

#[test]
fn validate_valido_sale_con_0_y_emite_resumen_humano() {
    let (d, m) = bundle_valido();
    let out = universo(&[
        "validate",
        d.path().to_str().unwrap(),
        "--manifiesto",
        m.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "OK: 2 nodos, 1 aristas\n"
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn validate_datos_invalidos_sale_con_1_y_reporta_el_error_en_stderr() {
    let d = Constructor::nuevo()
        .archivo(
            "n/x.md",
            "---\ntype: a\ntitle: X\nrel: [\"[[nadie]]\"]\n---\n",
        )
        .escribir();
    let m = manifiesto_toml(d.path());
    let out = universo(&[
        "validate",
        d.path().to_str().unwrap(),
        "--manifiesto",
        m.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("n/x:4: TargetColgante"), "{stderr}");
}

#[test]
fn verificar_es_alias_compatible_de_validate() {
    let (d, m) = bundle_valido();
    let validate = universo(&[
        "validate",
        d.path().to_str().unwrap(),
        "--manifiesto",
        m.to_str().unwrap(),
    ]);
    let verificar = universo(&[
        "verificar",
        d.path().to_str().unwrap(),
        "--manifiesto",
        m.to_str().unwrap(),
    ]);

    assert_eq!(validate.status.code(), Some(0));
    assert_eq!(verificar.status.code(), validate.status.code());
    assert_eq!(verificar.stdout, validate.stdout);
    assert_eq!(verificar.stderr, validate.stderr);
    assert_eq!(
        String::from_utf8(verificar.stdout).unwrap(),
        "OK: 2 nodos, 1 aristas\n"
    );

    let validate_json = validar(d.path(), &m, "json");
    let verificar_json = universo(&[
        "verificar",
        d.path().to_str().unwrap(),
        "--manifiesto",
        m.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(verificar_json.status.code(), validate_json.status.code());
    assert_eq!(verificar_json.stdout, validate_json.stdout);
    assert_eq!(verificar_json.stderr, validate_json.stderr);

    let invalido = Constructor::nuevo()
        .archivo(
            "n/x.md",
            "---\ntype: a\ntitle: X\nrel: [\"[[nadie]]\"]\n---\n",
        )
        .escribir();
    let m_invalido = manifiesto_toml(invalido.path());
    let validate = universo(&[
        "validate",
        invalido.path().to_str().unwrap(),
        "--manifiesto",
        m_invalido.to_str().unwrap(),
    ]);
    let verificar = universo(&[
        "verificar",
        invalido.path().to_str().unwrap(),
        "--manifiesto",
        m_invalido.to_str().unwrap(),
    ]);

    assert_eq!(validate.status.code(), Some(1));
    assert_eq!(verificar.status.code(), validate.status.code());
    assert_eq!(verificar.stdout, validate.stdout);
    assert_eq!(verificar.stderr, validate.stderr);

    let validate_json = validar(invalido.path(), &m_invalido, "json");
    let verificar_json = universo(&[
        "verificar",
        invalido.path().to_str().unwrap(),
        "--manifiesto",
        m_invalido.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(verificar_json.status.code(), validate_json.status.code());
    assert_eq!(verificar_json.stdout, validate_json.stdout);
    assert_eq!(verificar_json.stderr, validate_json.stderr);
}

#[test]
fn ayuda_sale_con_0_y_menciona_validate_y_el_alias() {
    for args in [
        vec!["--help"],
        vec!["validate", "--help"],
        vec!["verificar", "--help"],
        vec!["validate", "--format", "--help"],
    ] {
        let out = universo(&args);
        assert_eq!(out.status.code(), Some(0), "args {args:?}");
        assert!(out.stderr.is_empty(), "args {args:?}");
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert!(stdout.contains("validate"), "args {args:?}: {stdout}");
        assert!(stdout.contains("verificar"), "args {args:?}: {stdout}");
    }
}

#[test]
fn uso_invalido_de_validate_sale_con_2() {
    for args in [
        vec!["validate"],
        vec!["validate", "--manifiesto", "m.toml"],
        vec!["validate", "bundle", "--manifiesto"],
        vec!["validate", "bundle", "--manifiesto", "--format"],
        vec!["validate", "--desconocida", "--manifiesto", "m.toml"],
        vec![
            "validate",
            "bundle",
            "--manifiesto",
            "m.toml",
            "--formato",
            "json",
        ],
    ] {
        let out = universo(&args);
        assert_eq!(out.status.code(), Some(2), "args {args:?}");
        assert!(out.stdout.is_empty(), "args {args:?}");
        assert!(
            String::from_utf8(out.stderr).unwrap().contains("Uso:"),
            "args {args:?}"
        );
    }
}

fn assert_salida_invalida_json(out: std::process::Output) {
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty());
    assert!(out.stdout.ends_with(b"\n"));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "validate");
    assert_eq!(json["status"], "invalid");
    assert!(json["summary"].is_null());
    assert!(!json["errors"].as_array().unwrap().is_empty());
    assert!(
        json["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["code"] == "Io")
    );
}

fn assert_validate_path_error(raiz: &std::path::Path, manifiesto: &std::path::Path) {
    let texto = universo(&[
        "validate",
        raiz.to_str().unwrap(),
        "--manifiesto",
        manifiesto.to_str().unwrap(),
    ]);
    assert_eq!(texto.status.code(), Some(1));
    assert!(texto.stdout.is_empty());
    assert!(!texto.stderr.is_empty());

    assert_salida_invalida_json(validar(raiz, manifiesto, "json"));
}

#[test]
fn validate_rechaza_raiz_inexistente_en_texto_y_json() {
    let d = Constructor::nuevo().escribir();
    let m = manifiesto_toml(d.path());
    let raiz_inexistente = d.path().join("raiz-inexistente");

    assert_validate_path_error(&raiz_inexistente, &m);
}

#[test]
fn validate_rechaza_directorio_declarado_ausente_en_texto_y_json() {
    let d = Constructor::nuevo().escribir();
    let m = manifiesto_toml(d.path());

    assert_validate_path_error(d.path(), &m);
}

#[test]
fn validate_json_exitoso_es_parseable_versionado_y_estable() {
    let (d, m) = bundle_valido();
    let primera = validar(d.path(), &m, "json");
    let segunda = validar(d.path(), &m, "json");

    assert_eq!(primera.status.code(), Some(0));
    assert!(primera.stderr.is_empty());
    assert_eq!(primera.stdout, segunda.stdout);
    assert!(primera.stdout.ends_with(b"\n"));
    let json: serde_json::Value = serde_json::from_slice(&primera.stdout).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "validate");
    assert_eq!(json["status"], "valid");
    assert_eq!(json["summary"]["nodes"], 2);
    assert_eq!(json["summary"]["edges"], 1);
    assert_eq!(json["errors"], serde_json::json!([]));
}

#[test]
fn validate_json_de_error_es_parseable_versionado_y_estable() {
    let d = Constructor::nuevo()
        .archivo(
            "n/x.md",
            "---\ntype: a\ntitle: X\nrel: [\"[[nadie]]\"]\n---\n",
        )
        .escribir();
    let m = manifiesto_toml(d.path());
    let primera = validar(d.path(), &m, "json");
    let segunda = validar(d.path(), &m, "json");

    assert_eq!(primera.status.code(), Some(1));
    assert!(primera.stderr.is_empty());
    assert_eq!(primera.stdout, segunda.stdout);
    assert!(primera.stdout.ends_with(b"\n"));
    let json: serde_json::Value = serde_json::from_slice(&primera.stdout).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "validate");
    assert_eq!(json["status"], "invalid");
    assert!(json["summary"].is_null());
    assert_eq!(json["errors"].as_array().unwrap().len(), 1);
    assert_eq!(json["errors"][0]["code"], "TargetColgante");
    assert_eq!(json["errors"][0]["path"], "n/x");
    assert_eq!(json["errors"][0]["line"], 4);
    assert!(!json["errors"][0]["message"].as_str().unwrap().is_empty());
}
