//! Fase 10, U1 y U2: repositorio de criterio con `felix.toml` y universos.
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

/// Ejecuta `universo` en `cwd`, sin heredar `FELIX_CRITERIO` ni `FELIX_UNIVERSO`.
fn universo_en(cwd: &Path, args: &[&str], env: &[(&str, &Path)]) -> Salida {
    let mut comando = Command::new(env!("CARGO_BIN_EXE_universo"));
    comando
        .args(args)
        .current_dir(cwd)
        .env_remove("FELIX_CRITERIO")
        .env_remove("FELIX_UNIVERSO");
    for (clave, valor) in env {
        comando.env(clave, valor);
    }
    let out = comando.output().unwrap();
    Salida {
        codigo: out.status.code().unwrap(),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

fn git(cwd: &Path, args: &[&str]) -> String {
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
    String::from_utf8(out.stdout).unwrap()
}

/// Crea un repositorio de criterio con `init --git` y devuelve su ruta.
fn repo(dir: &Path) -> PathBuf {
    let repo = dir.join("mi-criterio");
    let s = universo_en(dir, &["init", repo.to_str().unwrap(), "--git"], &[]);
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    repo
}

/// Aplica un delta en el universo por defecto, sin rutas.
fn aplicar(repo: &Path, delta: &str) {
    let archivo = repo.join("..").join("delta.json");
    std::fs::write(&archivo, delta).unwrap();
    let d = format!("--delta={}", archivo.display());
    let s = universo_en(
        repo,
        &["propose", &d, "--emit-snapshot-token", "--format=json"],
        &[],
    );
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    let token = serde_json::from_str::<Value>(&s.stdout).unwrap()["snapshot_token"]
        .as_str()
        .unwrap()
        .to_string();
    let esperado = format!("--expect-snapshot={token}");
    let s = universo_en(
        repo,
        &[
            "propose",
            &d,
            "--apply",
            &esperado,
            "--author=yo",
            "--reason=prueba",
        ],
        &[],
    );
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
}

const NOTA: &str = "criterios/bienvenida-al-criterio";
const ACTIVAR: &str = r#"{"schema":"felix-delta-v1","add":["Estado(criterios/bienvenida-al-criterio, vigente)"],"remove":["Estado(criterios/bienvenida-al-criterio, candidato)"]}"#;

/// Siembra la base vacía con una candidata, sin rutas.
fn sembrar(repo: &Path) {
    let s = universo_en(
        repo,
        &[
            "capturar",
            "Bienvenida al criterio",
            "--proyecto",
            "Prueba",
            "--fecha",
            "2026-11-18",
        ],
        &[],
    );
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
}

#[test]
fn init_git_crea_un_repositorio_de_criterio_listo() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    for archivo in [
        "felix.toml",
        "README.md",
        "AGENTS.md",
        ".gitignore",
        ".gitattributes",
        ".githooks/pre-commit",
        ".githooks/post-merge",
        "universos/candidatos/manifest.toml",
        "universos/candidatos/invariants.sx",
        "universos/candidatos/bundle/criterios/.gitkeep",
        "universos/vigente/bundle/criterios/.gitkeep",
        "universos/vigente/manifest.toml",
        "universos/vigente/invariants.sx",
    ] {
        assert!(repo.join(archivo).is_file(), "{archivo}");
    }
    assert!(repo.join(".git").is_dir());
    let toml = std::fs::read_to_string(repo.join("felix.toml")).unwrap();
    assert!(toml.contains("schema = \"felix-criterio-v1\""), "{toml}");
    assert!(toml.contains("por_defecto = \"candidatos\""), "{toml}");
    // El hook queda instalado pero apagado: git no lo usa sin core.hooksPath.
    assert!(!repo.join(".git/hooks/pre-commit").exists());
    assert!(!git(&repo, &["config", "--list", "--local"]).contains("core.hookspath"));

    // Cada universo pasa guard sin rutas.
    assert_eq!(universo_en(&repo, &["guard"], &[]).codigo, 0);
    assert_eq!(
        universo_en(&repo, &["guard", "--universo", "vigente"], &[]).codigo,
        0
    );

    // Tras un apply, git no ve el lock ni otros transitorios; sí el log y el sello.
    sembrar(&repo);
    aplicar(&repo, ACTIVAR);
    let estado = git(&repo, &["status", "--porcelain", "--untracked-files=all"]);
    assert!(!estado.contains(".felix.lock"), "{estado}");
    assert!(
        estado.contains("universos/candidatos/.bundle.felix.log"),
        "{estado}"
    );
    assert!(
        estado.contains("universos/candidatos/.bundle.felix.seal"),
        "{estado}"
    );
    // Un repositorio no se crea dos veces encima de otro.
    let s = universo_en(dir.path(), &["init", repo.to_str().unwrap(), "--git"], &[]);
    assert_eq!(s.codigo, 1, "{}", s.stdout);
}

#[test]
fn un_clon_con_autocrlf_pasa_guard_en_todos_los_universos() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    sembrar(&repo);
    aplicar(&repo, ACTIVAR);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "inicio"]);
    let clon = dir.path().join("clon");
    git(
        dir.path(),
        &[
            "-c",
            "core.autocrlf=true",
            "clone",
            "-q",
            repo.to_str().unwrap(),
            clon.to_str().unwrap(),
        ],
    );
    let nota = std::fs::read(
        clon.join("universos/candidatos/bundle")
            .join(format!("{NOTA}.md")),
    )
    .unwrap();
    assert!(!nota.contains(&b'\r'), "el clon convirtió a CRLF");
    for universo in ["candidatos", "vigente"] {
        let s = universo_en(&clon, &["guard", "--universo", universo], &[]);
        assert_eq!(s.codigo, 0, "{universo}: {}{}", s.stdout, s.stderr);
    }
    // El historial viaja con el clon.
    let s = universo_en(&clon, &["log"], &[]);
    assert!(s.stdout.contains("log: 2 entries"), "{}", s.stdout);
}

