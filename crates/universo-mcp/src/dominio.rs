//! Adaptador de consulta: no adquiere autoridad de publicación del bundle.
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf};
use universo_core::{
    BundlePath, Delta, ModoExplicacion, PRESUPUESTO_EXPLICACION_POR_DEFECTO, ResultadoPropuesta,
    explicar_propuesta,
};
use universo_okf::explicacion_json::{
    anotar_reglas, error_explicacion_codigo, explicaciones_medidas_a_json,
};
use universo_okf::{
    busqueda::{Alcance, buscar_en, tiene_terminos},
    parsear_delta_json,
    transaccion::simulacion::VistaSimulacion,
};

#[derive(Clone)]
pub struct ServerConfig {
    pub root: PathBuf,
    pub manifiesto: PathBuf,
    pub invariantes: PathBuf,
}

/// Un universo de un repositorio de criterio, servido con `--criterio`.
#[derive(Clone)]
pub struct UniversoMcp {
    pub nombre: String,
    pub config: ServerConfig,
    /// `escritura_agentes` en `felix.toml`: admite `capture`, `reinforce` y
    /// `object`.
    pub escritura: bool,
}

/// Herramientas que escriben (fase 10, U6): sólo en universos con
/// `escritura_agentes = true`.
pub const ESCRITURA: [&str; 3] = ["capture", "reinforce", "object"];

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

/// Herramientas que anuncia el servidor: con varios universos, todas aceptan
/// `universe`; con alguno que admite escritura, se suman las de captura.
pub fn herramientas(multiverso: bool, escritura: bool) -> Vec<Value> {
    let mut lista = herramientas_de_consulta();
    if escritura {
        lista.extend(herramientas_de_escritura());
    }
    if multiverso {
        for herramienta in &mut lista {
            herramienta["inputSchema"]["properties"]["universe"] = json!({
                "type": "string", "minLength": 1, "maxLength": 64,
                "description": "Universe of the criterio repository (felix.toml); default: its default universe. `search` also accepts \"todos\" (all universes)."
            });
        }
    }
    lista
}

fn herramientas_de_escritura() -> Vec<Value> {
    use universo_okf::registro_criterio::{MAX_DETALLE, MAX_PROYECTO, MAX_TEXTO, VALIDACIONES};
    ESCRITURA.into_iter().map(|name| {
        let mut properties = json!({
            "schema": {"type":"string","const":"felix-mcp-v1"},
            "project": {"type":"string","minLength":1,"maxLength":MAX_PROYECTO,"description":"Project where it was said or applied; created if missing."},
            "validation": {"type":"string","enum":VALIDACIONES,"default":"pendiente","description":"explicita: the owner said it was right; implicita: moved on without objecting; pendiente: not validated yet."},
            "author": {"type":"string","minLength":1,"maxLength":100,"description":"Agent name; recorded as agente:<author>."},
            "reason": {"type":"string","minLength":1,"maxLength":500}
        });
        let (required, description) = if name == "capture" {
            properties["text"] = json!({"type":"string","minLength":1,"maxLength":MAX_TEXTO,"description":"Concise paraphrase of the owner's criterio, not a transcript."});
            properties["task"] = json!({"type":"string","minLength":1,"maxLength":MAX_DETALLE});
            (vec!["schema", "text", "project", "author"],
             "Store a criterio the owner expressed as a candidate, with its project, task, date and validation. If the same criterio exists, status is \"similar\" with the existing notes: reinforce one instead. Writes the bundle (with CAS, recorded in the history); only universes with escritura_agentes.")
        } else {
            properties["note"] = json!({"type":"string","minLength":1,"maxLength":4096,"description":"Path of the criterio note, e.g. criterios/<slug>."});
            properties["context"] = json!({"type":"string","minLength":1,"maxLength":MAX_DETALLE});
            (vec!["schema", "note", "project", "context", "author"],
             if name == "reinforce" {
                 "Record that a criterio held in a project (evidence + confirmation in that project, once per project). Writes the bundle; only universes with escritura_agentes."
             } else {
                 "Record that a criterio did not hold in a project (evidence, no confirmation). Writes the bundle; only universes with escritura_agentes."
             })
        };
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}})
    }).collect()
}

