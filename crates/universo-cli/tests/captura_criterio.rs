//! Fase 10, U3 y U12: `capturar`, `reforzar` y `objetar` en un repositorio de
//! criterio creado con `init --git`.
use serde_json::Value;
use std::{
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

fn repo(dir: &Path) -> PathBuf {
    let repo = dir.join("mi-criterio");
    let s = universo_en(dir, &["init", repo.to_str().unwrap(), "--git"]);
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    repo
}

fn leer(repo: &Path, nota: &str) -> String {
    std::fs::read_to_string(
        repo.join("universos/candidatos/bundle")
            .join(format!("{nota}.md")),
    )
    .unwrap()
}

fn entradas_log(repo: &Path) -> usize {
    let s = universo_en(repo, &["log", "--format=json"]);
    serde_json::from_str::<Value>(&s.stdout).unwrap()["entries"]
        .as_array()
        .unwrap()
        .len()
}

const TEXTO: &str = "Los componentes deben ser 100 % reutilizables";
const NOTA: &str = "criterios/los-componentes-deben-ser-100-reutilizables";

fn capturar(repo: &Path, extra: &[&str]) -> Salida {
    let mut args = vec![
        "capturar",
        TEXTO,
        "--proyecto",
        "Tienda web",
        "--tarea",
        "refactor del carrito",
        "--validacion",
        "explicita",
        "--fecha",
        "2026-11-18",
        "--autor",
        "agente:prueba",
    ];
    args.extend(extra);
    universo_en(repo, &args)
}

#[test]
fn capturar_crea_la_candidata_con_su_contexto_y_su_proyecto() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());

    // --dry-run simula y no escribe nada.
    let s = capturar(&repo, &["--dry-run"]);
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    assert!(s.stdout.contains("dry run"), "{}", s.stdout);
    assert_eq!(entradas_log(&repo), 0);

    let s = capturar(&repo, &["--format=json"]);
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    assert_eq!(json["command"], "capturar");
    assert_eq!(json["status"], "applied");
    assert_eq!(json["note"], NOTA);
    assert_eq!(json["project"], "proyectos/proyecto-tienda-web");
    assert_eq!(json["project_created"], true);

    let nota = leer(&repo, NOTA);
    for esperado in [
        "type: criterio",
        &format!("title: {TEXTO}"),
        "estado: candidato",
        "validacion: explicita",
        "fecha: 2026-11-18",
        "confirmado_en: [\"[[proyecto-tienda-web]]\"]",
        "Tarea: refactor del carrito",
        "Proyecto: Tienda web",
    ] {
        assert!(nota.contains(esperado), "falta {esperado:?} en:\n{nota}");
    }
    assert!(leer(&repo, "proyectos/proyecto-tienda-web").contains("title: Tienda web"));
    assert_eq!(universo_en(&repo, &["guard"]).codigo, 0);
    let log = universo_en(&repo, &["log"]);
    assert!(
        log.stdout.contains("author=agente:prueba"),
        "{}",
        log.stdout
    );

    // La misma afirmación otra vez: no duplica salvo con --forzar.
    let s = capturar(&repo, &[]);
    assert_eq!(s.codigo, 1, "{}", s.stdout);
    assert!(
        s.stdout.contains("capture_similar") && s.stdout.contains(NOTA),
        "{}",
        s.stdout
    );
    let s = capturar(&repo, &["--forzar"]);
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    assert!(s.stdout.contains(&format!("{NOTA}-2")), "{}", s.stdout);
    // El proyecto ya existía: se reutiliza.
    assert!(
        s.stdout
            .contains("project: proyectos/proyecto-tienda-web (existing)"),
        "{}",
        s.stdout
    );
}

#[test]
fn reforzar_confirma_en_otro_proyecto_y_objetar_deja_la_evidencia() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    assert_eq!(capturar(&repo, &[]).codigo, 0);

    let reforzar = |proyecto: &str| {
        universo_en(
            &repo,
            &[
                "reforzar",
                NOTA,
                "--proyecto",
                proyecto,
                "--contexto",
                "se aplicó al crear el módulo de cobros",
                "--validacion",
                "implicita",
                "--fecha",
                "2026-11-19",
            ],
        )
    };
    let s = reforzar("API pagos");
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    assert!(
        s.stdout
            .contains("evidence: evidencias/evidencia-2026-11-19-"),
        "{}",
        s.stdout
    );
    let nota = leer(&repo, NOTA);
    assert!(
        nota.contains("[[proyecto-api-pagos]]") && nota.contains("[[proyecto-tienda-web]]"),
        "{nota}"
    );
    // Un segundo refuerzo en el mismo proyecto no suma otra confirmación.
    assert_eq!(reforzar("API pagos").codigo, 0);
    let nota_despues = leer(&repo, NOTA);
    assert_eq!(
        nota_despues.matches("proyecto-api-pagos").count(),
        1,
        "{nota_despues}"
    );

    let s = universo_en(
        &repo,
        &[
            "objetar",
            NOTA,
            "--proyecto",
            "Prototipo X",
            "--contexto",
            "aquí se prefirió duplicar para entregar antes",
            "--fecha",
            "2026-11-20",
        ],
    );
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    // Objetar no confirma.
    assert!(!leer(&repo, NOTA).contains("proyecto-prototipo-x"));
    let atomos = universo_en(&repo, &["atomos"]).stdout;
    assert_eq!(atomos.matches(", apoya)").count(), 2, "{atomos}");
    assert_eq!(atomos.matches(", objeta)").count(), 1, "{atomos}");
    assert_eq!(universo_en(&repo, &["guard"]).codigo, 0);
    assert_eq!(entradas_log(&repo), 4);
}

