use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use universo_testkit::bundle_sintetico::Constructor;

const MANIFIESTO: &str = r#"
version = "check-explain-test/1"
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

const MANIFIESTO_PRIVACIDAD: &str = r#"
version = "check-explain-privacy-test/1"
[nodos]
sortes = ["a", "Retirado", "Patrón"]
[[aristas]]
clave = "rel"
simetrica = true
[[predicados]]
nombre = "Nivel"
clave = "nivel"
dominio = ["bajo", "alto"]
[[predicados]]
nombre = "PRIVATE_ATTRIBUTE_PREDICATE_CANARY"
clave = "private_attribute"
dominio = ["PRIVATE_ATTRIBUTE_VALUE_CANARY"]
[layout]
directorios = ["n"]
"#;

const PATH_CANARY: &str = "private-node";
const LABEL_CANARY: &str = "LABEL_SECRET_7f31";
const VARIABLE_CANARY: &str = "PRIVATE_VARIABLE_CANARY";

fn bundle_valido() -> (tempfile::TempDir, PathBuf) {
    let bundle_path = format!("n/{PATH_CANARY}.md");
    let bundle_node = format!("---\ntype: a\ntitle: {LABEL_CANARY}\nnivel: bajo\n---\n");
    let root = Constructor::nuevo()
        .archivo(&bundle_path, &bundle_node)
        .escribir();
    let manifest = root.path().join("manifest.toml");
    std::fs::write(&manifest, MANIFIESTO).unwrap();
    (root, manifest)
}

fn bundle_con_dos_labels() -> (tempfile::TempDir, PathBuf) {
    let root = Constructor::nuevo()
        .archivo("n/a.md", "---\ntype: a\ntitle: SCHEMA_LABEL_A\n---\n")
        .archivo("n/b.md", "---\ntype: a\ntitle: SCHEMA_LABEL_B\n---\n")
        .escribir();
    let manifest = root.path().join("manifest.toml");
    std::fs::write(&manifest, MANIFIESTO).unwrap();
    (root, manifest)
}

fn bundle_con_canarios() -> (tempfile::TempDir, PathBuf) {
    let root = Constructor::nuevo()
        .archivo(
            "n/private/index.md",
            "---\ntype: a\ntitle: PRIVATE_ROOT_LABEL_CANARY\nprivate_attribute: PRIVATE_ATTRIBUTE_VALUE_CANARY\n---\n",
        )
        .archivo(
            "n/private/leaf.md",
            "---\ntype: a\ntitle: PRIVATE_LABEL_CANARY\nprivate_attribute: PRIVATE_ATTRIBUTE_VALUE_CANARY\nrel: [\"[[index]]\"]\n---\n",
        )
        .escribir();
    let manifest = root.path().join("manifest.toml");
    std::fs::write(&manifest, MANIFIESTO_PRIVACIDAD).unwrap();
    (root, manifest)
}

fn archivo_invariantes(root: &Path, contents: &str) -> PathBuf {
    let path = root.join("invariants.smt2");
    std::fs::write(&path, contents).unwrap();
    path
}

fn check_args(root: &Path, manifest: &Path, invariants: &Path) -> Vec<String> {
    vec![
        "check".into(),
        root.to_string_lossy().into_owned(),
        "--manifiesto".into(),
        manifest.to_string_lossy().into_owned(),
        "--invariantes".into(),
        invariants.to_string_lossy().into_owned(),
    ]
}

fn invoke(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .output()
        .unwrap()
}

fn check_explain(root: &Path, manifest: &Path, invariants: &Path, mode: &str) -> Output {
    let mut args = check_args(root, manifest, invariants);
    args.extend([
        "--explain".into(),
        mode.into(),
        "--format".into(),
        "json".into(),
    ]);
    invoke(&args)
}

fn check_explain_text(root: &Path, manifest: &Path, invariants: &Path, mode: &str) -> Output {
    let mut args = check_args(root, manifest, invariants);
    args.extend(["--explain".into(), mode.into()]);
    invoke(&args)
}

