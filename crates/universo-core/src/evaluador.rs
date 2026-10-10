//! Evaluador de referencia de FO bisortida sobre una estructura finita.
//!
//! La implementación es intencionalmente directa: recorre los dominios
//! finitos de nodos y aristas, y evalúa cada conectivo según Tarski. No usa
//! los índices de átomos ni instala abreviaturas para `tc Parent`.

use std::collections::{HashMap, HashSet};

use crate::atomos::IdArista;
use crate::estructura::Estructura;
use crate::formula::{
    Formula, SorteVar, SujetoTermino, TerminoArista, TerminoConstExt, TerminoEtiqueta, TerminoNodo,
    TerminoSorte, Var,
};
use crate::ids::{AristaId, NodoId};
use crate::path::BundlePath;
use crate::tipos::{Etiqueta, Sorte};

/// Máximo de nodos `Formula` en una ruta del AST; coincide con el límite del
/// parser de autoría, que admite 32 niveles y rechaza el 33.º.
pub const MAX_PROFUNDIDAD_AST: usize = 32;
/// Cantidad máxima de variables que puede ligar un cuantificador.
pub const MAX_LIGADURAS_POR_CUANTIFICADOR: usize = 64;
/// Presupuesto determinista predeterminado, medido en unidades de trabajo y
/// no en tiempo de pared.
pub const PRESUPUESTO_EVALUACION_POR_DEFECTO: u64 = 1_000_000;

/// Error al evaluar una fórmula que no está bien formada sobre la firma de
/// la estructura dada, o que no es una oración cerrada.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorEvaluacion {
    ProfundidadAstExcedida {
        maximo: usize,
    },
    DemasiadasLigaduras {
        cantidad: usize,
        maximo: usize,
    },
    PresupuestoAgotado {
        limite: u64,
    },
    VariableNoLigada {
        variable: Var,
        sorte: SorteVar,
    },
    VariableDeSorteIncorrecta {
        variable: Var,
        esperada: SorteVar,
    },
    VariableRedeclarada(Var),
    ConstanteNodoInexistente(BundlePath),
    ConstanteAristaInexistente(IdArista),
    SorteNodoNoDeclarada(Sorte),
    SorteAristaNoDeclarada(Sorte),
    EtiquetaNoEnUso(Etiqueta),
    PredicadoNoDeclarado(String),
    ConstanteFueraDeDominio {
        predicado: String,
        constante: String,
    },
    MetaNoInstanciada(Var),
    NodoLigadoInexistente(Var),
    AristaLigadaInexistente(Var),
}

impl std::fmt::Display for ErrorEvaluacion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProfundidadAstExcedida { maximo } => write!(
                f,
                "la fórmula excede la profundidad máxima de AST ({maximo})"
            ),
            Self::DemasiadasLigaduras { cantidad, maximo } => write!(
                f,
                "el cuantificador tiene {cantidad} ligaduras; el máximo es {maximo}"
            ),
            Self::PresupuestoAgotado { limite } => write!(
                f,
                "se agotó el presupuesto de evaluación ({limite} unidades de trabajo)"
            ),
            Self::VariableNoLigada { variable, sorte } => write!(
                f,
                "la variable {variable} de sorte {} no está ligada",
                sorte.como_str()
            ),
            Self::VariableDeSorteIncorrecta { variable, esperada } => write!(
                f,
                "la variable {variable} no está ligada con sorte {}",
                esperada.como_str()
            ),
            Self::VariableRedeclarada(variable) => {
                write!(f, "la variable {variable} está ligada más de una vez")
            }
            Self::ConstanteNodoInexistente(path) => {
                write!(f, "la constante de nodo @{path} no existe en la estructura")
            }
            Self::ConstanteAristaInexistente(id) => {
                write!(f, "la constante de arista @{id} no existe en la estructura")
            }
            Self::SorteNodoNoDeclarada(sorte) => {
                write!(f, "la sorte de nodo {sorte} no está declarada")
            }
            Self::SorteAristaNoDeclarada(sorte) => {
                write!(
                    f,
                    "la sorte de arista {} no está declarada",
                    sorte.como_str()
                )
            }
            Self::EtiquetaNoEnUso(etiqueta) => {
                write!(f, "la etiqueta {:?} no está en uso", etiqueta.como_str())
            }
            Self::PredicadoNoDeclarado(predicado) => {
                write!(
                    f,
                    "el predicado de extensión {predicado:?} no está declarado"
                )
            }
            Self::ConstanteFueraDeDominio {
                predicado,
                constante,
            } => write!(
                f,
                "la constante {constante:?} no pertenece al dominio de {predicado:?}"
            ),
            Self::MetaNoInstanciada(variable) => {
                write!(f, "la metavariable {variable} no fue instanciada")
            }
            Self::NodoLigadoInexistente(variable) => {
                write!(
                    f,
                    "la asignación de {variable} refiere a un nodo inexistente"
                )
            }
            Self::AristaLigadaInexistente(variable) => {
                write!(
                    f,
                    "la asignación de {variable} refiere a una arista inexistente"
                )
            }
        }
    }
}

