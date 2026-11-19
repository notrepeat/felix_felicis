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
        "capturar" | "reforzar" | "objetar" => (true, true, Extra::Libres),
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
