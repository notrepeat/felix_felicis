//! `universo conectar`: conecta un repositorio de criterio a Claude Code en un
//! paso. Por defecto sólo muestra qué haría; `--aplicar` lo hace y
//! `--verificar` comprueba que siga en su lugar.
//!
//! Pasos, cada uno idempotente:
//! - `binario`: `git config felix.bin` con la ruta absoluta de este `universo`,
//!   para que los hooks no dependan del `PATH`;
//! - `hooks_archivos`: `.githooks/` igual a las plantillas de esta versión;
//! - `hooks_activos`: `git config core.hooksPath .githooks`;
//! - `mcp`: el servidor `felix-criterio` en Claude Code a nivel de usuario
//!   (`claude mcp add --scope user`), con rutas absolutas;
//! - `skill`: `~/.claude/skills/felix-criterio/SKILL.md`, la de esta versión.
//!
//! Git se usa sólo para configuración local del repositorio (excepción a D4 de
//! la fase 10, aprobada el 2026-11-19): nunca hace commit ni push. Variables
//! para pruebas o instalaciones no estándar: `FELIX_CLAUDE_BIN` (el `claude`
//! a usar) y `FELIX_MCP_BIN` (el `universo-mcp`).
use super::{
    Formato,
    criterio::{resolver_repo, sin_prefijo_verbatim},
    escribir_salida, init,
};
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(super) const USO: &str = "Uso: universo conectar [--aplicar | --verificar] [--cliente claude-code] [--criterio <dir>] [--format text|json]\nConecta este repositorio de criterio a Claude Code: hooks de Git con la ruta de este binario, el MCP a nivel de usuario y la skill felix-criterio. Sin --aplicar sólo muestra qué cambiaría; --verificar termina con 1 si falta algo.\n";

const SKILL: &str = include_str!("../../../skills/felix-criterio/SKILL.md");
const SERVIDOR: &str = "felix-criterio";
/// Marca de los hooks que escribe Felix: un hook sin ella es del usuario y no
/// se sobrescribe.
const MARCA_HOOK: &str = "Hook de Felix";
/// `claude mcp get` arranca el servidor registrado para revisar su estado: si
/// no responde en este tiempo, el paso queda sin verificar.
const LIMITE_CLAUDE: Duration = Duration::from_secs(60);
const HOOKS: [(&str, &str); 2] = [
    ("pre-commit", init::HOOK),
    ("post-merge", init::HOOK_POST_MERGE),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modo {
    Preview,
    Aplicar,
    Verificar,
}

pub(super) struct Solicitud {
    modo: Modo,
    criterio: Option<PathBuf>,
    formato: Formato,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let (mut modo, mut criterio, mut formato, mut cliente) = (None, None, None, None);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let nuevo = match arg.as_str() {
            "--aplicar" => Some(Modo::Aplicar),
            "--verificar" => Some(Modo::Verificar),
            _ => None,
        };
        if let Some(m) = nuevo {
            if modo.replace(m).is_some() {
                return Err(fail());
            }
            i += 1;
            continue;
        }
        let (nombre, valor, consumidos) = match arg.split_once('=') {
            Some((n, v)) => (n, v.to_string(), 1),
            None => (arg.as_str(), args.get(i + 1).cloned().ok_or_else(fail)?, 2),
        };
        if valor.is_empty() || valor.starts_with('-') {
            return Err(fail());
        }
        let repetido = match nombre {
            "--criterio" => criterio.replace(PathBuf::from(valor)).is_some(),
            "--cliente" if valor == "claude-code" => cliente.replace(()).is_some(),
            "--format" => {
                let f = match valor.as_str() {
                    "text" => Formato::Texto,
                    "json" => Formato::Json,
                    _ => return Err(fail()),
                };
                formato.replace(f).is_some()
            }
            _ => return Err(fail()),
        };
        if repetido {
            return Err(fail());
        }
        i += consumidos;
    }
    Ok(Solicitud {
        modo: modo.unwrap_or(Modo::Preview),
        criterio,
        formato: formato.unwrap_or(Formato::Texto),
    })
}

