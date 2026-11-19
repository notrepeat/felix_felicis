//! Registro opcional del uso del MCP, para `universo metricas`: una línea JSON
//! por llamada a una herramienta, con la fecha (UTC, sin hora), la
//! herramienta, el universo pedido y si terminó bien. Nunca los argumentos ni
//! la respuesta. Si no se puede escribir, el servidor sigue igual.
use serde_json::json;
use std::{io::Write, path::PathBuf, sync::OnceLock};

static ARCHIVO: OnceLock<PathBuf> = OnceLock::new();

/// Activa el registro en `ruta` (se agrega al final). Sólo la primera vez cuenta.
pub fn registrar_uso_en(ruta: PathBuf) {
    let _ = ARCHIVO.set(ruta);
}

pub(crate) fn anotar(herramienta: &str, universo: Option<&str>, ok: bool) {
    let Some(ruta) = ARCHIVO.get() else {
        return;
    };
    let linea = json!({"fecha": universo_okf::transaccion::historial::fecha_hoy(),
                       "herramienta": herramienta, "universo": universo, "ok": ok});
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ruta)
        .and_then(|mut f| f.write_all(format!("{linea}\n").as_bytes()));
}
