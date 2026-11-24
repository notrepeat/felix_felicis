//! Fase 10, U11: `promover` de `candidatos` a `vigente`.
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

fn ok(s: &Salida) {
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
}

fn repo(dir: &Path) -> PathBuf {
    let repo = dir.join("mi-criterio");
    ok(&universo_en(
        dir,
        &["init", repo.to_str().unwrap(), "--git"],
    ));
    repo
}

fn entradas(repo: &Path, universo: &str) -> usize {
    let s = universo_en(repo, &["log", "--universo", universo, "--format=json"]);
    serde_json::from_str::<Value>(&s.stdout).unwrap()["entries"]
        .as_array()
        .unwrap()
        .len()
}

fn leer(repo: &Path, universo: &str, nota: &str) -> String {
    std::fs::read_to_string(
        repo.join("universos")
            .join(universo)
            .join("bundle")
            .join(format!("{nota}.md")),
    )
    .unwrap()
}

const NOTA: &str = "criterios/reutilizar-antes-de-crear";

/// Una candidata capturada en un proyecto.
fn candidata(repo: &Path) {
    ok(&universo_en(
        repo,
        &[
            "capturar",
            "Reutilizar antes de crear",
            "--proyecto",
            "Tienda web",
            "--validacion",
            "explicita",
            "--fecha",
            "2026-11-18",
        ],
    ));
}

fn reforzar(repo: &Path, proyecto: &str) {
    ok(&universo_en(
        repo,
        &[
            "reforzar",
            NOTA,
            "--proyecto",
            proyecto,
            "--contexto",
            "se reutilizó el componente de pagos",
            "--fecha",
            "2026-11-19",
        ],
    ));
}

fn promover(repo: &Path, extra: &[&str]) -> Salida {
    let mut args = vec![
        "promover",
        NOTA,
        "--a",
        "vigente",
        "--autor",
        "yo",
        "--fecha",
        "2026-11-20",
    ];
    args.extend(extra);
    universo_en(repo, &args)
}

#[test]
fn con_un_solo_proyecto_la_regla_de_vigente_rechaza_y_nada_se_escribe() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    let s = promover(&repo, &[]);
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
    assert!(s.stdout.contains("promover: reject"), "{}", s.stdout);
    assert!(s.stdout.contains("universe: vigente"), "{}", s.stdout);
    assert!(
        s.stdout.contains("rule: vigente-confirmado"),
        "{}",
        s.stdout
    );
    assert_eq!(entradas(&repo, "vigente"), 0);
    assert_eq!(entradas(&repo, "candidatos"), 1);
    assert!(leer(&repo, "candidatos", NOTA).contains("estado: candidato"));
}

#[test]
fn con_dos_proyectos_promueve_y_marca_la_candidata() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    reforzar(&repo, "API pagos");

    // --dry-run no escribe en ningún universo.
    let s = promover(&repo, &["--dry-run"]);
    ok(&s);
    assert!(s.stdout.contains("dry run"), "{}", s.stdout);
    assert_eq!(
        (entradas(&repo, "vigente"), entradas(&repo, "candidatos")),
        (0, 2)
    );

    let s = promover(&repo, &["--format=json"]);
    ok(&s);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    assert_eq!(json["status"], "applied");
    assert_eq!(json["from"], format!("candidatos/{NOTA}"));
    assert_eq!(json["to"], format!("vigente/{NOTA}"));

    let vigente = leer(&repo, "vigente", NOTA);
    for esperado in [
        "estado: vigente",
        "validacion: explicita",
        "fecha: 2026-11-18",
        "[[proyecto-tienda-web]]",
        "[[proyecto-api-pagos]]",
        "Promovido desde candidatos",
    ] {
        assert!(
            vigente.contains(esperado),
            "falta {esperado:?} en:\n{vigente}"
        );
    }
    let origen = leer(&repo, "candidatos", NOTA);
    assert!(origen.contains("estado: promovido"), "{origen}");
    assert!(
        origen.contains("Promovido a vigente el 2026-11-20"),
        "{origen}"
    );
    for universo in ["candidatos", "vigente"] {
        ok(&universo_en(&repo, &["guard", "--universo", universo]));
    }
    assert_eq!(
        (entradas(&repo, "vigente"), entradas(&repo, "candidatos")),
        (1, 3)
    );
    let log = universo_en(&repo, &["log", "--universo", "vigente"]);
    assert!(log.stdout.contains("author=yo"), "{}", log.stdout);

    // Promover otra vez no hace nada.
    let s = promover(&repo, &[]);
    assert_eq!(s.codigo, 1, "{}", s.stdout);
    assert!(s.stdout.contains("promote_already_done"), "{}", s.stdout);
}

