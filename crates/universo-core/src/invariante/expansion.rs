//! Instanciación determinista de programas de invariantes sobre una estructura.

use std::collections::{BTreeSet, HashMap};

use crate::estructura::Estructura;
use crate::formula::{
    DominioEsquema, Formula, Sentencia, TerminoConstExt, TerminoEtiqueta, TerminoSorte, Var,
};
use crate::manifiesto::Manifiesto;
use crate::tipos::{Etiqueta, Sorte};

use super::{ErrorValidacionInvariante, validar_sentencia};

/// Máximo predeterminado de instancias de esquemas generadas en una llamada.
pub const PRESUPUESTO_INSTANCIAS_POR_DEFECTO: u64 = 100_000;

/// Métricas de una expansión completa.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MetricasExpansion {
    /// Esquemas top-level recibidos, incluso los que generan cero instancias.
    pub plantillas: usize,
    /// Oraciones producidas por esquemas; no cuenta oraciones directas.
    pub instancias: u64,
}

/// Resultado completo. Las oraciones directas se conservan y las plantillas
/// se reemplazan por sus instancias en orden determinista.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultadoExpansion {
    pub oraciones: Vec<Formula>,
    pub metricas: MetricasExpansion,
}

/// Error al validar o expandir un programa de invariantes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorExpansion {
    SentenciaInvalida {
        indice: usize,
        errores: Vec<ErrorValidacionInvariante>,
    },
    DominioPredicadoDesconocido(String),
    PresupuestoAgotado {
        limite: u64,
    },
}

impl std::fmt::Display for ErrorExpansion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SentenciaInvalida { indice, errores } => write!(
                f,
                "la sentencia {indice} no es válida ({} diagnósticos)",
                errores.len()
            ),
            Self::DominioPredicadoDesconocido(nombre) => {
                write!(f, "no existe el dominio del predicado {nombre:?}")
            }
            Self::PresupuestoAgotado { limite } => {
                write!(f, "se agotó el presupuesto de instancias ({limite})")
            }
        }
    }
}

impl std::error::Error for ErrorExpansion {}

/// Expande un programa con el presupuesto predeterminado de instancias.
///
/// Primero valida todas las sentencias contra el manifiesto y la estructura;
/// si alguna falla, no devuelve oraciones parciales. `plantillas` cuenta los
/// esquemas de entrada y `instancias` solo las fórmulas generadas a partir de
/// ellos. Las oraciones directas se preservan, pero no consumen presupuesto.
pub fn expandir_sentencias(
    sentencias: &[Sentencia],
    manifiesto: &Manifiesto,
    estructura: &Estructura,
) -> Result<ResultadoExpansion, ErrorExpansion> {
    expandir_con_presupuesto(
        sentencias,
        manifiesto,
        estructura,
        PRESUPUESTO_INSTANCIAS_POR_DEFECTO,
    )
}

/// Variante con límite explícito de instancias generadas.
///
/// El límite es atómico: si una instancia adicional excede el presupuesto,
/// se devuelve [`ErrorExpansion::PresupuestoAgotado`] sin publicar ninguna
/// parte del programa expandido.
pub fn expandir_con_presupuesto(
    sentencias: &[Sentencia],
    manifiesto: &Manifiesto,
    estructura: &Estructura,
    limite: u64,
) -> Result<ResultadoExpansion, ErrorExpansion> {
    for (indice, sentencia) in sentencias.iter().enumerate() {
        let errores = validar_sentencia(sentencia, manifiesto, estructura);
        if !errores.is_empty() {
            return Err(ErrorExpansion::SentenciaInvalida { indice, errores });
        }
    }

    let plantillas = sentencias
        .iter()
        .filter(|sentencia| matches!(sentencia, Sentencia::Esquema { .. }))
        .count();
    let mut oraciones = Vec::new();
    let mut instancias = 0_u64;

    for sentencia in sentencias {
        match sentencia {
            Sentencia::Oracion(formula) => oraciones.push(formula.clone()),
            Sentencia::Esquema { ligaduras, cuerpo } => {
                let dominios = ligaduras
                    .iter()
                    .map(|(_, dominio)| dominio_instancia(dominio, manifiesto, estructura))
                    .collect::<Result<Vec<_>, _>>()?;
                if dominios.iter().any(Vec::is_empty) {
                    continue;
                }

                let mut indices = vec![0; ligaduras.len()];
                loop {
                    if instancias >= limite {
                        return Err(ErrorExpansion::PresupuestoAgotado { limite });
                    }
                    let valores = ligaduras
                        .iter()
                        .zip(&dominios)
                        .zip(&indices)
                        .map(|(((variable, _), dominio), indice)| {
                            (variable.clone(), dominio[*indice].clone())
                        })
                        .collect();
                    let mut instancia = cuerpo.clone();
                    sustituir_metas(&mut instancia, &valores);
                    oraciones.push(instancia);
                    instancias += 1;

                    if indices.is_empty() || !avanzar(&mut indices, &dominios) {
                        break;
                    }
                }
            }
        }
    }

    Ok(ResultadoExpansion {
        oraciones,
        metricas: MetricasExpansion {
            plantillas,
            instancias,
        },
    })
}

