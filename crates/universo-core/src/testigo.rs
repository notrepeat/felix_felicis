//! Testigo localizado: la primera asignación que falsea una oración universal.
//!
//! La explicación redactada (`veredicto`) dice qué regla falló y con qué forma,
//! pero no dónde, y materializa la evidencia de todas las instancias, lo que en
//! bundles grandes agota su presupuesto. El testigo es el complemento opt-in y
//! sólo local: recorre las asignaciones del cuantificador universal exterior en
//! el mismo orden canónico que el evaluador (nodos por ruta, aristas por
//! identidad), instancia la oración con constantes y la evalúa con el
//! evaluador booleano, sin evidencia. Devuelve la primera asignación falsa y,
//! si el cuerpo es otro universal, sigue dentro de él.
//!
//! **Revela rutas e identidades del bundle.** No lo use un canal que no deba
//! verlas: la CLI sólo lo emite con `--witness` y el MCP no lo expone.

use std::collections::HashMap;

use crate::estructura::Estructura;
use crate::evaluador::{
    ContextoPresupuestoEvaluacion, ErrorEvaluacion, LigaduraCanonica, ValorLigadura,
};
use crate::formula::{Formula, SorteVar, SujetoTermino, TerminoArista, TerminoNodo, Var};
use crate::{BundlePath, IdArista};

/// Presupuesto compartido de una búsqueda de testigo: unidades semánticas de
/// todas las evaluaciones (como el de `proponer`, acumulado entre oraciones) y
/// número de asignaciones examinadas.
pub struct PresupuestoTestigo {
    evaluacion: ContextoPresupuestoEvaluacion,
    examinadas: u64,
    max_instancias: u64,
}

impl PresupuestoTestigo {
    pub fn nuevo(limite_unidades: u64, max_instancias: u64) -> Self {
        Self {
            evaluacion: ContextoPresupuestoEvaluacion::nuevo(limite_unidades),
            examinadas: 0,
            max_instancias,
        }
    }
}

/// Primera asignación falsificante de los universales exteriores anidados de
/// `formula`, o `None` si la fórmula es verdadera. Una fórmula falsa que no
/// empieza por un universal da `Some(vec![])`.
///
/// Todo el trabajo se descuenta de `presupuesto`, que puede compartirse entre
/// varias oraciones: al agotarse devuelve `PresupuestoAgotado`.
pub fn testigo_de_oracion(
    formula: &Formula,
    estructura: &Estructura,
    presupuesto: &mut PresupuestoTestigo,
) -> Result<Option<Vec<LigaduraCanonica>>, ErrorEvaluacion> {
    buscar(formula, estructura, presupuesto)
}

fn buscar(
    formula: &Formula,
    estructura: &Estructura,
    presupuesto: &mut PresupuestoTestigo,
) -> Result<Option<Vec<LigaduraCanonica>>, ErrorEvaluacion> {
    let Formula::ParaTodo {
        ligaduras,
        guarda,
        cuerpo,
    } = formula
    else {
        return Ok((!presupuesto.evaluacion.evaluar(formula, estructura)?).then(Vec::new));
    };
    let mut nodos: Vec<BundlePath> = estructura.nodos().map(|(_, n)| n.path.clone()).collect();
    nodos.sort();
    let mut aristas: Vec<IdArista> = estructura
        .aristas()
        .filter_map(|(_, a)| {
            Some(IdArista {
                origen: estructura.nodo(a.origen)?.path.clone(),
                sorte: a.tipo.clone(),
                destino: estructura.nodo(a.destino)?.path.clone(),
                ordinal: a.ordinal,
            })
        })
        .collect();
    aristas.sort_by_key(|id| id.to_string());
    let tamanos: Vec<usize> = ligaduras
        .iter()
        .map(|(_, sorte)| match sorte {
            SorteVar::Nodo => nodos.len(),
            SorteVar::Arista => aristas.len(),
        })
        .collect();
    if tamanos.contains(&0) {
        return Ok(None);
    }
    let mut indices = vec![0usize; ligaduras.len()];
    loop {
        presupuesto.examinadas += 1;
        if presupuesto.examinadas > presupuesto.max_instancias {
            return Err(ErrorEvaluacion::PresupuestoAgotado {
                limite: presupuesto.max_instancias,
            });
        }
        let valores: HashMap<Var, ValorLigadura> = ligaduras
            .iter()
            .zip(&indices)
            .map(|((variable, sorte), &i)| {
                let valor = match sorte {
                    SorteVar::Nodo => ValorLigadura::Nodo(nodos[i].clone()),
                    SorteVar::Arista => ValorLigadura::Arista(aristas[i].clone()),
                };
                (variable.clone(), valor)
            })
            .collect();
        let cuerpo_instanciado = sustituir(cuerpo, &valores);
        let instancia = match guarda {
            Some(guarda) => Formula::Implica(
                Box::new(sustituir(guarda, &valores)),
                Box::new(cuerpo_instanciado.clone()),
            ),
            None => cuerpo_instanciado.clone(),
        };
        if !presupuesto.evaluacion.evaluar(&instancia, estructura)? {
            let mut testigo: Vec<LigaduraCanonica> = ligaduras
                .iter()
                .map(|(variable, _)| LigaduraCanonica {
                    variable: variable.clone(),
                    valor: valores[variable].clone(),
                })
                .collect();
            // La guarda es verdadera y el cuerpo falso: si el cuerpo es otro
            // universal, su primera asignación falsa completa el testigo.
            if matches!(cuerpo_instanciado, Formula::ParaTodo { .. })
                && let Some(interior) = buscar(&cuerpo_instanciado, estructura, presupuesto)?
            {
                testigo.extend(interior);
            }
            return Ok(Some(testigo));
        }
        // Odómetro: la última ligadura avanza en cada paso, como en el evaluador.
        let mut posicion = ligaduras.len();
        loop {
            if posicion == 0 {
                return Ok(None);
            }
            posicion -= 1;
            indices[posicion] += 1;
            if indices[posicion] < tamanos[posicion] {
                break;
            }
            indices[posicion] = 0;
        }
    }
}

