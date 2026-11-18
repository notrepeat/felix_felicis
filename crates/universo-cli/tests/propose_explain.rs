//! `propose --explain`: explicación redactada del candidato y metadatos de regla.
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::PathBuf, process::Command};

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    manifest: PathBuf,
    invariants: PathBuf,
    delta: PathBuf,
}

fn fixture(invariants: &str) -> Fixture {
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
    let invariants_path = dir.path().join("invariants.sx");
    fs::write(&invariants_path, invariants).unwrap();
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
        invariants: invariants_path,
        delta,
    }
}

fn propose(f: &Fixture, extra: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
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
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

const REGLAS: &str = "; PRIVATE_COMMENT\n(regla :id \"tipo-estable\" :mensaje \"El nodo no cambia de tipo.\"\n  (Type @n/PRIVATE_NODE a))\n(Type @n/PRIVATE_NODE a)\n";

#[test]
fn explain_json_reutiliza_el_formato_redactado_de_check() {
    let f = fixture(REGLAS);
    let (code, out, err) = propose(&f, &["--format", "json", "--explain", "enumerate"]);
    assert_eq!(code, 1, "{err}");
    assert!(!out.contains("PRIVATE"), "{out}");
    // El id y el mensaje de la regla sólo van al texto, nunca al JSON.
    assert!(
        !out.contains("tipo-estable") && !out.contains("cambia de tipo"),
        "{out}"
    );
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["status"], "reject");
    assert_eq!(value["explanation_error"], Value::Null);
    assert_eq!(value["summary"]["failed_invariants"], 2);
    let explanations = &value["explanations"];
    assert_eq!(explanations["mode"], "enumerate");
    let objects = explanations["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 2);
    let keys: BTreeSet<_> = objects[0].as_object().unwrap().keys().cloned().collect();
    let expected: BTreeSet<_> = [
        "kind",
        "source_index",
        "line",
        "column",
        "instance_index",
        "counterexample",
        "presence_core",
        "gap",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(keys, expected);
    assert_eq!(objects[0]["line"], 2);
}

#[test]
fn explain_texto_muestra_id_y_mensaje_de_la_regla_violada() {
    let f = fixture(REGLAS);
    let (code, out, _) = propose(&f, &["--explain", "decide"]);
    assert_eq!(code, 1);
    assert!(out.contains("EXPLANATIONS mode=decide objects=1"), "{out}");
    assert!(
        out.contains("rule: id=tipo-estable message=El nodo no cambia de tipo."),
        "{out}"
    );
    assert!(!out.contains("PRIVATE"), "{out}");
}

#[test]
fn explain_admitido_no_tiene_objetos_y_sin_flag_no_hay_explicacion() {
    let f = fixture("(Type @n/PRIVATE_NODE b)\n");
    let (code, out, _) = propose(&f, &["--format", "json", "--explain", "decide"]);
    assert_eq!(code, 0);
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["explanations"]["objects"], serde_json::json!([]));
    let (_, out, _) = propose(&f, &["--format", "json"]);
    let value: Value = serde_json::from_str(&out).unwrap();
    assert!(value.get("explanations").is_none());
}

#[test]
fn explain_no_se_combina_con_apply_y_exige_modo_valido() {
    let f = fixture(REGLAS);
    for extra in [
        vec!["--explain", "decide", "--apply", "--expect-snapshot", "00"],
        vec!["--explain", "todo"],
        vec!["--explain"],
    ] {
        assert_eq!(propose(&f, &extra).0, 2, "{extra:?}");
    }
}

#[test]
fn check_explain_texto_muestra_regla_y_json_v2_no_cambia() {
    let f = fixture("(regla :id \"tipo-b\" :mensaje \"Debe ser b.\" (Type @n/PRIVATE_NODE b))\n");
    let check = |extra: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_universo"))
            .arg("check")
            .arg(&f.root)
            .arg("--manifiesto")
            .arg(&f.manifest)
            .arg("--invariantes")
            .arg(&f.invariants)
            .args(extra)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let text = check(&["--explain", "decide"]);
    assert!(
        text.contains("rule: id=tipo-b message=Debe ser b."),
        "{text}"
    );
    let json_text = check(&["--explain", "decide", "--format", "json"]);
    assert!(
        !json_text.contains("tipo-b") && !json_text.contains("Debe ser"),
        "{json_text}"
    );
    let json: Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(json["schema_version"], 2);
    let object = &json["explanations"]["objects"][0];
    assert!(object.get("rule_id").is_none() && object.get("rule").is_none());
    assert!(
        !check(&[]).contains("tipo-b"),
        "sin --explain no se muestra"
    );
}

#[test]
fn explain_sin_invariantes_que_explicar_lo_declara_con_codigo() {
    let f = fixture(REGLAS);
    // Retirar un átomo inexistente: error técnico de la propuesta.
    fs::write(
        &f.delta,
        r#"{"schema":"felix-delta-v1","add":[],"remove":["Type(n/PRIVATE_NODE, b)"]}"#,
    )
    .unwrap();
    let (code, out, _) = propose(&f, &["--format", "json", "--explain", "decide"]);
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["status"], "error");
    assert_eq!(value["explanations"], Value::Null);
    assert_eq!(value["explanation_error"], "explanation_not_applicable");
}
