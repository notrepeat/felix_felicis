//! CLI del motor Universo de Conocimiento: comandos de lectura.
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use universo_core::Manifiesto;
use universo_okf::{ErrorBundle, ErrorManifiestoArchivo, cargar_bundle, cargar_manifiesto};

/// Uso breve, mostrado por errores de argumentos en `stderr`.
const USO: &str = "Uso:\n  universo atomos <raiz> --manifiesto <manifiesto.toml>\n  universo validate <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]\n  universo verificar <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]  (alias de validate)\nUse --manifiesto=<ruta> si la ruta del manifiesto empieza con '-'.\n";

/// Ayuda completa, mostrada en `stdout` con código de salida 0.
const AYUDA: &str = "CLI de Felix Felicis\n\nUso:\n  universo atomos <raiz> --manifiesto <manifiesto.toml>\n  universo validate <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]\n  universo verificar <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]\n\nComandos:\n  atomos     Imprime los átomos del bundle en orden canónico.\n  validate   Valida el manifiesto, el formato OKF y la estructura del bundle.\n  verificar  Alias compatible de validate.\n\nOpciones:\n  --manifiesto <ruta> | --manifiesto=<ruta>  Manifiesto TOML requerido.\n  --format text|json                         Formato de validate (por defecto: text).\n  -h, --help                                 Muestra esta ayuda y termina con código 0.\n\nSi la ruta del manifiesto empieza con `-`, usa `--manifiesto=<ruta>`.\nSi la raíz posicional empieza con `-`, antepone `./` (por ejemplo, `./-bundle`).\nvalidate es una comprobación estructural; no evalúa invariantes.\n\nCódigos de salida:\n  0  Bundle válido o ayuda mostrada.\n  1  Manifiesto o bundle inválido, o fallo de escritura.\n  2  Uso inválido.\n";

/// Formato de salida de `validate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Formato {
    Texto,
    Json,
}

/// Comando reconocido por la CLI, ya con sus argumentos resueltos.
enum Comando {
    Ayuda,
    Atomos {
        raiz: PathBuf,
        manifiesto: PathBuf,
    },
    Validate {
        raiz: PathBuf,
        manifiesto: PathBuf,
        formato: Formato,
    },
}

/// Interpreta los argumentos de línea de comandos (sin el nombre del
/// binario). Acepta `--manifiesto <valor>` y `--manifiesto=<valor>`.
fn parsear_args(args: &[String]) -> Result<Comando, String> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        return Ok(Comando::Ayuda);
    }

    let (nombre_comando, resto) = args.split_first().ok_or_else(|| USO.to_string())?;
    let tipo_comando = match nombre_comando.as_str() {
        "atomos" => 0,
        "validate" | "verificar" => 1,
        "-h" | "--help" => return Ok(Comando::Ayuda),
        _ => return Err(USO.to_string()),
    };

    let mut raiz: Option<PathBuf> = None;
    let mut manifiesto: Option<PathBuf> = None;
    let mut formato: Option<Formato> = None;
    let mut i = 0;
    while i < resto.len() {
        let arg = &resto[i];
        if matches!(arg.as_str(), "-h" | "--help") {
            return Ok(Comando::Ayuda);
        } else if let Some(valor) = arg.strip_prefix("--manifiesto=") {
            if manifiesto.is_some() || valor.is_empty() {
                return Err(USO.to_string());
            }
            manifiesto = Some(PathBuf::from(valor));
            i += 1;
        } else if arg == "--manifiesto" {
            let valor = resto.get(i + 1).ok_or_else(|| USO.to_string())?;
            if manifiesto.is_some() || valor.is_empty() || valor.starts_with('-') {
                return Err(USO.to_string());
            }
            manifiesto = Some(PathBuf::from(valor));
            i += 2;
        } else if let Some(valor) = arg.strip_prefix("--format=") {
            if tipo_comando != 1 || formato.is_some() {
                return Err(USO.to_string());
            }
            formato = Some(parsear_formato(valor)?);
            i += 1;
        } else if arg == "--format" {
            let valor = resto.get(i + 1).ok_or_else(|| USO.to_string())?;
            if tipo_comando != 1 || formato.is_some() || valor.starts_with('-') {
                return Err(USO.to_string());
            }
            formato = Some(parsear_formato(valor)?);
            i += 2;
        } else if arg.starts_with('-') {
            return Err(USO.to_string());
        } else if raiz.is_none() {
            raiz = Some(PathBuf::from(arg));
            i += 1;
        } else {
            return Err(USO.to_string());
        }
    }

    let raiz = raiz.ok_or_else(|| USO.to_string())?;
    let manifiesto = manifiesto.ok_or_else(|| USO.to_string())?;
    Ok(if tipo_comando == 0 {
        Comando::Atomos { raiz, manifiesto }
    } else {
        Comando::Validate {
            raiz,
            manifiesto,
            formato: formato.unwrap_or(Formato::Texto),
        }
    })
}

