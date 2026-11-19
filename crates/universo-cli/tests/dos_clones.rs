//! Fase 10, U9 (pasos 1 a 5): el mismo repositorio de criterio en dos
//! computadores, simulados con dos clones de un remoto local.
use std::{path::Path, process::Command};

fn universo(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .current_dir(cwd)
        .env_remove("FELIX_CRITERIO")
        .env_remove("FELIX_UNIVERSO")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "universo {args:?}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=Prueba",
            "-c",
            "user.email=prueba@ejemplo.invalid",
        ])
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Lo que ven los comandos de lectura, para comparar dos clones.
fn vista(repo: &Path) -> Vec<String> {
    let mut salida = Vec::new();
    for universo_ in ["candidatos", "vigente"] {
        salida.push(universo(repo, &["guard", "--universo", universo_]));
        salida.push(universo(
            repo,
            &["log", "--universo", universo_, "--format=json"],
        ));
        salida.push(universo(
            repo,
            &["diff", "0", "--universo", universo_, "--format=json"],
        ));
    }
    salida.push(universo(repo, &["show", "1", "--format=json"]));
    salida.push(universo(
        repo,
        &["search", "--universo", "todos", "criterio", "--format=json"],
    ));
    salida
}

#[test]
fn dos_clones_ven_lo_mismo_y_el_historial_sigue_encadenado() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    // 1. Repositorio de criterio en A y un remoto local.
    let a = base.join("a");
    universo(base, &["init", a.to_str().unwrap(), "--git"]);
    let remoto = base.join("remoto.git");
    git(base, &["init", "-q", "--bare", remoto.to_str().unwrap()]);
    git(&a, &["remote", "add", "origin", remoto.to_str().unwrap()]);

    // 2. En A: tres candidatas, un refuerzo y una promoción.
    for (texto, proyecto) in [
        ("Primer criterio de prueba", "Alfa"),
        ("Segundo criterio de prueba", "Beta"),
        ("Tercer criterio de prueba", "Alfa"),
    ] {
        universo(
            &a,
            &[
                "capturar",
                texto,
                "--proyecto",
                proyecto,
                "--fecha",
                "2026-11-18",
            ],
        );
    }
    let nota = "criterios/primer-criterio-de-prueba";
    universo(
        &a,
        &[
            "reforzar",
            nota,
            "--proyecto",
            "Beta",
            "--contexto",
            "se aplicó",
            "--fecha",
            "2026-11-19",
        ],
    );
    universo(
        &a,
        &["promover", nota, "--a", "vigente", "--fecha", "2026-11-20"],
    );
    git(&a, &["add", "-A"]);
    git(&a, &["commit", "-q", "-m", "criterio inicial"]);
    git(&a, &["push", "-q", "origin", "HEAD:main"]);

    // 3. Clon en B: los mismos resultados en cada comando de lectura.
    let b = base.join("b");
    git(
        base,
        &[
            "clone",
            "-q",
            "--branch",
            "main",
            remoto.to_str().unwrap(),
            b.to_str().unwrap(),
        ],
    );
    assert_eq!(vista(&a), vista(&b));

    // 4. Un cambio en B llega a A y la cadena del historial no se rompe.
    universo(
        &b,
        &[
            "capturar",
            "Cuarto criterio desde otro computador",
            "--proyecto",
            "Gamma",
            "--fecha",
            "2026-11-21",
        ],
    );
    git(&b, &["add", "-A"]);
    git(&b, &["commit", "-q", "-m", "criterio desde B"]);
    git(&b, &["push", "-q", "origin", "HEAD:main"]);
    git(&a, &["pull", "-q", "--rebase", "origin", "main"]);
    assert_eq!(vista(&a), vista(&b));
    // diff desde el estado 0 cubre los cambios de candidatos sin huecos.
    let diff = universo(&a, &["diff", "0"]);
    assert!(
        diff.contains("create criterios/cuarto-criterio-desde-otro-computador"),
        "{diff}"
    );
    let log = universo(&a, &["log"]);
    assert!(log.starts_with("log: 6 entries"), "{log}");
}

/// Como `git`, pero devuelve si terminó bien (un merge con conflictos falla).
fn git_ok(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args([
            "-c",
            "user.name=Prueba",
            "-c",
            "user.email=prueba@ejemplo.invalid",
        ])
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
        .status
        .success()
}