#[test]
fn los_comandos_resuelven_el_universo_sin_rutas_y_respetan_la_precedencia() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    sembrar(&repo);
    aplicar(&repo, ACTIVAR);
    let afuera = dir.path();

    // Dentro del repositorio y desde un subdirectorio.
    for cwd in [
        repo.clone(),
        repo.join("universos/candidatos/bundle/criterios"),
    ] {
        assert_eq!(
            universo_en(&cwd, &["check"], &[]).codigo,
            0,
            "{}",
            cwd.display()
        );
        assert_eq!(universo_en(&cwd, &["validate"], &[]).codigo, 0);
        let s = universo_en(&cwd, &["search", "bienvenida"], &[]);
        assert!(
            s.stdout.starts_with("search: 1 of 1 matches"),
            "{}",
            s.stdout
        );
    }
    assert_eq!(universo_en(&repo, &["show", "1"], &[]).codigo, 0);
    assert_eq!(universo_en(&repo, &["diff", "0"], &[]).codigo, 0);
    assert_eq!(universo_en(&repo, &["status"], &[]).codigo, 0);

    // Fuera, con la variable de entorno o con --criterio.
    let s = universo_en(
        afuera,
        &["search", "bienvenida"],
        &[("FELIX_CRITERIO", &repo)],
    );
    assert!(
        s.stdout.starts_with("search: 1 of 1 matches"),
        "{}{}",
        s.stdout,
        s.stderr
    );
    let criterio = format!("--criterio={}", repo.display());
    let s = universo_en(afuera, &["search", &criterio, "bienvenida"], &[]);
    assert!(
        s.stdout.starts_with("search: 1 of 1 matches"),
        "{}{}",
        s.stdout,
        s.stderr
    );
    // Otro universo: vigente está vacío.
    let s = universo_en(
        &repo,
        &["search", "--universo", "vigente", "bienvenida"],
        &[],
    );
    assert!(
        s.stdout.starts_with("search: 0 of 0 matches"),
        "{}{}",
        s.stdout,
        s.stderr
    );
    let s = universo_en(
        afuera,
        &["search", "bienvenida"],
        &[
            ("FELIX_CRITERIO", &repo),
            ("FELIX_UNIVERSO", Path::new("vigente")),
        ],
    );
    assert!(
        s.stdout.starts_with("search: 0 of 0 matches"),
        "{}{}",
        s.stdout,
        s.stderr
    );

    // Las rutas explícitas mandan, también dentro del repositorio.
    let bundle = repo.join("universos/vigente/bundle");
    let manifiesto = format!(
        "--manifiesto={}",
        repo.join("universos/vigente/manifest.toml").display()
    );
    let s = universo_en(
        &repo,
        &[
            "search",
            bundle.to_str().unwrap(),
            &manifiesto,
            "bienvenida",
        ],
        &[],
    );
    assert!(
        s.stdout.starts_with("search: 0 of 0 matches"),
        "{}{}",
        s.stdout,
        s.stderr
    );

    // Errores: universo inexistente (1), opciones en conflicto (2), sin repositorio (2).
    let s = universo_en(&repo, &["check", "--universo", "axiomas"], &[]);
    assert_eq!(s.codigo, 1);
    assert!(
        s.stderr.contains("universe_not_found") && s.stderr.contains("candidatos, vigente"),
        "{}",
        s.stderr
    );
    assert_eq!(
        universo_en(&repo, &["check", &criterio, &manifiesto], &[]).codigo,
        2
    );
    assert_eq!(universo_en(afuera, &["check"], &[]).codigo, 2);
    let s = universo_en(
        afuera,
        &["check", "--criterio", afuera.to_str().unwrap()],
        &[],
    );
    assert_eq!(s.codigo, 1);
    assert!(s.stderr.contains("criterio_not_found"), "{}", s.stderr);
}

