//! Fase 10, U9 (pasos 1 a 4): el mismo repositorio de criterio en dos
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