fn assert_keys(value: &Value, expected: &[&str]) {
    let object = value.as_object().expect("se esperaba un objeto JSON");
    let mut actual: Vec<_> = object.keys().map(String::as_str).collect();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

fn assert_counterexample_tree(node: &Value) {
    assert!(node.is_object());
    assert!(node["value"].is_boolean());
    let rule = node["rule"].as_str().expect("regla textual allowlisted");
    match rule {
        "atom" => {
            assert_keys(node, &["rule", "value", "predicate"]);
            assert!(matches!(
                node["predicate"].as_str(),
                Some(
                    "type"
                        | "edge"
                        | "parent"
                        | "has_label_node"
                        | "has_label_edge"
                        | "extension"
                        | "tc_parent"
                        | "equal_node"
                        | "equal_edge"
                )
            ));
        }
        "not" => {
            assert_keys(node, &["rule", "value", "inner"]);
            assert_counterexample_tree(&node["inner"]);
        }
        "and" | "or" => {
            assert_keys(node, &["rule", "value", "children"]);
            for child in node["children"].as_array().unwrap() {
                assert_counterexample_tree(child);
            }
        }
        "implies" => {
            assert_keys(node, &["rule", "value", "antecedent", "consequent"]);
            assert_counterexample_tree(&node["antecedent"]);
            if !node["consequent"].is_null() {
                assert_counterexample_tree(&node["consequent"]);
            }
        }
        "forall" | "exists" => {
            assert_keys(node, &["rule", "value", "search_exhausted", "instances"]);
            assert!(node["search_exhausted"].is_boolean());
            for instance in node["instances"].as_array().unwrap() {
                assert_keys(instance, &["value", "guard", "body"]);
                assert!(instance["value"].is_boolean());
                for child in [&instance["guard"], &instance["body"]] {
                    if !child.is_null() {
                        assert_counterexample_tree(child);
                    }
                }
            }
        }
        other => panic!("regla fuera de la allowlist: {other}"),
    }
}

fn contiene_predicado(node: &Value, predicate: &str) -> bool {
    match node["rule"].as_str() {
        Some("atom") => node["predicate"] == predicate,
        Some("not") => contiene_predicado(&node["inner"], predicate),
        Some("and" | "or") => node["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|child| contiene_predicado(child, predicate)),
        Some("implies") => {
            contiene_predicado(&node["antecedent"], predicate)
                || (!node["consequent"].is_null()
                    && contiene_predicado(&node["consequent"], predicate))
        }
        Some("forall" | "exists") => node["instances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|instance| {
                [&instance["guard"], &instance["body"]]
                    .into_iter()
                    .any(|child| !child.is_null() && contiene_predicado(child, predicate))
            }),
        _ => false,
    }
}

fn assert_positive_form(form: &Value) {
    match form["kind"].as_str().expect("forma allowlisted") {
        "atom" => {
            assert_keys(form, &["kind", "predicate"]);
            assert!(matches!(
                form["predicate"].as_str(),
                Some(
                    "type"
                        | "edge"
                        | "parent"
                        | "has_label_node"
                        | "has_label_edge"
                        | "extension"
                        | "tc_parent"
                        | "equal_node"
                        | "equal_edge"
                )
            ));
        }
        "and" | "or" => {
            assert_keys(form, &["kind", "children"]);
            for child in form["children"].as_array().unwrap() {
                assert_positive_form(child);
            }
        }
        "exists" => {
            assert_keys(
                form,
                &["kind", "node_binders", "edge_binders", "guard", "body"],
            );
            if !form["guard"].is_null() {
                assert_positive_form(&form["guard"]);
            }
            assert_positive_form(&form["body"]);
        }
        other => panic!("forma fuera de la allowlist: {other}"),
    }
}

fn assert_objeto_explicacion(object: &Value) {
    assert_keys(
        object,
        &[
            "kind",
            "source_index",
            "line",
            "column",
            "instance_index",
            "counterexample",
            "presence_core",
            "gap",
        ],
    );
    assert_eq!(object["kind"], "counterexample");
    assert!(object["source_index"].is_u64());
    assert!(object["line"].is_u64());
    assert!(object["column"].is_u64());
    assert!(object["instance_index"].is_null() || object["instance_index"].is_u64());
    assert_counterexample_tree(&object["counterexample"]);
    if !object["presence_core"].is_null() {
        assert_keys(&object["presence_core"], &["atom_count"]);
        assert!(object["presence_core"]["atom_count"].is_u64());
    }
    if !object["gap"].is_null() {
        let gap = &object["gap"];
        assert_keys(gap, &["class", "counts", "form"]);
        assert!(matches!(
            gap["class"].as_str(),
            Some("existence_without_witness" | "forall_exists_without_witness")
        ));
        assert_keys(
            &gap["counts"],
            &[
                "available_nodes",
                "available_edges",
                "existential_candidates_evaluated",
                "universal_bindings_examined",
            ],
        );
        assert_keys(
            &gap["form"],
            &["node_binders", "edge_binders", "guard", "body"],
        );
    }
}

fn resultado_explicativo(output: &Output, exit: i32) -> Value {
    assert_eq!(output.status.code(), Some(exit));
    assert!(
        output.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.ends_with(b"\n"));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.is_object());
    assert_keys(
        &json,
        &[
            "command",
            "errors",
            "schema_version",
            "status",
            "summary",
            "explanations",
        ],
    );
    assert_eq!(json["command"], "check");
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["errors"], serde_json::json!([]));
    json
}

