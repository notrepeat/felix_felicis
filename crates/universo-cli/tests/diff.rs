//! `universo diff <raiz> <desde> [<hasta>]`: diferencia neta entre dos estados
//! del historial, compuesta con los deltas que guardó cada apply.
use serde_json::{Value, json};
use std::{path::Path, process::Command};

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

fn aplicar(kb: &Path, delta: &str, motivo: &str) {
    let archivo = kb.join("delta.json");
    std::fs::write(&archivo, delta).unwrap();
    let base = [
        "propose".to_string(),
        kb.join("bundle").display().to_string(),
        format!("--manifiesto={}", kb.join("manifest.toml").display()),
        format!("--invariantes={}", kb.join("invariants.sx").display()),
        format!("--delta={}", archivo.display()),
        "--format=json".to_string(),
    ];
    let args: Vec<&str> = base.iter().map(String::as_str).collect();
    let mut sim = args.clone();
    sim.push("--emit-snapshot-token");
    let (_, salida) = universo(&sim);
    let token = serde_json::from_str::<Value>(&salida).unwrap()["snapshot_token"]
        .as_str()
        .unwrap()
        .to_string();
    let esperado = format!("--expect-snapshot={token}");
    let razon = format!("--reason={motivo}");
    let mut apply = args.clone();
    apply.extend(["--apply", &esperado, "--author=ana", &razon]);
    let (codigo, salida) = universo(&apply);
    assert_eq!(codigo, 0, "{salida}");
}

const ACTIVAR: &str = r#"{"schema":"felix-delta-v1","add":["Estado(notas/bienvenida, vigente)"],"remove":["Estado(notas/bienvenida, borrador)"]}"#;
const CREAR: &str = r#"{"schema":"felix-delta-v2","create":[{"path":"notas/nueva","type":"fuente","title":"Nueva"}],"update":[],"add":["Estado(notas/nueva, borrador)","HasLab(notas/nueva, \"Nueva\")","Type(notas/nueva, fuente)"],"remove":[]}"#;
const VOLVER: &str = r#"{"schema":"felix-delta-v1","add":["Estado(notas/bienvenida, borrador)"],"remove":["Estado(notas/bienvenida, vigente)"]}"#;

#[test]
fn diff_compone_el_rango_en_texto_y_json() {
    let dir = tempfile::tempdir().unwrap();
    let kb = dir.path().join("kb");
    assert_eq!(universo(&["init", kb.to_str().unwrap()]).0, 0);
    aplicar(&kb, ACTIVAR, "activar");
    aplicar(&kb, CREAR, "crear");
    aplicar(&kb, VOLVER, "volver");
    let raiz = kb.join("bundle");
    let raiz = raiz.to_str().unwrap();

    let (codigo, texto) = universo(&["diff", raiz, "0", "2"]);
    assert_eq!(codigo, 0, "{texto}");
    assert_eq!(
        texto,
        "diff: state 0 -> state 2 (3 entries)\n\
         create notas/nueva (entry 2)\n\
         \x20 + Estado(notas/nueva, borrador)\n\
         \x20 + HasLab(notas/nueva, \"Nueva\")\n\
         \x20 + Type(notas/nueva, fuente)\n\
         - Estado(notas/bienvenida, borrador)\n\
         + Estado(notas/bienvenida, vigente)\n"
    );

    // Ida y vuelta del estado de la bienvenida: sólo queda la nota creada.
    // Sin <hasta>, llega al último estado.
    let (codigo, json) = universo(&["diff", raiz, "0", "--format=json"]);
    assert_eq!(codigo, 0, "{json}");
    let json: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json["command"], "diff");
    assert_eq!(
        (json["from"].clone(), json["to"].clone()),
        (json!(0), json!(3))
    );
    assert_eq!(json["add"], json!([]));
    assert_eq!(json["remove"], json!([]));
    assert_eq!(json["body_updated"], json!([]));
    assert_eq!(
        json["created"],
        json!([{"path":"notas/nueva","entry":2,
                "add":["Estado(notas/nueva, borrador)","HasLab(notas/nueva, \"Nueva\")",
                       "Type(notas/nueva, fuente)"],"remove":[]}])
    );

    // Desde un estado posterior a la creación, la nota ya existía.
    let (_, texto) = universo(&["diff", raiz, "2", "3"]);
    assert_eq!(
        texto,
        "diff: state 2 -> state 3 (3 entries)\n\
         - Estado(notas/bienvenida, vigente)\n\
         + Estado(notas/bienvenida, borrador)\n"
    );
    let (_, texto) = universo(&["diff", raiz, "3", "3"]);
    assert_eq!(texto, "diff: state 3 -> state 3 (3 entries)\nno changes\n");
}

