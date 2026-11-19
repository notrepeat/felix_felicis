//! `universo conectar`: conecta un repositorio de criterio a Claude Code en un
//! paso (hooks con la ruta del binario, MCP a nivel de usuario y la skill),
//! con vista previa por defecto, `--aplicar` y `--verificar`.
#![cfg(unix)]
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

struct Salida {
    codigo: i32,
    stdout: String,
    stderr: String,
}

/// Entorno de prueba: HOME aislado, `claude` falso que guarda el servidor MCP
/// en un archivo y un `universo-mcp` falso.
struct Entorno {
    _dir: tempfile::TempDir,
    raiz: PathBuf,
    repo: PathBuf,
    home: PathBuf,
    claude: PathBuf,
    mcp: PathBuf,
    estado_mcp: PathBuf,
}

const CLAUDE_FALSO: &str = r#"#!/bin/sh
estado="$FAKE_CLAUDE_ESTADO"
echo "$*" >> "$estado.log"
case "$1 $2" in
  "mcp get") if [ -f "$estado" ]; then cat "$estado"; exit 0; fi
             echo "No MCP server named \"$3\""; exit 1 ;;
  "mcp add") shift 2; echo "$*" > "$estado.add"
             # `mcp add --scope user <nombre> -- <comando> <args...>`
             while [ "$1" != "--" ]; do shift; done; shift
             cmd="$1"; shift
             printf 'felix-criterio:\n  Scope: User config (available in all your projects)\n  Command: %s\n  Args: %s\n' "$cmd" "$*" > "$estado"
             exit 0 ;;
  "mcp remove") rm -f "$estado"; exit 0 ;;
esac
exit 2
"#;

fn ejecutable(ruta: &Path, texto: &str) {
    fs::write(ruta, texto).unwrap();
    fs::set_permissions(ruta, fs::Permissions::from_mode(0o755)).unwrap();
}

fn entorno() -> Entorno {
    let dir = tempfile::tempdir().unwrap();
    let raiz = dir.path().canonicalize().unwrap();
    let home = raiz.join("home");
    fs::create_dir_all(&home).unwrap();
    let claude = raiz.join("claude");
    ejecutable(&claude, CLAUDE_FALSO);
    let mcp = raiz.join("universo-mcp");
    ejecutable(&mcp, "#!/bin/sh\nexit 0\n");
    let repo = raiz.join("mi-criterio");
    let e = Entorno {
        _dir: dir,
        raiz: raiz.clone(),
        repo: repo.clone(),
        home,
        claude,
        mcp,
        estado_mcp: raiz.join("mcp-estado"),
    };
    let s = e.universo(&raiz, &["init", repo.to_str().unwrap(), "--git"]);
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    e
}

impl Entorno {
    fn comando(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_universo"));
        c.args(args)
            .current_dir(cwd)
            .env_remove("FELIX_CRITERIO")
            .env_remove("FELIX_UNIVERSO")
            .env_remove("FELIX_BIN")
            .env("HOME", &self.home)
            .env("FELIX_CLAUDE_BIN", &self.claude)
            .env("FELIX_MCP_BIN", &self.mcp)
            .env("FAKE_CLAUDE_ESTADO", &self.estado_mcp);
        c
    }

    fn universo(&self, cwd: &Path, args: &[&str]) -> Salida {
        let out = self.comando(cwd, args).output().unwrap();
        Salida {
            codigo: out.status.code().unwrap(),
            stdout: String::from_utf8(out.stdout).unwrap(),
            stderr: String::from_utf8(out.stderr).unwrap(),
        }
    }

    fn conectar(&self, extra: &[&str]) -> Salida {
        let mut args = vec!["conectar"];
        args.extend(extra);
        self.universo(&self.repo, &args)
    }

    fn json(&self, extra: &[&str]) -> (i32, Value) {
        let mut args = extra.to_vec();
        args.push("--format=json");
        let s = self.conectar(&args);
        let json = serde_json::from_str(&s.stdout)
            .unwrap_or_else(|_| panic!("no es JSON: {}{}", s.stdout, s.stderr));
        (s.codigo, json)
    }

    fn git_config(&self, clave: &str) -> Option<String> {
        let out = Command::new("git")
            .args(["config", "--local", "--get", clave])
            .current_dir(&self.repo)
            .output()
            .unwrap();
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).unwrap().trim().to_string())
    }

    fn skill(&self) -> PathBuf {
        self.home.join(".claude/skills/felix-criterio/SKILL.md")
    }

    fn log_claude(&self) -> String {
        fs::read_to_string(self.estado_mcp.with_extension("log")).unwrap_or_default()
    }
}

