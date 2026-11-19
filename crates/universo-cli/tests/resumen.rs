//! Fase 10, U8: `universo resumen`.
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
    for (texto, proyecto) in [
        ("Regla firme", "Alfa"),
        ("Regla dudosa", "Alfa"),
        ("Regla suelta", "Gamma"),
    ] {
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
        ));
    }
    ok(&universo_en(
        &repo,
        &[
            "reforzar",
            "criterios/regla-firme",
            "--proyecto",
            "Beta",
            "--contexto",
            "c",
            "--fecha",
            "2026-11-19",
        ],
    ));
    ok(&universo_en(
        &repo,
        &[
            "objetar",
            "criterios/regla-dudosa",
            "--proyecto",
            "Beta",
            "--contexto",
            "c",
            "--fecha",
            "2026-11-19",
        ],
    ));
    repo
}

fn universo<'a>(json: &'a Value, nombre: &str) -> &'a Value {
    json["universes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"] == nombre)
        .unwrap()
}

#[test]
fn resumen_cuenta_por_universo_y_dice_que_promover_y_que_revisar() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let s = universo_en(&repo, &["resumen", "--format=json"]);
    ok(&s);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    assert_eq!(json["command"], "resumen");
    let c = universo(&json, "candidatos");
    assert_eq!(c["notes"]["criterio"], 3, "{c}");
    assert_eq!(c["notes"]["evidencia"], 2, "{c}");
    assert_eq!(c["notes"]["proyecto"], 3, "{c}");
    assert_eq!(c["criterios_by_state"]["candidato"], 3, "{c}");
    assert_eq!(c["rules"], 5);
    // regla-firme: Alfa y Beta. regla-dudosa: objetada. Ninguna sin relaciones.
    let listas: Vec<&str> = c["ready_to_promote"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(listas, vec!["criterios/regla-firme"], "{c}");
    assert_eq!(
        c["objected"],
        serde_json::json!(["criterios/regla-dudosa"]),
        "{c}"
    );
    assert_eq!(c["without_relations"], serde_json::json!([]), "{c}");
    assert_eq!(c["recent"].as_array().unwrap().len(), 3, "{c}");
    let v = universo(&json, "vigente");
    assert_eq!(v["notes"], serde_json::json!({}), "{v}");
    assert_eq!(v["rules"], 9);

    // «Listas para promover» es lo que vigente admite.
    for nota in ["criterios/regla-firme", "criterios/regla-suelta"] {
        let s = universo_en(&repo, &["promover", nota, "--a", "vigente", "--dry-run"]);
        let admite = s.codigo == 0;
        assert_eq!(admite, listas.contains(&nota), "{nota}: {}", s.stdout);
    }

    let texto = universo_en(&repo, &["resumen"]);
    ok(&texto);
    assert!(
        texto.stdout.contains("universe: candidatos"),
        "{}",
        texto.stdout
    );
    assert!(
        texto
            .stdout
            .contains("ready to promote: criterios/regla-firme (2 projects)"),
        "{}",
        texto.stdout
    );

    let s = universo_en(dir.path(), &["resumen"]);
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("criterio_not_found"), "{}", s.stdout);
}

#[test]
fn resumen_respeta_ignorar_version_json_y_universos_invalidos() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    ok(&universo_en(
        &repo,
        &["promover", "criterios/regla-firme", "--a", "vigente"],
    ));
    // Un universo roto no impide ver el otro.
    std::fs::write(
        repo.join("universos/vigente/bundle/criterios/roto.md"),
        "sin frontmatter\n",
    )
    .unwrap();
    let s = universo_en(&repo, &["resumen", "--format=json"]);
    ok(&s);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    assert_eq!(universo(&json, "candidatos")["status"], "ok");
    assert_ne!(universo(&json, "vigente")["status"], "ok", "{json}");
    // Lo promovido ya no cuenta como objetado ni listo.
    let c = universo(&json, "candidatos");
    assert_eq!(c["ready_to_promote"], serde_json::json!([]), "{c}");
    // Versión de motor fuera de rango, en JSON.
    let ruta = repo.join("felix.toml");
    let texto = std::fs::read_to_string(&ruta).unwrap();
    let linea = texto
        .lines()
        .find(|l| l.starts_with("motor = "))
        .unwrap()
        .to_string();
    std::fs::write(&ruta, texto.replace(&linea, "motor = \"=9.9.9\"")).unwrap();
    let s = universo_en(&repo, &["resumen", "--format=json"]);
    assert_eq!(s.codigo, 1);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    assert_eq!(json["errors"][0]["code"], "engine_version_mismatch");
    ok(&universo_en(&repo, &["resumen", "--ignorar-version"]));
}

/// U12: una objeción a un criterio ya vigente no lo retira, pero `resumen`
/// la marca para revisarla.
#[test]
fn resumen_marca_una_objecion_a_un_criterio_vigente() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    ok(&universo_en(
        &repo,
        &["promover", "criterios/regla-firme", "--a", "vigente"],
    ));
    ok(&universo_en(
        &repo,
        &[
            "objetar",
            "criterios/regla-firme",
            "--universo",
            "vigente",
            "--proyecto",
            "Delta",
            "--contexto",
            "no aplicó",
            "--fecha",
            "2026-11-20",
        ],
    ));
    let s = universo_en(&repo, &["resumen", "--format=json"]);
    ok(&s);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    let v = universo(&json, "vigente");
    assert_eq!(
        v["objected"],
        serde_json::json!(["criterios/regla-firme"]),
        "{v}"
    );
    assert_eq!(v["criterios_by_state"]["vigente"], 1, "{v}");
    // Retirado, ya no se marca.
    ok(&universo_en(
        &repo,
        &[
            "atributo",
            "criterios/regla-firme",
            "estado",
            "retirado",
            "--universo",
            "vigente",
        ],
    ));
    let s = universo_en(&repo, &["resumen", "--format=json"]);
    let json2: Value = serde_json::from_str(&s.stdout).unwrap();
    assert_eq!(
        universo(&json2, "vigente")["objected"],
        serde_json::json!([]),
        "{}",
        s.stdout
    );
    // Lo promovido que queda en candidatos no se marca allí.
    let c = universo(&json, "candidatos");
    assert_eq!(
        c["objected"],
        serde_json::json!(["criterios/regla-dudosa"]),
        "{c}"
    );
}
