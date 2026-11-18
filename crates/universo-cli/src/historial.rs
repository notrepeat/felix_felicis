//! `universo log`: historial de procedencia de los cambios aplicados.
use super::{Formato, escribir_salida};
use serde_json::json;
use std::{io::Write, path::PathBuf};
use universo_okf::transaccion::historial::leer;

pub(super) const USO: &str = "Uso: universo log <raiz> [--format text|json]\n";

pub(super) struct Solicitud {
    raiz: PathBuf,
    formato: Formato,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let (mut raiz, mut formato) = (None, None);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let valor = if let Some(v) = arg.strip_prefix("--format=") {
            i += 1;
            Some(v)
        } else if arg == "--format" {
            i += 2;
            Some(args.get(i - 1).ok_or_else(fail)?.as_str())
        } else {
            None
        };
        if let Some(valor) = valor {
            let nuevo = match valor {
                "text" => Formato::Texto,
                "json" => Formato::Json,
                _ => return Err(fail()),
            };
            if formato.replace(nuevo).is_some() {
                return Err(fail());
            }
            continue;
        }
        if arg.is_empty() || arg.starts_with('-') || raiz.replace(PathBuf::from(arg)).is_some() {
            return Err(fail());
        }
        i += 1;
    }
    Ok(Solicitud {
        raiz: raiz.ok_or_else(fail)?,
        formato: formato.unwrap_or(Formato::Texto),
    })
}

/// Muestra autor, motivo, fecha y sello de cada cambio; nunca el token.
pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let (payload, codigo) = match (leer(&s.raiz), s.formato) {
        (Ok(historial), Formato::Json) => {
            let entries: Vec<_> = historial
                .entradas
                .iter()
                .map(|e| json!({"time": e.fecha, "author": e.autor, "reason": e.motivo, "sealed": e.sellado}))
                .collect();
            (
                format!(
                    "{}\n",
                    json!({"schema_version":1,"command":"log","status":"ok","entries":entries,"discarded_lines":historial.descartadas,"errors":[]})
                ),
                0,
            )
        }
        (Ok(historial), Formato::Texto) => {
            let mut text = format!(
                "log: {} entries; {} discarded lines\n",
                historial.entradas.len(),
                historial.descartadas
            );
            for e in &historial.entradas {
                text.push_str(&format!(
                    "{} author={} sealed={} reason={}\n",
                    e.fecha,
                    e.autor.as_deref().unwrap_or("none"),
                    if e.sellado { "yes" } else { "no" },
                    e.motivo.as_deref().unwrap_or("none"),
                ));
            }
            (text, 0)
        }
        (Err(e), Formato::Json) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":"log","status":"error","errors":[{"code":e.codigo()}]})
            ),
            1,
        ),
        (Err(e), Formato::Texto) => (format!("log: error\ncode: {}\n", e.codigo()), 1),
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") {
        return 1;
    }
    codigo
}