impl std::error::Error for ErrorEvaluacion {}

struct Presupuesto {
    limite: u64,
    restante: u64,
}

impl Presupuesto {
    fn nuevo(limite: u64) -> Self {
        Self {
            limite,
            restante: limite,
        }
    }

    fn consumir(&mut self, cantidad: u64) -> Result<(), ErrorEvaluacion> {
        if self.restante < cantidad {
            return Err(ErrorEvaluacion::PresupuestoAgotado {
                limite: self.limite,
            });
        }
        self.restante -= cantidad;
        Ok(())
    }
}

/// Evalúa una fórmula cerrada en la estructura finita dada.
///
/// Primero comprueba toda la fórmula contra la firma y la estructura, incluso
/// las ramas que la evaluación booleana pudiera cortocircuitar. Así, una
/// constante desconocida no se convierte en `false` ni queda oculta por una
/// rama no visitada. Después aplica la semántica clásica de Tarski, con
/// [`PRESUPUESTO_EVALUACION_POR_DEFECTO`] unidades deterministas de trabajo.
///
/// Para un AST no confiable o construido directamente, antes de evaluar el
/// llamador debe validar la oración
/// `Sentencia::Oracion(formula.to_owned())` mediante
/// [`crate::invariante::validar_sentencia`], pasando `estructura.manifiesto()`
/// y `estructura`, y proceder solo si el vector de diagnósticos está vacío.
/// Aquí `estructura` ya es `&Estructura`; si el llamador tiene un valor propio,
/// debe pasar `&estructura`. Este evaluador no sustituye la validación estática
/// de autoría ni expande esquemas.
pub fn evaluar(formula: &Formula, estructura: &Estructura) -> Result<bool, ErrorEvaluacion> {
    evaluar_con_presupuesto(formula, estructura, PRESUPUESTO_EVALUACION_POR_DEFECTO)
}

/// Evalúa una fórmula cerrada con un presupuesto explícito de unidades de
/// trabajo. Se consume al visitar nodos del AST durante el preflight y la
/// evaluación, asignaciones de ligaduras, elementos de dominios inspeccionados,
/// coste estimado de ordenarlos y pasos de `tc Parent`. El resultado no depende
/// del tiempo de pared; si el límite no alcanza, devuelve
/// [`ErrorEvaluacion::PresupuestoAgotado`].
///
/// Para un AST no confiable o construido directamente, antes de evaluar el
/// llamador debe validar la oración
/// `Sentencia::Oracion(formula.to_owned())` mediante
/// [`crate::invariante::validar_sentencia`], pasando `estructura.manifiesto()`
/// y `estructura`, y proceder solo si el vector de diagnósticos está vacío.
/// Aquí `estructura` ya es `&Estructura`; si el llamador tiene un valor propio,
/// debe pasar `&estructura`. Este evaluador no sustituye la validación estática
/// de autoría ni expande esquemas.
pub fn evaluar_con_presupuesto(
    formula: &Formula,
    estructura: &Estructura,
    limite: u64,
) -> Result<bool, ErrorEvaluacion> {
    let mut presupuesto = Presupuesto::nuevo(limite);
    validar_formula(
        formula,
        estructura,
        &mut Alcance::default(),
        &mut presupuesto,
        0,
    )?;
    evaluar_con(
        formula,
        estructura,
        &mut Entorno::default(),
        &mut presupuesto,
    )
}

