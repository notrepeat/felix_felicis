//! Adaptador de consulta: no adquiere autoridad de publicación del bundle.
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf};
use universo_core::{
    Atomo, BundlePath, Delta, ModoExplicacion, PRESUPUESTO_EXPLICACION_POR_DEFECTO,
    ResultadoPropuesta, Sujeto, explicar_propuesta,
};
use universo_okf::explicacion_json::{
    anotar_reglas, error_explicacion_codigo, explicaciones_medidas_a_json,
};
use universo_okf::{parsear_delta_json, transaccion::simulacion::VistaSimulacion};

pub struct ServerConfig {
    pub root: PathBuf,
    pub manifiesto: PathBuf,
    pub invariantes: PathBuf,
}

#[derive(Debug)]
pub struct DomainError {
    pub codigo: &'static str,
    pub protocolo: bool,
}
fn protocolo() -> DomainError {
    DomainError {
        codigo: "invalid_tool_arguments",
        protocolo: true,
    }
}
fn dominio(codigo: &'static str) -> DomainError {
    DomainError {
        codigo,
        protocolo: false,
    }
}

pub fn herramientas() -> Vec<Value> {
    ["context", "propose", "check", "why"].into_iter().map(|name| {
        let mut properties = json!({"schema":{"type":"string","const":"felix-mcp-v1"}});
        let mut required = vec!["schema"];
        let description = match name {
            "context" => {
                properties["node"] = json!({"type":"string","maxLength":4096});
                properties["depth"] = json!({"type":"integer","minimum":0,"maximum":2,"default":0});
                properties["include_values"] = json!({"type":"boolean","default":false});
                required.push("node");
                "Read a bounded neighborhood. Values and canonical invariants require explicit include_values; document bodies are never returned."
            }
            "propose" => {
                properties["delta"] = json!({"type":"object"});
                required.push("delta");
                "Evaluate a delta without publishing it. Domain rejection is a successful tool result."
            }
            "check" => {
                properties["delta"] = json!({"type":"object"});
                "Evaluate the base state or a supplied candidate delta without writing."
            }
            _ => {
                properties["delta"] = json!({"type":"object"});
                "Like propose, plus the redacted explanation of the first failing rule: its id and message and the abstract shape of the counterexample (rules, boolean values, counts). No paths, labels or values of the bundle are emitted."
            }
        };
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})
    }).collect()
}

pub fn ejecutar(nombre: &str, args: Value, config: &ServerConfig) -> Result<Value, DomainError> {
    let object = args.as_object().ok_or_else(protocolo)?;
    if object.get("schema").and_then(Value::as_str) != Some("felix-mcp-v1") {
        return Err(protocolo());
    }
    let allowed: &[&str] = match nombre {
        "context" => &["schema", "node", "depth", "include_values"],
        "propose" | "check" | "why" => &["schema", "delta"],
        _ => return Err(protocolo()),
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(protocolo());
    }
    // Validate tool arguments before touching configured files.
    let context = if nombre == "context" {
        let node = object
            .get("node")
            .and_then(Value::as_str)
            .ok_or_else(protocolo)?;
        if node.len() > 4096 {
            return Err(protocolo());
        }
        let path = BundlePath::nuevo(node).map_err(|_| protocolo())?;
        let depth = match object.get("depth") {
            None => 0,
            Some(value) => value
                .as_u64()
                .filter(|depth| *depth <= 2)
                .ok_or_else(protocolo)?,
        };
        let values = match object.get("include_values") {
            None => false,
            Some(value) => value.as_bool().ok_or_else(protocolo)?,
        };
        Some((path, depth, values))
    } else {
        None
    };
    let delta = if context.is_none() {
        match object.get("delta") {
            Some(value) if value.is_object() => {
                Some(parsear_delta_json(&value.to_string()).map_err(|_| dominio("delta_invalid"))?)
            }
            Some(_) => return Err(protocolo()),
            None if nombre == "propose" => return Err(protocolo()),
            None => Some(Delta::nuevo(vec![], vec![]).map_err(|_| dominio("delta_invalid"))?),
        }
    } else {
        None
    };
    let vista = VistaSimulacion::capturar(&config.root, &config.manifiesto, &config.invariantes)
        .map_err(|error| dominio(error.codigo()))?;
    if let Some((path, depth, values)) = context {
        return contexto(&vista, path, depth, values);
    }
    let delta = delta.unwrap();
    let resultado = vista
        .proponer(&delta)
        .map_err(|error| dominio(error.codigo()))?;
    // `why`: la misma explicación redactada que `propose --explain decide
    // --rules` de la CLI. Sólo hay qué explicar si fallan invariantes.
    let explicacion = (nombre == "why").then(|| {
        if !matches!(resultado, ResultadoPropuesta::RechazadaInvariantes(_)) {
            return (Value::Null, json!("explanation_not_applicable"));
        }
        match explicar_propuesta(
            vista.estructura(),
            &delta,
            vista.invariantes(),
            ModoExplicacion::Decidir,
            PRESUPUESTO_EXPLICACION_POR_DEFECTO,
        ) {
            Some(Ok(medido)) => {
                let mut objetos = explicaciones_medidas_a_json(
                    &medido,
                    ModoExplicacion::Decidir,
                    PRESUPUESTO_EXPLICACION_POR_DEFECTO,
                );
                let reglas: Vec<_> = vista
                    .invariantes()
                    .iter()
                    .map(|f| (f.id.clone(), f.mensaje.clone()))
                    .collect();
                anotar_reglas(&mut objetos, &reglas);
                acotar_explicacion(objetos)
            }
            Some(Err(error)) => (Value::Null, json!(error_explicacion_codigo(error))),
            None => (Value::Null, json!("explanation_not_applicable")),
        }
    });
    let (status, conflicts, failed, diagnostics, provenance) = match resultado {
        ResultadoPropuesta::Admitida(_) => ("admit", 0, 0, vec![], vec![]),
        ResultadoPropuesta::RechazadaConflictos(conflictos) => {
            let diagnostics = conflictos
                .diagnosticos()
                .iter()
                .map(|d| json!({"class":d.clase().to_string(),"count":d.cantidad()}))
                .collect();
            (
                "reject",
                conflictos.total_conflictos(),
                0,
                diagnostics,
                vec![],
            )
        }
        ResultadoPropuesta::RechazadaInvariantes(fuentes) => {
            // Id y mensaje de la regla (`(regla :id ... :mensaje ...)`), `null`
            // si no los declara: los escribe el autor de las reglas, no son
            // datos del bundle, y son lo que permite actuar sobre el rechazo.
            let reglas = vista.invariantes();
            let provenance = fuentes
                .iter()
                .map(|p| {
                    let fuente = reglas.get(p.source_index);
                    json!({
                        "source_index": p.source_index,
                        "line": p.linea,
                        "column": p.columna,
                        "instance_index": p.instance_index,
                        "rule_id": fuente.and_then(|f| f.id.clone()),
                        "rule_message": fuente.and_then(|f| f.mensaje.clone()),
                    })
                })
                .collect();
            ("reject", 0, fuentes.len(), vec![], provenance)
        }
    };
    let mut salida = json!({"schema":"felix-mcp-v1","tool":nombre,"status":status,"summary":{"conflicts":conflicts,"failed_invariants":failed},"diagnostics":diagnostics,"provenance":provenance,"persisted":false});
    if let Some((explanations, error)) = explicacion {
        salida["explanations"] = explanations;
        salida["explanation_error"] = error;
    }
    Ok(salida)
}

