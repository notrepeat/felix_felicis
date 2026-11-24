//! Reglas que fallan y todas sus asignaciones falsas (`check --witness`,
//! `propose --witness all`). Las asignaciones revelan rutas del bundle: sólo
//! salen con una opción explícita y nunca por el MCP.

use serde_json::{Value, json};
use universo_core::{LigaduraCanonica, SentenciaFuente, TestigosFuente, ValorLigadura};

/// Asignaciones falsas que se muestran por omisión, sumando todas las reglas.
pub(crate) const LIMITE_POR_OMISION: usize = 100;

/// Una regla del programa con al menos una instancia falsa.
pub(crate) struct ReglaFallida {
    pub source_index: usize,
    pub rule_id: Option<String>,
    pub rule_message: Option<String>,
    pub linea: usize,
    pub columna: usize,
    pub instancias_falsas: usize,
    pub testigos: Vec<Testigo>,
    /// Hay asignaciones falsas que no se muestran (por el límite).
    pub truncado: bool,
}

pub(crate) struct Testigo {
    pub instance_index: Option<u64>,
    /// (variable, `node` | `edge`, ruta o identidad).
    pub ligaduras: Vec<(String, &'static str, String)>,
}

fn ligadura(l: &LigaduraCanonica) -> (String, &'static str, String) {
    match &l.valor {
        ValorLigadura::Nodo(path) => (l.variable.to_string(), "node", path.to_string()),
        ValorLigadura::Arista(id) => (l.variable.to_string(), "edge", id.to_string()),
    }
}

/// Agrupa por regla, en el orden del programa. `falsas` son los
/// `source_index` de las instancias falsas (con repetición si un esquema
/// falla en varias instancias); `testigos`, lo que encontró la búsqueda.
pub(crate) fn agrupar(
    fuentes: &[SentenciaFuente],
    falsas: &[usize],
    testigos: &[TestigosFuente],
) -> Vec<ReglaFallida> {
    let mut reglas: Vec<ReglaFallida> = Vec::new();
    for &source_index in falsas {
        if let Some(regla) = reglas.iter_mut().find(|r| r.source_index == source_index) {
            regla.instancias_falsas += 1;
            continue;
        }
        let fuente = fuentes.get(source_index);
        reglas.push(ReglaFallida {
            source_index,
            rule_id: fuente.and_then(|f| f.id.clone()),
            rule_message: fuente.and_then(|f| f.mensaje.clone()),
            linea: fuente.map_or(0, |f| f.linea),
            columna: fuente.map_or(0, |f| f.columna),
            instancias_falsas: 1,
            testigos: Vec::new(),
            truncado: false,
        });
    }
    for t in testigos {
        let Some(regla) = reglas.iter_mut().find(|r| r.source_index == t.source_index) else {
            continue;
        };
        regla.truncado |= t.testigos.truncado;
        regla
            .testigos
            .extend(t.testigos.asignaciones.iter().map(|asignacion| Testigo {
                instance_index: t.instance_index,
                ligaduras: asignacion.iter().map(ligadura).collect(),
            }));
    }
    reglas
}

/// `failed_rules` del JSON. Sin `con_testigos` no lleva asignaciones.
pub(crate) fn a_json(reglas: &[ReglaFallida], con_testigos: bool) -> Value {
    Value::Array(
        reglas
            .iter()
            .map(|r| {
                let mut v = json!({
                    "source_index": r.source_index,
                    "rule_id": r.rule_id,
                    "rule_message": r.rule_message,
                    "line": r.linea,
                    "column": r.columna,
                    "failed_instances": r.instancias_falsas,
                });
                if con_testigos {
                    v["witnesses"] = r
                        .testigos
                        .iter()
                        .map(|t| {
                            json!({
                                "instance_index": t.instance_index,
                                "bindings": t.ligaduras.iter().map(|(variable, kind, value)| json!({
                                    "variable": variable, "kind": kind, "value": value,
                                })).collect::<Vec<_>>(),
                            })
                        })
                        .collect();
                    v["truncated"] = json!(r.truncado);
                }
                v
            })
            .collect(),
    )
}

/// Una línea por regla y, debajo, una por asignación falsa.
pub(crate) fn a_texto(reglas: &[ReglaFallida], con_testigos: bool) -> String {
    let mut texto = String::new();
    for r in reglas {
        let nombre = r
            .rule_id
            .clone()
            .unwrap_or_else(|| format!("source {}", r.source_index));
        let mensaje = r
            .rule_message
            .as_deref()
            .map(|m| format!(" — {m}"))
            .unwrap_or_default();
        texto.push_str(&format!(
            "rule: {nombre}{mensaje} (línea {}; instancias falsas: {})\n",
            r.linea, r.instancias_falsas
        ));
        if !con_testigos {
            continue;
        }
        for t in &r.testigos {
            let instancia = t
                .instance_index
                .map(|i| format!("[instancia {i}] "))
                .unwrap_or_default();
            let ligaduras: Vec<String> = t
                .ligaduras
                .iter()
                .map(|(v, _, x)| format!("{v}={x}"))
                .collect();
            texto.push_str(&format!("  {instancia}{}\n", ligaduras.join(" ")));
        }
        if r.truncado {
            texto.push_str("  … hay más asignaciones falsas: sube --witness-limit\n");
        }
    }
    texto
}