#[test]
fn una_version_de_motor_fuera_de_rango_se_informa_y_se_puede_saltar() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let ruta = repo.join("felix.toml");
    let texto = std::fs::read_to_string(&ruta).unwrap();
    let linea = texto
        .lines()
        .find(|l| l.starts_with("motor = "))
        .unwrap()
        .to_string();
    std::fs::write(&ruta, texto.replace(&linea, "motor = \"=9.9.9\"")).unwrap();
    let s = universo_en(&repo, &["check"], &[]);
    assert_eq!(s.codigo, 1);
    assert!(
        s.stderr.contains("engine_version_mismatch") && s.stderr.contains("=9.9.9"),
        "{}",
        s.stderr
    );
    assert_eq!(
        universo_en(&repo, &["check", "--ignorar-version"], &[]).codigo,
        0
    );
}

#[test]
fn ayuda_transitorios_hook_y_entorno_invalido() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    // La ayuda se muestra aunque felix.toml pida otra versión.
    let ruta = repo.join("felix.toml");
    let original = std::fs::read_to_string(&ruta).unwrap();
    let linea = original
        .lines()
        .find(|l| l.starts_with("motor = "))
        .unwrap()
        .to_string();
    std::fs::write(&ruta, original.replace(&linea, "motor = \"=9.9.9\"")).unwrap();
    for args in [vec!["check", "--help"], vec!["log", "-h"]] {
        let s = universo_en(&repo, &args, &[]);
        assert_eq!(s.codigo, 0, "{args:?}: {}", s.stderr);
        assert!(s.stdout.contains("Uso:"), "{}", s.stdout);
    }
    std::fs::write(&ruta, &original).unwrap();

    // Ningún transitorio de Felix aparece en git status.
    for transitorio in [
        ".bundle.felix.seal.tmp",
        ".bundle.felix.journal.tmp",
        ".bundle.felix.lock",
    ] {
        std::fs::write(repo.join("universos/candidatos").join(transitorio), "").unwrap();
    }
    let estado = git(&repo, &["status", "--porcelain", "--untracked-files=all"]);
    assert!(
        !estado.contains(".tmp") && !estado.contains(".lock"),
        "{estado}"
    );
    // Simulaban una transacción a medias: se quitan para que guard pase.
    for transitorio in [".bundle.felix.seal.tmp", ".bundle.felix.journal.tmp"] {
        std::fs::remove_file(repo.join("universos/candidatos").join(transitorio)).unwrap();
    }

    // El hook verifica este repositorio aunque FELIX_CRITERIO apunte a otro.
    let hook = std::fs::read_to_string(repo.join(".githooks/pre-commit")).unwrap();
    assert!(
        hook.contains("raiz=\"$(git rev-parse --show-toplevel)\""),
        "{hook}"
    );
    assert!(hook.contains("guard --criterio \"$raiz\""), "{hook}");
    // Y funciona: corre guard sobre los dos universos de este repositorio.
    let corrida = Command::new("sh")
        .arg(repo.join(".githooks/pre-commit"))
        .current_dir(&repo)
        .env("FELIX_BIN", env!("CARGO_BIN_EXE_universo"))
        .env("FELIX_CRITERIO", dir.path())
        .output()
        .unwrap();
    assert!(
        corrida.status.success(),
        "{}",
        String::from_utf8_lossy(&corrida.stdout)
    );
}

