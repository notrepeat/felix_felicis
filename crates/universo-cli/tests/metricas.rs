//! `universo metricas`: cómo se usa el repositorio de criterio, medido desde
//! su historial y sus notas (y, si está activado, el uso del MCP).
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

/// Tres capturas del agente y una del dueño. Del agente: «a» se refuerza y se
/// promueve (9 días), «b» se objeta y «c» queda estancada desde septiembre.
fn repo(dir: &Path) -> PathBuf {
    let repo = dir.join("r");
    ok(&universo_en(
        dir,
        &["init", repo.to_str().unwrap(), "--git"],
    ));
    let capturar = |texto: &str, autor: Option<&str>, fecha: &str| {
        let mut args = vec!["capturar", texto, "--proyecto", "Tienda", "--fecha", fecha];
        if let Some(a) = autor {
            args.extend(["--autor", a]);
        }
        ok(&universo_en(&repo, &args));
    };
    capturar("Regla a", Some("agente:claude"), "2026-11-01");
    capturar("Regla b", Some("agente:claude"), "2026-11-02");
    capturar("Regla c", Some("agente:claude"), "2026-09-01");
    capturar("Regla d", None, "2026-11-03");
    ok(&universo_en(
        &repo,
        &[
            "reforzar",
            "criterios/regla-a",
            "--proyecto",
            "Pagos",
            "--contexto",
            "c",
        ],
    ));
    ok(&universo_en(
        &repo,
        &[
            "objetar",
            "criterios/regla-b",
            "--proyecto",
            "Pagos",
            "--contexto",
            "no",
        ],
    ));
    ok(&universo_en(
        &repo,
        &[
            "promover",
            "criterios/regla-a",
            "--a",
            "vigente",
            "--fecha",
            "2026-11-10",
        ],
    ));
    repo
}

fn metricas(repo: &Path, extra: &[&str]) -> Value {
    let mut args = vec!["metricas", "--format=json", "--hasta", "2026-11-19"];
    args.extend(extra);
    let s = universo_en(repo, &args);
    ok(&s);
    serde_json::from_str(&s.stdout).unwrap()
}

#[test]
fn mide_actividad_precision_maduracion_y_estancadas() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let m = metricas(&repo, &[]);
    assert_eq!(m["schema"], "felix-metricas-v1");

    let a = &m["actividad"];
    assert_eq!(a["capturas"]["agente"], 3, "{a}");
    assert_eq!(a["capturas"]["dueno"], 1, "{a}");
    assert_eq!(a["refuerzos"]["dueno"], 1, "{a}");
    assert_eq!(a["objeciones"]["dueno"], 1, "{a}");
    assert_eq!(a["promociones"]["total"], 1, "{a}");
    // Todo ocurrió hoy: una sola semana con actividad.
    assert_eq!(m["por_semana"].as_array().unwrap().len(), 1, "{m}");

    let p = &m["precision_agente"];
    assert_eq!(p["capturas"], 3, "{p}");
    assert_eq!(p["confirmadas"], 1, "{p}");
    assert_eq!(p["objetadas"], 1, "{p}");
    assert_eq!(p["pendientes"], 1, "{p}");
    assert_eq!(p["precision"], 0.5, "{p}");

    let mad = &m["maduracion"];
    assert_eq!(mad["promovidas"], 1, "{mad}");
    assert_eq!(mad["dias_mediana"], 9, "{mad}");

    let est = &m["estancadas"];
    assert_eq!(est["dias"], 30);
    assert_eq!(
        est["notas"],
        serde_json::json!(["criterios/regla-c"]),
        "{est}"
    );

    assert_eq!(m["uso_mcp"]["estado"], "no_medido", "{m}");
}

#[test]
fn el_uso_del_mcp_se_cuenta_si_esta_registrado() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let lineas = [
        r#"{"fecha":"2026-11-18","herramienta":"search","universo":"todos","ok":true}"#,
        r#"{"fecha":"2026-11-18","herramienta":"search","universo":"vigente","ok":true}"#,
        r#"{"fecha":"2026-11-19","herramienta":"capture","universo":"candidatos","ok":false}"#,
        "linea rota",
    ];
    fs::write(
        repo.join(".git/felix-uso-mcp.jsonl"),
        lineas.join("\n") + "\n",
    )
    .unwrap();
    let u = &metricas(&repo, &[])["uso_mcp"];
    assert_eq!(u["estado"], "medido", "{u}");
    assert_eq!(u["llamadas"], 3, "{u}");
    assert_eq!(u["errores"], 1, "{u}");
    assert_eq!(u["por_herramienta"]["search"], 2, "{u}");
    assert_eq!(u["descartadas"], 1, "{u}");
}

#[test]
fn texto_y_opciones() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let s = universo_en(&repo, &["metricas", "--hasta", "2026-11-19"]);
    ok(&s);
    for esperado in [
        "capturas",
        "precisión",
        "maduración",
        "estancadas",
        "uso del MCP",
    ] {
        assert!(
            s.stdout.contains(esperado),
            "falta {esperado}: {}",
            s.stdout
        );
    }
    let m = metricas(&repo, &["--estancada-dias", "200"]);
    assert_eq!(m["estancadas"]["notas"], serde_json::json!([]));
    for args in [
        &["metricas", "--hasta", "ayer"][..],
        &["metricas", "--estancada-dias", "x"],
        &["metricas", "--otra"],
    ] {
        assert_eq!(universo_en(&repo, args).codigo, 2, "{args:?}");
    }
    let s = universo_en(dir.path(), &["metricas"]);
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
}

#[test]
fn el_uso_se_lee_del_gitdir_real_y_las_fechas_invalidas_son_uso() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let real = repo.join(".git");
    let movido = dir.path().join("gitdir-real");
    fs::rename(&real, &movido).unwrap();
    fs::write(&real, format!("gitdir: {}\n", movido.display())).unwrap();
    fs::write(
        movido.join("felix-uso-mcp.jsonl"),
        r#"{"fecha":"2026-11-18","herramienta":"search","universo":"todos","ok":true}"#.to_string()
            + "\n",
    )
    .unwrap();
    assert_eq!(metricas(&repo, &[])["uso_mcp"]["llamadas"], 1);
    for fecha in ["2026-02-31", "2026-11-19xyz"] {
        assert_eq!(
            universo_en(&repo, &["metricas", "--hasta", fecha]).codigo,
            2,
            "{fecha}"
        );
    }
}

/// Activado pero sin llamadas todavía no es «no medido».
#[test]
fn medicion_activada_sin_llamadas() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    assert!(
        Command::new("git")
            .args(["config", "felix.medirUso", "true"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success()
    );
    let u = &metricas(&repo, &[])["uso_mcp"];
    assert_eq!(u["estado"], "sin_llamadas", "{u}");
    let s = universo_en(&repo, &["metricas", "--hasta", "2026-11-19"]);
    assert!(
        s.stdout.contains("activado, sin llamadas todavía"),
        "{}",
        s.stdout
    );
}