#[derive(Default)]
struct Alcance {
    activas: HashMap<Var, SorteVar>,
    declaradas: HashMap<Var, SorteVar>,
}

fn validar_formula(
    formula: &Formula,
    estructura: &Estructura,
    alcance: &mut Alcance,
    presupuesto: &mut Presupuesto,
    profundidad: usize,
) -> Result<(), ErrorEvaluacion> {
    presupuesto.consumir(1)?;
    if profundidad >= MAX_PROFUNDIDAD_AST {
        return Err(ErrorEvaluacion::ProfundidadAstExcedida {
            maximo: MAX_PROFUNDIDAD_AST,
        });
    }
    match formula {
        Formula::Type { sujeto, sorte } => {
            validar_nodo(sujeto, estructura, alcance)?;
            validar_sorte(sorte, estructura, SorteVar::Nodo)
        }
        Formula::Edge {
            arista,
            origen,
            destino,
            sorte,
        } => {
            validar_arista(arista, estructura, alcance, presupuesto)?;
            validar_nodo(origen, estructura, alcance)?;
            validar_nodo(destino, estructura, alcance)?;
            validar_sorte(sorte, estructura, SorteVar::Arista)
        }
        Formula::Parent { hijo, padre } => {
            validar_nodo(hijo, estructura, alcance)?;
            validar_nodo(padre, estructura, alcance)
        }
        Formula::HasLab { sujeto, etiqueta } => {
            match sujeto {
                SujetoTermino::Nodo(termino) => validar_nodo(termino, estructura, alcance)?,
                SujetoTermino::Arista(termino) => {
                    validar_arista(termino, estructura, alcance, presupuesto)?
                }
            }
            validar_etiqueta(etiqueta, estructura, presupuesto)
        }
        Formula::Ext {
            predicado,
            sujeto,
            constante,
        } => {
            validar_nodo(sujeto, estructura, alcance)?;
            let declaracion = estructura
                .manifiesto()
                .predicado(predicado)
                .ok_or_else(|| ErrorEvaluacion::PredicadoNoDeclarado(predicado.clone()))?;
            match constante {
                TerminoConstExt::Literal(valor) => {
                    if declaracion.dominio.contains(valor) {
                        Ok(())
                    } else {
                        Err(ErrorEvaluacion::ConstanteFueraDeDominio {
                            predicado: predicado.clone(),
                            constante: valor.clone(),
                        })
                    }
                }
                TerminoConstExt::Meta(variable) => {
                    Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone()))
                }
            }
        }
        Formula::TcParent { desde, hasta } => {
            validar_nodo(desde, estructura, alcance)?;
            validar_nodo(hasta, estructura, alcance)
        }
        Formula::IgualNodo(a, b) => {
            validar_nodo(a, estructura, alcance)?;
            validar_nodo(b, estructura, alcance)
        }
        Formula::IgualArista(a, b) => {
            validar_arista(a, estructura, alcance, presupuesto)?;
            validar_arista(b, estructura, alcance, presupuesto)
        }
        Formula::No(interior) => {
            validar_formula(interior, estructura, alcance, presupuesto, profundidad + 1)
        }
        Formula::Y(partes) | Formula::O(partes) => {
            for parte in partes {
                validar_formula(parte, estructura, alcance, presupuesto, profundidad + 1)?;
            }
            Ok(())
        }
        Formula::Implica(antecedente, consecuente) => {
            validar_formula(
                antecedente,
                estructura,
                alcance,
                presupuesto,
                profundidad + 1,
            )?;
            validar_formula(
                consecuente,
                estructura,
                alcance,
                presupuesto,
                profundidad + 1,
            )
        }
        Formula::ParaTodo {
            ligaduras,
            guarda,
            cuerpo,
        }
        | Formula::Existe {
            ligaduras,
            guarda,
            cuerpo,
        } => {
            if ligaduras.len() > MAX_LIGADURAS_POR_CUANTIFICADOR {
                return Err(ErrorEvaluacion::DemasiadasLigaduras {
                    cantidad: ligaduras.len(),
                    maximo: MAX_LIGADURAS_POR_CUANTIFICADOR,
                });
            }
            let anteriores = alcance.activas.clone();
            let mut locales = HashSet::new();
            for (variable, sorte) in ligaduras {
                presupuesto.consumir(1)?;
                if !locales.insert(variable.clone()) || alcance.activas.contains_key(variable) {
                    return Err(ErrorEvaluacion::VariableRedeclarada(variable.clone()));
                }
                if alcance
                    .declaradas
                    .get(variable)
                    .is_some_and(|declarada| declarada != sorte)
                {
                    return Err(ErrorEvaluacion::VariableRedeclarada(variable.clone()));
                }
                alcance.declaradas.entry(variable.clone()).or_insert(*sorte);
                alcance.activas.insert(variable.clone(), *sorte);
            }
            let resultado = (|| {
                if let Some(guarda) = guarda {
                    validar_formula(guarda, estructura, alcance, presupuesto, profundidad + 1)?;
                }
                validar_formula(cuerpo, estructura, alcance, presupuesto, profundidad + 1)
            })();
            alcance.activas = anteriores;
            resultado
        }
    }
}