fn parsear_formato(valor: &str) -> Result<Formato, String> {
    match valor {
        "text" => Ok(Formato::Texto),
        "json" => Ok(Formato::Json),
        _ => Err(USO.to_string()),
    }
}

/// Informa un fallo de escritura. Si el escritor proporcionado también falla,
/// intenta dejar una última nota en el `stderr` real del proceso.
fn reportar_fallo_escritura(error: &mut impl Write, destino: &str, causa: &io::Error) {
    let mensaje = format!("error al escribir {destino}: {causa}\n");
    if error.write_all(mensaje.as_bytes()).is_err() {
        let _ = io::stderr().write_all(mensaje.as_bytes());
    }
}

fn escribir_salida(
    salida: &mut impl Write,
    error: &mut impl Write,
    bytes: &[u8],
    destino: &str,
) -> bool {
    match salida.write_all(bytes) {
        Ok(()) => true,
        Err(causa) => {
            reportar_fallo_escritura(error, destino, &causa);
            false
        }
    }
}

/// Carga el manifiesto o escribe el error de carga en `error` y devuelve el
/// código de salida 1.
fn cargar_manifiesto_o_falla(ruta: &Path, error: &mut impl Write) -> Result<Manifiesto, i32> {
    cargar_manifiesto(ruta).map_err(|e| {
        let mensaje = format!("{e}\n");
        if let Err(causa) = error.write_all(mensaje.as_bytes()) {
            reportar_fallo_escritura(error, "stderr", &causa);
        }
        1
    })
}

/// Escribe cada `ErrorBundle` de `errores` en `error`, uno por línea, y
/// devuelve el código de salida 1.
fn reportar_errores(errores: &[ErrorBundle], error: &mut impl Write) -> i32 {
    let mut buffer = String::new();
    for error_bundle in errores {
        buffer.push_str(&error_bundle.to_string());
        buffer.push('\n');
    }
    if let Err(causa) = error.write_all(buffer.as_bytes()) {
        reportar_fallo_escritura(error, "stderr", &causa);
    }
    1
}

/// Ejecuta `atomos`: imprime cada átomo (orden canónico) en `salida`, uno
/// por línea; o los errores del bundle en `error`, uno por línea.
fn ejecutar_atomos(
    raiz: &Path,
    manifiesto: &Path,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    let manifiesto = match cargar_manifiesto_o_falla(manifiesto, error) {
        Ok(m) => m,
        Err(codigo) => return codigo,
    };
    match cargar_bundle(raiz, manifiesto) {
        Ok(estructura) => {
            let mut buffer = String::new();
            for atomo in estructura.atomos() {
                buffer.push_str(&atomo.to_string());
                buffer.push('\n');
            }
            if escribir_salida(salida, error, buffer.as_bytes(), "stdout") {
                0
            } else {
                1
            }
        }
        Err(errores) => reportar_errores(&errores, error),
    }
}

fn error_json(codigo: &str, mensaje: String, ruta: Option<String>, linea: Option<usize>) -> Value {
    json!({
        "code": codigo,
        "message": mensaje,
        "path": ruta,
        "line": linea,
    })
}

fn resultado_json_validate(status: &str, summary: Option<Value>, errors: Vec<Value>) -> Value {
    json!({
        "schema_version": 1,
        "command": "validate",
        "status": status,
        "summary": summary,
        "errors": errors,
    })
}

fn escribir_json_validate(
    status: &str,
    summary: Option<Value>,
    errors: Vec<Value>,
    salida: &mut impl Write,
    error: &mut impl Write,
    codigo_salida: i32,
) -> i32 {
    let mut bytes = match serde_json::to_vec(&resultado_json_validate(status, summary, errors)) {
        Ok(bytes) => bytes,
        Err(causa) => {
            let causa = io::Error::other(causa);
            reportar_fallo_escritura(error, "JSON", &causa);
            return 1;
        }
    };
    bytes.push(b'\n');
    if escribir_salida(salida, error, &bytes, "stdout") {
        codigo_salida
    } else {
        1
    }
}

