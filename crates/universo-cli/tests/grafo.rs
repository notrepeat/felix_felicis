//! `universo grafo`: una página HTML local con el grafo del repositorio de
//! criterio (o de un bundle OKF), con los datos incrustados y sin recursos
//! externos.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Salida {
    codigo: i32,
    stdout: String,
    stderr: String,
}

fn universo_en(cwd: &Path, args: &[&str]) -> Salida {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .current_dir(cwd)
        .env_remove("FELIX_CRITERIO")
        // Sin repositorio de usuario: nunca se lee la configuración real.
        .env("XDG_CONFIG_HOME", "/felix-tests/sin-config")
        .env("APPDATA", "/felix-tests/sin-config")
        .env_remove("FELIX_UNIVERSO")
        .output()
        .unwrap();
    Salida {
        codigo: out.status.code().unwrap(),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

fn ok(s: &Salida) {
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
}

/// Repositorio con: un criterio promovido a vigente, una candidata lista para
/// promover y otra objetada.
fn repo(dir: &Path) -> PathBuf {
    let repo = dir.join("mi-criterio");
    ok(&universo_en(
        dir,
        &["init", repo.to_str().unwrap(), "--git"],
    ));
    let capturar = |texto: &str, proyecto: &str| {
        ok(&universo_en(
            &repo,
            &[
                "capturar",
                texto,
                "--proyecto",
                proyecto,
                "--fecha",
                "2026-11-18",
            ],
        ))
    };
    let reforzar = |nota: &str, proyecto: &str| {
        ok(&universo_en(
            &repo,
            &[
                "reforzar",
                nota,
                "--proyecto",
                proyecto,
                "--contexto",
                "se sostuvo",
            ],
        ))
    };
    capturar("Reutilizar antes de crear", "Tienda");
    reforzar("criterios/reutilizar-antes-de-crear", "Pagos");
    ok(&universo_en(
        &repo,
        &[
            "promover",
            "criterios/reutilizar-antes-de-crear",
            "--a",
            "vigente",
        ],
    ));
    capturar("Fechas en UTC", "Tienda");
    reforzar("criterios/fechas-en-utc", "Pagos");
    capturar("Usar herencia en componentes", "Tienda");
    ok(&universo_en(
        &repo,
        &[
            "objetar",
            "criterios/usar-herencia-en-componentes",
            "--proyecto",
            "Pagos",
            "--contexto",
            "la composición resultó más simple",
        ],
    ));
    repo
}

/// Los datos incrustados en la página.
fn datos(html: &str) -> Value {
    let inicio = html
        .find(r#"<script id="datos" type="application/json">"#)
        .expect("sin bloque de datos");
    let resto = &html[inicio..];
    let abre = resto.find('>').unwrap() + 1;
    let cierra = resto.find("</script>").unwrap();
    serde_json::from_str(&resto[abre..cierra]).unwrap()
}

fn nodo<'a>(d: &'a Value, id: &str) -> &'a Value {
    d["nodos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
        .unwrap_or_else(|| panic!("sin nodo {id}"))
}

fn generar(repo: &Path, extra: &[&str]) -> (Value, String) {
    let salida = repo.parent().unwrap().join("grafo.html");
    let mut args = vec!["grafo", "--salida", salida.to_str().unwrap()];
    args.extend(extra);
    let s = universo_en(repo, &args);
    ok(&s);
    assert!(s.stdout.contains(salida.to_str().unwrap()), "{}", s.stdout);
    let html = fs::read_to_string(&salida).unwrap();
    (datos(&html), html)
}

#[test]
fn el_grafo_de_un_repositorio_tiene_ambos_universos_y_sus_relaciones() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let (d, html) = generar(&repo, &[]);
    assert_eq!(d["schema"], "felix-grafo-v1");
    assert_eq!(d["universos"], serde_json::json!(["candidatos", "vigente"]));

    let promovida = nodo(&d, "candidatos/criterios/reutilizar-antes-de-crear");
    assert_eq!(promovida["tipo"], "criterio");
    assert_eq!(promovida["atributos"]["Estado"], "promovido");
    let vigente = nodo(&d, "vigente/criterios/reutilizar-antes-de-crear");
    assert_eq!(vigente["atributos"]["Estado"], "vigente");
    assert!(
        vigente["cuerpo"]
            .as_str()
            .unwrap()
            .contains("Reutilizar antes de crear")
    );

    let aristas = d["aristas"].as_array().unwrap();
    let hay = |origen: &str, destino: &str, clave: &str| {
        aristas
            .iter()
            .any(|a| a["origen"] == origen && a["destino"] == destino && a["clave"] == clave)
    };
    assert!(hay(
        "vigente/criterios/reutilizar-antes-de-crear",
        "vigente/proyectos/proyecto-tienda",
        "confirmado_en"
    ));
    // La promoción une los dos universos.
    assert!(hay(
        "candidatos/criterios/reutilizar-antes-de-crear",
        "vigente/criterios/reutilizar-antes-de-crear",
        "promovido"
    ));
    // Toda arista une nodos que existen.
    let ids: Vec<&str> = d["nodos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    for a in aristas {
        assert!(ids.contains(&a["origen"].as_str().unwrap()), "{a}");
        assert!(ids.contains(&a["destino"].as_str().unwrap()), "{a}");
    }

    // Las marcas son las de `resumen`.
    let marcas = |id: &str| nodo(&d, id)["marcas"].clone();
    assert_eq!(
        marcas("candidatos/criterios/fechas-en-utc"),
        serde_json::json!(["lista_para_promover"])
    );
    assert_eq!(
        marcas("candidatos/criterios/usar-herencia-en-componentes"),
        serde_json::json!(["objetada"])
    );
    assert_eq!(
        marcas("vigente/criterios/reutilizar-antes-de-crear"),
        serde_json::json!([])
    );

    // Página autocontenida: nada de red.
    assert!(
        html.contains(r#"http-equiv="Content-Security-Policy" content="default-src 'none';"#),
        "sin CSP restrictiva"
    );
    for externo in ["http://", "https://", "src=\"//"] {
        assert!(!html.contains(externo), "recurso externo: {externo}");
    }
}

#[test]
fn un_solo_universo() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let (d, _) = generar(&repo, &["--universo", "vigente"]);
    assert_eq!(d["universos"], serde_json::json!(["vigente"]));
    for n in d["nodos"].as_array().unwrap() {
        assert_eq!(n["universo"], "vigente", "{n}");
    }
    assert!(
        d["aristas"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["clave"] != "promovido")
    );
}

#[test]
fn un_titulo_con_cierre_de_script_no_rompe_la_pagina() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    ok(&universo_en(
        dir.path(),
        &["init", repo.to_str().unwrap(), "--git"],
    ));
    let malo = "Cierre </script><script>alert(1)</script> y & <b>";
    ok(&universo_en(&repo, &["capturar", malo, "--proyecto", "P"]));
    let (d, html) = generar(&repo, &[]);
    assert!(!html.contains("<script>alert(1)"), "inyección en el HTML");
    let titulos: Vec<&str> = d["nodos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["titulo"].as_str().unwrap())
        .collect();
    assert!(titulos.contains(&malo), "{titulos:?}");
}

#[test]
fn un_bundle_okf_cualquiera_se_grafica_con_sus_rutas() {
    let dir = tempfile::tempdir().unwrap();
    let kb = dir.path().join("kb");
    ok(&universo_en(dir.path(), &["init", kb.to_str().unwrap()]));
    let salida = dir.path().join("g.html");
    let s = universo_en(
        dir.path(),
        &[
            "grafo",
            kb.join("bundle").to_str().unwrap(),
            "--manifiesto",
            kb.join("manifest.toml").to_str().unwrap(),
            "--invariantes",
            kb.join("invariants.sx").to_str().unwrap(),
            "--salida",
            salida.to_str().unwrap(),
        ],
    );
    ok(&s);
    let d = datos(&fs::read_to_string(&salida).unwrap());
    assert_eq!(d["universos"], serde_json::json!(["bundle"]));
    assert_eq!(nodo(&d, "bundle/notas/bienvenida")["titulo"], "Bienvenida");
}

#[test]
fn salida_a_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let s = universo_en(&repo, &["grafo", "--salida", "-"]);
    ok(&s);
    assert!(
        s.stdout.starts_with("<!doctype html>"),
        "{}",
        &s.stdout[..80]
    );
    assert_eq!(datos(&s.stdout)["schema"], "felix-grafo-v1");
}