fn herramientas_de_consulta() -> Vec<Value> {
    ["context", "propose", "check", "why", "search"].into_iter().map(|name| {
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
            "search" => {
                properties["query"] = json!({"type":"string","minLength":1,"maxLength":MAX_CONSULTA_CARACTERES});
                properties["limit"] = json!({"type":"integer","minimum":1,"maximum":MAX_LIMITE_BUSQUEDA,"default":LIMITE_BUSQUEDA});
                // Toda respuesta de search revela contenido del bundle (aun un
                // conteo confirma palabras), así que el opt-in es obligatorio.
                properties["include_values"] = json!({"type":"boolean","const":true});
                required.push("query");
                required.push("include_values");
                "Lexical search over note titles, types and attributes (never bodies or other frontmatter fields): notes where every query term starts a word, ignoring case and accents. Returns path, type, title, matched fields and score, so it discloses bundle values and requires include_values:true. The query must contain at least one word."
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
        "search" => &["schema", "query", "limit", "include_values"],
        "capture" => &[
            "schema",
            "text",
            "project",
            "task",
            "validation",
            "author",
            "reason",
        ],
        "reinforce" | "object" => &[
            "schema",
            "note",
            "project",
            "context",
            "validation",
            "author",
            "reason",
        ],
        _ => return Err(protocolo()),
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(protocolo());
    }
    if ESCRITURA.contains(&nombre) {
        return registrar_mcp(nombre, object, config);
    }
    // Validate tool arguments before touching configured files.
    if nombre == "search" {
        let (consulta, limite) = argumentos_busqueda(object)?;
        let vista =
            VistaSimulacion::capturar(&config.root, &config.manifiesto, &config.invariantes)
                .map_err(|error| dominio(error.codigo()))?;
        let salida = busqueda(&vista, consulta, limite);
        // La respuesta del MCP lleva este JSON dos veces (texto escapado y
        // `structuredContent`) bajo un tope de 2 MiB: 700 KB deja margen.
        if salida.to_string().len() > MAX_BUSQUEDA_BYTES {
            return Err(dominio("search_resource_limit"));
        }
        return Ok(salida);
    }
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

const LIMITE_BUSQUEDA: u64 = 20;
const MAX_LIMITE_BUSQUEDA: u64 = 100;
const MAX_CONSULTA_CARACTERES: usize = 1024;
const MAX_BUSQUEDA_BYTES: usize = 700_000;

/// Valida `query`, `limit` e `include_values` (que debe ser `true`).
fn argumentos_busqueda(
    object: &serde_json::Map<String, Value>,
) -> Result<(&str, usize), DomainError> {
    if object.get("include_values") != Some(&Value::Bool(true)) {
        return Err(protocolo());
    }
    let consulta = object
        .get("query")
        .and_then(Value::as_str)
        .filter(|q| q.chars().count() <= MAX_CONSULTA_CARACTERES && tiene_terminos(q))
        .ok_or_else(protocolo)?;
    let limite = match object.get("limit") {
        None => LIMITE_BUSQUEDA,
        // JSON Schema acepta 5.0 como entero.
        Some(value) => value
            .as_u64()
            .or_else(|| {
                value
                    .as_f64()
                    .filter(|f| f.fract() == 0.0 && *f >= 0.0 && *f <= u64::MAX as f64)
                    .map(|f| f as u64)
            })
            .filter(|n| (1..=MAX_LIMITE_BUSQUEDA).contains(n))
            .ok_or_else(protocolo)?,
    };
    Ok((consulta, limite as usize))
}

/// `search`: la búsqueda léxica de `universo search`, limitada a título, tipo
/// y atributos, los valores que `context` revela con opt-in. No consulta el
/// cuerpo ni los campos extra del frontmatter: un conteo o un campo `body`
/// bastarían para sondear su vocabulario.
fn busqueda(vista: &VistaSimulacion, consulta: &str, limite: usize) -> Value {
    let resultado = buscar_en(vista.estructura(), consulta, limite, Alcance::SinAnexo);
    let matches: Vec<_> = resultado
        .coincidencias
        .iter()
        .map(|c| {
            json!({
                "path": c.path.to_string(),
                "type": c.tipo,
                "title": c.titulo,
                "fields": c.campos.iter().map(|campo| campo.nombre()).collect::<Vec<_>>(),
                "score": c.puntaje,
            })
        })
        .collect();
    json!({"schema":"felix-mcp-v1","tool":"search","status":"ok","summary":{"total":resultado.total,"returned":resultado.coincidencias.len()},"values_included":true,"matches":matches,"persisted":false})
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
            let ends = atomo.paths();
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
        .filter(|atomo| atomo.paths().iter().all(|path| nodes.contains(*path)))
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

/// `capture`, `reinforce` y `object`: el mismo registro que la CLI
/// (`universo_okf::registro_criterio`), con autor `agente:<author>`.
fn registrar_mcp(
    nombre: &str,
    object: &serde_json::Map<String, Value>,
    config: &ServerConfig,
) -> Result<Value, DomainError> {
    use universo_okf::{
        registro_criterio::{
            Accion, MAX_DETALLE, MAX_PROYECTO, MAX_TEXTO, Peticion, Rutas, VALIDACIONES, Veredicto,
            linea_valida, registrar,
        },
        transaccion::historial::Procedencia,
    };
    let texto =
        |clave: &str, maximo: usize, obligatorio: bool| -> Result<Option<String>, DomainError> {
            match object.get(clave) {
                None if !obligatorio => Ok(None),
                Some(Value::String(v)) if linea_valida(v, maximo) => Ok(Some(v.clone())),
                _ => Err(protocolo()),
            }
        };
    let accion = match nombre {
        "capture" => Accion::Capturar,
        "reinforce" => Accion::Reforzar,
        _ => Accion::Objetar,
    };
    let (objetivo, detalle) = if accion == Accion::Capturar {
        (
            texto("text", MAX_TEXTO, true)?,
            texto("task", MAX_DETALLE, false)?,
        )
    } else {
        (
            texto("note", 4096, true)?,
            texto("context", MAX_DETALLE, true)?,
        )
    };
    let validacion = texto("validation", 20, false)?.unwrap_or_else(|| "pendiente".into());
    if !VALIDACIONES.contains(&validacion.as_str()) {
        return Err(protocolo());
    }
    let autor = texto("author", 100, true)?.unwrap_or_default();
    let objetivo = objetivo.unwrap_or_default();
    let motivo = texto("reason", 500, false)?.unwrap_or_else(|| {
        format!("{}: {}", accion.nombre(), objetivo.trim())
            .chars()
            .take(500)
            .collect()
    });
    let peticion = Peticion {
        accion,
        objetivo,
        proyecto: texto("project", MAX_PROYECTO, true)?.unwrap_or_default(),
        detalle,
        validacion,
        fecha: None,
        forzar: false,
    };
    let procedencia = Procedencia {
        autor: Some(format!("agente:{autor}")),
        motivo: Some(motivo),
    };
    let rutas = Rutas {
        raiz: &config.root,
        manifiesto: &config.manifiesto,
        invariantes: &config.invariantes,
    };
    match registrar(&rutas, &peticion, &procedencia, false) {
        Ok((
            plan,
            Veredicto::Admitido {
                sellado,
                registrado,
                ..
            },
        )) => Ok(
            json!({"schema":"felix-mcp-v1","tool":nombre,"status":"applied",
            "note":plan.nota,"project":plan.proyecto,"project_created":plan.proyecto_creado,
            "evidence":plan.evidencia,"logged":registrado,"sealed":sellado,"persisted":true}),
        ),
        Ok((_, Veredicto::Rechazado { reglas, conflictos })) => Ok(json!({"schema":"felix-mcp-v1",
            "tool":nombre,"status":"reject","rules":reglas,"conflicts":conflictos,"persisted":false})),
        // Ya existe: se informa para que el agente refuerce en vez de duplicar.
        Err(f) if f.codigo == "capture_similar" => {
            Ok(json!({"schema":"felix-mcp-v1","tool":nombre,
            "status":"similar","existing":f.detalle,"persisted":false}))
        }
        Err(f) => Err(dominio(f.codigo)),
    }
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