fn validar_nodo(
    termino: &TerminoNodo,
    estructura: &Estructura,
    alcance: &Alcance,
) -> Result<(), ErrorEvaluacion> {
    match termino {
        TerminoNodo::Var(variable) => validar_variable(variable, SorteVar::Nodo, alcance),
        TerminoNodo::Constante(path) => estructura
            .por_path(path)
            .map(|_| ())
            .ok_or_else(|| ErrorEvaluacion::ConstanteNodoInexistente(path.clone())),
    }
}

fn validar_arista(
    termino: &TerminoArista,
    estructura: &Estructura,
    alcance: &Alcance,
    presupuesto: &mut Presupuesto,
) -> Result<(), ErrorEvaluacion> {
    match termino {
        TerminoArista::Var(variable) => validar_variable(variable, SorteVar::Arista, alcance),
        TerminoArista::Constante(id) => {
            if encontrar_arista(estructura, id, presupuesto)?.is_some() {
                Ok(())
            } else {
                Err(ErrorEvaluacion::ConstanteAristaInexistente(id.clone()))
            }
        }
    }
}

fn validar_variable(
    variable: &Var,
    esperada: SorteVar,
    alcance: &Alcance,
) -> Result<(), ErrorEvaluacion> {
    match alcance.activas.get(variable) {
        Some(encontrada) if *encontrada == esperada => Ok(()),
        Some(_) => Err(ErrorEvaluacion::VariableDeSorteIncorrecta {
            variable: variable.clone(),
            esperada,
        }),
        None => Err(ErrorEvaluacion::VariableNoLigada {
            variable: variable.clone(),
            sorte: esperada,
        }),
    }
}

fn validar_sorte(
    termino: &TerminoSorte,
    estructura: &Estructura,
    contexto: SorteVar,
) -> Result<(), ErrorEvaluacion> {
    match termino {
        TerminoSorte::Meta(variable) => Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone())),
        TerminoSorte::Literal(sorte) => match contexto {
            SorteVar::Nodo if estructura.manifiesto().es_sorte_nodo(sorte) => Ok(()),
            SorteVar::Nodo => Err(ErrorEvaluacion::SorteNodoNoDeclarada(sorte.clone())),
            SorteVar::Arista if estructura.manifiesto().es_sorte_arista(sorte.como_str()) => Ok(()),
            SorteVar::Arista => Err(ErrorEvaluacion::SorteAristaNoDeclarada(sorte.clone())),
        },
    }
}

