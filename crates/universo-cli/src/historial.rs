//! `universo log` y `universo show`: historial de procedencia de los cambios
//! aplicados y detalle de uno de ellos.
use super::{Formato, escribir_salida};
use serde_json::{Value, json};
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

pub(super) const USO_SHOW: &str = "Uso: universo show <raiz> <n> [--format text|json]\n";

pub(super) struct SolicitudShow {
    raiz: PathBuf,
    numero: usize,
    formato: Formato,
}

/// `show <raiz> <n>`: `n` es la posición 1-based en la lista de `universo log`
/// (las líneas descartadas del historial no cuentan).
pub(super) fn parsear_show(args: &[String]) -> Result<SolicitudShow, String> {
    let fail = || USO_SHOW.to_string();
    let mut posicionales = Vec::new();
    let mut formato = None;
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
        if arg.is_empty() || arg.starts_with('-') {
            return Err(fail());
        }
        posicionales.push(arg.clone());
        i += 1;
    }
    let [raiz, numero] = posicionales.as_slice() else {
        return Err(fail());
    };
    let numero: usize = numero.parse().ok().filter(|n| *n >= 1).ok_or_else(fail)?;
    Ok(SolicitudShow {
        raiz: PathBuf::from(raiz),
        numero,
        formato: formato.unwrap_or(Formato::Texto),
    })
}

/// Texto seguro para un terminal: los caracteres de control (salvo el salto de
/// línea), invisibles o bidi se muestran como `\u{..}` en vez de enviarse crudos.
fn visible(texto: &str) -> String {
    texto
        .chars()
        .map(|c| {
            if c != '\n' && (c.is_control() || universo_core::sexpr::caracter_no_imprimible(c)) {
                format!("\\u{{{:x}}}", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Muestra una entrada del historial con el delta que aplicó. **Su salida
/// incluye datos del bundle** (átomos, rutas y, al crear notas, títulos).
pub(super) fn ejecutar_show(
    s: SolicitudShow,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    let resultado = leer(&s.raiz).map_err(|e| e.codigo()).and_then(|historial| {
        let total = historial.entradas.len();
        historial
            .entradas
            .into_iter()
            .nth(s.numero - 1)
            .map(|entrada| (entrada, total))
            .ok_or("entry_not_found")
    });
    let (payload, codigo) = match (resultado, s.formato) {
        (Ok((e, total)), Formato::Json) => {
            let delta = e
                .delta
                .as_deref()
                .and_then(|d| serde_json::from_str::<Value>(d).ok());
            (
                format!(
                    "{}\n",
                    json!({"schema_version":1,"command":"show","status":"ok","entry":s.numero,"entries":total,
                           "time":e.fecha,"author":e.autor,"reason":e.motivo,"sealed":e.sellado,
                           "delta":delta,"delta_omitted":e.delta_omitido,"errors":[]})
                ),
                0,
            )
        }
        (Ok((e, total)), Formato::Texto) => {
            let mut text = format!(
                "show: entry {} of {}\ntime: {}\nauthor: {}\nreason: {}\nsealed: {}\n",
                s.numero,
                total,
                e.fecha,
                e.autor.as_deref().unwrap_or("none"),
                e.motivo.as_deref().unwrap_or("none"),
                if e.sellado { "yes" } else { "no" },
            );
            match e
                .delta
                .as_deref()
                .and_then(|d| serde_json::from_str::<Value>(d).ok())
            {
                None if e.delta_omitido => text.push_str("delta: omitted (too large)\n"),
                None => text.push_str("delta: not recorded\n"),
                Some(delta) => {
                    let lista = |clave: &str| delta[clave].as_array().cloned().unwrap_or_default();
                    for nota in lista("create") {
                        text.push_str(&format!(
                            "create {} type={} title={}\n",
                            nota["path"].as_str().unwrap_or(""),
                            nota["type"].as_str().unwrap_or(""),
                            nota["title"].as_str().unwrap_or("")
                        ));
                    }
                    for nota in lista("update") {
                        text.push_str(&format!(
                            "update body {}\n",
                            nota["path"].as_str().unwrap_or("")
                        ));
                    }
                    for atomo in lista("remove") {
                        text.push_str(&format!("- {}\n", atomo.as_str().unwrap_or("")));
                    }
                    for atomo in lista("add") {
                        text.push_str(&format!("+ {}\n", atomo.as_str().unwrap_or("")));
                    }
                }
            }
            (visible(&text), 0)
        }
        (Err(codigo), Formato::Json) => (
            format!(
                "{}\n",
                json!({"schema_version":1,"command":"show","status":"error","errors":[{"code":codigo}]})
            ),
            1,
        ),
        (Err(codigo), Formato::Texto) => (format!("show: error\ncode: {codigo}\n"), 1),
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") {
        return 1;
    }
    codigo
}
