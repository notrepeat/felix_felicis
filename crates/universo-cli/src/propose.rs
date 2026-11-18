//! CLI de propuesta: privacidad por defecto y token opt-in.
use super::{Formato, escribir_salida};
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf};
use universo_core::{ModoExplicacion, PRESUPUESTO_EXPLICACION_POR_DEFECTO};
use universo_okf::transaccion::simulacion::{
    EstadoSimulacion, simular_archivo, simular_archivo_explicado,
};

pub(super) const USO: &str = "Uso: universo propose <raiz> --manifiesto <m> --invariantes <i> --delta <d.json> [--format text|json] [--explain decide|enumerate [--rules]] [--emit-snapshot-token | --apply --expect-snapshot <token> [--author <a>] [--reason <r>]]\nSimula por defecto; apply requiere snapshot esperado. Usa --opcion=<ruta> si una ruta empieza con '-'.\n";
pub(super) struct Solicitud {
    raiz: PathBuf,
    manifiesto: PathBuf,
    invariantes: PathBuf,
    delta: PathBuf,
    formato: Formato,
    emit_token: bool,
    aplicar: bool,
    esperado: Option<String>,
    explicar: Option<ModoExplicacion>,
    con_reglas: bool,
    autor: Option<String>,
    motivo: Option<String>,
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
    let mut explicar = None;
    let mut con_reglas = false;
    let mut autor = None;
    let mut motivo = None;
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
        if arg == "--rules" {
            if con_reglas {
                return Err(fail());
            }
            con_reglas = true;
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
            "--author" if autor.is_none() => autor = Some(value.to_owned()),
            "--reason" if motivo.is_none() => motivo = Some(value.to_owned()),
            "--explain" if explicar.is_none() => {
                explicar = Some(match value {
                    "decide" => ModoExplicacion::Decidir,
                    "enumerate" => ModoExplicacion::enumerar_por_defecto(),
                    _ => return Err(fail()),
                })
            }
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
    // --explain sólo simula: un rechazo de apply se explica simulando el mismo delta.
    // --author/--reason describen un cambio publicado: sólo con --apply.
    if con_reglas && explicar.is_none() {
        return Err(fail());
    }
    if aplicar != esperado.is_some()
        || aplicar && (emit_token || explicar.is_some())
        || !aplicar && (autor.is_some() || motivo.is_some())
    {
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
        explicar,
        con_reglas,
        autor,
        motivo,
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
                autor: solicitud.autor,
                motivo: solicitud.motivo,
            },
            salida,
            error,
        );
    }
    let mut explicacion = None;
    let result = match solicitud.explicar {
        None => simular_archivo(
            &solicitud.raiz,
            &solicitud.manifiesto,
            &solicitud.invariantes,
            &solicitud.delta,
        ),
        Some(modo) => simular_archivo_explicado(
            &solicitud.raiz,
            &solicitud.manifiesto,
            &solicitud.invariantes,
            &solicitud.delta,
            modo,
            PRESUPUESTO_EXPLICACION_POR_DEFECTO,
        )
        .map(|(resultado, explicado)| {
            explicacion = Some((modo, explicado));
            resultado
        }),
    }
    .map_err(|e| e.codigo());
    // JSON con el mismo formato redactado que `check --explain` (v2), o un
    // código si no hay nada que explicar o se agotó el presupuesto.
    let (explanations, explanation_error, reglas) = match explicacion {
        None => (None, None, Vec::new()),
        Some((modo, explicado)) => match explicado.medido {
            Some(Ok(medido)) => (
                Some(super::explicaciones_medidas_a_json(
                    &medido,
                    modo,
                    PRESUPUESTO_EXPLICACION_POR_DEFECTO,
                )),
                None,
                explicado.reglas,
            ),
            Some(Err(e)) => (
                None,
                Some(super::error_explicacion_codigo(e)),
                explicado.reglas,
            ),
            None => (None, Some("explanation_not_applicable"), explicado.reglas),
        },
    };
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
            // Con --explain las dos claves están siempre presentes; en un error
            // técnico de la propuesta no hay invariantes que explicar.
            if solicitud.explicar.is_some() {
                let mut explanations = explanations.clone().unwrap_or(Value::Null);
                if solicitud.con_reglas && !explanations.is_null() {
                    super::anotar_reglas(&mut explanations, &reglas);
                }
                value["explanations"] = explanations;
                value["explanation_error"] = match (status, explanation_error) {
                    ("error", _) => json!("explanation_not_applicable"),
                    (_, Some(code)) => json!(code),
                    (_, None) => Value::Null,
                };
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
            if solicitud.explicar.is_some() {
                match (&explanations, explanation_error) {
                    (Some(explanations), _) if status != "error" => {
                        value.push_str(&super::explicaciones_a_texto(explanations, &reglas));
                    }
                    (_, Some(code)) if status != "error" => {
                        value.push_str(&format!("EXPLANATIONS unavailable: {code}\n"));
                    }
                    _ => value.push_str("EXPLANATIONS unavailable: explanation_not_applicable\n"),
                }
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
