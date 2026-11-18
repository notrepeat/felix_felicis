//! JSON redactado de las explicaciones del verificador, compartido por la CLI
//! (`check --explain`, `propose --explain`) y el MCP (`why`). Sólo formas,
//! valores booleanos, conteos y, con `anotar_reglas`, id y mensaje de regla:
//! nunca rutas, etiquetas ni valores del bundle.
use serde_json::{Value, json};
use universo_core::ErrorEvaluacion;
use universo_core::evaluador::{
    ConstructorAtomico, EvidenciaEvaluacion, ReglaEvidencia, TipoCuantificador,
};
use universo_core::veredicto::{
    AtomoBrecha, Brecha, ClaseBrecha, ErrorVeredicto, FormaPositiva, ModoExplicacion,
    ObjetoExplicacion, ResultadoExplicaciones, ResultadoExplicacionesMedido,
};

/// Añade `rule_id` y `rule_message` (o `null`) a cada objeto de explicación
/// según su `source_index` (JSON v3 de `check` y `propose --explain --rules`).
pub fn anotar_reglas(explanations: &mut Value, reglas: &[(Option<String>, Option<String>)]) {
    for object in explanations["objects"].as_array_mut().into_iter().flatten() {
        let regla = object["source_index"]
            .as_u64()
            .and_then(|i| reglas.get(usize::try_from(i).ok()?));
        let (id, mensaje) = regla.cloned().unwrap_or((None, None));
        object["rule_id"] = json!(id);
        object["rule_message"] = json!(mensaje);
    }
}

fn constructor_atomico_nombre(constructor: ConstructorAtomico) -> &'static str {
    match constructor {
        ConstructorAtomico::Type => "type",
        ConstructorAtomico::Edge => "edge",
        ConstructorAtomico::Parent => "parent",
        ConstructorAtomico::HasLabNodo => "has_label_node",
        ConstructorAtomico::HasLabArista => "has_label_edge",
        ConstructorAtomico::Ext => "extension",
        ConstructorAtomico::TcParent => "tc_parent",
        ConstructorAtomico::IgualNodo => "equal_node",
        ConstructorAtomico::IgualArista => "equal_edge",
        ConstructorAtomico::CompararExt => "extension_order",
    }
}

pub fn evidencia_a_json(evidencia: &EvidenciaEvaluacion) -> Value {
    let valor = evidencia.valor;
    match &evidencia.regla {
        ReglaEvidencia::Atomo { constructor, .. } => json!({
            "rule": "atom",
            "value": valor,
            "predicate": constructor_atomico_nombre(*constructor),
        }),
        ReglaEvidencia::No { interior } => json!({
            "rule": "not",
            "value": valor,
            "inner": evidencia_a_json(interior),
        }),
        ReglaEvidencia::Conjuncion { partes_evaluadas } => json!({
            "rule": "and",
            "value": valor,
            "children": partes_evaluadas.iter().map(evidencia_a_json).collect::<Vec<_>>(),
        }),
        ReglaEvidencia::Disyuncion { partes_evaluadas } => json!({
            "rule": "or",
            "value": valor,
            "children": partes_evaluadas.iter().map(evidencia_a_json).collect::<Vec<_>>(),
        }),
        ReglaEvidencia::Implicacion {
            antecedente,
            consecuente,
        } => json!({
            "rule": "implies",
            "value": valor,
            "antecedent": evidencia_a_json(antecedente),
            "consequent": consecuente.as_deref().map(evidencia_a_json),
        }),
        ReglaEvidencia::Cuantificador {
            tipo,
            instancias,
            busqueda_agotada,
        } => {
            let rule = match tipo {
                TipoCuantificador::ParaTodo => "forall",
                TipoCuantificador::Existe => "exists",
                TipoCuantificador::AlMenos { .. } => "at_least",
            };
            let instancias = instancias
                .iter()
                .map(|instancia| {
                    json!({
                        "value": instancia.valor,
                        "guard": instancia.guarda.as_deref().map(evidencia_a_json),
                        "body": instancia.cuerpo.as_deref().map(evidencia_a_json),
                    })
                })
                .collect::<Vec<_>>();
            let mut nodo = json!({
                "rule": rule,
                "value": valor,
                "search_exhausted": busqueda_agotada,
                "instances": instancias,
            });
            if let TipoCuantificador::AlMenos { minimo } = tipo {
                nodo["minimum"] = json!(minimo);
            }
            nodo
        }
    }
}

fn atomo_brecha_nombre(atomo: AtomoBrecha) -> &'static str {
    match atomo {
        AtomoBrecha::Type => "type",
        AtomoBrecha::Edge => "edge",
        AtomoBrecha::Parent => "parent",
        AtomoBrecha::HasLabNodo => "has_label_node",
        AtomoBrecha::HasLabArista => "has_label_edge",
        AtomoBrecha::Ext => "extension",
        AtomoBrecha::TcParent => "tc_parent",
        AtomoBrecha::IgualNodo => "equal_node",
        AtomoBrecha::IgualArista => "equal_edge",
    }
}

