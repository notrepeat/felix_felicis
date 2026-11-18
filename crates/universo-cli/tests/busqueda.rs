//! `universo search`: búsqueda léxica sobre las notas del bundle (P3).
use serde_json::Value;
use std::process::Command;

fn universo(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn search_encuentra_por_titulo_y_cuerpo_en_texto_y_json() {
    let dir = tempfile::tempdir().unwrap();
    let kb = dir.path().join("kb");
    assert_eq!(universo(&["init", kb.to_str().unwrap()]).0, 0);
    std::fs::write(
        kb.join("bundle/notas/cache.md"),
        "---\ntype: criterio\ntitle: Caché de lectura\nestado: vigente\n---\nUna caché local\u{7} para el repositorio.\n",
    )
    .unwrap();
    let raiz = kb.join("bundle");
    let raiz = raiz.to_str().unwrap();
    let manifiesto = format!("--manifiesto={}", kb.join("manifest.toml").display());

    let (codigo, texto, _) = universo(&["search", raiz, &manifiesto, "REPOSITORIO"]);
    assert_eq!(codigo, 0);
    assert!(texto.starts_with("search: 2 of 2 matches\n"), "{texto}");
    // Ambas sólo en el cuerpo: empate de puntaje, desempate por ruta.
    let cache = texto.find("notas/bienvenida").unwrap();
    assert!(cache < texto.find("notas/cache").unwrap(), "{texto}");
    // Los caracteres de control del bundle salen escapados.
    assert!(texto.contains("Una caché local\\u{7} para"), "{texto}");

    let (codigo, json, _) = universo(&[
        "search",
        raiz,
        &manifiesto,
        "cache",
        "lectura",
        "--format=json",
        "--limit",
        "5",
    ]);
    assert_eq!(codigo, 0);
    let json: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json["command"], "search");
    assert_eq!(json["query"], "cache lectura");
    assert_eq!(json["total"], 1);
    let m = &json["matches"][0];
    assert_eq!(m["path"], "notas/cache");
    assert_eq!(m["type"], "criterio");
    assert_eq!(m["title"], "Caché de lectura");
    assert_eq!(m["fields"], serde_json::json!(["title", "body"]));

    let (codigo, texto, _) = universo(&["search", raiz, &manifiesto, "inexistente"]);
    assert_eq!((codigo, texto.as_str()), (0, "search: 0 of 0 matches\n"));

    let (_, texto, _) = universo(&["search", raiz, &manifiesto, "repositorio", "--limit=1"]);
    assert!(texto.starts_with("search: 1 of 2 matches\n"), "{texto}");

    // Tras `--`, un término que empieza con '-' es un término, no una opción.
    let (codigo, texto, _) = universo(&["search", raiz, &manifiesto, "--", "-x"]);
    assert_eq!((codigo, texto.as_str()), (0, "search: 0 of 0 matches\n"));
}

#[test]
fn search_rechaza_uso_invalido() {
    for args in [
        vec!["search"],
        vec!["search", "raiz", "--manifiesto=m.toml"],
        vec!["search", "raiz", "termino"],
        vec!["search", "raiz", "--manifiesto=m.toml", "t", "--limit=0"],
        vec!["search", "raiz", "--manifiesto=m.toml", "t", "--format=xml"],
        vec!["search", "raiz", "--manifiesto=m.toml", "   "],
        vec!["search", "raiz", "--manifiesto=m.toml", "-x"],
        vec![
            "search",
            "raiz",
            "--manifiesto=m.toml",
            "t",
            "--limit=2",
            "--limit=3",
        ],
    ] {
        assert_eq!(universo(&args).0, 2, "{args:?}");
    }
}