fn estados(json: &Value) -> Vec<(String, String)> {
    json["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap().to_string(),
                p["status"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn estado(json: &Value, id: &str) -> String {
    estados(json)
        .into_iter()
        .find(|(i, _)| i == id)
        .unwrap_or_else(|| panic!("sin paso {id}: {json}"))
        .1
}

#[test]
fn la_vista_previa_lista_los_cambios_sin_escribir_nada() {
    let e = entorno();
    let (codigo, json) = e.json(&[]);
    assert_eq!(codigo, 0, "{json}");
    assert_eq!(json["mode"], "preview");
    assert_eq!(
        estados(&json),
        [
            ("binario", "falta"),
            ("hooks_archivos", "ok"),
            ("hooks_activos", "falta"),
            ("mcp", "falta"),
            ("skill", "falta"),
        ]
        .map(|(a, b)| (a.to_string(), b.to_string()))
    );
    for paso in json["steps"].as_array().unwrap() {
        if paso["status"] != "ok" {
            assert!(!paso["action"].as_str().unwrap_or("").is_empty(), "{paso}");
        }
    }
    assert_eq!(e.git_config("felix.bin"), None);
    assert_eq!(e.git_config("core.hooksPath"), None);
    assert!(!e.skill().exists());
    assert!(!e.estado_mcp.exists());
    assert!(!e.log_claude().contains("mcp add"));
    let s = e.conectar(&[]);
    assert_eq!(s.codigo, 0);
    assert!(s.stdout.contains("--aplicar"), "{}", s.stdout);
}

#[test]
fn aplicar_deja_todo_conectado_y_es_idempotente() {
    let e = entorno();
    let s = e.conectar(&["--aplicar"]);
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);

    let binario = PathBuf::from(env!("CARGO_BIN_EXE_universo"))
        .canonicalize()
        .unwrap();
    assert_eq!(e.git_config("felix.bin").map(PathBuf::from), Some(binario));
    assert_eq!(e.git_config("core.hooksPath").as_deref(), Some(".githooks"));
    let skill = fs::read_to_string(e.skill()).unwrap();
    assert!(skill.starts_with("---\nname: felix-criterio"), "{skill}");
    let mcp = fs::read_to_string(e.estado_mcp.with_extension("add")).unwrap();
    assert_eq!(
        mcp.trim(),
        format!(
            "--scope user felix-criterio -- {} --criterio {}",
            e.mcp.display(),
            e.repo.display()
        )
    );

    let (codigo, json) = e.json(&["--verificar"]);
    assert_eq!(codigo, 0, "{json}");
    assert!(estados(&json).iter().all(|(_, s)| s == "ok"), "{json}");

    // Otra vez: nada que hacer, y claude no vuelve a agregar el servidor.
    let antes = e.log_claude().len();
    let s = e.conectar(&["--aplicar"]);
    assert_eq!(s.codigo, 0, "{}", s.stdout);
    assert!(s.stdout.contains("nada que hacer"), "{}", s.stdout);
    assert!(
        !e.log_claude()[antes..].contains("mcp add"),
        "{}",
        e.log_claude()
    );
}

#[test]
fn verificar_falla_mientras_falte_algo_y_no_escribe() {
    let e = entorno();
    let (codigo, json) = e.json(&["--verificar"]);
    assert_eq!(codigo, 1, "{json}");
    assert_eq!(json["ok"], false);
    assert!(!e.skill().exists());
    assert_eq!(e.git_config("felix.bin"), None);
}

#[test]
fn hooks_viejos_skill_vieja_y_un_mcp_que_apunta_a_otro_lado_se_actualizan() {
    let e = entorno();
    assert_eq!(e.conectar(&["--aplicar"]).codigo, 0);
    let hook = e.repo.join(".githooks/pre-commit");
    ejecutable(
        &hook,
        "#!/bin/sh\n# Hook de Felix: versión vieja\nuniverso guard\n",
    );
    // Otro repositorio cuyo nombre empieza igual: no cuenta como conectado.
    fs::write(
        &e.estado_mcp,
        format!(
            "felix-criterio:\n  Scope: User config\n  Command: {}\n  Args: --criterio {}-viejo\n",
            e.mcp.display(),
            e.repo.display()
        ),
    )
    .unwrap();
    fs::write(e.skill(), "vieja").unwrap();
    let antes = (
        fs::read_to_string(&hook).unwrap(),
        fs::read_to_string(&e.estado_mcp).unwrap(),
    );

    for modo in [&[][..], &["--verificar"]] {
        let (_, json) = e.json(modo);
        assert_eq!(estado(&json, "hooks_archivos"), "desactualizado");
        assert_eq!(estado(&json, "mcp"), "desactualizado");
        assert_eq!(estado(&json, "skill"), "desactualizado");
        // La vista previa dice a qué repositorio apunta hoy el MCP.
        let mcp = json["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "mcp")
            .unwrap();
        assert!(mcp["detail"].as_str().unwrap().contains("-viejo"), "{mcp}");
    }
    // Ni la vista previa ni --verificar escriben.
    assert_eq!(
        (
            fs::read_to_string(&hook).unwrap(),
            fs::read_to_string(&e.estado_mcp).unwrap()
        ),
        antes
    );
    assert_eq!(fs::read_to_string(e.skill()).unwrap(), "vieja");

    let s = e.conectar(&["--aplicar"]);
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
    // Tras reemplazar el MCP, el detalle no describe el estado anterior como
    // si fuera el actual.
    let linea_mcp = s.stdout.lines().find(|l| l.contains("] mcp")).unwrap();
    assert!(linea_mcp.starts_with("[aplicado] mcp"), "{linea_mcp}");
    assert!(!linea_mcp.contains("hoy apunta a"), "{linea_mcp}");
    assert!(linea_mcp.contains("antes apuntaba a"), "{linea_mcp}");
    // Los hooks están versionados: queda un commit pendiente.
    assert!(s.stdout.contains("git commit"), "{}", s.stdout);
    assert!(fs::read_to_string(&hook).unwrap().contains("felix.bin"));
    assert!(
        e.log_claude()
            .contains("mcp remove felix-criterio --scope user"),
        "{}",
        e.log_claude()
    );
    assert_eq!(e.json(&["--verificar"]).0, 0);
}

#[test]
fn sin_claude_el_resto_se_aplica_y_se_muestra_el_comando_manual() {
    let e = entorno();
    let out = e
        .comando(&e.repo, &["conectar", "--aplicar"])
        .env("FELIX_CLAUDE_BIN", e.raiz.join("no-existe"))
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("no_disponible"), "{stdout}");
    assert!(
        stdout.contains("claude mcp add --scope user felix-criterio"),
        "{stdout}"
    );
    assert!(e.skill().exists());
    assert!(e.git_config("felix.bin").is_some());
}

#[test]
fn los_hooks_usan_felix_bin_sin_depender_del_path() {
    let e = entorno();
    assert_eq!(e.conectar(&["--aplicar"]).codigo, 0);
    let git = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let dir_git = Path::new(String::from_utf8(git.stdout).unwrap().trim())
        .parent()
        .unwrap()
        .to_path_buf();
    // PATH mínimo, sin universo: el hook encuentra el binario por felix.bin.
    let path = format!("{}:/usr/bin:/bin", dir_git.display());
    let hook = || {
        Command::new("sh")
            .arg(e.repo.join(".githooks/pre-commit"))
            .current_dir(&e.repo)
            .env("PATH", &path)
            .env_remove("FELIX_BIN")
            .output()
            .unwrap()
    };
    let out = hook();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // Con felix.bin roto, falla con un mensaje que dice cómo arreglarlo.
    assert!(
        Command::new("git")
            .args(["config", "felix.bin", "/no/existe/universo"])
            .current_dir(&e.repo)
            .status()
            .unwrap()
            .success()
    );
    let out = hook();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("universo conectar --aplicar"), "{stderr}");
}

#[test]
fn fuera_de_un_repositorio_de_criterio_es_error() {
    let e = entorno();
    let s = e.universo(&e.raiz, &["conectar"]);
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
    assert!(s.stdout.contains("code:"), "{}", s.stdout);
}

#[test]
fn opciones_invalidas_son_uso() {
    let e = entorno();
    for args in [
        &["--aplicar", "--verificar"][..],
        &["--cliente", "codex"],
        &["--desconocida"],
    ] {
        let s = e.conectar(args);
        assert_eq!(s.codigo, 2, "{args:?}: {}{}", s.stdout, s.stderr);
    }
}

#[test]
fn un_hook_propio_no_se_sobrescribe() {
    let e = entorno();
    let hook = e.repo.join(".githooks/pre-commit");
    ejecutable(&hook, "#!/bin/sh\necho mi hook\n");
    let (_, json) = e.json(&[]);
    assert_eq!(estado(&json, "hooks_archivos"), "no_disponible");
    let s = e.conectar(&["--aplicar"]);
    assert_eq!(s.codigo, 1, "{}", s.stdout);
    assert_eq!(
        fs::read_to_string(&hook).unwrap(),
        "#!/bin/sh\necho mi hook\n"
    );
    // El otro hook, que sí es de Felix, se activa igual.
    assert_eq!(e.git_config("core.hooksPath").as_deref(), Some(".githooks"));
}

#[test]
fn un_servidor_con_el_mismo_nombre_en_otro_alcance_no_se_toca() {
    let e = entorno();
    fs::write(
        &e.estado_mcp,
        "felix-criterio:\n  Scope: Local config (private to you in this project)\n  Command: otro\n  Args: x\n",
    )
    .unwrap();
    let (_, json) = e.json(&[]);
    assert_eq!(estado(&json, "mcp"), "no_disponible");
    let s = e.conectar(&["--aplicar"]);
    assert_eq!(s.codigo, 1, "{}", s.stdout);
    assert!(!e.log_claude().contains("mcp remove"), "{}", e.log_claude());
    assert!(!e.log_claude().contains("mcp add"), "{}", e.log_claude());
}

#[test]
fn con_home_vacio_la_skill_no_se_escribe_en_el_directorio_actual() {
    let e = entorno();
    let out = e
        .comando(&e.repo, &["conectar", "--aplicar", "--format=json"])
        .env("HOME", "")
        .env_remove("USERPROFILE")
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(estado(&json, "skill"), "no_disponible");
    assert!(!e.repo.join(".claude").exists());
}
