//! `universo reglas aplicar <archivo>` y `universo manifiesto aplicar
//! <archivo>` (fase 10, U5): cambian las reglas o el manifiesto de un universo
//! sólo si el bundle actual cumple la configuración nueva, y lo dejan en el
//! historial (`kind: "config"`) para que `diff` lo cruce sin ver un hueco.
//! El texto muestra id y mensaje de las reglas que fallan: vienen del archivo
//! nuevo, no del bundle.
use super::{Formato, escribir_salida, historial::visible};
use serde_json::json;
use std::{
    io::{Read, Write},
    path::PathBuf,
};
use universo_okf::transaccion::{
    cambio_config::{CambioConfig, ResultadoConfig, aplicar_config},
    historial::Procedencia,
};

pub(super) const USO: &str = "Uso:\n  universo reglas aplicar <archivo.sx> [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]\n  universo manifiesto aplicar <archivo.toml> [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]\nFuera de un repositorio de criterio: universo reglas|manifiesto <raiz> aplicar <archivo> --manifiesto <m> --invariantes <i> ...\nInstala el archivo sólo si el bundle actual lo cumple; si no, muestra las reglas que fallan y no cambia nada.\n";

/// Tamaño máximo del archivo de configuración nuevo.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Que {
    Reglas,
    Manifiesto,
}

pub(super) struct Solicitud {
    que: Que,
    archivo: PathBuf,
    raiz: PathBuf,
    manifiesto: PathBuf,
    invariantes: PathBuf,
    autor: Option<String>,
    motivo: Option<String>,
    dry_run: bool,
    formato: Formato,
}

impl Solicitud {
    fn comando(&self) -> &'static str {
        match self.que {
            Que::Reglas => "reglas aplicar",
            Que::Manifiesto => "manifiesto aplicar",
        }
    }
}

/// `comando` es `reglas` o `manifiesto`.
pub(super) fn parsear(comando: &str, args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let que = match comando {
        "reglas" => Que::Reglas,
        "manifiesto" => Que::Manifiesto,
        _ => return Err(fail()),
    };
    let mut posicionales = Vec::new();
    let mut valores = std::collections::BTreeMap::new();
    let mut dry_run = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--dry-run" {
            if std::mem::replace(&mut dry_run, true) {
                return Err(fail());
            }
            i += 1;
            continue;
        }
        if !arg.starts_with('-') {
            posicionales.push(arg.clone());
            i += 1;
            continue;
        }
        let (nombre, valor, consumidos) = match arg.split_once('=') {
            Some((n, v)) => (n, v.to_string(), 1),
            None => (arg.as_str(), args.get(i + 1).cloned().ok_or_else(fail)?, 2),
        };
        let clave = match nombre {
            "--manifiesto" => "manifiesto",
            "--invariantes" => "invariantes",
            "--autor" => "autor",
            "--motivo" => "motivo",
            "--format" => "format",
            _ => return Err(fail()),
        };
        if valor.is_empty() || valores.insert(clave, valor).is_some() {
            return Err(fail());
        }
        i += consumidos;
    }
    let (raiz, archivo) = match posicionales.as_slice() {
        [raiz, sub, archivo] if sub == "aplicar" && !archivo.is_empty() => {
            (PathBuf::from(raiz), PathBuf::from(archivo))
        }
        _ => return Err(fail()),
    };
    let formato = match valores.remove("format").as_deref() {
        None | Some("text") => Formato::Texto,
        Some("json") => Formato::Json,
        Some(_) => return Err(fail()),
    };
    Ok(Solicitud {
        que,
        archivo,
        raiz,
        manifiesto: PathBuf::from(valores.remove("manifiesto").ok_or_else(fail)?),
        invariantes: PathBuf::from(valores.remove("invariantes").ok_or_else(fail)?),
        autor: valores.remove("autor"),
        motivo: valores.remove("motivo"),
        dry_run,
        formato,
    })
}

