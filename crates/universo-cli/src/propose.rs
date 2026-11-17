//! CLI de propuesta: privacidad por defecto y token opt-in.
use super::{Formato, escribir_salida};
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf};
use universo_okf::transaccion::simulacion::{EstadoSimulacion, simular_archivo};

pub(super) const USO: &str = "Uso: universo propose <raiz> --manifiesto <m> --invariantes <i> --delta <d.json> [--format text|json] [--emit-snapshot-token | --apply --expect-snapshot <token>]\nSimula por defecto; apply requiere snapshot esperado. Usa --opcion=<ruta> si una ruta empieza con '-'.\n";
pub(super) struct Solicitud {
    raiz: PathBuf,
    manifiesto: PathBuf,
    invariantes: PathBuf,
    delta: PathBuf,
    formato: Formato,
    emit_token: bool,
    aplicar: bool,
    esperado: Option<String>,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let mut raiz = None;
    let mut manifiesto = None;
    let mut invariantes = None;
    let mut delta = None;
    let mut formato = None;
    let mut emit_token = false;
    let mut aplicar = false;
    let mut esperado = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--apply" {
            if aplicar {
                return Err(fail());
            }
            aplicar = true;
            i += 1;
            continue;
        }
        if arg == "--emit-snapshot-token" {
            if emit_token {
                return Err(fail());
            }
            emit_token = true;
            i += 1;
            continue;
        }
        if !arg.starts_with('-') {
            if arg.is_empty() || raiz.replace(PathBuf::from(arg)).is_some() {
                return Err(fail());
            }
            i += 1;
            continue;
        }
        let (name, value, consumed) = if let Some((name, value)) = arg.split_once('=') {
            (name, value, 1)
        } else {
            let value = args.get(i + 1).ok_or_else(fail)?;
            if value.starts_with('-') {
                return Err(fail());
            }
            (arg.as_str(), value.as_str(), 2)
        };
        if value.is_empty() {
            return Err(fail());
        }
        match name {
            "--manifiesto" if manifiesto.is_none() => manifiesto = Some(PathBuf::from(value)),
            "--invariantes" if invariantes.is_none() => invariantes = Some(PathBuf::from(value)),
            "--delta" if delta.is_none() => delta = Some(PathBuf::from(value)),
            "--expect-snapshot" if esperado.is_none() => esperado = Some(value.to_owned()),
            "--format" if formato.is_none() => {
                formato = Some(match value {
                    "text" => Formato::Texto,
                    "json" => Formato::Json,
                    _ => return Err(fail()),
                })
            }
            _ => return Err(fail()),
        }
        i += consumed;
    }
    if aplicar != esperado.is_some() || aplicar && emit_token {
        return Err(fail());
    }
    Ok(Solicitud {
        raiz: raiz.ok_or_else(fail)?,
        manifiesto: manifiesto.ok_or_else(fail)?,
        invariantes: invariantes.ok_or_else(fail)?,
        delta: delta.ok_or_else(fail)?,
        formato: formato.unwrap_or(Formato::Texto),
        emit_token,
        aplicar,
        esperado,
    })
}

pub(super) fn ejecutar(
    solicitud: Solicitud,
    salida: &mut impl Write,
    error: &mut impl Write,
) -> i32 {
    if solicitud.aplicar {
        return super::apply::ejecutar(
            super::apply::SolicitudApply {
                raiz: solicitud.raiz,
                manifiesto: solicitud.manifiesto,
                invariantes: solicitud.invariantes,
                delta: solicitud.delta,
                formato: solicitud.formato,
                esperado: solicitud.esperado.unwrap(),
            },
            salida,
            error,
        );
    }
    let result = simular_archivo(
        &solicitud.raiz,
        &solicitud.manifiesto,
        &solicitud.invariantes,
        &solicitud.delta,
    )
    .map_err(|e| e.codigo());
    let (status, summary, errors, token) = match result {
        Ok(result) => {
            let status = if result.estado == EstadoSimulacion::Admitida {
                "admit"
            } else {
                "reject"
            };
            let token = solicitud
                .emit_token
                .then(|| result.snapshot_token().to_hex());
            (
                status,
                json!({"conflicts": result.conflictos, "failed_invariants": result.invariantes_fallidas}),
                json!([]),
                token,
            )
        }
        Err(code) => ("error", Value::Null, json!([{"code":code}]), None),
    };
    let payload = match solicitud.formato {
        Formato::Json => {
            let mut value = json!({"schema_version":1,"command":"propose","mode":"simulate","status":status,"summary":summary,"errors":errors});
            if let Some(token) = token {
                value["snapshot_token"] = json!(token);
            }
            format!("{value}\n")
        }
        Formato::Texto => {
            let mut value = format!("propose simulate: {status}\n");
            if status == "error" {
                value.push_str(&format!("code: {}\n", errors[0]["code"].as_str().unwrap()));
            } else {
                value.push_str(&format!(
                    "conflicts: {}; failed_invariants: {}\n",
                    summary["conflicts"], summary["failed_invariants"]
                ));
            }
            if let Some(token) = token {
                value.push_str(&format!("snapshot_token: {token}\n"));
            }
            value
        }
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") || status != "admit" {
        1
    } else {
        0
    }
}