#[test]
fn diff_errores_de_dominio_salen_con_1_y_de_uso_con_2() {
    let dir = tempfile::tempdir().unwrap();
    let kb = dir.path().join("kb");
    assert_eq!(universo(&["init", kb.to_str().unwrap()]).0, 0);
    aplicar(&kb, ACTIVAR, "activar");
    let raiz = kb.join("bundle");
    let raiz = raiz.to_str().unwrap();

    let (codigo, texto) = universo(&["diff", raiz, "1", "0"]);
    assert_eq!(
        (codigo, texto.as_str()),
        (1, "diff: error\ncode: range_invalid\n")
    );
    let (codigo, json) = universo(&["diff", raiz, "0", "5", "--format=json"]);
    assert_eq!(codigo, 1);
    let json: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(
        json["errors"],
        json!([{"code":"entry_not_found","entry":5}])
    );

    for args in [
        vec!["diff", raiz],
        vec!["diff", raiz, "x"],
        vec!["diff", raiz, "-1"],
        vec!["diff", raiz, "0", "1", "2"],
        vec!["diff", raiz, "0", "--format=xml"],
    ] {
        assert_eq!(universo(&args).0, 2, "{args:?}");
    }
}

/// Un alta de algo ya presente es un no-op que el delta guardado no distingue:
/// el diff compone el efecto registrado, no el delta pedido.
#[test]
fn diff_no_confunde_un_alta_ya_presente_con_un_cambio() {
    let dir = tempfile::tempdir().unwrap();
    let kb = dir.path().join("kb");
    assert_eq!(universo(&["init", kb.to_str().unwrap()]).0, 0);
    // La bienvenida ya está en borrador: este alta no cambia nada.
    aplicar(
        &kb,
        r#"{"schema":"felix-delta-v1","add":["Estado(notas/bienvenida, borrador)"],"remove":[]}"#,
        "nada",
    );
    aplicar(&kb, ACTIVAR, "activar");
    let raiz = kb.join("bundle");
    let raiz = raiz.to_str().unwrap();
    let (_, texto) = universo(&["diff", raiz, "0", "1"]);
    assert_eq!(texto, "diff: state 0 -> state 1 (2 entries)\nno changes\n");
    let (_, texto) = universo(&["diff", raiz, "0", "2"]);
    assert_eq!(
        texto,
        "diff: state 0 -> state 2 (2 entries)\n\
         - Estado(notas/bienvenida, borrador)\n\
         + Estado(notas/bienvenida, vigente)\n"
    );
}

/// Un `seal` que acepta una edición externa deja una entrada `seal`: diff no
/// sabe qué cambió y lo informa (`external_change`) en vez de omitirlo.
#[test]
fn diff_detecta_el_hueco_de_un_cambio_fuera_del_historial() {
    let dir = tempfile::tempdir().unwrap();
    let kb = dir.path().join("kb");
    assert_eq!(universo(&["init", kb.to_str().unwrap()]).0, 0);
    aplicar(&kb, ACTIVAR, "activar");
    let nota = kb.join("bundle/notas/bienvenida.md");
    let texto = std::fs::read_to_string(&nota).unwrap();
    std::fs::write(&nota, format!("{texto}Editado a mano.\n")).unwrap();
    let raiz = kb.join("bundle");
    let raiz = raiz.to_str().unwrap();
    let manifiesto = format!("--manifiesto={}", kb.join("manifest.toml").display());
    let invariantes = format!("--invariantes={}", kb.join("invariants.sx").display());
    assert_eq!(universo(&["seal", raiz, &manifiesto, &invariantes]).0, 0);
    aplicar(&kb, VOLVER, "volver");

    let (codigo, texto) = universo(&["diff", raiz, "0", "3"]);
    assert_eq!(codigo, 1);
    assert!(
        texto.starts_with("diff: error\ncode: external_change\nentry: 2\n"),
        "{texto}"
    );
    assert_eq!(universo(&["diff", raiz, "1", "3"]).0, 1);
    // Los tramos que no cruzan el sello siguen disponibles.
    assert_eq!(universo(&["diff", raiz, "0", "1"]).0, 0);
    assert_eq!(universo(&["diff", raiz, "2", "3"]).0, 0);
}

#[test]
fn diff_un_cuerpo_cambiado_y_revertido_no_cuenta() {
    let dir = tempfile::tempdir().unwrap();
    let kb = dir.path().join("kb");
    assert_eq!(universo(&["init", kb.to_str().unwrap()]).0, 0);
    let nota = kb.join("bundle/notas/bienvenida.md");
    let original = std::fs::read_to_string(&nota).unwrap();
    let cuerpo_original = original.splitn(3, "---\n").nth(2).unwrap().to_string();
    let actualizar = |cuerpo: &str| {
        json!({"schema":"felix-delta-v2","create":[],"update":[{"path":"notas/bienvenida","body":cuerpo}],"add":[],"remove":[]})
            .to_string()
    };
    aplicar(&kb, &actualizar("Otro cuerpo.\n"), "cambiar");
    aplicar(&kb, &actualizar(&cuerpo_original), "revertir");
    let raiz = kb.join("bundle");
    let raiz = raiz.to_str().unwrap();
    let (_, texto) = universo(&["diff", raiz, "0", "1"]);
    assert_eq!(
        texto,
        "diff: state 0 -> state 1 (2 entries)\nupdate body notas/bienvenida\n"
    );
    let (_, texto) = universo(&["diff", raiz, "0", "2"]);
    assert_eq!(texto, "diff: state 0 -> state 2 (2 entries)\nno changes\n");
}