/// Estado de un paso antes (y, con `--aplicar`, después) de actuar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Estado {
    Ok,
    Falta,
    Desactualizado,
    NoDisponible,
    Aplicado,
    Error,
}

impl Estado {
    fn nombre(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Falta => "falta",
            Self::Desactualizado => "desactualizado",
            Self::NoDisponible => "no_disponible",
            Self::Aplicado => "aplicado",
            Self::Error => "error",
        }
    }

    fn pendiente(self) -> bool {
        matches!(self, Self::Falta | Self::Desactualizado)
    }
}

struct Paso {
    id: &'static str,
    estado: Estado,
    /// Qué hace (o haría) el paso; vacío si ya está bien.
    accion: String,
    detalle: Option<String>,
}

/// Lo que `conectar` necesita del entorno, resuelto una vez.
struct Contexto {
    raiz: PathBuf,
    universo: Option<PathBuf>,
    mcp: Option<PathBuf>,
    /// Candidatos para `claude`, en orden (en Windows, también `claude.cmd`).
    claude: Vec<PathBuf>,
    skill: Option<PathBuf>,
}

fn ruta_absoluta(p: PathBuf) -> PathBuf {
    sin_prefijo_verbatim(p.canonicalize().unwrap_or(p))
}

fn contexto(raiz: &Path) -> Contexto {
    let exe = std::env::current_exe().ok().map(ruta_absoluta);
    let mcp = std::env::var_os("FELIX_MCP_BIN")
        .map(PathBuf::from)
        .or_else(|| {
            exe.as_ref()
                .map(|e| e.with_file_name(format!("universo-mcp{}", std::env::consts::EXE_SUFFIX)))
        })
        .filter(|p| p.is_file())
        .map(ruta_absoluta);
    // En Windows manda USERPROFILE (Claude Code usa ese HOME); un valor vacío o
    // relativo no sirve: escribiría la skill en el directorio actual.
    let orden: &[&str] = if cfg!(windows) {
        &["USERPROFILE", "HOME"]
    } else {
        &["HOME"]
    };
    let home = orden
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|p| p.is_absolute());
    let claude = match std::env::var_os("FELIX_CLAUDE_BIN") {
        Some(ruta) => vec![PathBuf::from(ruta)],
        None if cfg!(windows) => vec![PathBuf::from("claude"), PathBuf::from("claude.cmd")],
        None => vec![PathBuf::from("claude")],
    };
    Contexto {
        raiz: ruta_absoluta(raiz.to_path_buf()),
        universo: exe,
        mcp,
        claude,
        skill: home.map(|h| h.join(".claude/skills/felix-criterio/SKILL.md")),
    }
}

fn git(raiz: &Path, args: &[&str]) -> Option<(bool, String)> {
    let out = Command::new("git")
        .arg("-C")
        .arg(raiz)
        .args(args)
        .output()
        .ok()?;
    Some((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
    ))
}

fn config_local(raiz: &Path, clave: &str) -> Option<String> {
    git(raiz, &["config", "--local", "--get", clave]).and_then(|(ok, valor)| ok.then_some(valor))
}

/// Corre `claude` con `args`: (éxito, stdout), o por qué no se pudo.
fn claude(c: &Contexto, args: &[&str]) -> Result<(bool, String), String> {
    for candidato in &c.claude {
        let hijo = Command::new(candidato)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut hijo) = hijo else { continue };
        let mut stdout = hijo.stdout.take().ok_or("sin stdout")?;
        let lector = std::thread::spawn(move || {
            let mut texto = String::new();
            let _ = stdout.read_to_string(&mut texto);
            texto
        });
        let inicio = Instant::now();
        loop {
            match hijo.try_wait() {
                Ok(Some(estado)) => {
                    return Ok((estado.success(), lector.join().unwrap_or_default()));
                }
                Ok(None) if inicio.elapsed() > LIMITE_CLAUDE => {
                    let _ = hijo.kill();
                    let _ = hijo.wait();
                    return Err(format!(
                        "{} no respondió en {} s",
                        candidato.display(),
                        LIMITE_CLAUDE.as_secs()
                    ));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(e.to_string()),
            }
        }
    }
    Err(format!(
        "no se encontró {}",
        c.claude
            .first()
            .map_or_else(String::new, |p| p.display().to_string())
    ))
}