fn resultado_error_explicativo(output: &Output, kind: &str, code: &str) -> Value {
    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stderr.is_empty(),
        "stderr JSON debe quedar vacío: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.ends_with(b"\n"));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_keys(
        &json,
        &[
            "command",
            "errors",
            "schema_version",
            "status",
            "summary",
            "explanations",
        ],
    );
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["command"], "check");
    assert_eq!(json["status"], "error");
    assert_eq!(json["summary"], Value::Null);
    assert_eq!(json["explanations"], Value::Null);
    let errors = json["errors"].as_array().expect("lista de diagnósticos");
    assert_eq!(errors.len(), 1);
    assert_keys(
        &errors[0],
        &[
            "kind", "code", "message", "path", "line", "column", "location",
        ],
    );
    assert_eq!(errors[0]["kind"], kind);
    assert_eq!(errors[0]["code"], code);
    assert_eq!(errors[0]["path"], Value::Null);
    json
}

#[test]
fn salida_sin_explain_conserva_bytes_y_status_anteriores() {
    let (root, manifest) = bundle_valido();

    let admit = archivo_invariantes(root.path(), "(forall ((x N)) (Type x a))\n");
    let admit_output = invoke(&check_args(root.path(), &manifest, &admit));
    assert_eq!(admit_output.status.code(), Some(0));
    assert_eq!(
        admit_output.stdout,
        b"ADMIT: 1 fuentes, 1 instancias, 1 evaluadas, 0 fallidas\n"
    );
    assert!(admit_output.stderr.is_empty());

    let reject = archivo_invariantes(
        root.path(),
        &format!("(forall (({VARIABLE_CANARY} N)) (not (Type {VARIABLE_CANARY} a)))\n"),
    );
    let reject_output = invoke(&check_args(root.path(), &manifest, &reject));
    assert_eq!(reject_output.status.code(), Some(1));
    assert_eq!(
        reject_output.stdout,
        b"REJECT: 1 fuentes, 1 instancias, 1 evaluadas, 1 fallidas\n"
    );
    assert!(reject_output.stderr.is_empty());
}

