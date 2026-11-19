use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use universo_testkit::bundle_sintetico::Constructor;

const MANIFIESTO: &str = r#"
version = "check-test/1"
[nodos]
sortes = ["a", "Retirado", "Patrón"]
[[aristas]]
clave = "rel"
simetrica = true
[[predicados]]
nombre = "Nivel"
clave = "nivel"
dominio = ["bajo", "alto"]
[layout]
directorios = ["n"]
"#;

fn universo(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .output()
        .unwrap()
}

fn bundle_valido() -> (tempfile::TempDir, PathBuf) {
    let root = Constructor::nuevo()
        .archivo(
            "n/private-node.md",
            "---\ntype: a\ntitle: LABEL_SECRET_7f31\nnivel: bajo\n---\n",
        )
        .escribir();
    let manifest = root.path().join("manifest.toml");
    std::fs::write(&manifest, MANIFIESTO).unwrap();
    (root, manifest)
}

fn archivo_invariantes(root: &Path, contents: &str) -> PathBuf {
    let path = root.join("invariants.smt2");
    std::fs::write(&path, contents).unwrap();
    path
}

fn check(root: &Path, manifest: &Path, invariants: &Path) -> Output {
    universo(&[
        "check",
        root.to_str().unwrap(),
        "--manifiesto",
        manifest.to_str().unwrap(),
        "--invariantes",
        invariants.to_str().unwrap(),
        "--format",
        "json",
    ])
}

fn check_result(output: &Output, exit: i32) -> Value {
    assert_eq!(output.status.code(), Some(exit));
    assert!(
        output.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.ends_with(b"\n"));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "check");
    json
}

fn assert_resumen(json: &Value, sources: u64, instances: u64, evaluated: u64, failed: u64) {
    assert_eq!(json["summary"]["sources"], sources);
    assert_eq!(json["summary"]["instances"], instances);
    assert_eq!(json["summary"]["evaluated"], evaluated);
    assert_eq!(json["summary"]["failed"], failed);
    assert_eq!(json["errors"], serde_json::json!([]));
}

#[test]
fn admite_y_rechaza_sobre_bundle_valido_sin_exponer_testigos() {
    let (root, manifest) = bundle_valido();
    let validacion = universo(&[
        "validate",
        root.path().to_str().unwrap(),
        "--manifiesto",
        manifest.to_str().unwrap(),
    ]);
    assert_eq!(validacion.status.code(), Some(0));

    let admit_file = archivo_invariantes(root.path(), "(forall ((x N)) (Type x a))\n");
    let admit_one = check(root.path(), &manifest, &admit_file);
    let admit_two = check(root.path(), &manifest, &admit_file);
    let admit = check_result(&admit_one, 0);
    assert_eq!(admit["status"], "admit");
    assert_resumen(&admit, 1, 1, 1, 0);
    assert_eq!(admit_one.stdout, admit_two.stdout);

    let reject_file = archivo_invariantes(root.path(), "(forall ((x N)) (not (Type x a)))\n");
    let reject_one = check(root.path(), &manifest, &reject_file);
    let reject_two = check(root.path(), &manifest, &reject_file);
    let reject = check_result(&reject_one, 1);
    assert_eq!(reject["status"], "reject");
    assert_resumen(&reject, 1, 1, 1, 1);
    assert_eq!(reject_one.stdout, reject_two.stdout);

    let text = String::from_utf8(reject_one.stdout).unwrap();
    for forbidden in [
        "private-node",
        "LABEL_SECRET_7f31",
        "witness",
        "testigo",
        "@private",
    ] {
        assert!(
            !text.contains(forbidden),
            "salida contiene {forbidden:?}: {text}"
        );
    }
}

#[test]
fn valida_datos_antes_de_abrir_o_parsear_invariantes() {
    let root = Constructor::nuevo()
        .archivo(
            "n/private-node.md",
            "---\ntype: a\ntitle: LABEL_SECRET_7f31\nrel: [\"[[missing-node]]\"]\n---\n",
        )
        .escribir();
    let manifest = root.path().join("manifest.toml");
    std::fs::write(&manifest, MANIFIESTO).unwrap();

    // Si el check intenta abrir este archivo antes de validar el bundle, el
    // resultado cambiaría de error de datos a error de recurso.
    let missing = root.path().join("absent-invariants.smt2");
    let missing_output = check(root.path(), &manifest, &missing);
    assert_eq!(
        missing_output.stdout,
        check(root.path(), &manifest, &missing).stdout
    );
    let missing_json = check_result(&missing_output, 1);
    assert_eq!(missing_json["status"], "error");
    assert!(missing_json["summary"].is_null());
    assert_eq!(missing_json["errors"][0]["kind"], "data");

    // Un archivo disponible pero sintácticamente roto tampoco debe analizarse
    // cuando la validación estructural ya falló.
    let malformed = archivo_invariantes(root.path(), "(esto no es una sentencia");
    let malformed_output = check(root.path(), &manifest, &malformed);
    assert_eq!(
        malformed_output.stdout,
        check(root.path(), &manifest, &malformed).stdout
    );
    let malformed_json = check_result(&malformed_output, 1);
    assert_eq!(malformed_json["status"], "error");
    assert_eq!(malformed_json["errors"][0]["kind"], "data");
    assert_eq!(malformed_json["errors"][0]["code"], "TargetColgante");
    assert_eq!(malformed_json["errors"][0]["path"], "n/private-node");
    assert_eq!(malformed_json["errors"][0]["line"], 4);
}