/// Tope del JSON de la explicación de `why`, por debajo del límite de 2 MiB de
/// la respuesta del MCP: una explicación mayor se omite con
/// `explanation_too_large` y el rechazo y su procedencia siguen llegando.
const MAX_EXPLICACION_BYTES: usize = 1_000_000;

fn acotar_explicacion(objetos: Value) -> (Value, Value) {
    if objetos.to_string().len() > MAX_EXPLICACION_BYTES {
        (Value::Null, json!("explanation_too_large"))
    } else {
        (objetos, Value::Null)
    }
}

fn extremos(atomo: &Atomo) -> Vec<&BundlePath> {
    match atomo {
        Atomo::Type { sujeto, .. } | Atomo::Ext { sujeto, .. } => vec![sujeto],
        Atomo::Edge {
            origen, destino, ..
        } => vec![origen, destino],
        Atomo::Parent { hijo, padre } => vec![hijo, padre],
        Atomo::HasLab {
            sujeto: Sujeto::Nodo(path),
            ..
        } => vec![path],
        Atomo::HasLab {
            sujeto: Sujeto::Arista(id),
            ..
        } => vec![&id.origen, &id.destino],
    }
}

fn contexto(
    vista: &VistaSimulacion,
    path: BundlePath,
    depth: u64,
    values: bool,
) -> Result<Value, DomainError> {
    if !vista
        .estructura()
        .nodos()
        .any(|(_, node)| node.path == path)
    {
        return Err(dominio("context_node_missing"));
    }
    let atomos = vista.estructura().atomos();
    let mut nodes = BTreeSet::from([path]);
    for _ in 0..depth {
        let mut next = nodes.clone();
        for atomo in &atomos {
            let ends = extremos(atomo);
            if ends.iter().any(|path| nodes.contains(*path)) {
                next.extend(ends.into_iter().cloned());
            }
        }
        nodes = next;
        if nodes.len() > 256 {
            return Err(dominio("context_resource_limit"));
        }
    }
    // Induced neighborhood: edges crossing its boundary are not disclosed.
    let selected: Vec<_> = atomos
        .iter()
        .filter(|atomo| extremos(atomo).iter().all(|path| nodes.contains(*path)))
        .collect();
    if selected.len() > 4096 || vista.invariantes().len() > 256 {
        return Err(dominio("context_resource_limit"));
    }
    let invariants: Vec<_> = vista
        .invariantes()
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let mut item = json!({
                "source_index": index,
                "line": source.linea,
                "column": source.columna,
                "rule_id": source.id,
                "rule_message": source.mensaje,
            });
            if values {
                item["formula"] = json!(source.sentencia.to_string());
            }
            item
        })
        .collect();
    let mut result = json!({"schema":"felix-mcp-v1","tool":"context","status":"ok","summary":{"nodes":nodes.len(),"atoms":selected.len(),"invariants":invariants.len()},"invariant_selection":"all_conservative","invariants":invariants,"values_included":values,"persisted":false});
    if values {
        result["atoms"] = json!(
            selected
                .into_iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
    }
    // Also bound direct adapter callers; transport applies its independent cap.
    if result.to_string().len() > 1024 * 1024 {
        return Err(dominio("context_resource_limit"));
    }
    Ok(result)
}

#[cfg(test)]
mod tests_explicacion {
    use super::*;

    #[test]
    fn una_explicacion_demasiado_grande_se_omite_sin_perder_la_respuesta() {
        let chica = json!({"objects": [{"rule_id": "r"}]});
        assert_eq!(acotar_explicacion(chica.clone()), (chica, Value::Null));
        let grande = json!({"objects": vec![json!({"x": "y".repeat(1000)}); 1100]});
        assert_eq!(
            acotar_explicacion(grande),
            (Value::Null, json!("explanation_too_large"))
        );
    }
}