#[test]
fn salida_json_sin_explain_conserva_schema_v1_byte_por_byte() {
    let (root, manifest) = bundle_valido();
    let invariants = archivo_invariantes(root.path(), "(forall ((x N)) (Type x a))\n");
    let mut args = check_args(root.path(), &manifest, &invariants);
    args.extend(["--format".into(), "json".into()]);
    let output = invoke(&args);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        b"{\"command\":\"check\",\"errors\":[],\"schema_version\":1,\"status\":\"admit\",\"summary\":{\"evaluated\":1,\"failed\":0,\"instances\":1,\"sources\":1}}\n"
    );
}

#[test]
fn error_legacy_sin_explain_conserva_schema_v1_y_shape_de_diagnostico() {
    let (root, manifest) = bundle_valido();
    let invariants = archivo_invariantes(root.path(), "(forall ((x N)) (Type x a)\n");
    let mut args = check_args(root.path(), &manifest, &invariants);
    args.extend(["--format".into(), "json".into()]);
    let output = invoke(&args);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    assert!(output.stdout.ends_with(b"\n"));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_keys(
        &json,
        &["command", "errors", "schema_version", "status", "summary"],
    );
    assert_eq!(json["command"], "check");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["status"], "error");
    assert_eq!(json["summary"], Value::Null);
    let errors = json["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_keys(
        &errors[0],
        &[
            "kind", "code", "message", "path", "line", "column", "location",
        ],
    );
    assert_eq!(errors[0]["kind"], "authorship");
    assert_eq!(errors[0]["code"], "invariant_syntax");
    assert!(errors[0]["message"].is_string());
    assert!(
        errors[0]["path"]
            .as_str()
            .unwrap()
            .ends_with("invariants.smt2")
    );
    assert_eq!(errors[0]["line"], 1);
    assert!(errors[0]["column"].is_u64());
    assert_eq!(errors[0]["location"], Value::Null);
}

#[test]
fn explicacion_admitida_no_crea_objetos() {
    let (root, manifest) = bundle_valido();
    let invariants = archivo_invariantes(root.path(), "(forall ((x N)) (Type x a))\n");

    for mode in ["decide", "enumerate"] {
        let output = check_explain(root.path(), &manifest, &invariants, mode);
        let json = resultado_explicativo(&output, 0);
        assert_eq!(json["status"], "admit");
        assert_eq!(json["summary"]["failed"], 0);
        assert_eq!(json["explanations"]["mode"], mode);
        assert_eq!(json["explanations"]["objects"], serde_json::json!([]));
        assert_eq!(json["explanations"]["truncated"], false);
        assert_eq!(json["explanations"]["reason"], Value::Null);
    }
}