fn leer_archivo(ruta: &std::path::Path) -> Result<String, &'static str> {
    let archivo = std::fs::File::open(ruta).map_err(|_| "config_file_unreadable")?;
    if !archivo
        .metadata()
        .map_err(|_| "config_file_unreadable")?
        .is_file()
    {
        return Err("config_file_unreadable");
    }
    let mut bytes = Vec::new();
    archivo
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "config_file_unreadable")?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err("config_file_too_large");
    }
    String::from_utf8(bytes).map_err(|_| "config_file_not_utf8")
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let comando = s.comando();
    let (texto, codigo) = match resolver(&s) {
        Ok(resultado) => informe(comando, &resultado, s.dry_run, s.formato),
        Err(code) => (
            match s.formato {
                Formato::Json => format!(
                    "{}\n",
                    json!({"schema_version":1,"command":comando,"status":"error",
                           "errors":[{"code":code}]})
                ),
                Formato::Texto if code == "config_seal_mismatch" => format!(
                    "{comando}: error\ncode: {code}\nnote: el estado actual no es el último sellado (o no hay sello). Revísalo con `universo guard` y, si es correcto, acéptalo con `universo seal --motivo <m>` antes de cambiar la configuración.\n"
                ),
                Formato::Texto => format!("{comando}: error\ncode: {code}\n"),
            },
            1,
        ),
    };
    if escribir_salida(salida, error, texto.as_bytes(), "stdout") {
        codigo
    } else {
        1
    }
}

fn resolver(s: &Solicitud) -> Result<ResultadoConfig, &'static str> {
    let contenido = leer_archivo(&s.archivo)?;
    let cambio = match s.que {
        Que::Reglas => CambioConfig {
            manifiesto: None,
            invariantes: Some(&contenido),
        },
        Que::Manifiesto => CambioConfig {
            manifiesto: Some(&contenido),
            invariantes: None,
        },
    };
    let procedencia = Procedencia {
        autor: s.autor.clone(),
        motivo: s.motivo.clone(),
    };
    aplicar_config(
        &s.raiz,
        &s.manifiesto,
        &s.invariantes,
        &cambio,
        &procedencia,
        s.dry_run,
    )
    .map_err(|e| e.0)
}

fn informe(
    comando: &str,
    resultado: &ResultadoConfig,
    dry_run: bool,
    formato: Formato,
) -> (String, i32) {
    match resultado {
        ResultadoConfig::Admitido {
            aplicado,
            sellado,
            registrado,
        } => {
            let status = if *aplicado { "applied" } else { "admit" };
            let texto = match formato {
                Formato::Json => format!(
                    "{}\n",
                    json!({"schema_version":1,"command":comando,"status":status,
                           "dry_run":dry_run,"sealed":sellado,"recorded":registrado,"errors":[]})
                ),
                Formato::Texto if !aplicado => format!(
                    "{comando}: admit\nel bundle actual cumple la configuración nueva; con --dry-run no se escribió nada.\n"
                ),
                Formato::Texto => {
                    let mut t =
                        format!("{comando}: applied\nsealed: {sellado}\nrecorded: {registrado}\n");
                    if !sellado {
                        t.push_str("note: la configuración quedó instalada pero no se pudo sellar; revisa con `universo guard` y, si es correcto, `universo seal`.\n");
                    }
                    if !registrado {
                        t.push_str("note: no se pudo escribir el historial; `universo diff` no podrá cruzar este cambio.\n");
                    }
                    t
                }
            };
            (texto, 0)
        }
        ResultadoConfig::Rechazado { reglas, conflictos } => {
            let texto = match formato {
                Formato::Json => {
                    let fallas: Vec<_> = reglas
                        .iter()
                        .map(|(id, mensaje)| json!({"rule_id":id,"rule_message":mensaje}))
                        .collect();
                    format!(
                        "{}\n",
                        json!({"schema_version":1,"command":comando,"status":"reject",
                               "dry_run":dry_run,"failed_rules":fallas,"conflicts":conflictos,
                               "errors":[]})
                    )
                }
                Formato::Texto => {
                    let mut t = format!(
                        "{comando}: reject\nel bundle actual no cumple la configuración nueva; no se cambió nada.\n"
                    );
                    for (id, mensaje) in reglas {
                        t.push_str(&format!(
                            "  rule: {} — {}\n",
                            id.as_deref().unwrap_or("(sin id)"),
                            mensaje.as_deref().unwrap_or("(sin mensaje)")
                        ));
                    }
                    if *conflictos > 0 {
                        t.push_str(&format!("  conflicts: {conflictos}\n"));
                    }
                    visible(&t)
                }
            };
            (texto, 1)
        }
    }
}
