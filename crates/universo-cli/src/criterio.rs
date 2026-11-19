//! Comandos sin rutas dentro de un repositorio de criterio (fase 10, U1).
//!
//! Antes de interpretar un comando, si se usa la forma sin rutas, se busca el
//! repositorio (`--criterio`, `FELIX_CRITERIO` o el `felix.toml` más cercano
//! hacia arriba), se elige el universo (`--universo`, `FELIX_UNIVERSO` o el
//! `por_defecto`) y se reescriben los argumentos: se inserta la raíz del bundle
//! y, si el comando las usa, `--manifiesto` y `--invariantes`. Los parsers de
//! cada comando no cambian. La forma con rutas explícitas sigue igual y tiene
//! precedencia.
use serde_json::json;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
use universo_okf::criterio::{ErrorCriterio, RepoCriterio, buscar_hacia_arriba};

/// Qué pide cada comando del universo.
#[derive(Clone, Copy)]
struct Uso {
    manifiesto: bool,
    invariantes: bool,
    /// Posicionales propios del comando además de la raíz.
    extra: Extra,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Extra {
    /// Ninguno: con un posicional, es la raíz explícita.
    Ninguno,
    /// Exactamente uno (`show <n>`): con dos, el primero es la raíz.
    Uno,
    /// `diff <desde> [<hasta>]`: números; si el primero no lo es, es la raíz.
    Numeros,
    /// Términos libres (`search`) o ninguno con opciones de rutas: sólo cuenta
    /// `--manifiesto` o `--invariantes`.
    Libres,
}

fn uso(comando: &str) -> Option<Uso> {
    let (manifiesto, invariantes, extra) = match comando {
        "atomos" | "validate" | "verificar" => (true, false, Extra::Libres),
        "check" | "propose" | "guard" | "seal" => (true, true, Extra::Libres),
        "search" | "delta" => (true, false, Extra::Libres),
        "capturar" | "reforzar" | "objetar" | "relacion" | "atributo" | "nota" | "reglas"
        | "manifiesto" => (true, true, Extra::Libres),
        "status" | "recover" | "log" => (false, false, Extra::Ninguno),
        "show" => (false, false, Extra::Uno),
        "diff" => (false, false, Extra::Numeros),
        _ => return None,
    };
    Some(Uso {
        manifiesto,
        invariantes,
        extra,
    })
}

/// Nombre reservado: `search --universo todos` busca en todos los universos.
const TODOS: &str = "todos";

/// Opciones de los comandos que llevan un valor en el argumento siguiente.
const CON_VALOR: &[&str] = &[
    "--format",
    "--manifiesto",
    "--invariantes",
    "--delta",
    "--desde",
    "--limit",
    "--explain",
    "--expect-snapshot",
    "--author",
    "--reason",
    "--proyecto",
    "--tarea",
    "--contexto",
    "--validacion",
    "--fecha",
    "--autor",
    "--motivo",
];

/// Posicionales de `args` (sin el comando), saltando los valores de opciones.
fn posicionales(args: &[String]) -> Vec<&str> {
    let mut salida = Vec::new();
    let mut i = 0;
    let mut solo_posicionales = false;
    while i < args.len() {
        let arg = args[i].as_str();
        if solo_posicionales || !arg.starts_with('-') || arg == "-" {
            salida.push(arg);
        } else if arg == "--" {
            solo_posicionales = true;
        } else if CON_VALOR.contains(&arg) {
            i += 1;
        }
        i += 1;
    }
    salida
}

/// Resultado de preparar los argumentos.
pub(super) enum Preparados {
    /// Argumentos listos para el parser habitual.
    Listos(Vec<String>),
    /// Uso inválido (salida 2) con su mensaje.
    Uso(String),
    /// Error del repositorio de criterio (salida 1), ya formateado, y si va a
    /// stdout (JSON) o a stderr (texto).
    Error { texto: String, json: bool },
    /// `search --universo todos`: cada universo (nombre, bundle, manifiesto) y
    /// los argumentos del comando sin las opciones de criterio.
    BuscarTodos {
        universos: Vec<(String, PathBuf, PathBuf)>,
        resto: Vec<String>,
    },
}

fn error(comando: &str, json: bool, e: &ErrorCriterio) -> Preparados {
    let texto = if json {
        format!(
            "{}\n",
            json!({"schema_version":1,"command":comando,"status":"error",
                   "errors":[{"code":e.codigo(),"message":e.to_string()}]})
        )
    } else {
        format!("criterio: error\ncode: {}\n{e}\n", e.codigo())
    };
    Preparados::Error { texto, json }
}

/// `--criterio`, `--universo` e `--ignorar-version` sacados de los argumentos.
struct Opciones {
    criterio: Option<PathBuf>,
    universo: Option<String>,
    ignorar_version: bool,
    resto: Vec<String>,
}

/// Saca las opciones de criterio de `args`, en ambas formas (con `=` o con el
/// valor en el argumento siguiente). Tras `--` todo es término.
fn extraer(args: &[String]) -> Result<Opciones, ()> {
    let mut opciones = Opciones {
        criterio: None,
        universo: None,
        ignorar_version: false,
        resto: Vec::new(),
    };
    let mut i = 0;
    let mut solo_posicionales = false;
    while i < args.len() {
        let arg = &args[i];
        if solo_posicionales {
            opciones.resto.push(arg.clone());
            i += 1;
            continue;
        }
        let (nombre, valor, consumidos) = if let Some((n, v)) = arg.split_once('=') {
            (n, Some(v.to_string()), 1)
        } else {
            (arg.as_str(), args.get(i + 1).cloned(), 2)
        };
        // El valor de otra opción (`--reason --universo`) no es una opción.
        if CON_VALOR.contains(&arg.as_str()) {
            opciones
                .resto
                .extend(args[i..(i + 2).min(args.len())].iter().cloned());
            i += 2;
            continue;
        }
        match nombre {
            "--criterio" | "--universo" => {
                let valor = valor
                    .filter(|v| !v.is_empty() && !v.starts_with('-'))
                    .ok_or(())?;
                let repetido = if nombre == "--criterio" {
                    opciones.criterio.replace(PathBuf::from(valor)).is_some()
                } else {
                    opciones.universo.replace(valor).is_some()
                };
                if repetido {
                    return Err(());
                }
                i += consumidos;
            }
            "--ignorar-version" if !arg.contains('=') => {
                opciones.ignorar_version = true;
                i += 1;
            }
            _ => {
                if arg == "--" {
                    solo_posicionales = true;
                }
                opciones.resto.push(arg.clone());
                i += 1;
            }
        }
    }
    Ok(opciones)
}

fn de_entorno(clave: &str) -> Option<OsString> {
    std::env::var_os(clave).filter(|v| !v.is_empty())
}

/// Ruta como argumento: una relativa que empezara con `-` se leería como
/// opción, así que se ancla en `./`.
fn ruta(ruta: &Path) -> String {
    let texto = ruta.display().to_string();
    if texto.starts_with('-') {
        format!("./{texto}")
    } else {
        texto
    }
}

/// El repositorio de criterio para comandos que usan varios universos
/// (`promover`): `--criterio`, `FELIX_CRITERIO` o el `felix.toml` más
/// cercano, con el control de versión del motor.
pub(super) fn resolver_repo(
    criterio: Option<PathBuf>,
    ignorar_version: bool,
) -> Result<RepoCriterio, ErrorCriterio> {
    let raiz = match (criterio, de_entorno("FELIX_CRITERIO")) {
        (Some(dir), _) => dir,
        (None, Some(dir)) => PathBuf::from(dir),
        (None, None) => std::env::current_dir()
            .ok()
            .and_then(|cwd| buscar_hacia_arriba(&cwd))
            .ok_or(ErrorCriterio::NoEncontrado)?,
    };
    let repo = RepoCriterio::cargar(&raiz)?;
    if !ignorar_version {
        repo.comprobar_motor(env!("CARGO_PKG_VERSION"))?;
    }
    Ok(repo)
}

/// Prepara `args` (con el comando en la primera posición) para el parser.
pub(super) fn preparar(args: Vec<String>, uso_general: &str) -> Preparados {
    let Some((comando, resto)) = args.split_first() else {
        return Preparados::Listos(args);
    };
    let Some(necesita) = uso(comando) else {
        return Preparados::Listos(args);
    };
    // La ayuda no depende del repositorio: aunque felix.toml esté roto.
    if resto
        .iter()
        .take_while(|a| *a != "--")
        .any(|a| a == "-h" || a == "--help")
    {
        return Preparados::Listos(args);
    }
    let Ok(opciones) = extraer(resto) else {
        return Preparados::Uso(uso_general.to_string());
    };
    let forzado = opciones.criterio.is_some() || opciones.universo.is_some();
    let antes_del_separador = || opciones.resto.iter().take_while(|a| *a != "--");
    let con_rutas = antes_del_separador()
        .any(|a| a.starts_with("--manifiesto") || a.starts_with("--invariantes"));
    if forzado && con_rutas {
        return Preparados::Uso(uso_general.to_string());
    }
    let posicionales = posicionales(&opciones.resto);
    let raiz_explicita = con_rutas
        || match necesita.extra {
            Extra::Libres => false,
            Extra::Ninguno => !posicionales.is_empty(),
            Extra::Uno => posicionales.len() >= 2,
            Extra::Numeros => posicionales
                .first()
                .is_some_and(|p| p.parse::<usize>().is_err()),
        };
    let lista: Vec<&String> = antes_del_separador().collect();
    let json = lista
        .windows(2)
        .any(|w| w[0] == "--format" && w[1] == "json")
        || lista.iter().any(|a| *a == "--format=json");
    let sin_cambios = || {
        let mut args = vec![comando.clone()];
        args.extend(opciones.resto.iter().cloned());
        Preparados::Listos(args)
    };
    if raiz_explicita && !forzado {
        return sin_cambios();
    }
    // Repositorio: --criterio, FELIX_CRITERIO o el felix.toml más cercano.
    let raiz: Option<PathBuf> = match (&opciones.criterio, de_entorno("FELIX_CRITERIO")) {
        (Some(dir), _) => Some(dir.clone()),
        (None, Some(dir)) => Some(PathBuf::from(dir)),
        (None, None) => std::env::current_dir()
            .ok()
            .and_then(|cwd| buscar_hacia_arriba(&cwd)),
    };
    let Some(raiz) = raiz else {
        // Sin repositorio, --universo no tiene a qué aplicarse; sin él, el
        // comando sigue como siempre y su parser informa el uso.
        return if forzado {
            error(comando, json, &ErrorCriterio::NoEncontrado)
        } else {
            sin_cambios()
        };
    };
    let repo = match RepoCriterio::cargar(&raiz) {
        Ok(repo) => repo,
        Err(e) => return error(comando, json, &e),
    };
    if !opciones.ignorar_version {
        if let Err(e) = repo.comprobar_motor(env!("CARGO_PKG_VERSION")) {
            return error(comando, json, &e);
        }
    }
    // Un FELIX_UNIVERSO que no es texto es un error, no el universo por defecto.
    let nombre = match (opciones.universo.clone(), de_entorno("FELIX_UNIVERSO")) {
        (Some(nombre), _) => Some(nombre),
        (None, Some(valor)) => match valor.into_string() {
            Ok(nombre) => Some(nombre),
            Err(_) => return Preparados::Uso(uso_general.to_string()),
        },
        (None, None) => None,
    };
    // `todos` está reservado: la búsqueda recorre cada universo.
    if comando == "search" && nombre.as_deref() == Some(TODOS) {
        let mut universos = Vec::new();
        for nombre in repo.universos.keys() {
            match repo.universo(Some(nombre)) {
                Ok(u) => universos.push((u.nombre.clone(), u.bundle(), u.manifiesto())),
                Err(e) => return error(comando, json, &e),
            }
        }
        return Preparados::BuscarTodos {
            universos,
            resto: opciones.resto.clone(),
        };
    }
    let universo = match repo.universo(nombre.as_deref()) {
        Ok(universo) => universo,
        Err(e) => return error(comando, json, &e),
    };
    // Raíz primero; las opciones agregadas, antes de un posible `--`.
    let mut agregadas = Vec::new();
    if necesita.manifiesto {
        agregadas.push(format!("--manifiesto={}", ruta(&universo.manifiesto())));
    }
    if necesita.invariantes {
        agregadas.push(format!("--invariantes={}", ruta(&universo.invariantes())));
    }
    let mut args = vec![comando.clone(), ruta(&universo.bundle())];
    args.extend(agregadas);
    args.extend(opciones.resto.iter().cloned());
    Preparados::Listos(args)
}

/// `universo mcp-config`: la configuración del MCP para conectar un agente a
/// este repositorio de criterio, con rutas absolutas.
pub(super) struct SolicitudMcpConfig {
    cliente: String,
    nombre: String,
    criterio: Option<PathBuf>,
    universo: Option<String>,
}

pub(super) const USO_MCP_CONFIG: &str = "Uso: universo mcp-config [--cliente claude-code|codex|json] [--nombre <servidor>] [--criterio <dir>] [--universo <nombre>]\nImprime la configuración para que un cliente MCP use este repositorio de criterio con universo-mcp.\n";

pub(super) fn parsear_mcp_config(args: &[String]) -> Result<SolicitudMcpConfig, String> {
    let fail = || USO_MCP_CONFIG.to_string();
    let mut valores = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let (nombre, valor, consumidos) = match arg.split_once('=') {
            Some((n, v)) => (n, v.to_string(), 1),
            None => (arg.as_str(), args.get(i + 1).cloned().ok_or_else(fail)?, 2),
        };
        let clave = match nombre {
            "--cliente" => "cliente",
            "--nombre" => "nombre",
            "--criterio" => "criterio",
            "--universo" => "universo",
            _ => return Err(fail()),
        };
        if valor.is_empty() || valor.starts_with('-') || valores.insert(clave, valor).is_some() {
            return Err(fail());
        }
        i += consumidos;
    }
    let cliente = valores
        .remove("cliente")
        .unwrap_or_else(|| "claude-code".into());
    let nombre = valores
        .remove("nombre")
        .unwrap_or_else(|| "felix-criterio".into());
    let nombre_valido = nombre
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !["claude-code", "codex", "json"].contains(&cliente.as_str()) || !nombre_valido {
        return Err(fail());
    }
    Ok(SolicitudMcpConfig {
        cliente,
        nombre,
        criterio: valores.remove("criterio").map(PathBuf::from),
        universo: valores.remove("universo"),
    })
}

