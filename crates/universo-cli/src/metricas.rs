//! `universo metricas`: cómo se usa el repositorio de criterio. El cálculo
//! está en `universo_okf::metricas`, compartido con la herramienta `metrics`
//! del MCP.
use super::{Formato, criterio::resolver_repo_lectura, escribir_salida};
use serde_json::Value;
use std::{collections::BTreeMap, io::Write, path::PathBuf};
use universo_okf::metricas::{ESTANCADA_DIAS, dias, medir_repositorio};

pub(super) const USO: &str = "Uso: universo metricas [--criterio <dir>] [--hasta AAAA-MM-DD] [--estancada-dias <n>] [--ignorar-version] [--format text|json]\nMide el uso del repositorio de criterio: capturas, refuerzos, objeciones y promociones (agente o dueño, por semana), precisión de lo que capturó el agente, días hasta la promoción, candidatas estancadas y, si está activado, el uso del MCP.\n";

pub(super) struct Solicitud {
    criterio: Option<PathBuf>,
    hasta: Option<String>,
    estancada_dias: i64,
    ignorar_version: bool,
    formato: Formato,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let mut valores: BTreeMap<&str, String> = BTreeMap::new();
    let mut ignorar_version = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--ignorar-version" {
            if std::mem::replace(&mut ignorar_version, true) {
                return Err(fail());
            }
            i += 1;
            continue;
        }
        let (nombre, valor, consumidos) = match arg.split_once('=') {
            Some((n, v)) => (n, v.to_string(), 1),
            None => (arg.as_str(), args.get(i + 1).cloned().ok_or_else(fail)?, 2),
        };
        let clave = match nombre {
            "--criterio" => "criterio",
            "--hasta" => "hasta",
            "--estancada-dias" => "estancada",
            "--format" => "format",
            _ => return Err(fail()),
        };
        if valor.is_empty() || valor.starts_with('-') || valores.insert(clave, valor).is_some() {
            return Err(fail());
        }
        i += consumidos;
    }
    let hasta = valores.remove("hasta");
    if hasta.as_deref().is_some_and(|h| dias(h).is_none()) {
        return Err(fail());
    }
    let estancada_dias = match valores.remove("estancada") {
        Some(v) => v.parse::<i64>().ok().filter(|n| *n >= 0).ok_or_else(fail)?,
        None => ESTANCADA_DIAS,
    };
    let formato = match valores.remove("format").as_deref() {
        None | Some("text") => Formato::Texto,
        Some("json") => Formato::Json,
        Some(_) => return Err(fail()),
    };
    Ok(Solicitud {
        criterio: valores.remove("criterio").map(PathBuf::from),
        hasta,
        estancada_dias,
        ignorar_version,
        formato,
    })
}

/// Días desde 1970-01-01 de una fecha `AAAA-MM-DD` (algoritmo de Howard
/// Hinnant), o `None` si no es una fecha válida.
fn texto(m: &Value) -> String {
    let mut t = format!("metricas: hasta {}\n", m["hasta"].as_str().unwrap_or(""));
    for u in m["universos"].as_array().into_iter().flatten() {
        t.push_str(&format!(
            "universo {}: {} notas, {} cambios\n",
            u["nombre"].as_str().unwrap_or(""),
            u["notas"],
            u["cambios"]
        ));
    }
    for (nombre, v) in m["actividad"].as_object().into_iter().flatten() {
        t.push_str(&format!(
            "{nombre}: {} (agente {}, dueño {})\n",
            v["total"], v["agente"], v["dueno"]
        ));
    }
    let p = &m["precision_agente"];
    let precision = p["precision"]
        .as_f64()
        .map_or("sin datos".to_string(), |x| format!("{:.0} %", x * 100.0));
    t.push_str(&format!(
        "precisión del agente: {precision} ({} confirmadas, {} objetadas, {} retiradas, {} pendientes de {} capturas)\n",
        p["confirmadas"], p["objetadas"], p["retiradas"], p["pendientes"], p["capturas"]
    ));
    let mad = &m["maduracion"];
    t.push_str(&format!(
        "maduración: {} promovidas; mediana {} días\n",
        mad["promovidas"],
        mad["dias_mediana"]
            .as_i64()
            .map_or("-".into(), |d| d.to_string())
    ));
    let est = &m["estancadas"];
    let notas: Vec<&str> = est["notas"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    t.push_str(&format!(
        "estancadas (más de {} días sin segundo proyecto): {}\n",
        est["dias"],
        if notas.is_empty() {
            "ninguna".to_string()
        } else {
            notas.join(", ")
        }
    ));
    let u = &m["uso_mcp"];
    if u["estado"] == "medido" {
        t.push_str(&format!(
            "uso del MCP: {} llamadas, {} con error\n",
            u["llamadas"], u["errores"]
        ));
    } else if u["estado"] == "sin_llamadas" {
        t.push_str("uso del MCP: activado, sin llamadas todavía\n");
    } else {
        t.push_str("uso del MCP: no medido (universo conectar --aplicar --medir-uso lo activa)\n");
    }
    t
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let repo = match resolver_repo_lectura(s.criterio, s.ignorar_version) {
        Ok(repo) => repo,
        Err(e) => {
            let texto = format!("metricas: error\ncode: {}\n{e}\n", e.codigo());
            escribir_salida(salida, error, texto.as_bytes(), "stdout");
            return 1;
        }
    };
    let m = match medir_repositorio(&repo, s.hasta.as_deref(), s.estancada_dias, true) {
        Ok(m) => m,
        Err(codigo) => {
            let texto = format!("metricas: error\ncode: {codigo}\n");
            escribir_salida(salida, error, texto.as_bytes(), "stdout");
            return 1;
        }
    };
    let cuerpo = match s.formato {
        Formato::Json => format!("{m}\n"),
        Formato::Texto => texto(&m),
    };
    if escribir_salida(salida, error, cuerpo.as_bytes(), "stdout") {
        0
    } else {
        1
    }
}