/// En Windows las rutas no distinguen mayúsculas.
fn misma_ruta(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Campos `Scope:`, `Command:` y `Args:` de `claude mcp get`.
fn campo<'a>(salida: &'a str, nombre: &str) -> Option<&'a str> {
    salida
        .lines()
        .find_map(|l| l.trim().strip_prefix(nombre)?.strip_prefix(':'))
        .map(str::trim)
}

fn comando_manual(c: &Contexto) -> String {
    let mcp = c
        .mcp
        .as_ref()
        .map_or_else(|| "universo-mcp".to_string(), |p| p.display().to_string());
    format!(
        "claude mcp add --scope user {SERVIDOR} -- '{mcp}' --criterio '{}'",
        c.raiz.display()
    )
}

fn paso_binario(c: &Contexto) -> Paso {
    let (estado, accion) = match (&c.universo, config_local(&c.raiz, "felix.bin")) {
        (None, _) => (
            Estado::NoDisponible,
            "no se pudo saber la ruta de este binario".to_string(),
        ),
        (Some(exe), actual) => {
            let accion = format!("git config felix.bin {}", exe.display());
            match actual {
                Some(v) if misma_ruta(Path::new(&v), exe) => (Estado::Ok, String::new()),
                Some(_) => (Estado::Desactualizado, accion),
                None => (Estado::Falta, accion),
            }
        }
    };
    Paso {
        id: "binario",
        estado,
        accion,
        detalle: None,
    }
}

fn paso_hooks_archivos(c: &Contexto) -> Paso {
    let dir = c.raiz.join(".githooks");
    let actual = |nombre: &str| fs::read_to_string(dir.join(nombre)).ok();
    let propios: Vec<&str> = HOOKS
        .iter()
        .filter(|(nombre, _)| actual(nombre).is_some_and(|t| !t.contains(MARCA_HOOK)))
        .map(|(nombre, _)| *nombre)
        .collect();
    if !propios.is_empty() {
        return Paso {
            id: "hooks_archivos",
            estado: Estado::NoDisponible,
            accion: String::new(),
            detalle: Some(format!(
                "hooks propios, no se tocan: .githooks/{}; agrega a mano la llamada a universo guard",
                propios.join(", .githooks/")
            )),
        };
    }
    let viejos: Vec<&str> = HOOKS
        .iter()
        .filter(|(nombre, plantilla)| actual(nombre).as_deref() != Some(*plantilla))
        .map(|(nombre, _)| *nombre)
        .collect();
    let estado = if viejos.is_empty() {
        Estado::Ok
    } else if viejos.iter().all(|n| !dir.join(n).exists()) {
        Estado::Falta
    } else {
        Estado::Desactualizado
    };
    let accion = if viejos.is_empty() {
        String::new()
    } else {
        format!(
            "reescribir .githooks/{} con la plantilla de esta versión (queda un git commit pendiente)",
            viejos.join(", .githooks/")
        )
    };
    Paso {
        id: "hooks_archivos",
        estado,
        accion,
        detalle: None,
    }
}

fn paso_hooks_activos(c: &Contexto) -> Paso {
    let accion = "git config core.hooksPath .githooks".to_string();
    let (estado, accion) = match config_local(&c.raiz, "core.hooksPath").as_deref() {
        Some(".githooks") => (Estado::Ok, String::new()),
        Some(_) => (Estado::Desactualizado, accion),
        None => (Estado::Falta, accion),
    };
    Paso {
        id: "hooks_activos",
        estado,
        accion,
        detalle: None,
    }
}