#[test]
fn una_promocion_a_medias_se_completa_sin_duplicar() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    reforzar(&repo, "API pagos");
    ok(&promover(&repo, &[]));
    // Simula que la segunda transacción no llegó a marcar la candidata.
    ok(&universo_en(
        &repo,
        &["atributo", NOTA, "estado", "candidato"],
    ));
    let s = promover(&repo, &[]);
    ok(&s);
    assert!(
        s.stdout.contains(&format!("to: vigente/{NOTA} (existing)")),
        "{}",
        s.stdout
    );
    assert_eq!(entradas(&repo, "vigente"), 1);
    assert!(leer(&repo, "candidatos", NOTA).contains("estado: promovido"));
}

#[test]
fn si_el_origen_rechaza_la_marca_lo_dice_y_el_destino_queda_escrito() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    reforzar(&repo, "API pagos");
    // Una regla del origen que prohíbe marcar candidatas como promovidas.
    let reglas = repo.join("universos/candidatos/invariants.sx");
    let mas_una = dir.path().join("reglas.sx");
    std::fs::write(
        &mas_una,
        std::fs::read_to_string(&reglas).unwrap()
            + "\n(regla :id \"nada-promovido\" :mensaje \"Ningún criterio de candidatos se marca como promovido.\"\n  (forall ((x N)) (not (Estado x promovido))))\n",
    )
    .unwrap();
    ok(&universo_en(
        &repo,
        &[
            "reglas",
            "aplicar",
            mas_una.to_str().unwrap(),
            "--universo",
            "candidatos",
        ],
    ));
    let s = promover(&repo, &[]);
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
    assert!(s.stdout.contains("promote_source_rejected"), "{}", s.stdout);
    assert!(s.stdout.contains("rule: nada-promovido"), "{}", s.stdout);
    assert!(leer(&repo, "vigente", NOTA).contains("estado: vigente"));
    assert!(leer(&repo, "candidatos", NOTA).contains("estado: candidato"));
}

#[test]
fn errores_de_promocion() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    let s = universo_en(&repo, &["promover", "criterios/nada", "--a", "vigente"]);
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("note_not_found"), "{}", s.stdout);
    let s = universo_en(&repo, &["promover", NOTA, "--a", "candidatos"]);
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("promote_same_universe"), "{}", s.stdout);
    let s = universo_en(&repo, &["promover", NOTA, "--a", "axiomas"]);
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("universe_not_found"), "{}", s.stdout);
    // Falta --a, o sobra un argumento.
    assert_eq!(universo_en(&repo, &["promover", NOTA]).codigo, 2);
    assert_eq!(
        universo_en(&repo, &["promover", NOTA, "x", "--a", "vigente"]).codigo,
        2
    );
    // Fuera de un repositorio de criterio.
    let s = universo_en(dir.path(), &["promover", NOTA, "--a", "vigente"]);
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("criterio_not_found"), "{}", s.stdout);
}

#[test]
fn un_criterio_distinto_con_el_mismo_slug_no_se_toma_por_ya_promovido() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let base = "Los componentes deben ser reutilizables en todos los proyectos nuevos de";
    let mut notas = Vec::new();
    for (sufijo, proyectos) in [("web", ["Alfa", "Beta"]), ("mobile", ["Gamma", "Delta"])] {
        let texto = format!("{base} {sufijo}");
        let s = universo_en(
            &repo,
            &[
                "capturar",
                &texto,
                "--proyecto",
                proyectos[0],
                "--fecha",
                "2026-11-18",
                "--forzar",
            ],
        );
        ok(&s);
        let nota = s
            .stdout
            .lines()
            .find_map(|l| l.strip_prefix("note: "))
            .unwrap()
            .to_string();
        ok(&universo_en(
            &repo,
            &[
                "reforzar",
                &nota,
                "--proyecto",
                proyectos[1],
                "--contexto",
                "c",
                "--fecha",
                "2026-11-19",
            ],
        ));
        notas.push(nota);
    }
    for nota in &notas {
        let s = universo_en(
            &repo,
            &["promover", nota, "--a", "vigente", "--fecha", "2026-11-20"],
        );
        ok(&s);
        assert!(!s.stdout.contains("(existing)"), "{}", s.stdout);
    }
    assert_eq!(entradas(&repo, "vigente"), 2);
}