/// Cita una ruta para la shell sólo si hace falta.
fn citar(texto: &str) -> String {
    if texto
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-:".contains(c))
    {
        texto.to_string()
    } else {
        format!("'{}'", texto.replace('\'', "'\\''"))
    }
}

pub(super) fn ejecutar_mcp_config(
    s: SolicitudMcpConfig,
    salida: &mut impl std::io::Write,
    error: &mut impl std::io::Write,
) -> i32 {
    let repo = match resolver_repo(s.criterio, true) {
        Ok(repo) => repo,
        Err(e) => {
            let texto = format!("mcp-config: error\ncode: {}\n{e}\n", e.codigo());
            super::escribir_salida(salida, error, texto.as_bytes(), "stdout");
            return 1;
        }
    };
    let raiz = repo.raiz.canonicalize().unwrap_or(repo.raiz.clone());
    // universo-mcp se instala junto a universo; si no está, se confía en el PATH.
    let binario = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            let hermano =
                exe.with_file_name(format!("universo-mcp{}", std::env::consts::EXE_SUFFIX));
            hermano.is_file().then_some(hermano)
        })
        .map_or_else(
            || {
                let _ = error.write_all(
                    b"aviso: universo-mcp no est\xc3\xa1 junto a universo; se usa el del PATH\n",
                );
                "universo-mcp".to_string()
            },
            |p| p.display().to_string(),
        );
    if let Some(u) = &s.universo {
        if let Err(e) = repo.universo(Some(u)) {
            let texto = format!("mcp-config: error\ncode: {}\n{e}\n", e.codigo());
            super::escribir_salida(salida, error, texto.as_bytes(), "stdout");
            return 1;
        }
    }
    let mut argumentos = vec!["--criterio".to_string(), raiz.display().to_string()];
    if let Some(u) = &s.universo {
        argumentos.extend(["--universo".to_string(), u.clone()]);
    }
    let texto = match s.cliente.as_str() {
        "claude-code" => format!(
            "# A nivel de usuario: el criterio se consulta desde cualquier proyecto.\nclaude mcp add --scope user {} -- {} {}\n",
            s.nombre,
            citar(&binario),
            argumentos
                .iter()
                .map(|a| citar(a))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        "codex" => format!(
            "# En ~/.codex/config.toml\n[mcp_servers.{}]\ncommand = {}\nargs = {}\n",
            s.nombre,
            serde_json::to_string(&binario).unwrap_or_default(),
            serde_json::to_string(&argumentos).unwrap_or_default()
        ),
        _ => format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({"mcpServers": {s.nombre.clone(): {
                "command": binario, "args": argumentos}}}))
            .unwrap_or_default()
        ),
    };
    if super::escribir_salida(salida, error, texto.as_bytes(), "stdout") {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn cuenta_posicionales_sin_los_valores_de_opciones() {
        assert_eq!(posicionales(&v(&["--format", "json", "3"])), vec!["3"]);
        assert_eq!(
            posicionales(&v(&["--format=json", "r", "3"])),
            vec!["r", "3"]
        );
        assert_eq!(posicionales(&v(&["--", "-x"])), vec!["-x"]);
    }

    #[test]
    fn extrae_las_opciones_de_criterio_en_ambas_formas() {
        let o = extraer(&v(&[
            "--criterio",
            "r",
            "--universo=vigente",
            "--ignorar-version",
            "t",
        ]))
        .unwrap();
        assert_eq!(o.criterio.as_deref(), Some(Path::new("r")));
        assert_eq!(o.universo.as_deref(), Some("vigente"));
        assert!(o.ignorar_version);
        assert_eq!(o.resto, v(&["t"]));
        assert!(extraer(&v(&["--universo"])).is_err());
        assert!(extraer(&v(&["--universo", "a", "--universo", "b"])).is_err());
        // El valor de otra opción no es una opción de criterio.
        let o = extraer(&v(&["--reason", "--universo", "x"])).unwrap();
        assert_eq!(o.universo, None);
        assert_eq!(o.resto, v(&["--reason", "--universo", "x"]));
        // Tras `--`, todo es término.
        let o = extraer(&v(&["--", "--universo", "x"])).unwrap();
        assert_eq!(o.resto, v(&["--", "--universo", "x"]));
    }

    #[test]
    fn las_rutas_agregadas_van_antes_de_los_terminos() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("felix.toml"),
            format!(
                "schema = \"felix-criterio-v1\"\nmotor = \"{}\"\npor_defecto = \"c\"\n[universos.c]\ndir = \"u\"\n",
                universo_okf::criterio::rango_para(env!("CARGO_PKG_VERSION"))
            ),
        )
        .unwrap();
        let criterio = format!("--criterio={}", dir.path().display());
        let Preparados::Listos(args) = preparar(v(&["search", &criterio, "--", "-x"]), "uso")
        else {
            panic!("se esperaba Listos");
        };
        let separador = args.iter().position(|a| a == "--").unwrap();
        assert!(
            args[..separador]
                .iter()
                .any(|a| a.starts_with("--manifiesto=")),
            "{args:?}"
        );
        assert_eq!(&args[separador + 1..], &v(&["-x"])[..]);
        assert!(args[1].ends_with("bundle"), "{args:?}");
    }
}
