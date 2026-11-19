//! Herramienta `metrics`: las métricas de uso del repositorio de criterio
//! (`universo_okf::metricas`), sólo cuando el servidor corre con `--criterio`.
//! Por defecto devuelve conteos; las rutas de las candidatas estancadas son
//! datos del repositorio y sólo salen con `include_values: true`.
use serde_json::{Map, Value, json};
use std::{path::PathBuf, sync::OnceLock};

static REPO: OnceLock<PathBuf> = OnceLock::new();
const MAX_DIAS: u64 = 3650;

/// Activa `metrics` sobre el repositorio de criterio en `raiz`.
pub fn usar_repositorio(raiz: PathBuf) {
    let _ = REPO.set(raiz);
}

pub(crate) fn activa() -> bool {
    REPO.get().is_some()
}

pub(crate) fn definicion() -> Value {
    json!({"name": "metrics",
        "description": "Usage metrics of the owner's criterio repository: captures, reinforcements, objections and promotions by author (agent or owner) and week; precision of what agents captured; days to promotion; stale candidates; and MCP usage if enabled. Returns counts and the universe names of felix.toml; include_values:true adds the paths of stale candidates. Use it when the owner asks how their criterio is doing.",
        "inputSchema": {"type": "object", "properties": {
            "schema": {"type": "string", "const": "felix-mcp-v1"},
            "include_values": {"type": "boolean", "default": false},
            "stale_days": {"type": "integer", "minimum": 0, "maximum": MAX_DIAS, "default": 30},
            "until": {"type": "string", "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$"}},
            "required": ["schema"], "additionalProperties": false},
        "annotations": {"readOnlyHint": true, "destructiveHint": false,
            "idempotentHint": true, "openWorldHint": false}})
}

/// (respuesta, es error). Corre en un hilo de bloqueo: lee el repositorio.
pub(crate) fn ejecutar(argumentos: &Map<String, Value>) -> (Value, bool) {
    let error = |codigo: &str| {
        (
            json!({"schema": "felix-mcp-v1", "status": "error", "code": codigo}),
            true,
        )
    };
    let Some(raiz) = REPO.get() else {
        return error("unknown_tool");
    };
    let permitidos = ["schema", "include_values", "stale_days", "until"];
    if argumentos.get("schema").and_then(Value::as_str) != Some("felix-mcp-v1")
        || argumentos.keys().any(|k| !permitidos.contains(&k.as_str()))
    {
        return error("invalid_tool_arguments");
    }
    let incluir = match argumentos.get("include_values") {
        None => false,
        Some(v) => match v.as_bool() {
            Some(b) => b,
            None => return error("invalid_tool_arguments"),
        },
    };
    let dias = match argumentos.get("stale_days") {
        None => universo_okf::metricas::ESTANCADA_DIAS,
        Some(v) => match v.as_u64().filter(|d| *d <= MAX_DIAS) {
            Some(d) => d as i64,
            None => return error("invalid_tool_arguments"),
        },
    };
    let hasta = match argumentos.get("until") {
        None => None,
        Some(v) => match v
            .as_str()
            .filter(|h| universo_okf::metricas::dias(h).is_some())
        {
            Some(h) => Some(h),
            None => return error("invalid_tool_arguments"),
        },
    };
    let repo = match universo_okf::criterio::RepoCriterio::cargar(raiz) {
        Ok(repo) => repo,
        Err(e) => return error(e.codigo()),
    };
    match universo_okf::metricas::medir_repositorio(&repo, hasta, dias, incluir) {
        Ok(m) => (
            json!({"schema": "felix-mcp-v1", "status": "ok", "metrics": m}),
            false,
        ),
        Err(codigo) => error(&codigo),
    }
}
