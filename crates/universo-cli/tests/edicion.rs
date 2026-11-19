//! Fase 10, U3: `relacion`, `atributo` y `nota editar`.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

struct Salida {
    codigo: i32,
    stdout: String,
    stderr: String,
}

fn universo_con(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Salida {
    let mut comando = Command::new(env!("CARGO_BIN_EXE_universo"));
    comando
        .args(args)
        .current_dir(cwd)
        .env_remove("FELIX_CRITERIO")
        // Sin repositorio de usuario: nunca se lee la configuración real.
        .env("XDG_CONFIG_HOME", "/felix-tests/sin-config")
        .env("APPDATA", "/felix-tests/sin-config")
        .env_remove("FELIX_UNIVERSO")
        .env_remove("VISUAL")
        .env_remove("EDITOR");
    for (k, v) in env {
        comando.env(k, v);
    }
    let out = comando.output().unwrap();
    Salida {
        codigo: out.status.code().unwrap(),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

fn universo_en(cwd: &Path, args: &[&str]) -> Salida {
    universo_con(cwd, args, &[])
}

fn ok(s: &Salida) {
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
}

/// Repositorio con dos candidatas en proyectos distintos.
fn repo(dir: &Path) -> PathBuf {
    let repo = dir.join("mi-criterio");
    ok(&universo_en(
        dir,
        &["init", repo.to_str().unwrap(), "--git"],
    ));
    for (texto, proyecto) in [("Regla uno", "Alfa"), ("Regla dos", "Beta")] {
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
    repo
}

fn leer(repo: &Path, nota: &str) -> String {
    std::fs::read_to_string(
        repo.join("universos/candidatos/bundle")
            .join(format!("{nota}.md")),
    )
    .unwrap()
}

#[test]
fn relacion_agrega_quita_y_respeta_las_reglas() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let s = universo_en(
        &repo,
        &[
            "relacion",
            "criterios/regla-uno",
            "confirmado_en",
            "proyectos/proyecto-beta",
        ],
    );
    ok(&s);
    assert!(s.stdout.contains("relacion: applied"), "{}", s.stdout);
    assert!(leer(&repo, "criterios/regla-uno").contains("[[proyecto-beta]]"));
    let s = universo_en(
        &repo,
        &[
            "relacion",
            "criterios/regla-uno",
            "confirmado_en",
            "proyectos/proyecto-beta",
            "--quitar",
        ],
    );
    ok(&s);
    assert!(!leer(&repo, "criterios/regla-uno").contains("[[proyecto-beta]]"));
    // Quitar lo que no está, una clave desconocida o una nota inexistente.
    for (args, codigo) in [
        (
            vec![
                "relacion",
                "criterios/regla-uno",
                "confirmado_en",
                "proyectos/proyecto-beta",
                "--quitar",
            ],
            "relation_not_found",
        ),
        (
            vec![
                "relacion",
                "criterios/regla-uno",
                "inventada",
                "criterios/regla-dos",
            ],
            "relation_unknown",
        ),
        (
            vec![
                "relacion",
                "criterios/nada",
                "confirmado_en",
                "proyectos/proyecto-beta",
            ],
            "note_not_found",
        ),
    ] {
        let s = universo_en(&repo, &args);
        assert_eq!(s.codigo, 1, "{args:?}");
        assert!(s.stdout.contains(codigo), "{args:?}: {}", s.stdout);
    }
    // Una relación mal tipada la rechaza la regla.
    let s = universo_en(
        &repo,
        &[
            "relacion",
            "criterios/regla-uno",
            "apoya",
            "criterios/regla-dos",
        ],
    );
    assert_eq!(s.codigo, 1);
    assert!(
        s.stdout.contains("rule: aristas-bien-tipadas"),
        "{}",
        s.stdout
    );
    ok(&universo_en(&repo, &["guard"]));
}

#[test]
fn atributo_reemplaza_un_valor_y_valida_el_dominio() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    assert!(leer(&repo, "criterios/regla-uno").contains("validacion: pendiente"));
    let s = universo_en(
        &repo,
        &["atributo", "criterios/regla-uno", "validacion", "explicita"],
    );
    ok(&s);
    let nota = leer(&repo, "criterios/regla-uno");
    assert!(
        nota.contains("validacion: explicita") && !nota.contains("validacion: pendiente"),
        "{nota}"
    );
    // El mismo valor otra vez: nada que hacer.
    let s = universo_en(
        &repo,
        &["atributo", "criterios/regla-uno", "validacion", "explicita"],
    );
    ok(&s);
    assert!(s.stdout.contains("no changes"), "{}", s.stdout);
    // Fuera del dominio, atributo desconocido.
    let s = universo_en(
        &repo,
        &["atributo", "criterios/regla-uno", "validacion", "quizas"],
    );
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("value_not_in_domain"), "{}", s.stdout);
    let s = universo_en(&repo, &["atributo", "criterios/regla-uno", "color", "rojo"]);
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("attribute_unknown"), "{}", s.stdout);
    // Una fecha.
    ok(&universo_en(
        &repo,
        &["atributo", "criterios/regla-uno", "fecha", "2026-01-02"],
    ));
    assert!(leer(&repo, "criterios/regla-uno").contains("fecha: 2026-01-02"));
    ok(&universo_en(&repo, &["guard"]));
}

#[cfg(unix)]
#[test]
fn nota_editar_aplica_lo_que_cambia_el_editor() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    // Un «editor» que agrega una línea al cuerpo.
    let editor = "sh -c 'printf \"Línea agregada.\\n\" >> \"$1\"' sh";
    let s = universo_con(
        &repo,
        &["nota", "editar", "criterios/regla-uno"],
        &[("EDITOR", editor)],
    );
    ok(&s);
    assert!(s.stdout.contains("nota editar: applied"), "{}", s.stdout);
    assert!(leer(&repo, "criterios/regla-uno").contains("Línea agregada."));
    let diff = universo_en(&repo, &["diff", "2"]);
    assert!(
        diff.stdout.contains("update body criterios/regla-uno"),
        "{}",
        diff.stdout
    );
    // Un editor que no cambia nada.
    let s = universo_con(
        &repo,
        &["nota", "editar", "criterios/regla-uno"],
        &[("EDITOR", "true")],
    );
    ok(&s);
    assert!(s.stdout.contains("no changes"), "{}", s.stdout);
    // Sin editor.
    let s = universo_en(&repo, &["nota", "editar", "criterios/regla-uno"]);
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("editor_missing"), "{}", s.stdout);
    ok(&universo_en(&repo, &["guard"]));
}

#[cfg(unix)]
#[test]
fn nota_editar_rechaza_cambios_fuera_de_la_nota() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    // Un «editor» que además toca otra nota.
    let editor = "sh -c 'printf \"x\\n\" >> \"$1\"; printf \"y\\n\" >> \"$(dirname \"$1\")/regla-dos.md\"' sh";
    let s = universo_con(
        &repo,
        &["nota", "editar", "criterios/regla-uno"],
        &[("EDITOR", editor)],
    );
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
    assert!(s.stdout.contains("edit_outside_note"), "{}", s.stdout);
    assert!(s.stdout.contains("criterios/regla-dos"), "{}", s.stdout);
    assert!(!leer(&repo, "criterios/regla-uno").ends_with("x\n"));
    ok(&universo_en(&repo, &["guard"]));
}