fn validar_etiqueta(
    termino: &TerminoEtiqueta,
    estructura: &Estructura,
    presupuesto: &mut Presupuesto,
) -> Result<(), ErrorEvaluacion> {
    match termino {
        TerminoEtiqueta::Meta(variable) => {
            Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone()))
        }
        TerminoEtiqueta::Literal(etiqueta) => {
            for (_, nodo) in estructura.nodos() {
                presupuesto.consumir(1)?;
                if nodo.etiqueta == *etiqueta {
                    return Ok(());
                }
            }
            for (_, arista) in estructura.aristas() {
                presupuesto.consumir(1)?;
                if arista.etiqueta == *etiqueta {
                    return Ok(());
                }
            }
            Err(ErrorEvaluacion::EtiquetaNoEnUso(etiqueta.clone()))
        }
    }
}

#[derive(Default)]
struct Entorno {
    nodos: HashMap<Var, NodoId>,
    aristas: HashMap<Var, AristaId>,
}

fn evaluar_con(
    formula: &Formula,
    estructura: &Estructura,
    entorno: &mut Entorno,
    presupuesto: &mut Presupuesto,
) -> Result<bool, ErrorEvaluacion> {
    presupuesto.consumir(1)?;
    match formula {
        Formula::Type { sujeto, sorte } => {
            let id = resolver_nodo(sujeto, estructura, entorno)?;
            let esperado = resolver_sorte(sorte)?;
            let nodo = estructura.nodo(id).ok_or_else(|| {
                ErrorEvaluacion::NodoLigadoInexistente(nombre_variable_nodo(sujeto))
            })?;
            Ok(nodo.tipo == esperado)
        }
        Formula::Edge {
            arista,
            origen,
            destino,
            sorte,
        } => {
            let id = resolver_arista(arista, estructura, entorno, presupuesto)?;
            let origen = resolver_nodo(origen, estructura, entorno)?;
            let destino = resolver_nodo(destino, estructura, entorno)?;
            let sorte = resolver_sorte(sorte)?;
            let arista = estructura.arista(id).ok_or_else(|| {
                ErrorEvaluacion::AristaLigadaInexistente(nombre_variable_arista(arista))
            })?;
            Ok(arista.origen == origen && arista.destino == destino && arista.tipo == sorte)
        }
        Formula::Parent { hijo, padre } => {
            let hijo = resolver_nodo(hijo, estructura, entorno)?;
            let padre = resolver_nodo(padre, estructura, entorno)?;
            Ok(estructura.padre(hijo) == Some(padre))
        }
        Formula::HasLab { sujeto, etiqueta } => {
            let etiqueta = resolver_etiqueta(etiqueta)?;
            match sujeto {
                SujetoTermino::Nodo(termino) => {
                    let id = resolver_nodo(termino, estructura, entorno)?;
                    let nodo = estructura.nodo(id).ok_or_else(|| {
                        ErrorEvaluacion::NodoLigadoInexistente(nombre_variable_nodo(termino))
                    })?;
                    Ok(nodo.etiqueta == etiqueta)
                }
                SujetoTermino::Arista(termino) => {
                    let id = resolver_arista(termino, estructura, entorno, presupuesto)?;
                    let arista = estructura.arista(id).ok_or_else(|| {
                        ErrorEvaluacion::AristaLigadaInexistente(nombre_variable_arista(termino))
                    })?;
                    Ok(arista.etiqueta == etiqueta)
                }
            }
        }
        Formula::Ext {
            predicado,
            sujeto,
            constante,
        } => {
            let id = resolver_nodo(sujeto, estructura, entorno)?;
            let constante = resolver_const_ext(constante)?;
            if estructura.nodo(id).is_none() {
                return Err(ErrorEvaluacion::NodoLigadoInexistente(
                    nombre_variable_nodo(sujeto),
                ));
            }
            Ok(estructura.atributos(id).get(predicado) == Some(&constante))
        }
        Formula::TcParent { desde, hasta } => {
            let desde = resolver_nodo(desde, estructura, entorno)?;
            let hasta = resolver_nodo(hasta, estructura, entorno)?;
            let mut actual = estructura.padre(desde);
            let mut visitados = HashSet::new();
            while let Some(ancestro) = actual {
                presupuesto.consumir(1)?;
                if ancestro == hasta {
                    return Ok(true);
                }
                if !visitados.insert(ancestro) {
                    break;
                }
                actual = estructura.padre(ancestro);
            }
            Ok(false)
        }
        Formula::IgualNodo(a, b) => {
            Ok(resolver_nodo(a, estructura, entorno)? == resolver_nodo(b, estructura, entorno)?)
        }
        Formula::IgualArista(a, b) => Ok(resolver_arista(a, estructura, entorno, presupuesto)?
            == resolver_arista(b, estructura, entorno, presupuesto)?),
        Formula::No(interior) => Ok(!evaluar_con(interior, estructura, entorno, presupuesto)?),
        Formula::Y(partes) => {
            for parte in partes {
                if !evaluar_con(parte, estructura, entorno, presupuesto)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Formula::O(partes) => {
            for parte in partes {
                if evaluar_con(parte, estructura, entorno, presupuesto)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Formula::Implica(antecedente, consecuente) => {
            if !evaluar_con(antecedente, estructura, entorno, presupuesto)? {
                Ok(true)
            } else {
                evaluar_con(consecuente, estructura, entorno, presupuesto)
            }
        }
        Formula::ParaTodo {
            ligaduras,
            guarda,
            cuerpo,
        } => evaluar_cuantificador(
            ligaduras,
            guarda.as_deref(),
            cuerpo,
            true,
            estructura,
            entorno,
            presupuesto,
        ),
        Formula::Existe {
            ligaduras,
            guarda,
            cuerpo,
        } => evaluar_cuantificador(
            ligaduras,
            guarda.as_deref(),
            cuerpo,
            false,
            estructura,
            entorno,
            presupuesto,
        ),
    }
}

fn evaluar_cuantificador(
    ligaduras: &[(Var, SorteVar)],
    guarda: Option<&Formula>,
    cuerpo: &Formula,
    universal: bool,
    estructura: &Estructura,
    entorno: &mut Entorno,
    presupuesto: &mut Presupuesto,
) -> Result<bool, ErrorEvaluacion> {
    if ligaduras.is_empty() {
        presupuesto.consumir(1)?;
        return evaluar_instancia_cuantificada(
            guarda,
            cuerpo,
            universal,
            estructura,
            entorno,
            presupuesto,
        );
    }

    let mut dominio_nodos = Vec::new();
    for (id, _) in estructura.nodos() {
        presupuesto.consumir(1)?;
        dominio_nodos.push(id);
    }
    consumir_costo_ordenamiento(presupuesto, dominio_nodos.len())?;
    dominio_nodos.sort_by(|a, b| {
        estructura
            .nodo(*a)
            .map(|n| &n.path)
            .cmp(&estructura.nodo(*b).map(|n| &n.path))
    });
    let mut dominio_aristas = Vec::new();
    for (id, _) in estructura.aristas() {
        presupuesto.consumir(1)?;
        dominio_aristas.push(id);
    }
    consumir_costo_ordenamiento(presupuesto, dominio_aristas.len())?;
    dominio_aristas.sort_by(|a, b| {
        identidad_arista(estructura, *a)
            .map(|id| id.to_string())
            .cmp(&identidad_arista(estructura, *b).map(|id| id.to_string()))
    });

    // El producto cartesiano se recorre con un odómetro. Así, el número de
    // ligaduras no añade recursión a la pila; solo la profundidad del AST lo
    // hace, y esa queda limitada por el preflight.
    if ligaduras.iter().any(|(_, sorte)| match sorte {
        SorteVar::Nodo => dominio_nodos.is_empty(),
        SorteVar::Arista => dominio_aristas.is_empty(),
    }) {
        return Ok(universal);
    }

    let anteriores_nodos: Vec<(Var, Option<NodoId>)> = ligaduras
        .iter()
        .filter(|(_, sorte)| *sorte == SorteVar::Nodo)
        .map(|(variable, _)| (variable.clone(), entorno.nodos.get(variable).copied()))
        .collect();
    let anteriores_aristas: Vec<(Var, Option<AristaId>)> = ligaduras
        .iter()
        .filter(|(_, sorte)| *sorte == SorteVar::Arista)
        .map(|(variable, _)| (variable.clone(), entorno.aristas.get(variable).copied()))
        .collect();

    let resultado = (|| {
        let mut indices = vec![0; ligaduras.len()];
        loop {
            presupuesto.consumir(1)?;
            for (indice, (variable, sorte)) in ligaduras.iter().enumerate() {
                presupuesto.consumir(1)?;
                match sorte {
                    SorteVar::Nodo => {
                        entorno
                            .nodos
                            .insert(variable.clone(), dominio_nodos[indices[indice]]);
                    }
                    SorteVar::Arista => {
                        entorno
                            .aristas
                            .insert(variable.clone(), dominio_aristas[indices[indice]]);
                    }
                }
            }

            let valor = evaluar_instancia_cuantificada(
                guarda,
                cuerpo,
                universal,
                estructura,
                entorno,
                presupuesto,
            )?;
            if universal && !valor {
                return Ok(false);
            }
            if !universal && valor {
                return Ok(true);
            }
            if !avanzar_combinacion(
                &mut indices,
                ligaduras,
                dominio_nodos.len(),
                dominio_aristas.len(),
            ) {
                return Ok(universal);
            }
        }
    })();

    for (variable, anterior) in anteriores_nodos {
        restaurar(&mut entorno.nodos, &variable, anterior);
    }
    for (variable, anterior) in anteriores_aristas {
        restaurar(&mut entorno.aristas, &variable, anterior);
    }
    resultado
}

fn evaluar_instancia_cuantificada(
    guarda: Option<&Formula>,
    cuerpo: &Formula,
    universal: bool,
    estructura: &Estructura,
    entorno: &mut Entorno,
    presupuesto: &mut Presupuesto,
) -> Result<bool, ErrorEvaluacion> {
    let cubierta = match guarda {
        Some(guarda) => evaluar_con(guarda, estructura, entorno, presupuesto)?,
        None => true,
    };
    if !cubierta {
        return Ok(universal);
    }
    evaluar_con(cuerpo, estructura, entorno, presupuesto)
}

fn avanzar_combinacion(
    indices: &mut [usize],
    ligaduras: &[(Var, SorteVar)],
    cantidad_nodos: usize,
    cantidad_aristas: usize,
) -> bool {
    for indice in (0..ligaduras.len()).rev() {
        indices[indice] += 1;
        let cantidad = match ligaduras[indice].1 {
            SorteVar::Nodo => cantidad_nodos,
            SorteVar::Arista => cantidad_aristas,
        };
        if indices[indice] < cantidad {
            return true;
        }
        indices[indice] = 0;
    }
    false
}

fn consumir_costo_ordenamiento(
    presupuesto: &mut Presupuesto,
    cantidad: usize,
) -> Result<(), ErrorEvaluacion> {
    if cantidad < 2 {
        return Ok(());
    }
    let niveles = usize::BITS - (cantidad - 1).leading_zeros();
    let cantidad = u64::try_from(cantidad).unwrap_or(u64::MAX);
    let costo = cantidad.saturating_mul(u64::from(niveles));
    presupuesto.consumir(costo)
}

fn restaurar<K: std::hash::Hash + Eq + Clone, V>(
    entorno: &mut HashMap<K, V>,
    llave: &K,
    anterior: Option<V>,
) {
    if let Some(anterior) = anterior {
        entorno.insert(llave.clone(), anterior);
    } else {
        entorno.remove(llave);
    }
}

fn resolver_nodo(
    termino: &TerminoNodo,
    estructura: &Estructura,
    entorno: &Entorno,
) -> Result<NodoId, ErrorEvaluacion> {
    match termino {
        TerminoNodo::Var(variable) => {
            entorno
                .nodos
                .get(variable)
                .copied()
                .ok_or_else(|| ErrorEvaluacion::VariableNoLigada {
                    variable: variable.clone(),
                    sorte: SorteVar::Nodo,
                })
        }
        TerminoNodo::Constante(path) => estructura
            .por_path(path)
            .ok_or_else(|| ErrorEvaluacion::ConstanteNodoInexistente(path.clone())),
    }
}

fn resolver_arista(
    termino: &TerminoArista,
    estructura: &Estructura,
    entorno: &Entorno,
    presupuesto: &mut Presupuesto,
) -> Result<AristaId, ErrorEvaluacion> {
    match termino {
        TerminoArista::Var(variable) => entorno.aristas.get(variable).copied().ok_or_else(|| {
            ErrorEvaluacion::VariableNoLigada {
                variable: variable.clone(),
                sorte: SorteVar::Arista,
            }
        }),
        TerminoArista::Constante(id) => encontrar_arista(estructura, id, presupuesto)?
            .ok_or_else(|| ErrorEvaluacion::ConstanteAristaInexistente(id.clone())),
    }
}

fn resolver_sorte(termino: &TerminoSorte) -> Result<Sorte, ErrorEvaluacion> {
    match termino {
        TerminoSorte::Literal(sorte) => Ok(sorte.clone()),
        TerminoSorte::Meta(variable) => Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone())),
    }
}

fn resolver_etiqueta(termino: &TerminoEtiqueta) -> Result<Etiqueta, ErrorEvaluacion> {
    match termino {
        TerminoEtiqueta::Literal(etiqueta) => Ok(etiqueta.clone()),
        TerminoEtiqueta::Meta(variable) => {
            Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone()))
        }
    }
}