fn paso_mcp(c: &Contexto) -> Paso {
    let paso = |estado, accion, detalle| Paso {
        id: "mcp",
        estado,
        accion,
        detalle,
    };
    let Some(mcp) = &c.mcp else {
        return paso(
            Estado::NoDisponible,
            comando_manual(c),
            Some("universo-mcp no está junto a universo".into()),
        );
    };
    match claude(c, &["mcp", "get", SERVIDOR]) {
        Err(motivo) => paso(Estado::NoDisponible, comando_manual(c), Some(motivo)),
        Ok((false, _)) => paso(Estado::Falta, comando_manual(c), None),
        Ok((true, salida)) => {
            let alcance = campo(&salida, "Scope").unwrap_or_default();
            if !alcance.starts_with("User") {
                return paso(
                    Estado::NoDisponible,
                    String::new(),
                    Some(format!(
                        "ya hay un servidor {SERVIDOR} en otro alcance ({alcance}); revísalo con claude mcp get {SERVIDOR}"
                    )),
                );
            }
            let comando = campo(&salida, "Command").unwrap_or_default();
            let args = campo(&salida, "Args").unwrap_or_default();
            let esperado = format!("--criterio {}", c.raiz.display());
            if misma_ruta(Path::new(comando), mcp) && args == esperado {
                paso(Estado::Ok, String::new(), None)
            } else {
                // El nombre es uno por usuario: reemplazarlo desconecta el
                // repositorio al que apunta hoy.
                paso(
                    Estado::Desactualizado,
                    format!(
                        "claude mcp remove {SERVIDOR} --scope user; {}",
                        comando_manual(c)
                    ),
                    Some(format!("hoy apunta a: {comando} {args}")),
                )
            }
        }
    }
}

fn paso_skill(c: &Contexto) -> Paso {
    let (estado, accion) = match &c.skill {
        None => (Estado::NoDisponible, "no se conoce HOME".to_string()),
        Some(ruta) => {
            let accion = format!("escribir {} (la de esta versión)", ruta.display());
            match fs::read_to_string(ruta) {
                Ok(t) if t == SKILL => (Estado::Ok, String::new()),
                Ok(_) => (Estado::Desactualizado, accion),
                Err(_) => (Estado::Falta, accion),
            }
        }
    };
    Paso {
        id: "skill",
        estado,
        accion,
        detalle: None,
    }
}

/// Revisa cada paso sin escribir nada.
fn revisar(c: &Contexto) -> Vec<Paso> {
    vec![
        paso_binario(c),
        paso_hooks_archivos(c),
        paso_hooks_activos(c),
        paso_mcp(c),
        paso_skill(c),
    ]
}