#[cfg(unix)]
#[test]
fn felix_universo_no_unicode_es_error_de_uso() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let invalido = std::ffi::OsStr::from_bytes(b"\xff");
    let s = universo_en(&repo, &["log"], &[("FELIX_UNIVERSO", Path::new(invalido))]);
    assert_eq!(s.codigo, 2, "{}{}", s.stdout, s.stderr);
}

#[test]
fn mcp_config_imprime_la_configuracion_para_cada_cliente() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    // La ruta que imprime: canónica, sin el prefijo `\\?\` de Windows, y
    // entre comillas simples si tiene caracteres fuera de `[A-Za-z0-9/._:-]`.
    let canonica = repo.canonicalize().unwrap().display().to_string();
    let canonica = canonica
        .strip_prefix(r"\\?\")
        .map_or(canonica.clone(), str::to_string);
    let citada = if canonica
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-:".contains(c))
    {
        canonica.clone()
    } else {
        format!("'{canonica}'")
    };
    let s = universo_en(&repo, &["mcp-config"], &[]);
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    assert!(
        s.stdout
            .contains("claude mcp add --scope user felix-criterio -- "),
        "{}",
        s.stdout
    );
    assert!(
        s.stdout.contains(&format!("--criterio {citada}")),
        "{}",
        s.stdout
    );
    let s = universo_en(
        &repo,
        &["mcp-config", "--cliente", "json", "--nombre", "mi-criterio"],
        &[],
    );
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    let servidor = &json["mcpServers"]["mi-criterio"];
    assert!(
        servidor["command"]
            .as_str()
            .unwrap()
            .contains("universo-mcp"),
        "{json}"
    );
    assert_eq!(servidor["args"][0], "--criterio");
    assert_eq!(servidor["args"][1], canonica);
    let s = universo_en(&repo, &["mcp-config", "--cliente=codex"], &[]);
    assert!(
        s.stdout.contains("[mcp_servers.felix-criterio]"),
        "{}",
        s.stdout
    );
    // Fuera de un repositorio, y un cliente desconocido.
    let s = universo_en(dir.path(), &["mcp-config"], &[]);
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("criterio_not_found"), "{}", s.stdout);
    assert_eq!(
        universo_en(&repo, &["mcp-config", "--cliente", "otro"], &[]).codigo,
        2
    );
}

#[test]
fn mcp_config_fija_el_universo_de_arranque() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let s = universo_en(
        &repo,
        &["mcp-config", "--cliente", "json", "--universo", "vigente"],
        &[],
    );
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    let args = &json["mcpServers"]["felix-criterio"]["args"];
    assert_eq!(
        (args[2].as_str(), args[3].as_str()),
        (Some("--universo"), Some("vigente")),
        "{json}"
    );
    let s = universo_en(&repo, &["mcp-config", "--universo", "axiomas"], &[]);
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("universe_not_found"), "{}", s.stdout);
}

/// U7: el hook `post-merge` corre `guard` en cada universo y, si algo no
/// coincide, explica cómo aceptarlo; termina con 0 (el merge ya ocurrió).
#[cfg(unix)]
#[test]
fn el_hook_post_merge_avisa_sin_fallar() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let hook = repo.join(".githooks/post-merge");
    assert!(hook.is_file());
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&hook).unwrap().permissions().mode() & 0o111,
        0o111
    );
    let correr = || {
        let out = Command::new("sh")
            .arg(&hook)
            .current_dir(&repo)
            .env("FELIX_BIN", env!("CARGO_BIN_EXE_universo"))
            .env_remove("FELIX_CRITERIO")
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap() + &String::from_utf8(out.stderr).unwrap()
    };
    assert!(!correr().contains("felix:"), "{}", correr());
    std::fs::write(
        repo.join("universos/vigente/bundle/criterios/a-mano.md"),
        "---\ntype: criterio\ntitle: A mano\n---\n",
    )
    .unwrap();
    let aviso = correr();
    assert!(aviso.contains("felix: vigente"), "{aviso}");
    assert!(
        aviso.contains("universo seal --universo vigente"),
        "{aviso}"
    );
    assert!(!aviso.contains("felix: candidatos"), "{aviso}");
}