#[test]
fn errores_de_captura() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    // Nota inexistente.
    let s = universo_en(
        &repo,
        &[
            "reforzar",
            "criterios/nada",
            "--proyecto",
            "P",
            "--contexto",
            "c",
        ],
    );
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("note_not_found"), "{}", s.stdout);
    // Uso inválido: validación fuera del dominio, falta el proyecto, el texto o
    // el contexto, o la fecha no es válida.
    for args in [
        vec!["capturar", "x", "--proyecto", "P", "--validacion", "quizas"],
        vec!["capturar", "x"],
        vec!["capturar", "--proyecto", "P"],
        vec!["reforzar", NOTA, "--proyecto", "P"],
        vec!["capturar", "x", "--proyecto", "P", "--fecha", "2026-13-40"],
    ] {
        assert_eq!(universo_en(&repo, &args).codigo, 2, "{args:?}");
    }
    // Un bundle sin el esquema de captura (init sin --git).
    let viejo = dir.path().join("viejo");
    assert_eq!(
        universo_en(dir.path(), &["init", viejo.to_str().unwrap()]).codigo,
        0
    );
    let manifiesto = format!("--manifiesto={}", viejo.join("manifest.toml").display());
    let invariantes = format!("--invariantes={}", viejo.join("invariants.sx").display());
    let bundle = viejo.join("bundle");
    let s = universo_en(
        dir.path(),
        &[
            "capturar",
            bundle.to_str().unwrap(),
            &manifiesto,
            &invariantes,
            "algo",
            "--proyecto",
            "P",
        ],
    );
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
    assert!(s.stdout.contains("capture_schema_missing"), "{}", s.stdout);
}

#[test]
fn una_captura_rechazada_muestra_la_regla_y_no_escribe_nada() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let reglas = repo.join("universos/candidatos/invariants.sx");
    let mut texto = std::fs::read_to_string(&reglas).unwrap();
    texto.push_str(
        "\n(regla :id \"cerrado\" :mensaje \"Este universo no admite criterios nuevos.\"\n  (forall ((x N)) (not (Type x criterio))))\n",
    );
    std::fs::write(&reglas, texto).unwrap();
    let antes = std::fs::read_dir(repo.join("universos/candidatos/bundle/criterios"))
        .unwrap()
        .count();
    let s = capturar(&repo, &[]);
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
    assert!(s.stdout.contains("capturar: reject"), "{}", s.stdout);
    assert!(
        s.stdout
            .contains("rule: cerrado — Este universo no admite criterios nuevos."),
        "{}",
        s.stdout
    );
    let despues = std::fs::read_dir(repo.join("universos/candidatos/bundle/criterios"))
        .unwrap()
        .count();
    assert_eq!(antes, despues);
    assert!(
        !repo
            .join("universos/candidatos/bundle/proyectos/proyecto-tienda-web.md")
            .exists()
    );
    assert_eq!(entradas_log(&repo), 0);
}

#[test]
fn proyectos_y_duplicados_equivalentes_y_nombres_reservados() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let cap = |texto: &str, proyecto: &str| {
        universo_en(
            &repo,
            &[
                "capturar",
                texto,
                "--proyecto",
                proyecto,
                "--fecha",
                "2026-11-18",
            ],
        )
    };
    // El mismo proyecto escrito de otra forma no crea otro.
    assert_eq!(cap("Primera regla", "tienda-web").codigo, 0);
    let s = cap("Segunda regla", "Tienda Web");
    assert!(
        s.stdout
            .contains("project: proyectos/proyecto-tienda-web (existing)"),
        "{}",
        s.stdout
    );
    // La misma afirmación con otra puntuación es un duplicado.
    assert_eq!(cap("Usar X.", "tienda-web").codigo, 0);
    let s = cap("usar x", "tienda-web");
    assert_eq!(s.codigo, 1, "{}", s.stdout);
    assert!(s.stdout.contains("capture_similar"), "{}", s.stdout);
    // Un nombre reservado del sistema de archivos recibe un sufijo.
    let s = cap("con", "tienda-web");
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    assert!(
        s.stdout.contains("note: criterios/con-nota"),
        "{}",
        s.stdout
    );
    let s = cap("Index", "tienda-web");
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    assert!(
        s.stdout.contains("note: criterios/index-nota"),
        "{}",
        s.stdout
    );
    assert_eq!(universo_en(&repo, &["guard"]).codigo, 0);
}

#[test]
fn un_nombre_que_solo_difiere_en_mayusculas_no_choca() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    // Una nota hecha a mano con el mismo nombre salvo mayúsculas.
    std::fs::write(
        repo.join("universos/candidatos/bundle/proyectos/Proyecto-Zeta.md"),
        "---\ntype: proyecto\ntitle: Otra cosa\n---\n",
    )
    .unwrap();
    let s = universo_en(
        &repo,
        &[
            "capturar",
            "Regla zeta",
            "--proyecto",
            "Zeta",
            "--fecha",
            "2026-11-18",
        ],
    );
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    assert!(
        s.stdout
            .contains("project: proyectos/proyecto-zeta-2 (created)"),
        "{}",
        s.stdout
    );
}