#[test]
fn errores_de_uso_y_fuera_de_un_repositorio() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["grafo", "--desconocida"][..],
        &["grafo", "algo"],
        &["grafo", "--salida"],
        &["grafo", "-x"],
    ] {
        let s = universo_en(dir.path(), args);
        assert_eq!(s.codigo, 2, "{args:?}: {}{}", s.stdout, s.stderr);
    }
    let s = universo_en(dir.path(), &["grafo"]);
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
    assert!(s.stdout.contains("code:"), "{}", s.stdout);
    let repo = repo(dir.path());
    let s = universo_en(&repo, &["grafo", "--universo", "no-existe"]);
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
}

/// Sólo la marca que deja `promover` en un criterio une universos: una nota
/// que cita la frase no inventa una promoción, y una relación repetida se
/// dibuja una vez.
#[test]
fn promociones_falsas_y_relaciones_repetidas_no_aparecen() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    ok(&universo_en(
        &repo,
        &[
            "capturar",
            "Citar Promovido desde candidatos (criterios/fechas-en-utc) no es promover",
            "--proyecto",
            "Tienda",
        ],
    ));
    // Una relación repetida a mano, aceptada con seal.
    let nota = repo.join("universos/candidatos/bundle/criterios/fechas-en-utc.md");
    let texto = fs::read_to_string(&nota).unwrap();
    let repetido = texto.replacen(
        "confirmado_en: [\"[[proyecto-tienda]]\"",
        "confirmado_en: [\"[[proyecto-tienda]]\", \"[[proyecto-tienda]]\"",
        1,
    );
    assert_ne!(texto, repetido, "{texto}");
    fs::write(&nota, repetido).unwrap();
    ok(&universo_en(
        &repo,
        &["seal", "--universo", "candidatos", "--motivo", "a mano"],
    ));

    let (d, _) = generar(&repo, &[]);
    let aristas = d["aristas"].as_array().unwrap();
    let promovidos: Vec<&Value> = aristas
        .iter()
        .filter(|a| a["clave"] == "promovido")
        .collect();
    assert_eq!(promovidos.len(), 1, "{promovidos:?}");
    let mut vistas = std::collections::BTreeSet::new();
    for a in aristas {
        assert!(vistas.insert(a.to_string()), "arista repetida: {a}");
    }
}

/// Sin --salida, cada corrida escribe un archivo nuevo y privado en el
/// directorio temporal: nunca sobre uno que ya exista.
#[cfg(unix)]
#[test]
fn la_salida_por_defecto_es_un_archivo_nuevo_y_privado() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let ruta = |s: &Salida| {
        PathBuf::from(
            s.stdout
                .lines()
                .next()
                .unwrap()
                .strip_prefix("grafo: ")
                .unwrap(),
        )
    };
    let (a, b) = (
        universo_en(&repo, &["grafo"]),
        universo_en(&repo, &["grafo"]),
    );
    ok(&a);
    ok(&b);
    let (ra, rb) = (ruta(&a), ruta(&b));
    assert_ne!(ra, rb);
    for r in [&ra, &rb] {
        assert_eq!(
            fs::metadata(r).unwrap().permissions().mode() & 0o777,
            0o600,
            "{r:?}"
        );
        fs::remove_file(r).unwrap();
    }
}