#[derive(Clone)]
enum ValorMeta {
    Sorte(Sorte),
    Etiqueta(Etiqueta),
    ConstanteExt(String),
}

fn dominio_instancia(
    dominio: &DominioEsquema,
    manifiesto: &Manifiesto,
    estructura: &Estructura,
) -> Result<Vec<ValorMeta>, ErrorExpansion> {
    let mut valores: Vec<ValorMeta> = match dominio {
        DominioEsquema::SortesNodo => manifiesto
            .sortes_nodo
            .iter()
            .cloned()
            .map(ValorMeta::Sorte)
            .collect(),
        DominioEsquema::SortesArista => manifiesto
            .aristas
            .iter()
            .map(|arista| ValorMeta::Sorte(Sorte::nuevo(&arista.clave)))
            .collect(),
        DominioEsquema::Etiquetas => estructura
            .nodos()
            .map(|(_, nodo)| nodo.etiqueta.clone())
            .chain(
                estructura
                    .aristas()
                    .map(|(_, arista)| arista.etiqueta.clone()),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(ValorMeta::Etiqueta)
            .collect(),
        DominioEsquema::DominioPredicado(nombre) => manifiesto
            .predicado(nombre)
            .ok_or_else(|| ErrorExpansion::DominioPredicadoDesconocido(nombre.clone()))?
            .dominio
            .iter()
            .cloned()
            .map(ValorMeta::ConstanteExt)
            .collect(),
    };
    valores.sort_by_key(ValorMeta::orden_canonico);
    Ok(valores)
}

impl ValorMeta {
    fn orden_canonico(&self) -> String {
        match self {
            Self::Sorte(valor) => valor.como_str().to_string(),
            Self::Etiqueta(valor) => valor.como_str().to_string(),
            Self::ConstanteExt(valor) => valor.clone(),
        }
    }
}

fn avanzar(indices: &mut [usize], dominios: &[Vec<ValorMeta>]) -> bool {
    for indice in (0..indices.len()).rev() {
        indices[indice] += 1;
        if indices[indice] < dominios[indice].len() {
            return true;
        }
        indices[indice] = 0;
    }
    false
}

fn sustituir_metas(formula: &mut Formula, valores: &HashMap<Var, ValorMeta>) {
    match formula {
        Formula::Type { sorte, .. } | Formula::Edge { sorte, .. } => {
            if let TerminoSorte::Meta(variable) = sorte {
                if let Some(ValorMeta::Sorte(valor)) = valores.get(variable) {
                    *sorte = TerminoSorte::Literal(valor.clone());
                }
            }
        }
        Formula::HasLab { etiqueta, .. } => {
            if let TerminoEtiqueta::Meta(variable) = etiqueta {
                if let Some(ValorMeta::Etiqueta(valor)) = valores.get(variable) {
                    *etiqueta = TerminoEtiqueta::Literal(valor.clone());
                }
            }
        }
        Formula::Ext { constante, .. } => {
            if let TerminoConstExt::Meta(variable) = constante {
                if let Some(ValorMeta::ConstanteExt(valor)) = valores.get(variable) {
                    *constante = TerminoConstExt::Literal(valor.clone());
                }
            }
        }
        Formula::No(interior) => sustituir_metas(interior, valores),
        Formula::Y(partes) | Formula::O(partes) => {
            for parte in partes {
                sustituir_metas(parte, valores);
            }
        }
        Formula::Implica(antecedente, consecuente) => {
            sustituir_metas(antecedente, valores);
            sustituir_metas(consecuente, valores);
        }
        Formula::ParaTodo { guarda, cuerpo, .. } | Formula::Existe { guarda, cuerpo, .. } => {
            if let Some(guarda) = guarda {
                sustituir_metas(guarda, valores);
            }
            sustituir_metas(cuerpo, valores);
        }
        Formula::Parent { .. }
        | Formula::TcParent { .. }
        | Formula::IgualNodo(..)
        | Formula::IgualArista(..) => {}
    }
}

#[cfg(test)]
#[path = "expansion/tests.rs"]
mod tests;