#[test]
fn otro_criterio_con_el_mismo_titulo_en_el_destino_es_un_conflicto() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    reforzar(&repo, "API pagos");
    ok(&promover(&repo, &[]));
    // Una candidata nueva con el mismo título (otra nota) no se da por promovida.
    let s = universo_en(
        &repo,
        &[
            "capturar",
            "Reutilizar antes de crear",
            "--proyecto",
            "Otro",
            "--fecha",
            "2026-11-21",
            "--forzar",
        ],
    );
    ok(&s);
    let nota = s
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("note: "))
        .unwrap()
        .to_string();
    ok(&universo_en(
        &repo,
        &[
            "reforzar",
            &nota,
            "--proyecto",
            "Uno mas",
            "--contexto",
            "c",
            "--fecha",
            "2026-11-21",
        ],
    ));
    let s = universo_en(&repo, &["promover", &nota, "--a", "vigente"]);
    assert_eq!(s.codigo, 1, "{}", s.stdout);
    assert!(s.stdout.contains("promote_conflict"), "{}", s.stdout);
    assert!(leer(&repo, "candidatos", &nota).contains("estado: candidato"));
}

#[test]
fn search_en_todos_los_universos_dice_de_cual_viene_cada_resultado() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    reforzar(&repo, "API pagos");
    ok(&promover(&repo, &[]));
    let s = universo_en(
        &repo,
        &[
            "search",
            "--universo",
            "todos",
            "reutilizar",
            "--format=json",
        ],
    );
    ok(&s);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    let universos: Vec<&str> = json["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["universe"].as_str().unwrap())
        .collect();
    assert!(
        universos.contains(&"candidatos") && universos.contains(&"vigente"),
        "{json}"
    );
    let s = universo_en(&repo, &["search", "--universo=todos", "reutilizar"]);
    ok(&s);
    assert!(
        s.stdout.contains(&format!("vigente: {NOTA}")),
        "{}",
        s.stdout
    );
}

#[test]
fn promover_muestra_las_evidencias_que_respaldan_la_confirmacion() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    reforzar(&repo, "API pagos");
    let s = promover(&repo, &["--dry-run"]);
    ok(&s);
    assert!(s.stdout.contains("evidence: evidencias/"), "{}", s.stdout);
    assert!(s.stdout.contains("(API pagos)"), "{}", s.stdout);
}

/// D2 (2026-11-19): la `forma` de una candidata viaja con la promoción, y en
/// `vigente` un criterio sólo reemplaza a otro ya retirado.
#[test]
fn la_forma_se_promueve_y_reemplazar_exige_retirar_antes() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    ok(&universo_en(
        &repo,
        &["atributo", NOTA, "forma", "principio"],
    ));
    reforzar(&repo, "API pagos");
    ok(&promover(&repo, &[]));
    let vigente = leer(&repo, "vigente", NOTA);
    assert!(vigente.contains("forma: principio"), "{vigente}");

    // Un segundo criterio vigente que reemplaza al primero.
    let nueva = "criterios/reutilizar-y-documentar-lo-reutilizado";
    for (args, proyecto) in [
        (
            vec![
                "capturar",
                "Reutilizar y documentar lo reutilizado",
                "--proyecto",
                "Tienda web",
                "--validacion",
                "explicita",
            ],
            None,
        ),
        (vec![], Some("API pagos")),
    ] {
        if let Some(p) = proyecto {
            ok(&universo_en(
                &repo,
                &["reforzar", nueva, "--proyecto", p, "--contexto", "c"],
            ));
        } else {
            ok(&universo_en(&repo, &args));
        }
    }
    ok(&universo_en(&repo, &["promover", nueva, "--a", "vigente"]));

    let s = universo_en(
        &repo,
        &[
            "relacion",
            nueva,
            "reemplaza",
            NOTA,
            "--universo",
            "vigente",
        ],
    );
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
    assert!(
        s.stdout.contains("rule: reemplazado-retirado"),
        "{}",
        s.stdout
    );
    ok(&universo_en(
        &repo,
        &[
            "atributo",
            NOTA,
            "estado",
            "retirado",
            "--universo",
            "vigente",
        ],
    ));
    ok(&universo_en(
        &repo,
        &[
            "relacion",
            nueva,
            "reemplaza",
            NOTA,
            "--universo",
            "vigente",
        ],
    ));
    ok(&universo_en(&repo, &["guard", "--universo", "vigente"]));
}