pub fn forma_positiva_a_json(forma: &FormaPositiva) -> Value {
    match forma {
        FormaPositiva::Atomo(atomo) => json!({
            "kind": "atom",
            "predicate": atomo_brecha_nombre(*atomo),
        }),
        FormaPositiva::Y(partes) => json!({
            "kind": "and",
            "children": partes.iter().map(forma_positiva_a_json).collect::<Vec<_>>(),
        }),
        FormaPositiva::O(partes) => json!({
            "kind": "or",
            "children": partes.iter().map(forma_positiva_a_json).collect::<Vec<_>>(),
        }),
        FormaPositiva::Existe {
            ligaduras_nodo,
            ligaduras_arista,
            guarda,
            cuerpo,
        } => json!({
            "kind": "exists",
            "node_binders": ligaduras_nodo,
            "edge_binders": ligaduras_arista,
            "guard": guarda.as_deref().map(forma_positiva_a_json),
            "body": forma_positiva_a_json(cuerpo),
        }),
    }
}

pub fn brecha_a_json(brecha: &Brecha) -> Value {
    let class = match brecha.clase {
        ClaseBrecha::ExistenciaSinTestigo => "existence_without_witness",
        ClaseBrecha::ParaTodoExisteSinTestigo => "forall_exists_without_witness",
    };
    json!({
        "class": class,
        "counts": {
            "available_nodes": brecha.conteos.nodos_disponibles,
            "available_edges": brecha.conteos.aristas_disponibles,
            "existential_candidates_evaluated": brecha.conteos.candidatos_existencial_principal_evaluados,
            "universal_bindings_examined": brecha.conteos.asignaciones_universales_examinadas,
        },
        "form": {
            "node_binders": brecha.forma.ligaduras_nodo,
            "edge_binders": brecha.forma.ligaduras_arista,
            "guard": brecha.forma.guarda.as_ref().map(forma_positiva_a_json),
            "body": forma_positiva_a_json(&brecha.forma.cuerpo),
        },
    })
}

pub fn objeto_explicacion_a_json(objeto: &ObjetoExplicacion) -> Value {
    let core = objeto.core_presencia().map_or(
        Value::Null,
        |core| json!({ "atom_count": core.atomos_sensibles().len() }),
    );
    json!({
        "kind": "counterexample",
        "source_index": objeto.procedencia.source_index,
        "line": objeto.procedencia.linea,
        "column": objeto.procedencia.columna,
        "instance_index": objeto.procedencia.instance_index,
        "counterexample": evidencia_a_json(&objeto.counterexample.evidencia),
        "presence_core": core,
        "gap": objeto.gap().map_or(Value::Null, brecha_a_json),
    })
}

pub fn explicaciones_medidas_a_json(
    medido: &ResultadoExplicacionesMedido,
    modo: ModoExplicacion,
    limite: u64,
) -> Value {
    let (objects, truncated, reason) = match &medido.resultado {
        ResultadoExplicaciones::Decision(objeto) => (
            objeto
                .iter()
                .map(objeto_explicacion_a_json)
                .collect::<Vec<_>>(),
            false,
            None,
        ),
        ResultadoExplicaciones::Enumeracion {
            objetos,
            truncated,
            reason,
        } => (
            objetos
                .iter()
                .map(objeto_explicacion_a_json)
                .collect::<Vec<_>>(),
            *truncated,
            reason.map(|reason| reason.as_str()),
        ),
    };
    let mode = match modo {
        ModoExplicacion::Decidir => "decide",
        ModoExplicacion::Enumerar { .. } => "enumerate",
    };
    json!({
        "mode": mode,
        "objects": objects,
        "truncated": truncated,
        "reason": reason,
        "budget": {
            "limit": limite,
            "consumed": medido.unidades_consumidas,
            "semantic_units": medido.unidades_semanticas,
            "evidence_units": medido.unidades_evidencia,
            "aggregator_units": medido.unidades_agregador,
        },
    })
}

/// Código redactado de un fallo del explicador (compartido con `propose`).
pub fn error_explicacion_codigo(error: ErrorVeredicto) -> &'static str {
    match error {
        ErrorVeredicto::PresupuestoAgotado { .. }
        | ErrorVeredicto::Evaluacion(
            ErrorEvaluacion::PresupuestoAgotado { .. }
            | ErrorEvaluacion::PresupuestoEvidenciaAgotado { .. },
        ) => "explanation_budget_exceeded",
        ErrorVeredicto::Evaluacion(_) => "explanation_evaluation_invalid",
    }
}