#[test]
fn decide_y_enumerate_son_deterministas_y_redactan_canarios() {
    let (root, manifest) = bundle_valido();
    let invariants = archivo_invariantes(
        root.path(),
        &format!(
            "(forall (({VARIABLE_CANARY} N)) (not (Type {VARIABLE_CANARY} a)))\n(forall ((y N)) (not (HasLab y \"{LABEL_CANARY}\")))\n(exists (({VARIABLE_CANARY} N)) (Parent {VARIABLE_CANARY} {VARIABLE_CANARY}))\n"
        ),
    );

    let decide_first = check_explain(root.path(), &manifest, &invariants, "decide");
    let decide_second = check_explain(root.path(), &manifest, &invariants, "decide");
    let decide_json = resultado_explicativo(&decide_first, 1);
    assert_eq!(decide_first.stdout, decide_second.stdout);
    let mut reordered = check_args(root.path(), &manifest, &invariants);
    reordered.extend([
        "--format".into(),
        "json".into(),
        "--explain".into(),
        "decide".into(),
    ]);
    assert_eq!(invoke(&reordered).stdout, decide_first.stdout);
    assert_eq!(decide_json["status"], "reject");
    assert_eq!(decide_json["explanations"]["mode"], "decide");
    assert_eq!(
        decide_json["explanations"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(decide_json["explanations"]["truncated"], false);
    assert_eq!(decide_json["explanations"]["objects"][0]["source_index"], 0);
    assert_eq!(decide_json["summary"]["sources"], 3);
    assert_eq!(decide_json["summary"]["instances"], 3);
    assert_eq!(decide_json["summary"]["evaluated"], 3);
    assert_eq!(decide_json["summary"]["failed"], 3);
    let explanation = &decide_json["explanations"];
    assert_keys(
        explanation,
        &["mode", "objects", "truncated", "reason", "budget"],
    );
    assert_eq!(explanation["reason"], Value::Null);
    assert_keys(
        &explanation["budget"],
        &[
            "limit",
            "consumed",
            "semantic_units",
            "evidence_units",
            "aggregator_units",
        ],
    );
    assert_eq!(explanation["budget"]["limit"], 1_000_000);
    assert!(explanation["budget"]["consumed"].as_u64().unwrap() <= 1_000_000);
    assert_objeto_explicacion(&explanation["objects"][0]);
    assert_eq!(explanation["objects"][0]["presence_core"], Value::Null);
    assert_eq!(explanation["objects"][0]["gap"], Value::Null);

    let enumerate_first = check_explain(root.path(), &manifest, &invariants, "enumerate");
    let enumerate_second = check_explain(root.path(), &manifest, &invariants, "enumerate");
    let enumerate_json = resultado_explicativo(&enumerate_first, 1);
    assert_eq!(enumerate_first.stdout, enumerate_second.stdout);
    assert_eq!(enumerate_json["status"], "reject");
    assert_eq!(enumerate_json["explanations"]["mode"], "enumerate");
    assert_eq!(
        enumerate_json["explanations"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(enumerate_json["explanations"]["truncated"], false);
    assert_eq!(
        enumerate_json["explanations"]["objects"][0]["source_index"],
        0
    );
    assert_eq!(
        enumerate_json["explanations"]["objects"][1]["source_index"],
        1
    );
    assert_eq!(
        enumerate_json["explanations"]["objects"][2]["source_index"],
        2
    );
    for object in enumerate_json["explanations"]["objects"]
        .as_array()
        .unwrap()
    {
        assert_objeto_explicacion(object);
    }
    let gap = &enumerate_json["explanations"]["objects"][2]["gap"];
    assert_eq!(gap["class"], "existence_without_witness");
    assert_eq!(gap["counts"]["available_nodes"], 1);
    assert_eq!(gap["counts"]["available_edges"], 0);
    assert_eq!(gap["counts"]["existential_candidates_evaluated"], 1);
    assert_eq!(gap["counts"]["universal_bindings_examined"], 0);
    assert_eq!(gap["form"]["node_binders"], 1);
    assert_eq!(gap["form"]["edge_binders"], 0);
    assert!(gap["form"]["guard"].is_null());
    assert_positive_form(&gap["form"]["body"]);
    assert_ne!(
        decide_first.stdout, enumerate_first.stdout,
        "enumerate debe distinguirse de decide cuando hay varias instancias fallidas"
    );

    for output in [decide_first, enumerate_first] {
        let text = String::from_utf8(output.stdout).unwrap();
        for canary in [PATH_CANARY, LABEL_CANARY, VARIABLE_CANARY] {
            assert!(!text.contains(canary), "salida filtró {canary:?}: {text}");
        }
    }
}

#[test]
fn enumerate_rechazado_combina_core_y_gap_y_redacta_canarios_de_edge_ext_y_fuente() {
    let (root, manifest) = bundle_con_canarios();
    const FORMULA_SOURCE_CANARY: &str = "(exists ((PRIVATE_VARIABLE_CANARY N)) (Parent PRIVATE_VARIABLE_CANARY PRIVATE_VARIABLE_CANARY))";
    let invariants = archivo_invariantes(
        root.path(),
        &format!(
            "; PRIVATE_INVARIANT_SOURCE_CANARY\n\
             ; (forall ((PRIVATE_FORMULA_TEXT_CANARY N)) (Type PRIVATE_FORMULA_TEXT_CANARY a))\n\
             (forall ((x N)) :guarda (and (Type x a) (HasLab x \"PRIVATE_LABEL_CANARY\"))\
               (not (and (Parent x @n/private/index))))\n\
             {FORMULA_SOURCE_CANARY}\n\
             (forall ((e E)) (not (Edge e @n/private/leaf @n/private/index rel)))\n\
             (forall ((z N)) (not (PRIVATE_ATTRIBUTE_PREDICATE_CANARY z PRIVATE_ATTRIBUTE_VALUE_CANARY)))\n"
        ),
    );

    let json_output = check_explain(root.path(), &manifest, &invariants, "enumerate");
    let json = resultado_explicativo(&json_output, 1);
    assert_eq!(json["status"], "reject");
    assert_eq!(json["summary"]["sources"], 4);
    assert_eq!(json["summary"]["failed"], 4);
    let objects = json["explanations"]["objects"]
        .as_array()
        .expect("enumerate debe entregar objetos");
    assert_eq!(objects.len(), 4);
    assert!(objects.iter().all(|object| object.get("repair").is_none()));

    let objects_with_core: Vec<_> = objects
        .iter()
        .filter(|object| !object["presence_core"].is_null())
        .collect();
    assert_eq!(objects_with_core.len(), 1);
    let presence = objects_with_core[0];
    assert_eq!(presence["source_index"], 0);
    assert!(presence["presence_core"]["atom_count"].as_u64().unwrap() > 0);
    assert!(presence["gap"].is_null());
    let objects_with_gap: Vec<_> = objects
        .iter()
        .filter(|object| !object["gap"].is_null())
        .collect();
    assert_eq!(objects_with_gap.len(), 1);
    let gap = objects_with_gap[0];
    assert_eq!(gap["source_index"], 1);
    assert_eq!(gap["gap"]["class"], "existence_without_witness");
    assert!(gap["presence_core"].is_null());
    let edge = objects
        .iter()
        .find(|object| object["source_index"] == 2)
        .expect("la fórmula de Edge es la fuente 2");
    assert!(edge["presence_core"].is_null());
    assert!(edge["gap"].is_null());
    assert!(contiene_predicado(&edge["counterexample"], "edge"));
    let extension = objects
        .iter()
        .find(|object| object["source_index"] == 3)
        .expect("la fórmula de Ext es la fuente 3");
    assert!(extension["presence_core"].is_null());
    assert!(extension["gap"].is_null());
    assert!(contiene_predicado(
        &extension["counterexample"],
        "extension"
    ));

    let text = String::from_utf8(json_output.stdout).unwrap();
    let canaries = [
        "n/private/leaf→n/private/index#rel#0",
        "n/private/leaf",
        "n/private/index",
        "PRIVATE_ROOT_LABEL_CANARY",
        "PRIVATE_LABEL_CANARY",
        "PRIVATE_ATTRIBUTE_PREDICATE_CANARY",
        "PRIVATE_ATTRIBUTE_VALUE_CANARY",
        "PRIVATE_VARIABLE_CANARY",
        "PRIVATE_FORMULA_TEXT_CANARY",
        "PRIVATE_INVARIANT_SOURCE_CANARY",
        FORMULA_SOURCE_CANARY,
    ];
    for canary in canaries {
        assert!(!text.contains(canary), "JSON opt-in filtró {canary:?}");
    }

    let text_output = check_explain_text(root.path(), &manifest, &invariants, "enumerate");
    assert_eq!(text_output.status.code(), Some(1));
    assert!(text_output.stderr.is_empty());
    let text = String::from_utf8(text_output.stdout).unwrap();
    assert!(text.starts_with("REJECT: 4 fuentes, 4 instancias, 4 evaluadas, 4 fallidas\n"));
    assert!(text.contains("EXPLANATIONS mode=enumerate"));
    for canary in canaries {
        assert!(!text.contains(canary), "texto opt-in filtró {canary:?}");
    }
}

#[test]
fn esquema_expandido_vincula_ordinals_de_fuente_e_instancia_deterministicamente() {
    let (root, manifest) = bundle_con_dos_labels();
    let invariants = archivo_invariantes(
        root.path(),
        "(Type @n/a a)\n(esquema ((l Lab)) (forall ((x N)) (not (HasLab x l))))\n",
    );

    let decide_a = check_explain(root.path(), &manifest, &invariants, "decide");
    let decide_b = check_explain(root.path(), &manifest, &invariants, "decide");
    assert_eq!(decide_a.stdout, decide_b.stdout);
    let decide = resultado_explicativo(&decide_a, 1);
    assert_eq!(decide["status"], "reject");
    assert_eq!(decide["summary"]["sources"], 2);
    assert_eq!(decide["summary"]["instances"], 3);
    assert_eq!(decide["summary"]["failed"], 2);
    let decision = &decide["explanations"]["objects"];
    assert_eq!(decision.as_array().unwrap().len(), 1);
    assert_eq!(decision[0]["source_index"], 1);
    assert_eq!(decision[0]["instance_index"], 0);
    assert_eq!(decision[0]["line"], 2);
    assert_eq!(decision[0]["column"], 1);

    let enumerate_a = check_explain(root.path(), &manifest, &invariants, "enumerate");
    let enumerate_b = check_explain(root.path(), &manifest, &invariants, "enumerate");
    assert_eq!(enumerate_a.stdout, enumerate_b.stdout);
    let enumerate = resultado_explicativo(&enumerate_a, 1);
    assert_eq!(enumerate["status"], "reject");
    let objects = enumerate["explanations"]["objects"]
        .as_array()
        .expect("enumerate debe mostrar ambas instancias");
    assert_eq!(objects.len(), 2);
    assert_eq!(
        objects
            .iter()
            .map(|object| object["source_index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 1]
    );
    assert_eq!(
        objects
            .iter()
            .map(|object| object["instance_index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(
        objects
            .iter()
            .all(|object| object["line"] == 2 && object["column"] == 1)
    );
}

#[test]
fn explain_texto_anade_marker_redactado_y_conserva_linea_legacy() {
    let (root, manifest) = bundle_valido();
    let invariants = archivo_invariantes(
        root.path(),
        &format!("(forall (({VARIABLE_CANARY} N)) (not (Type {VARIABLE_CANARY} a)))\n"),
    );
    let first = check_explain_text(root.path(), &manifest, &invariants, "decide");
    let second = check_explain_text(root.path(), &manifest, &invariants, "decide");

    assert_eq!(first.status.code(), Some(1));
    assert!(first.stderr.is_empty());
    assert_eq!(first.stdout, second.stdout);
    let text = String::from_utf8(first.stdout).unwrap();
    assert!(text.starts_with("REJECT: 1 fuentes, 1 instancias, 1 evaluadas, 1 fallidas\n"));
    assert!(text.contains("EXPLANATIONS mode=decide"));
    assert!(text.contains("objects=1"));
    assert!(text.contains("truncated=false"));
    for canary in [PATH_CANARY, LABEL_CANARY, VARIABLE_CANARY] {
        assert!(!text.contains(canary), "salida de texto filtró {canary:?}");
    }
}

#[test]
fn errores_optin_json_v2_y_texto_son_redactados_y_no_dejan_parcial() {
    let (root, manifest) = bundle_valido();
    let malformed = archivo_invariantes(
        root.path(),
        &format!("(not-a-form PRIVATE_VARIABLE_CANARY {LABEL_CANARY})\n"),
    );

    let json_output = check_explain(root.path(), &manifest, &malformed, "enumerate");
    let json = resultado_error_explicativo(&json_output, "authorship", "invariant_syntax");
    assert!(json["errors"][0]["message"].is_string());
    assert!(json["errors"][0]["line"].is_u64());
    assert!(json["errors"][0]["column"].is_u64());
    let json_text = String::from_utf8(json_output.stdout).unwrap();
    for canary in [PATH_CANARY, LABEL_CANARY, VARIABLE_CANARY] {
        assert!(
            !json_text.contains(canary),
            "JSON de error filtró {canary:?}"
        );
    }

    let text_first = check_explain_text(root.path(), &manifest, &malformed, "enumerate");
    let text_second = check_explain_text(root.path(), &manifest, &malformed, "enumerate");
    assert_eq!(text_first.status.code(), Some(1));
    assert!(text_first.stdout.is_empty());
    assert_eq!(text_first.stderr, text_second.stderr);
    let stderr = String::from_utf8(text_first.stderr).unwrap();
    assert!(stderr.contains("invariant_syntax"));
    for canary in [PATH_CANARY, LABEL_CANARY, VARIABLE_CANARY] {
        assert!(!stderr.contains(canary), "texto de error filtró {canary:?}");
    }

    let missing = root.path().join("MISSING_PRIVATE_PATH_CANARY.smt2");
    let missing_output = check_explain(root.path(), &manifest, &missing, "decide");
    let missing_json = resultado_error_explicativo(&missing_output, "resource", "invariants_io");
    assert_eq!(missing_json["explanations"], Value::Null);
    assert!(
        !String::from_utf8(missing_output.stdout)
            .unwrap()
            .contains("MISSING_PRIVATE_PATH_CANARY")
    );
}

#[test]
fn enumerate_trunca_en_cien_y_detecta_la_instancia_ciento_una() {
    let (root, manifest) = bundle_valido();
    let formula = "(forall ((x N)) (not (Type x a)))\n";
    let invariants = archivo_invariantes(root.path(), &formula.repeat(101));
    let output = check_explain(root.path(), &manifest, &invariants, "enumerate");
    let json = resultado_explicativo(&output, 1);

    assert_eq!(json["status"], "reject");
    assert_eq!(json["explanations"]["mode"], "enumerate");
    assert_eq!(
        json["explanations"]["objects"].as_array().unwrap().len(),
        100
    );
    assert_eq!(json["explanations"]["truncated"], true);
    assert_eq!(json["explanations"]["reason"], "result_limit");
}

#[test]
fn enumerate_exactamente_cien_no_se_marca_truncado() {
    let (root, manifest) = bundle_valido();
    let formula = "(forall ((x N)) (not (Type x a)))\n";
    let invariants = archivo_invariantes(root.path(), &formula.repeat(100));
    let output = check_explain(root.path(), &manifest, &invariants, "enumerate");
    let json = resultado_explicativo(&output, 1);

    assert_eq!(json["status"], "reject");
    assert_eq!(json["summary"]["sources"], 100);
    assert_eq!(json["summary"]["instances"], 100);
    assert_eq!(json["summary"]["failed"], 100);
    assert_eq!(json["explanations"]["mode"], "enumerate");
    assert_eq!(
        json["explanations"]["objects"].as_array().unwrap().len(),
        100
    );
    assert_eq!(json["explanations"]["truncated"], false);
    assert_eq!(json["explanations"]["reason"], Value::Null);
}

#[test]
fn explain_invalido_o_sin_valor_es_error_de_uso() {
    let (root, manifest) = bundle_valido();
    let invariants = archivo_invariantes(root.path(), "(forall ((x N)) (Type x a))\n");
    let base = check_args(root.path(), &manifest, &invariants);

    for value in [None, Some("all")] {
        let mut args = base.clone();
        args.push("--explain".into());
        if let Some(value) = value {
            args.push(value.into());
        }
        let output = invoke(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Uso:"));
    }

    let mut other_command = vec![
        "validate".into(),
        root.path().to_string_lossy().into_owned(),
        "--manifiesto".into(),
        manifest.to_string_lossy().into_owned(),
        "--explain".into(),
        "decide".into(),
    ];
    let output = invoke(&other_command);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());

    other_command = check_args(root.path(), &manifest, &invariants);
    other_command.extend([
        "--explain".into(),
        "decide".into(),
        "--explain".into(),
        "enumerate".into(),
    ]);
    let output = invoke(&other_command);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
