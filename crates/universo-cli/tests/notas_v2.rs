//! `propose` con `felix-delta-v2`: simula y aplica la creación de notas.
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
fn el_apply_crea_la_nota_sellada_y_valida() {
    let f = fixture("(Type @n/existente a)\n", CREAR_B);
    let (_, value) = propose(&f, &["--emit-snapshot-token"]);
    let token = value["snapshot_token"].as_str().unwrap().to_string();
    let (code, value) = propose(&f, &["--apply", "--expect-snapshot", &token]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["status"], "admit");
    assert_eq!(value["sealed"], true);
    let texto = fs::read_to_string(f.root.join("n/nueva.md")).unwrap();
    assert!(
        texto.contains("type: b") && texto.contains("title: Nueva"),
        "{texto}"
    );
    assert!(texto.ends_with("Texto.\n"), "{texto}");
    let validar = Command::new(env!("CARGO_BIN_EXE_universo"))
        .arg("validate")
        .arg(&f.root)
        .arg("--manifiesto")
        .arg(&f.manifest)
        .output()
        .unwrap();
    assert!(validar.status.success(), "{validar:?}");
    // Repetir el mismo delta: la nota ya existe.
    let (_, value) = propose(&f, &[]);
    assert_eq!(value["errors"][0]["code"], "note_exists");
}

#[test]
fn el_apply_actualiza_el_cuerpo_de_una_nota_existente() {
    let f = fixture(
        "(Type @n/existente a)\n",
        r#"{"schema":"felix-delta-v2","create":[],"update":[{"path":"n/existente","body":"Prosa nueva.\n"}],"add":[],"remove":[]}"#,
    );
    let (_, value) = propose(&f, &["--emit-snapshot-token"]);
    assert_eq!(value["status"], "admit", "{value}");
    let token = value["snapshot_token"].as_str().unwrap().to_string();
    let (code, value) = propose(&f, &["--apply", "--expect-snapshot", &token]);
    assert_eq!(code, 0, "{value}");
    let texto = fs::read_to_string(f.root.join("n/existente.md")).unwrap();
    assert!(
        texto.starts_with("---\ntype: a\ntitle: Existente\n---\n"),
        "{texto}"
    );
    assert!(texto.ends_with("Prosa nueva.\n"), "{texto}");
}

/// Una arista hacia una nota cuyo nombre (stem) comparte otra nota no se
/// puede escribir como `[[stem]]`: el cargador la vería ambigua. La
/// simulación la rechaza como el apply, en vez de admitirla y fallar después.
#[test]
fn una_arista_hacia_un_nombre_repetido_no_se_admite() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::create_dir_all(root.join("m")).unwrap();
    fs::write(root.join("n/x.md"), "---\ntype: a\ntitle: X\n---\n").unwrap();
    let manifest = dir.path().join("manifest.toml");
    fs::write(
        &manifest,
        "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patrón\"]\n[[aristas]]\nclave = \"liga\"\n[layout]\ndirectorios = [\"n\", \"m\"]\n",
    )
    .unwrap();
    let invariants = dir.path().join("invariants.sx");
    fs::write(&invariants, "(Type @n/x a)\n").unwrap();
    let correr = |delta: &str| {
        let ruta = dir.path().join("delta.json");
        fs::write(&ruta, delta).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_universo"))
            .arg("propose")
            .arg(&root)
            .arg("--manifiesto")
            .arg(&manifest)
            .arg("--invariantes")
            .arg(&invariants)
            .arg("--delta")
            .arg(&ruta)
            .args(["--format", "json"])
            .output()
            .unwrap();
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        (out.status.code().unwrap(), v)
    };
    // m/x repite el nombre de n/x, y una arista apunta a uno de los dos.
    let (code, v) = correr(
        r#"{"schema":"felix-delta-v2","create":[{"path":"m/x","type":"a","title":"Otra X","body":""},{"path":"m/y","type":"a","title":"Y","body":""}],"add":["Edge(m/y→m/x#liga#0, m/y, m/x, liga)","HasLab(m/y→m/x#liga#0, \"\")"],"remove":[]}"#,
    );
    assert_eq!(code, 1, "{v}");
    assert_ne!(v["status"], "admit", "{v}");
    assert!(v.to_string().contains("edge_target_ambiguous"), "{v}");
    // También hacia una nota que ya existía y cuyo nombre repite una nueva.
    let (code, v) = correr(
        r#"{"schema":"felix-delta-v2","create":[{"path":"m/x","type":"a","title":"Otra X","body":""},{"path":"m/y","type":"a","title":"Y","body":""}],"add":["Edge(m/y→n/x#liga#0, m/y, n/x, liga)","HasLab(m/y→n/x#liga#0, \"\")"],"remove":[]}"#,
    );
    assert!(v.to_string().contains("edge_target_ambiguous"), "{code} {v}");
    // Un origen con nombre repetido hacia un destino único sí se escribe sin
    // ambigüedad: el enlace vive en el origen y nombra al destino.
    let (code, v) = correr(
        r#"{"schema":"felix-delta-v2","create":[{"path":"m/x","type":"a","title":"Otra X","body":""},{"path":"m/y","type":"a","title":"Y","body":""}],"add":["Edge(m/x→m/y#liga#0, m/x, m/y, liga)","HasLab(m/x→m/y#liga#0, \"\")"],"remove":[]}"#,
    );
    assert_eq!((code, &v["status"]), (0, &Value::from("admit")), "{v}");
    // Sin la arista, dos notas con el mismo nombre son válidas (el cargador
    // también las acepta).
    let (code, v) = correr(
        r#"{"schema":"felix-delta-v2","create":[{"path":"m/x","type":"a","title":"Otra X","body":""}],"add":[],"remove":[]}"#,
    );
    assert_eq!((code, &v["status"]), (0, &Value::from("admit")), "{v}");
}