fn resolver_const_ext(termino: &TerminoConstExt) -> Result<String, ErrorEvaluacion> {
    match termino {
        TerminoConstExt::Literal(constante) => Ok(constante.clone()),
        TerminoConstExt::Meta(variable) => {
            Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone()))
        }
    }
}

fn identidad_arista(estructura: &Estructura, id: AristaId) -> Option<IdArista> {
    let arista = estructura.arista(id)?;
    Some(IdArista {
        origen: estructura.nodo(arista.origen)?.path.clone(),
        sorte: arista.tipo.clone(),
        destino: estructura.nodo(arista.destino)?.path.clone(),
        ordinal: arista.ordinal,
    })
}

fn encontrar_arista(
    estructura: &Estructura,
    buscada: &IdArista,
    presupuesto: &mut Presupuesto,
) -> Result<Option<AristaId>, ErrorEvaluacion> {
    for (id, _) in estructura.aristas() {
        presupuesto.consumir(1)?;
        if identidad_arista(estructura, id).as_ref() == Some(buscada) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn nombre_variable_nodo(termino: &TerminoNodo) -> Var {
    match termino {
        TerminoNodo::Var(variable) => variable.clone(),
        TerminoNodo::Constante(path) => Var::nuevo(&format!("@{path}")),
    }
}

fn nombre_variable_arista(termino: &TerminoArista) -> Var {
    match termino {
        TerminoArista::Var(variable) => variable.clone(),
        TerminoArista::Constante(id) => Var::nuevo(&format!("@{id}")),
    }
}

#[cfg(test)]
#[path = "evaluador/tests.rs"]
mod tests;
