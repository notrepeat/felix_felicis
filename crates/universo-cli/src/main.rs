//! CLI del motor Universo de Conocimiento: comandos de lectura.
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use universo_core::invariante::expansion::{ErrorExpansion, expandir_sentencias};
use universo_core::invariante::validar_sentencia_fuente;
use universo_core::sexpr::{ErrorAutoria, SentenciaFuente, parsear_programa};
use universo_core::{ErrorEvaluacion, Manifiesto, evaluar};
use universo_okf::{ErrorBundle, ErrorManifiestoArchivo, Regla, cargar_bundle, cargar_manifiesto};

/// Uso breve, mostrado por errores de argumentos en `stderr`.
const USO: &str = "Uso:\n  universo atomos <raiz> --manifiesto <manifiesto.toml>\n  universo validate <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]\n  universo verificar <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]  (alias de validate)\n  universo check <raiz> --manifiesto <manifiesto.toml> --invariantes <archivo> [--format <text|json>]\nUse --manifiesto=<ruta> y --invariantes=<ruta> si una ruta empieza con '-'.\n";

/// Ayuda completa, mostrada en `stdout` con código de salida 0.
const AYUDA: &str = "CLI de Felix Felicis\n\nUso:\n  universo atomos <raiz> --manifiesto <manifiesto.toml>\n  universo validate <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]\n  universo verificar <raiz> --manifiesto <manifiesto.toml> [--format <text|json>]\n  universo check <raiz> --manifiesto <manifiesto.toml> --invariantes <archivo> [--format <text|json>]\n\nComandos:\n  atomos     Imprime los átomos del bundle en orden canónico.\n  validate   Valida el manifiesto, el formato OKF y la estructura del bundle.\n  verificar  Alias compatible de validate.\n  check      Evalúa el programa de invariantes sobre el bundle.\n\nOpciones:\n  --manifiesto <ruta> | --manifiesto=<ruta>  Manifiesto TOML requerido.\n  --invariantes <ruta> | --invariantes=<ruta> Programa de invariantes requerido por check.\n  --format text|json                         Formato de validate/check (por defecto: text).\n  -h, --help                                 Muestra esta ayuda y termina con código 0.\n\nSi la ruta de un archivo empieza con `-`, usa `--opcion=<ruta>`.\nSi la raíz posicional empieza con `-`, antepone `./` (por ejemplo, `./-bundle`).\nvalidate es una comprobación estructural; no evalúa invariantes.\ncheck informa admit/reject/error y no incluye testigos ni contenido del bundle.\n\nCódigos de salida:\n  0  Bundle válido, check admit o ayuda mostrada.\n  1  Bundle inválido, check reject/error o fallo de escritura.\n  2  Uso inválido.\n";

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
    Check {
        raiz: PathBuf,
        manifiesto: PathBuf,
        invariantes: PathBuf,
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
        "check" => 2,
        "-h" | "--help" => return Ok(Comando::Ayuda),
        _ => return Err(USO.to_string()),
    };

    let mut raiz: Option<PathBuf> = None;
    let mut manifiesto: Option<PathBuf> = None;
    let mut invariantes: Option<PathBuf> = None;
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
        } else if let Some(valor) = arg.strip_prefix("--invariantes=") {
            if tipo_comando != 2 || invariantes.is_some() || valor.is_empty() {
                return Err(USO.to_string());
            }
            invariantes = Some(PathBuf::from(valor));
            i += 1;
        } else if arg == "--invariantes" {
            let valor = resto.get(i + 1).ok_or_else(|| USO.to_string())?;
            if tipo_comando != 2
                || invariantes.is_some()
                || valor.is_empty()
                || valor.starts_with('-')
            {
                return Err(USO.to_string());
            }
            invariantes = Some(PathBuf::from(valor));
            i += 2;
        } else if let Some(valor) = arg.strip_prefix("--format=") {
            if tipo_comando == 0 || formato.is_some() {
                return Err(USO.to_string());
            }
            formato = Some(parsear_formato(valor)?);
            i += 1;
        } else if arg == "--format" {
            let valor = resto.get(i + 1).ok_or_else(|| USO.to_string())?;
            if tipo_comando == 0 || formato.is_some() || valor.starts_with('-') {
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
    match tipo_comando {
        0 => Ok(Comando::Atomos { raiz, manifiesto }),
        1 => Ok(Comando::Validate {
            raiz,
            manifiesto,
            formato: formato.unwrap_or(Formato::Texto),
        }),
        _ => Ok(Comando::Check {
            raiz,
            manifiesto,
            invariantes: invariantes.ok_or_else(|| USO.to_string())?,
            formato: formato.unwrap_or(Formato::Texto),
        }),
    }
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

struct DiagnosticoCheck {
    kind: &'static str,
    code: String,
    message: &'static str,
    path: Option<String>,
    line: Option<usize>,
    column: Option<usize>,
    location: Option<String>,
}

impl DiagnosticoCheck {
    fn nuevo(
        kind: &'static str,
        code: impl Into<String>,
        message: &'static str,
        path: Option<String>,
        line: Option<usize>,
        column: Option<usize>,
        location: Option<String>,
    ) -> Self {
        Self {
            kind,
            code: code.into(),
            message,
            path,
            line,
            column,
            location,
        }
    }

    fn a_json(&self) -> Value {
        json!({
            "kind": self.kind,
            "code": self.code,
            "message": self.message,
            "path": self.path,
            "line": self.line,
            "column": self.column,
            "location": self.location,
        })
    }

    fn a_texto(&self) -> String {
        let mut ubicacion = self.path.as_deref().unwrap_or("check").to_string();
        if let Some(linea) = self.line {
            ubicacion.push_str(&format!(":{linea}"));
            if let Some(columna) = self.column {
                ubicacion.push_str(&format!(":{columna}"));
            }
        }
        format!("{ubicacion}: {}: {}\n", self.code, self.message)
    }
}

fn resultado_json_check(
    status: &str,
    summary: Option<Value>,
    errors: &[DiagnosticoCheck],
) -> Value {
    json!({
        "schema_version": 1,
        "command": "check",
        "status": status,
        "summary": summary,
        "errors": errors.iter().map(DiagnosticoCheck::a_json).collect::<Vec<_>>(),
    })
}

fn resumen_check(sources: usize, instances: usize, evaluated: usize, failed: usize) -> Value {
    json!({
        "sources": sources,
        "instances": instances,
        "evaluated": evaluated,
        "failed": failed,
    })
}

fn escribir_resultado_check(
    status: &str,
    summary: Option<Value>,
    errors: &[DiagnosticoCheck],
    formato: Formato,
    codigo_salida: i32,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    if formato == Formato::Json {
        let mut bytes = match serde_json::to_vec(&resultado_json_check(status, summary, errors)) {
            Ok(bytes) => bytes,
            Err(causa) => {
                let causa = io::Error::other(causa);
                reportar_fallo_escritura(error, "JSON", &causa);
                return 1;
            }
        };
        bytes.push(b'\n');
        return if escribir_salida(salida, error, &bytes, "stdout") {
            codigo_salida
        } else {
            1
        };
    }

    if status == "error" {
        let bytes: String = errors.iter().map(DiagnosticoCheck::a_texto).collect();
        if let Err(causa) = error.write_all(bytes.as_bytes()) {
            reportar_fallo_escritura(error, "stderr", &causa);
        }
        return codigo_salida;
    }

    let summary = summary.expect("admit/reject siempre tiene resumen");
    let sources = summary["sources"].as_u64().unwrap_or_default();
    let instances = summary["instances"].as_u64().unwrap_or_default();
    let evaluated = summary["evaluated"].as_u64().unwrap_or_default();
    let failed = summary["failed"].as_u64().unwrap_or_default();
    let mensaje = format!(
        "{}: {sources} fuentes, {instances} instancias, {evaluated} evaluadas, {failed} fallidas\n",
        status.to_ascii_uppercase()
    );
    if escribir_salida(salida, error, mensaje.as_bytes(), "stdout") {
        codigo_salida
    } else {
        1
    }
}

fn escribir_error_check(
    errores: &[DiagnosticoCheck],
    formato: Formato,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    escribir_resultado_check("error", None, errores, formato, 1, salida, error)
}

fn error_manifiesto_check(ruta: &Path, error: &ErrorManifiestoArchivo) -> DiagnosticoCheck {
    let (kind, code, message) = match error {
        ErrorManifiestoArchivo::Io(_) => {
            ("resource", "manifest_io", "no se pudo leer el manifiesto")
        }
        ErrorManifiestoArchivo::Sintaxis(_) => {
            ("data", "manifest_syntax", "el manifiesto no es válido")
        }
        ErrorManifiestoArchivo::Validacion(_) => {
            ("data", "manifest_invalid", "el manifiesto no es válido")
        }
    };
    DiagnosticoCheck::nuevo(
        kind,
        code,
        message,
        Some(ruta.display().to_string()),
        None,
        None,
        None,
    )
}

fn errores_bundle_check(errores: &[ErrorBundle]) -> Vec<DiagnosticoCheck> {
    errores
        .iter()
        .map(|error| {
            DiagnosticoCheck::nuevo(
                if error.regla == Regla::Io {
                    "resource"
                } else {
                    "data"
                },
                error.regla.to_string(),
                "el bundle no pudo cargarse como estructura válida",
                error.path.clone(),
                error.linea,
                None,
                None,
            )
        })
        .collect()
}

fn error_autoria_check(ruta: &Path, error: &ErrorAutoria) -> DiagnosticoCheck {
    DiagnosticoCheck::nuevo(
        "authorship",
        "invariant_syntax",
        "el programa de invariantes no cumple la sintaxis",
        Some(ruta.display().to_string()),
        Some(error.linea),
        Some(error.columna),
        None,
    )
}

fn errores_validacion_check(
    ruta: &Path,
    fuentes: &[SentenciaFuente],
    manifiesto: &Manifiesto,
    estructura: &universo_core::Estructura,
) -> Vec<DiagnosticoCheck> {
    let mut errores = Vec::new();
    for fuente in fuentes {
        for error in validar_sentencia_fuente(fuente, manifiesto, estructura) {
            errores.push(DiagnosticoCheck::nuevo(
                "authorship",
                error.codigo.as_str(),
                "una sentencia no pasa la validación estática",
                Some(ruta.display().to_string()),
                error.linea,
                error.columna,
                Some(error.ubicacion),
            ));
        }
    }
    errores
}

fn errores_expansion_check(
    ruta: &Path,
    fuentes: &[SentenciaFuente],
    error: ErrorExpansion,
) -> Vec<DiagnosticoCheck> {
    match error {
        ErrorExpansion::SentenciaInvalida { indice, errores } => errores
            .into_iter()
            .map(|error| {
                DiagnosticoCheck::nuevo(
                    "authorship",
                    error.codigo.as_str(),
                    "una sentencia no pasa la validación estática",
                    Some(ruta.display().to_string()),
                    fuentes.get(indice).map(|fuente| fuente.linea),
                    fuentes.get(indice).map(|fuente| fuente.columna),
                    Some(error.ubicacion),
                )
            })
            .collect(),
        ErrorExpansion::DominioPredicadoDesconocido(_) => vec![DiagnosticoCheck::nuevo(
            "authorship",
            "schema_domain_unknown",
            "un esquema refiere a un dominio no disponible",
            Some(ruta.display().to_string()),
            None,
            None,
            None,
        )],
        ErrorExpansion::PresupuestoAgotado { .. } => vec![DiagnosticoCheck::nuevo(
            "resource",
            "expansion_budget_exceeded",
            "se agotó el presupuesto de expansión",
            Some(ruta.display().to_string()),
            None,
            None,
            None,
        )],
    }
}

fn error_evaluacion_check(ruta: &Path, error: ErrorEvaluacion) -> DiagnosticoCheck {
    let (kind, code, message) = match error {
        ErrorEvaluacion::PresupuestoAgotado { .. } => (
            "resource",
            "evaluation_budget_exceeded",
            "se agotó el presupuesto de evaluación",
        ),
        _ => (
            "authorship",
            "evaluation_invalid",
            "una fórmula no puede evaluarse en esta estructura",
        ),
    };
    DiagnosticoCheck::nuevo(
        kind,
        code,
        message,
        Some(ruta.display().to_string()),
        None,
        None,
        None,
    )
}

/// Ejecuta el pipeline cerrado de `check`. Ningún error de carga, autoría o
/// presupuesto se convierte en un veredicto lógico `reject`.
fn ejecutar_check(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    formato: Formato,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    let manifiesto_cargado = match cargar_manifiesto(manifiesto) {
        Ok(manifiesto) => manifiesto,
        Err(error_manifiesto) => {
            let diagnostico = error_manifiesto_check(manifiesto, &error_manifiesto);
            return escribir_error_check(&[diagnostico], formato, salida, error);
        }
    };
    let estructura = match cargar_bundle(raiz, manifiesto_cargado) {
        Ok(estructura) => estructura,
        Err(errores) => {
            let diagnosticos = errores_bundle_check(&errores);
            return escribir_error_check(&diagnosticos, formato, salida, error);
        }
    };

    let bytes = match fs::read(invariantes) {
        Ok(bytes) => bytes,
        Err(_) => {
            let diagnostico = DiagnosticoCheck::nuevo(
                "resource",
                "invariants_io",
                "no se pudo leer el archivo de invariantes",
                Some(invariantes.display().to_string()),
                None,
                None,
                None,
            );
            return escribir_error_check(&[diagnostico], formato, salida, error);
        }
    };
    let texto = match String::from_utf8(bytes) {
        Ok(texto) => texto,
        Err(_) => {
            let diagnostico = DiagnosticoCheck::nuevo(
                "authorship",
                "invariants_not_utf8",
                "el archivo de invariantes no es UTF-8 válido",
                Some(invariantes.display().to_string()),
                None,
                None,
                None,
            );
            return escribir_error_check(&[diagnostico], formato, salida, error);
        }
    };
    let fuentes = match parsear_programa(&texto) {
        Ok(fuentes) => fuentes,
        Err(error_autoria) => {
            let diagnostico = error_autoria_check(invariantes, &error_autoria);
            return escribir_error_check(&[diagnostico], formato, salida, error);
        }
    };

    let diagnosticos =
        errores_validacion_check(invariantes, &fuentes, estructura.manifiesto(), &estructura);
    if !diagnosticos.is_empty() {
        return escribir_error_check(&diagnosticos, formato, salida, error);
    }

    let sentencias: Vec<_> = fuentes
        .iter()
        .map(|fuente| fuente.sentencia.clone())
        .collect();
    let expansion = match expandir_sentencias(&sentencias, estructura.manifiesto(), &estructura) {
        Ok(expansion) => expansion,
        Err(error_expansion) => {
            let diagnosticos = errores_expansion_check(invariantes, &fuentes, error_expansion);
            return escribir_error_check(&diagnosticos, formato, salida, error);
        }
    };

    let mut evaluadas = 0;
    let mut fallidas = 0;
    for formula in &expansion.oraciones {
        match evaluar(formula, &estructura) {
            Ok(true) => evaluadas += 1,
            Ok(false) => {
                evaluadas += 1;
                fallidas += 1;
            }
            Err(error_evaluacion) => {
                let diagnostico = error_evaluacion_check(invariantes, error_evaluacion);
                return escribir_error_check(&[diagnostico], formato, salida, error);
            }
        }
    }
    let resumen = resumen_check(
        fuentes.len(),
        expansion.oraciones.len(),
        evaluadas,
        fallidas,
    );
    if fallidas == 0 {
        escribir_resultado_check("admit", Some(resumen), &[], formato, 0, salida, error)
    } else {
        escribir_resultado_check("reject", Some(resumen), &[], formato, 1, salida, error)
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
        Comando::Check {
            raiz,
            manifiesto,
            invariantes,
            formato,
        } => ejecutar_check(&raiz, &manifiesto, &invariantes, formato, salida, error),
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