/// Una segunda candidata, confirmada en dos proyectos, con `relacion` hacia
/// la primera.
fn segunda(repo: &Path, relacion: &str) -> &'static str {
    let nueva = "criterios/crear-sin-reutilizar";
    ok(&universo_en(
        repo,
        &[
            "capturar",
            "Crear sin reutilizar",
            "--proyecto",
            "Tienda web",
            "--validacion",
            "explicita",
        ],
    ));
    ok(&universo_en(
        repo,
        &[
            "reforzar",
            nueva,
            "--proyecto",
            "API pagos",
            "--contexto",
            "c",
        ],
    ));
    ok(&universo_en(repo, &["relacion", nueva, relacion, NOTA]));
    nueva
}

/// D2: `contradice` y `reemplaza` viajan con la promoción cuando su destino
/// ya está en vigente, y ahí deciden las reglas de vigente.
#[test]
fn las_relaciones_entre_criterios_se_promueven_y_las_reglas_de_vigente_deciden() {
    for (relacion, regla) in [
        ("contradice", "sin-contradiccion-vigente"),
        ("reemplaza", "reemplazado-retirado"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(dir.path());
        candidata(&repo);
        reforzar(&repo, "API pagos");
        ok(&promover(&repo, &[]));
        let nueva = segunda(&repo, relacion);
        let s = universo_en(&repo, &["promover", nueva, "--a", "vigente"]);
        assert_eq!(s.codigo, 1, "{relacion}: {}{}", s.stdout, s.stderr);
        assert!(s.stdout.contains(&format!("rule: {regla}")), "{}", s.stdout);

        // Con el anterior retirado, se admite y la relación queda en vigente.
        ok(&universo_en(
            &repo,
            &[
                "atributo",
                NOTA,
                "estado",
                "retirado",
                "--universo",
                "vigente",
            ],
        ));
        ok(&universo_en(&repo, &["promover", nueva, "--a", "vigente"]));
        let vigente = leer(&repo, "vigente", nueva);
        assert!(
            vigente.contains(&format!("{relacion}: [\"[[reutilizar-antes-de-crear]]\"]")),
            "{vigente}"
        );
        ok(&universo_en(&repo, &["guard", "--universo", "vigente"]));
    }
}

/// Si el destino de la relación no está en vigente, no hay nada que proteger
/// allí: la relación no se copia y la salida lo dice.
#[test]
fn una_relacion_hacia_un_criterio_que_no_esta_en_vigente_se_informa() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    let nueva = segunda(&repo, "contradice");
    let s = universo_en(
        &repo,
        &[
            "promover",
            nueva,
            "--a",
            "vigente",
            "--dry-run",
            "--format=json",
        ],
    );
    ok(&s);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    assert_eq!(
        json["relations_not_copied"],
        serde_json::json!([{"relation": "contradice", "to": NOTA}])
    );
    let s = universo_en(&repo, &["promover", nueva, "--a", "vigente"]);
    ok(&s);
    assert!(
        s.stdout
            .contains(&format!("relation not copied: contradice -> {NOTA}")),
        "{}",
        s.stdout
    );
    let vigente = leer(&repo, "vigente", nueva);
    assert!(!vigente.contains("contradice"), "{vigente}");
}

/// Una relación repetida en el origen llega una sola vez al destino.
#[test]
fn una_relacion_repetida_se_promueve_una_vez() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    candidata(&repo);
    reforzar(&repo, "API pagos");
    ok(&promover(&repo, &[]));
    ok(&universo_en(
        &repo,
        &[
            "atributo",
            NOTA,
            "estado",
            "retirado",
            "--universo",
            "vigente",
        ],
    ));
    let nueva = segunda(&repo, "contradice");
    // `relacion` no repite una relación; una edición a mano aceptada con
    // `seal`, sí.
    let ruta = repo
        .join("universos/candidatos/bundle")
        .join(format!("{nueva}.md"));
    let texto = std::fs::read_to_string(&ruta).unwrap().replace(
        "contradice: [\"[[reutilizar-antes-de-crear]]\"]",
        "contradice: [\"[[reutilizar-antes-de-crear]]\", \"[[reutilizar-antes-de-crear]]\"]",
    );
    std::fs::write(&ruta, texto).unwrap();
    ok(&universo_en(
        &repo,
        &["seal", "--universo", "candidatos", "--motivo", "a mano"],
    ));
    let origen = leer(&repo, "candidatos", nueva);
    assert_eq!(
        origen.matches("[[reutilizar-antes-de-crear]]").count(),
        2,
        "{origen}"
    );
    ok(&universo_en(&repo, &["promover", nueva, "--a", "vigente"]));
    let vigente = leer(&repo, "vigente", nueva);
    assert_eq!(
        vigente.matches("[[reutilizar-antes-de-crear]]").count(),
        1,
        "{vigente}"
    );
    ok(&universo_en(&repo, &["guard", "--universo", "vigente"]));
}