fn nodo(termino: &TerminoNodo, valores: &HashMap<Var, ValorLigadura>) -> TerminoNodo {
    match termino {
        TerminoNodo::Var(v) => match valores.get(v) {
            Some(ValorLigadura::Nodo(path)) => TerminoNodo::Constante(path.clone()),
            _ => termino.clone(),
        },
        TerminoNodo::Constante(_) => termino.clone(),
    }
}

fn arista(termino: &TerminoArista, valores: &HashMap<Var, ValorLigadura>) -> TerminoArista {
    match termino {
        TerminoArista::Var(v) => match valores.get(v) {
            Some(ValorLigadura::Arista(id)) => TerminoArista::Constante(id.clone()),
            _ => termino.clone(),
        },
        TerminoArista::Constante(_) => termino.clone(),
    }
}

/// Sustituye variables ligadas por constantes. El parser prohíbe volver a ligar
/// una variable dentro de su alcance, así que no hay captura que evitar.
fn sustituir(formula: &Formula, valores: &HashMap<Var, ValorLigadura>) -> Formula {
    let f = |g: &Formula| Box::new(sustituir(g, valores));
    match formula {
        Formula::Type { sujeto, sorte } => Formula::Type {
            sujeto: nodo(sujeto, valores),
            sorte: sorte.clone(),
        },
        Formula::Edge {
            arista: a,
            origen,
            destino,
            sorte,
        } => Formula::Edge {
            arista: arista(a, valores),
            origen: nodo(origen, valores),
            destino: nodo(destino, valores),
            sorte: sorte.clone(),
        },
        Formula::Parent { hijo, padre } => Formula::Parent {
            hijo: nodo(hijo, valores),
            padre: nodo(padre, valores),
        },
        Formula::HasLab { sujeto, etiqueta } => Formula::HasLab {
            sujeto: match sujeto {
                SujetoTermino::Nodo(t) => SujetoTermino::Nodo(nodo(t, valores)),
                SujetoTermino::Arista(t) => SujetoTermino::Arista(arista(t, valores)),
            },
            etiqueta: etiqueta.clone(),
        },
        Formula::Ext {
            predicado,
            sujeto,
            constante,
        } => Formula::Ext {
            predicado: predicado.clone(),
            sujeto: nodo(sujeto, valores),
            constante: constante.clone(),
        },
        Formula::Compara {
            op,
            predicado,
            sujeto,
            constante,
        } => Formula::Compara {
            op: *op,
            predicado: predicado.clone(),
            sujeto: nodo(sujeto, valores),
            constante: constante.clone(),
        },
        Formula::TcParent { desde, hasta } => Formula::TcParent {
            desde: nodo(desde, valores),
            hasta: nodo(hasta, valores),
        },
        Formula::IgualNodo(a, b) => Formula::IgualNodo(nodo(a, valores), nodo(b, valores)),
        Formula::IgualArista(a, b) => Formula::IgualArista(arista(a, valores), arista(b, valores)),
        Formula::No(interior) => Formula::No(f(interior)),
        Formula::Y(partes) => Formula::Y(partes.iter().map(|p| sustituir(p, valores)).collect()),
        Formula::O(partes) => Formula::O(partes.iter().map(|p| sustituir(p, valores)).collect()),
        Formula::Implica(a, b) => Formula::Implica(f(a), f(b)),
        Formula::ParaTodo {
            ligaduras,
            guarda,
            cuerpo,
        } => Formula::ParaTodo {
            ligaduras: ligaduras.clone(),
            guarda: guarda.as_deref().map(f),
            cuerpo: f(cuerpo),
        },
        Formula::Existe {
            ligaduras,
            guarda,
            cuerpo,
        } => Formula::Existe {
            ligaduras: ligaduras.clone(),
            guarda: guarda.as_deref().map(f),
            cuerpo: f(cuerpo),
        },
        Formula::AlMenos {
            minimo,
            ligaduras,
            guarda,
            cuerpo,
        } => Formula::AlMenos {
            minimo: *minimo,
            ligaduras: ligaduras.clone(),
            guarda: guarda.as_deref().map(f),
            cuerpo: f(cuerpo),
        },
    }
}