fn universo_codigo(cwd: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .current_dir(cwd)
        .env_remove("FELIX_CRITERIO")
        .env_remove("FELIX_UNIVERSO")
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

/// U9, paso 5: A y B capturan a la vez; el merge mezcla los bundles y une los
/// logs. `guard` lo detecta; `check` y `seal` lo aceptan y dejan la entrada
/// `seal`, que `diff` informa por su nombre.
#[test]
fn un_cambio_concurrente_se_mezcla_se_detecta_y_se_sella() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let a = base.join("a");
    universo(base, &["init", a.to_str().unwrap(), "--git"]);
    let remoto = base.join("remoto.git");
    git(base, &["init", "-q", "--bare", remoto.to_str().unwrap()]);
    git(&a, &["remote", "add", "origin", remoto.to_str().unwrap()]);
    universo(
        &a,
        &[
            "capturar",
            "Criterio común",
            "--proyecto",
            "Alfa",
            "--fecha",
            "2026-11-18",
        ],
    );
    git(&a, &["add", "-A"]);
    git(&a, &["commit", "-q", "-m", "base"]);
    git(&a, &["push", "-q", "origin", "HEAD:main"]);
    let b = base.join("b");
    git(
        base,
        &[
            "clone",
            "-q",
            "--branch",
            "main",
            remoto.to_str().unwrap(),
            b.to_str().unwrap(),
        ],
    );

    // A y B capturan a la vez, en proyectos distintos.
    for (repo, texto, proyecto) in [
        (&a, "Criterio desde A", "Beta"),
        (&b, "Criterio desde B", "Gamma"),
    ] {
        universo(
            repo,
            &[
                "capturar",
                texto,
                "--proyecto",
                proyecto,
                "--fecha",
                "2026-11-19",
            ],
        );
        git(repo, &["add", "-A"]);
        git(repo, &["commit", "-q", "-m", texto]);
    }
    git(&b, &["push", "-q", "origin", "HEAD:main"]);
    // El merge une los logs (merge=union); el sello choca.
    let _ = git_ok(&a, &["pull", "-q", "--no-rebase", "origin", "main"]);
    let log = std::fs::read_to_string(a.join("universos/candidatos/.bundle.felix.log")).unwrap();
    assert_eq!(log.lines().count(), 3, "{log}");
    for nota in ["criterio-desde-a", "criterio-desde-b"] {
        assert!(
            a.join(format!("universos/candidatos/bundle/criterios/{nota}.md"))
                .exists()
        );
    }

    // guard lo detecta; check lo admite; seal lo acepta y lo registra.
    let (codigo, _) = universo_codigo(&a, &["guard"]);
    assert_eq!(codigo, 1);
    universo(&a, &["check"]);
    let sello = universo(&a, &["seal", "--motivo", "merge", "--format=json"]);
    assert!(
        sello.contains("\"external_change_recorded\":true"),
        "{sello}"
    );
    universo(&a, &["guard"]);
    let log = universo(&a, &["log"]);
    assert!(log.starts_with("log: 4 entries"), "{log}");
    assert!(log.contains("kind=seal"), "{log}");
    // diff no cruza el merge en silencio: lo informa por su nombre.
    let (codigo, diff) = universo_codigo(&a, &["diff", "0", "--format=json"]);
    assert_eq!(codigo, 1);
    assert!(
        diff.contains("history_gap") || diff.contains("external_change"),
        "{diff}"
    );
    let (codigo, diff) = universo_codigo(&a, &["diff", "3", "4", "--format=json"]);
    assert_eq!(codigo, 1);
    assert!(diff.contains("\"code\":\"external_change\""), "{diff}");
    universo(&a, &["diff", "4"]);

    // Se versiona y B lo recibe igual.
    git(&a, &["add", "-A"]);
    git(&a, &["commit", "-q", "-m", "merge de criterio"]);
    git(&a, &["push", "-q", "origin", "HEAD:main"]);
    git(&b, &["pull", "-q", "--rebase", "origin", "main"]);
    universo(&b, &["guard"]);
    assert_eq!(universo(&a, &["log"]), universo(&b, &["log"]));
}