#[test]
fn admite_varias_expresiones_separadas_por_comentarios() {
    let (root, manifest) = bundle_valido();
    let invariants = archivo_invariantes(
        root.path(),
        "; cabecera\n(forall ((x N)) (Type x a)) ; comentario inline\n\n; segunda sentencia\n(forall ((x N)) (HasLab x \"LABEL_SECRET_7f31\"))\n",
    );
    let output = check_result(&check(root.path(), &manifest, &invariants), 0);

    assert_eq!(output["status"], "admit");
    assert_resumen(&output, 2, 2, 2, 0);
}

#[test]
fn informa_linea_de_error_sintactico_y_estatico() {
    let (root, manifest) = bundle_valido();

    let syntax = archivo_invariantes(root.path(), "; cabecera\n\n  (forall ((x N)) (Type x a)\n");
    let syntax_first = check(root.path(), &manifest, &syntax);
    let syntax_second = check(root.path(), &manifest, &syntax);
    assert_eq!(syntax_first.stdout, syntax_second.stdout);
    let syntax_json = check_result(&syntax_first, 1);
    assert_eq!(syntax_json["status"], "error");
    assert!(syntax_json["summary"].is_null());
    assert_eq!(syntax_json["errors"][0]["kind"], "authorship");
    assert!(
        !syntax_json["errors"][0]["code"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(syntax_json["errors"][0]["line"], 3);
    assert!(syntax_json["errors"][0]["column"].as_u64().unwrap() >= 1);

    let static_error = archivo_invariantes(
        root.path(),
        "; cabecera\n\n  (forall ((x N)) (Type x unknown-sort))\n",
    );
    let static_first = check(root.path(), &manifest, &static_error);
    let static_second = check(root.path(), &manifest, &static_error);
    assert_eq!(static_first.stdout, static_second.stdout);
    let static_json = check_result(&static_first, 1);
    assert_eq!(static_json["status"], "error");
    assert_eq!(static_json["errors"][0]["kind"], "authorship");
    assert!(
        !static_json["errors"][0]["code"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(static_json["errors"][0]["line"], 3);
    assert_eq!(static_json["errors"][0]["column"], 3);
}

#[test]
fn expande_esquema_sobre_dominio_cerrado_y_cuenta_instancias() {
    let (root, manifest) = bundle_valido();
    let invariants = archivo_invariantes(
        root.path(),
        "(esquema ((v Dom.Nivel)) (forall ((x N)) (or (Nivel x v) (not (Nivel x v)))))\n",
    );
    let first = check(root.path(), &manifest, &invariants);
    let second = check(root.path(), &manifest, &invariants);
    let json = check_result(&first, 0);

    assert_eq!(json["status"], "admit");
    assert_resumen(&json, 1, 2, 2, 0);
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn archivo_vacio_o_solo_comentarios_admite_cero_invariantes() {
    let (root, manifest) = bundle_valido();
    let invariants = archivo_invariantes(root.path(), " \n; nada que evaluar\n\t\n");
    let json = check_result(&check(root.path(), &manifest, &invariants), 0);

    assert_eq!(json["status"], "admit");
    assert_resumen(&json, 0, 0, 0, 0);
}

#[test]
fn distingue_archivo_de_invariantes_ausente_y_utf8_invalido() {
    let (root, manifest) = bundle_valido();
    let missing = root.path().join("missing-invariants.smt2");
    let missing_first = check(root.path(), &manifest, &missing);
    let missing_second = check(root.path(), &manifest, &missing);
    assert_eq!(missing_first.stdout, missing_second.stdout);
    let missing_json = check_result(&missing_first, 1);
    assert_eq!(missing_json["status"], "error");
    assert!(missing_json["summary"].is_null());
    assert_eq!(missing_json["errors"][0]["kind"], "resource");

    let invalid_utf8 = root.path().join("invalid-utf8.smt2");
    std::fs::write(&invalid_utf8, [0xff, 0xfe]).unwrap();
    let utf8_first = check(root.path(), &manifest, &invalid_utf8);
    let utf8_second = check(root.path(), &manifest, &invalid_utf8);
    assert_eq!(utf8_first.stdout, utf8_second.stdout);
    let utf8_json = check_result(&utf8_first, 1);
    assert_eq!(utf8_json["status"], "error");
    assert!(utf8_json["summary"].is_null());
    assert_eq!(utf8_json["errors"][0]["kind"], "authorship");
    assert!(utf8_json["errors"][0]["line"].is_null());
}

#[test]
fn uso_faltante_o_flag_desconocida_sale_con_2_sin_json() {
    let (root, manifest) = bundle_valido();
    let invariants = archivo_invariantes(root.path(), "(forall ((x N)) (Type x a))\n");
    for args in [
        vec!["check", root.path().to_str().unwrap()],
        vec![
            "check",
            root.path().to_str().unwrap(),
            "--manifiesto",
            manifest.to_str().unwrap(),
            "--invariantes",
            invariants.to_str().unwrap(),
            "--desconocida",
        ],
    ] {
        let output = universo(&args);
        assert_eq!(output.status.code(), Some(2), "args: {args:?}");
        assert!(output.stdout.is_empty(), "args: {args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("Uso:"));
    }
}