fn escribir_hook(ruta: &Path, texto: &str) -> std::io::Result<()> {
    if let Some(padre) = ruta.parent() {
        fs::create_dir_all(padre)?;
    }
    fs::write(ruta, texto)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(ruta, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn exito<E>(r: Result<(bool, String), E>) -> bool {
    r.is_ok_and(|(ok, _)| ok)
}

fn exito_git(r: Option<(bool, String)>) -> bool {
    r.is_some_and(|(ok, _)| ok)
}

/// Aplica un paso pendiente y deja su estado final.
fn aplicar(c: &Contexto, paso: &mut Paso) {
    let ok = match paso.id {
        "binario" => c.universo.as_ref().is_some_and(|exe| {
            exito_git(git(
                &c.raiz,
                &["config", "--local", "felix.bin", &exe.display().to_string()],
            ))
        }),
        "hooks_archivos" => HOOKS.iter().all(|(nombre, texto)| {
            escribir_hook(&c.raiz.join(".githooks").join(nombre), texto).is_ok()
        }),
        "hooks_activos" => exito_git(git(
            &c.raiz,
            &["config", "--local", "core.hooksPath", ".githooks"],
        )),
        "mcp" => c.mcp.as_ref().is_some_and(|mcp| {
            if paso.estado == Estado::Desactualizado
                && !exito(claude(c, &["mcp", "remove", SERVIDOR, "--scope", "user"]))
            {
                return false;
            }
            let (mcp, raiz) = (mcp.display().to_string(), c.raiz.display().to_string());
            let agregado = exito(claude(
                c,
                &[
                    "mcp",
                    "add",
                    "--scope",
                    "user",
                    SERVIDOR,
                    "--",
                    &mcp,
                    "--criterio",
                    &raiz,
                ],
            ));
            if !agregado && paso.estado == Estado::Desactualizado {
                paso.detalle = Some(format!(
                    "se quitó el registro anterior y no se pudo agregar el nuevo; corre: {}",
                    comando_manual(c)
                ));
            }
            agregado
        }),
        "skill" => c.skill.as_ref().is_some_and(|ruta| {
            ruta.parent().is_some_and(|p| fs::create_dir_all(p).is_ok())
                && fs::write(ruta, SKILL).is_ok()
        }),
        _ => false,
    };
    paso.estado = if ok { Estado::Aplicado } else { Estado::Error };
}

fn texto(
    modo: Modo,
    pendientes: usize,
    todo_ok: bool,
    c: &Contexto,
    pasos: &[Paso],
    commit: bool,
) -> String {
    let mut t = match modo {
        Modo::Preview if pendientes == 0 => {
            "conectar: todo conectado; nada que hacer\n".to_string()
        }
        Modo::Preview => {
            "conectar: vista previa (nada escrito; aplica con universo conectar --aplicar)\n"
                .to_string()
        }
        Modo::Aplicar if pendientes == 0 && todo_ok => "conectar: nada que hacer\n".to_string(),
        Modo::Aplicar if todo_ok => "conectar: aplicado\n".to_string(),
        Modo::Aplicar => {
            "conectar: aplicado en parte; revisa los pasos sin [aplicado] ni [ok]\n".to_string()
        }
        Modo::Verificar if todo_ok => "conectar: ok\n".to_string(),
        Modo::Verificar => {
            "conectar: incompleto (universo conectar --aplicar lo completa)\n".to_string()
        }
    };
    t.push_str(&format!("criterio: {}\n", c.raiz.display()));
    for p in pasos {
        t.push_str(&format!("[{}] {}", p.estado.nombre(), p.id));
        if !p.accion.is_empty() && !matches!(p.estado, Estado::Ok | Estado::Aplicado) {
            t.push_str(&format!(": {}", p.accion));
        }
        if let Some(d) = &p.detalle {
            t.push_str(&format!(" ({d})"));
        }
        t.push('\n');
    }
    if commit {
        t.push_str(
            "pendiente: los hooks están versionados; revisa y haz git commit de .githooks/\n",
        );
    }
    t
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let repo = match resolver_repo(s.criterio, true) {
        Ok(repo) => repo,
        Err(e) => {
            let texto = format!("conectar: error\ncode: {}\n{e}\n", e.codigo());
            escribir_salida(salida, error, texto.as_bytes(), "stdout");
            return 1;
        }
    };
    let c = contexto(&repo.raiz);
    let mut pasos = revisar(&c);
    let pendientes = pasos.iter().filter(|p| p.estado.pendiente()).count();
    let commit = s.modo == Modo::Aplicar
        && pasos
            .iter()
            .any(|p| p.id == "hooks_archivos" && p.estado.pendiente());
    if s.modo == Modo::Aplicar {
        for paso in pasos.iter_mut().filter(|p| p.estado.pendiente()) {
            aplicar(&c, paso);
        }
    }
    let todo_ok = pasos
        .iter()
        .all(|p| matches!(p.estado, Estado::Ok | Estado::Aplicado));
    let codigo = match s.modo {
        Modo::Preview => 0,
        Modo::Aplicar | Modo::Verificar => i32::from(!todo_ok),
    };
    let modo = match s.modo {
        Modo::Preview => "preview",
        Modo::Aplicar => "aplicar",
        Modo::Verificar => "verificar",
    };
    let cuerpo = match s.formato {
        Formato::Json => format!(
            "{}\n",
            json!({"schema_version": 1, "command": "conectar", "mode": modo, "ok": todo_ok,
                   "criterio": c.raiz.display().to_string(),
                   "steps": pasos.iter().map(|p| json!({"id": p.id, "status": p.estado.nombre(),
                       "action": p.accion, "detail": p.detalle})).collect::<Vec<_>>(),
                   "commit_pending": commit})
        ),
        Formato::Texto => texto(s.modo, pendientes, todo_ok, &c, &pasos, commit),
    };
    if escribir_salida(salida, error, cuerpo.as_bytes(), "stdout") {
        codigo
    } else {
        1
    }
}