fn error_manifiesto_json(ruta: &Path, error: &ErrorManifiestoArchivo) -> Value {
    let codigo = match error {
        ErrorManifiestoArchivo::Io(_) => "ManifestIo",
        ErrorManifiestoArchivo::Sintaxis(_) => "ManifestSyntax",
        ErrorManifiestoArchivo::Validacion(_) => "ManifestInvalid",
    };
    error_json(
        codigo,
        error.to_string(),
        Some(ruta.display().to_string()),
        None,
    )
}

fn errores_bundle_json(errores: &[ErrorBundle]) -> Vec<Value> {
    errores
        .iter()
        .map(|error| {
            error_json(
                &error.regla.to_string(),
                error.detalle.clone(),
                error.path.clone(),
                error.linea,
            )
        })
        .collect()
}

/// Ejecuta `validate`: solo carga el manifiesto y valida el formato OKF y la
/// estructura del bundle; nunca evalúa invariantes.
fn ejecutar_validate(
    raiz: &Path,
    manifiesto: &Path,
    formato: Formato,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    let manifiesto_cargado = match cargar_manifiesto(manifiesto) {
        Ok(m) => m,
        Err(error_manifiesto) => {
            if formato == Formato::Json {
                return escribir_json_validate(
                    "invalid",
                    None,
                    vec![error_manifiesto_json(manifiesto, &error_manifiesto)],
                    salida,
                    error,
                    1,
                );
            }
            let mensaje = format!("{error_manifiesto}\n");
            if let Err(causa) = error.write_all(mensaje.as_bytes()) {
                reportar_fallo_escritura(error, "stderr", &causa);
            }
            return 1;
        }
    };

    match cargar_bundle(raiz, manifiesto_cargado) {
        Ok(estructura) => match formato {
            Formato::Texto => {
                let mensaje = format!(
                    "OK: {} nodos, {} aristas\n",
                    estructura.len_nodos(),
                    estructura.len_aristas()
                );
                if escribir_salida(salida, error, mensaje.as_bytes(), "stdout") {
                    0
                } else {
                    1
                }
            }
            Formato::Json => escribir_json_validate(
                "valid",
                Some(json!({
                    "nodes": estructura.len_nodos(),
                    "edges": estructura.len_aristas(),
                })),
                Vec::new(),
                salida,
                error,
                0,
            ),
        },
        Err(errores) => match formato {
            Formato::Texto => reportar_errores(&errores, error),
            Formato::Json => escribir_json_validate(
                "invalid",
                None,
                errores_bundle_json(&errores),
                salida,
                error,
                1,
            ),
        },
    }
}

/// Interpreta `args` y ejecuta el comando correspondiente, escribiendo en
/// `salida`/`error` y devolviendo el código de salida del proceso. Nunca
/// entra en pánico ante entradas de usuario inválidas.
pub(crate) fn ejecutar(args: Vec<String>, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let comando = match parsear_args(&args) {
        Ok(comando) => comando,
        Err(uso) => {
            if let Err(causa) = error.write_all(uso.as_bytes()) {
                reportar_fallo_escritura(error, "stderr", &causa);
            }
            return 2;
        }
    };
    match comando {
        Comando::Ayuda => {
            if escribir_salida(salida, error, AYUDA.as_bytes(), "stdout") {
                0
            } else {
                1
            }
        }
        Comando::Atomos { raiz, manifiesto } => ejecutar_atomos(&raiz, &manifiesto, salida, error),
        Comando::Validate {
            raiz,
            manifiesto,
            formato,
        } => ejecutar_validate(&raiz, &manifiesto, formato, salida, error),
    }
}

fn main() {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    let mut args: Vec<String> = Vec::new();
    let mut argumento_no_unicode = false;
    for arg in std::env::args_os().skip(1) {
        match arg.into_string() {
            Ok(s) => args.push(s),
            Err(_) => {
                argumento_no_unicode = true;
                break;
            }
        }
    }

    let codigo = if argumento_no_unicode {
        if let Err(causa) = stderr.write_all(USO.as_bytes()) {
            reportar_fallo_escritura(&mut stderr, "stderr", &causa);
        }
        2
    } else {
        ejecutar(args, &mut stdout, &mut stderr)
    };

    if let Err(causa) = stdout.flush() {
        reportar_fallo_escritura(&mut stderr, "stdout", &causa);
        let _ = stderr.flush();
        std::process::exit(1);
    }
    if stderr.flush().is_err() {
        std::process::exit(1);
    }
    std::process::exit(codigo);
}
