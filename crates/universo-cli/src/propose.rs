//! CLI de propuesta: privacidad por defecto y token opt-in.
use super::{Formato, escribir_salida};
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf};
use universo_core::{ModoExplicacion, PRESUPUESTO_EXPLICACION_POR_DEFECTO};
use universo_okf::transaccion::simulacion::{
    EstadoSimulacion, simular_archivo, simular_archivo_explicado, testigo_archivo, testigos_archivo,
};

pub(super) const USO: &str = "Uso: universo propose <raiz> --manifiesto <m> --invariantes <i> --delta <d.json> [--format text|json] [--explain decide|enumerate [--rules]] [--witness [all] [--witness-limit <n>]] [--emit-snapshot-token | --apply --expect-snapshot <token> [--author <a>] [--reason <r>]]\nSimula por defecto; apply requiere snapshot esperado. Usa --opcion=<ruta> si una ruta empieza con '-'.\n";
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
    con_testigo: bool,
    /// `--witness all [--witness-limit N]`: todas las reglas que fallan y
    /// hasta N asignaciones falsas en total, en vez de sólo la primera.
    todos_los_testigos: Option<usize>,
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
    let mut con_testigo = false;
    let mut todos = false;
    let mut limite_testigos: Option<usize> = None;
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
        if arg == "--witness" {
            if con_testigo {
                return Err(fail());
            }
            con_testigo = true;
            i += 1;
            if args.get(i).is_some_and(|siguiente| siguiente == "all") {
                todos = true;
                i += 1;
            }
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
            "--witness-limit" if limite_testigos.is_none() => {
                limite_testigos = Some(value.parse().map_err(|_| fail())?)
            }
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
    // --witness-limit sólo acompaña a --witness all.
    if limite_testigos.is_some() && !todos {
        return Err(fail());
    }
    let todos_los_testigos =
        todos.then(|| limite_testigos.unwrap_or(super::testigos::LIMITE_POR_OMISION));
    if aplicar != esperado.is_some()
        || aplicar && (emit_token || explicar.is_some() || con_testigo)
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
        con_testigo,
        todos_los_testigos,
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
    // Testigo localizado opt-in: sólo si se pidió y el candidato incumple una
    // regla. Revela rutas del bundle; por eso nunca va por defecto.
    let testigo =
        if solicitud.con_testigo && solicitud.todos_los_testigos.is_none() && status == "reject" {
            Some(testigo_archivo(
                &solicitud.raiz,
                &solicitud.manifiesto,
                &solicitud.invariantes,
                &solicitud.delta,
            ))
        } else {
            None
        };
    // `--witness all`: reglas que fallan y sus asignaciones falsas.
    let todos = match (solicitud.todos_los_testigos, status) {
        (Some(limite), "reject") => Some(
            match testigos_archivo(
                &solicitud.raiz,
                &solicitud.manifiesto,
                &solicitud.invariantes,
                &solicitud.delta,
                limite,
            ) {
                Ok(Some((fuentes, encontrados))) => {
                    let falsas: Vec<usize> = encontrados.iter().map(|t| t.source_index).collect();
                    Ok(super::testigos::agrupar(&fuentes, &falsas, &encontrados))
                }
                Ok(None) => Ok(Vec::new()),
                Err(e) => Err(e.codigo()),
            },
        ),
        (Some(_), _) => Some(Ok(Vec::new())),
        _ => None,
    };
    let testigo_json = match &testigo {
        None => Value::Null,
        Some(Ok(None)) => Value::Null,
        Some(Err(e)) => json!({"error": e.codigo()}),
        Some(Ok(Some(t))) => json!({
            "source_index": t.source_index,
            "rule_id": t.rule_id,
            "rule_message": t.rule_message,
            "bindings": t.ligaduras.iter().map(|(variable, kind, value)| json!({
                "variable": variable, "kind": kind, "value": value,
            })).collect::<Vec<_>>(),
        }),
    };
    let payload = match solicitud.formato {
        Formato::Json => {
            let mut value = json!({"schema_version":1,"command":"propose","mode":"simulate","status":status,"summary":summary,"errors":errors});
            if let Some(token) = token {
                value["snapshot_token"] = json!(token);
            }
            match &todos {
                Some(Ok(reglas)) => {
                    value["failed_rules"] = super::testigos::a_json(reglas, true);
                    value["witness_error"] = Value::Null;
                }
                Some(Err(codigo)) => {
                    value["failed_rules"] = Value::Null;
                    value["witness_error"] = json!(codigo);
                }
                None if solicitud.con_testigo => value["witness"] = testigo_json.clone(),
                None => {}
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
            if let Some(Ok(Some(t))) = &testigo {
                let regla = t
                    .rule_id
                    .clone()
                    .unwrap_or_else(|| format!("source {}", t.source_index));
                let ligaduras: Vec<String> = t
                    .ligaduras
                    .iter()
                    .map(|(v, _, x)| format!("{v}={x}"))
                    .collect();
                value.push_str(&format!("witness: {regla}: {}\n", ligaduras.join(" ")));
            } else if let Some(Err(e)) = &testigo {
                value.push_str(&format!("witness: {}\n", e.codigo()));
            }
            match &todos {
                Some(Ok(reglas)) => value.push_str(&super::testigos::a_texto(reglas, true)),
                Some(Err(codigo)) => value.push_str(&format!("witness: {codigo}\n")),
                None => {}
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
