use std::collections::{HashMap, HashSet};

use crate::atomos::IdArista;
use crate::estructura::Estructura;
use crate::evaluador::ErrorEvaluacion;
use crate::formula::{
    Formula, SorteVar, SujetoTermino, TerminoArista, TerminoConstExt, TerminoEtiqueta, TerminoNodo,
    TerminoSorte, Var,
};
use crate::ids::{AristaId, NodoId};
use crate::tipos::{Etiqueta, Sorte};

// Oracle independiente: consulta nodos/aristas y padre/atributos desde
// la estructura, sin usar el evaluador ni Estructura::atomos().
#[derive(Clone, Default)]
pub(super) struct AsignacionOraculo {
    nodos: HashMap<Var, NodoId>,
    aristas: HashMap<Var, AristaId>,
}

impl AsignacionOraculo {
    pub(super) fn ligar_nodo(&mut self, variable: Var, nodo: NodoId) {
        self.nodos.insert(variable, nodo);
    }

    pub(super) fn ligar_arista(&mut self, variable: Var, arista: AristaId) {
        self.aristas.insert(variable, arista);
    }
}

fn buscar_nodo_oraculo(
    termino: &TerminoNodo,
    estructura: &Estructura,
    asignacion: &AsignacionOraculo,
) -> Result<NodoId, ErrorEvaluacion> {
    match termino {
        TerminoNodo::Var(variable) => asignacion.nodos.get(variable).copied().ok_or_else(|| {
            ErrorEvaluacion::VariableNoLigada {
                variable: variable.clone(),
                sorte: SorteVar::Nodo,
            }
        }),
        TerminoNodo::Constante(path) => estructura
            .por_path(path)
            .ok_or_else(|| ErrorEvaluacion::ConstanteNodoInexistente(path.clone())),
    }
}

fn identidad_arista_oraculo(estructura: &Estructura, id: AristaId) -> Option<IdArista> {
    let e = estructura.arista(id)?;
    let origen = estructura.nodo(e.origen)?.path.clone();
    let destino = estructura.nodo(e.destino)?.path.clone();
    Some(IdArista {
        origen,
        sorte: e.tipo.clone(),
        destino,
        ordinal: e.ordinal,
    })
}

fn cierre_parent_oraculo(estructura: &Estructura) -> HashSet<(NodoId, NodoId)> {
    let mut cierre: HashSet<(NodoId, NodoId)> = estructura
        .nodos()
        .filter_map(|(hijo, _)| estructura.padre(hijo).map(|padre| (hijo, padre)))
        .collect();
    loop {
        let mut nuevas = HashSet::new();
        for (inicio, intermedio) in cierre.iter().copied() {
            for (otro_inicio, fin) in cierre.iter().copied() {
                if intermedio == otro_inicio {
                    nuevas.insert((inicio, fin));
                }
            }
        }
        let longitud = cierre.len();
        cierre.extend(nuevas);
        if cierre.len() == longitud {
            return cierre;
        }
    }
}

fn restaurar_asignacion_oraculo<K: std::hash::Hash + Eq + Clone, V>(
    asignacion: &mut HashMap<K, V>,
    llave: &K,
    anterior: Option<V>,
) {
    match anterior {
        Some(valor) => {
            asignacion.insert(llave.clone(), valor);
        }
        None => {
            asignacion.remove(llave);
        }
    }
}

fn buscar_arista_oraculo(
    termino: &TerminoArista,
    estructura: &Estructura,
    asignacion: &AsignacionOraculo,
) -> Result<AristaId, ErrorEvaluacion> {
    match termino {
        TerminoArista::Var(variable) => {
            asignacion.aristas.get(variable).copied().ok_or_else(|| {
                ErrorEvaluacion::VariableNoLigada {
                    variable: variable.clone(),
                    sorte: SorteVar::Arista,
                }
            })
        }
        TerminoArista::Constante(id) => estructura
            .aristas()
            .map(|(candidato, _)| candidato)
            .find(|candidato| identidad_arista_oraculo(estructura, *candidato).as_ref() == Some(id))
            .ok_or_else(|| ErrorEvaluacion::ConstanteAristaInexistente(id.clone())),
    }
}

fn valor_sorte_oraculo(termino: &TerminoSorte) -> Result<Sorte, ErrorEvaluacion> {
    match termino {
        TerminoSorte::Literal(sorte) => Ok(sorte.clone()),
        TerminoSorte::Meta(variable) => Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone())),
    }
}

fn valor_etiqueta_oraculo(termino: &TerminoEtiqueta) -> Result<Etiqueta, ErrorEvaluacion> {
    match termino {
        TerminoEtiqueta::Literal(etiqueta) => Ok(etiqueta.clone()),
        TerminoEtiqueta::Meta(variable) => {
            Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone()))
        }
    }
}

fn valor_ext_oraculo(termino: &TerminoConstExt) -> Result<String, ErrorEvaluacion> {
    match termino {
        TerminoConstExt::Literal(valor) => Ok(valor.clone()),
        TerminoConstExt::Meta(variable) => {
            Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone()))
        }
    }
}

pub(super) fn oraculo(
    formula: &Formula,
    estructura: &Estructura,
    asignacion: &mut AsignacionOraculo,
) -> Result<bool, ErrorEvaluacion> {
    match formula {
        Formula::Type { sujeto, sorte } => {
            let id = buscar_nodo_oraculo(sujeto, estructura, asignacion)?;
            let sorte = valor_sorte_oraculo(sorte)?;
            Ok(estructura.nodo(id).is_some_and(|n| n.tipo == sorte))
        }
        Formula::Edge {
            arista,
            origen,
            destino,
            sorte,
        } => {
            let eid = buscar_arista_oraculo(arista, estructura, asignacion)?;
            let oid = buscar_nodo_oraculo(origen, estructura, asignacion)?;
            let did = buscar_nodo_oraculo(destino, estructura, asignacion)?;
            let sorte = valor_sorte_oraculo(sorte)?;
            Ok(estructura
                .arista(eid)
                .is_some_and(|e| e.origen == oid && e.destino == did && e.tipo == sorte))
        }
        Formula::Parent { hijo, padre } => {
            let hijo = buscar_nodo_oraculo(hijo, estructura, asignacion)?;
            let padre = buscar_nodo_oraculo(padre, estructura, asignacion)?;
            Ok(estructura.padre(hijo) == Some(padre))
        }
        Formula::HasLab { sujeto, etiqueta } => {
            let etiqueta = valor_etiqueta_oraculo(etiqueta)?;
            match sujeto {
                SujetoTermino::Nodo(termino) => {
                    let nodo = buscar_nodo_oraculo(termino, estructura, asignacion)?;
                    Ok(estructura
                        .nodo(nodo)
                        .is_some_and(|n| n.etiqueta == etiqueta))
                }
                SujetoTermino::Arista(termino) => {
                    let arista = buscar_arista_oraculo(termino, estructura, asignacion)?;
                    Ok(estructura
                        .arista(arista)
                        .is_some_and(|a| a.etiqueta == etiqueta))
                }
            }
        }
        Formula::Ext {
            predicado,
            sujeto,
            constante,
        } => {
            let nodo = buscar_nodo_oraculo(sujeto, estructura, asignacion)?;
            let constante = valor_ext_oraculo(constante)?;
            Ok(estructura.atributos(nodo).get(predicado) == Some(&constante))
        }
        Formula::TcParent { desde, hasta } => {
            let desde = buscar_nodo_oraculo(desde, estructura, asignacion)?;
            let hasta = buscar_nodo_oraculo(hasta, estructura, asignacion)?;
            Ok(cierre_parent_oraculo(estructura).contains(&(desde, hasta)))
        }
        Formula::IgualNodo(a, b) => Ok(buscar_nodo_oraculo(a, estructura, asignacion)?
            == buscar_nodo_oraculo(b, estructura, asignacion)?),
        Formula::IgualArista(a, b) => Ok(buscar_arista_oraculo(a, estructura, asignacion)?
            == buscar_arista_oraculo(b, estructura, asignacion)?),
        Formula::No(interior) => Ok(!oraculo(interior, estructura, asignacion)?),
        Formula::Y(partes) => {
            for parte in partes {
                if !oraculo(parte, estructura, asignacion)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Formula::O(partes) => {
            for parte in partes {
                if oraculo(parte, estructura, asignacion)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Formula::Implica(a, b) => {
            if !oraculo(a, estructura, asignacion)? {
                Ok(true)
            } else {
                oraculo(b, estructura, asignacion)
            }
        }
        Formula::ParaTodo {
            ligaduras,
            guarda,
            cuerpo,
        } => cuantificador_oraculo(
            ligaduras,
            guarda.as_deref(),
            cuerpo,
            true,
            estructura,
            asignacion,
        ),
        Formula::Existe {
            ligaduras,
            guarda,
            cuerpo,
        } => cuantificador_oraculo(
            ligaduras,
            guarda.as_deref(),
            cuerpo,
            false,
            estructura,
            asignacion,
        ),
    }
}

fn cuantificador_oraculo(
    ligaduras: &[(Var, SorteVar)],
    guarda: Option<&Formula>,
    cuerpo: &Formula,
    universal: bool,
    estructura: &Estructura,
    asignacion: &mut AsignacionOraculo,
) -> Result<bool, ErrorEvaluacion> {
    cuantificar_oraculo(
        0, ligaduras, guarda, cuerpo, universal, estructura, asignacion,
    )
}

fn cuantificar_oraculo(
    indice: usize,
    ligaduras: &[(Var, SorteVar)],
    guarda: Option<&Formula>,
    cuerpo: &Formula,
    universal: bool,
    estructura: &Estructura,
    asignacion: &mut AsignacionOraculo,
) -> Result<bool, ErrorEvaluacion> {
    if indice == ligaduras.len() {
        let aplica = match guarda {
            Some(guarda) => oraculo(guarda, estructura, asignacion)?,
            None => true,
        };
        if !aplica {
            return Ok(universal);
        }
        return oraculo(cuerpo, estructura, asignacion);
    }

    let (variable, sorte) = &ligaduras[indice];
    match sorte {
        SorteVar::Nodo => {
            for (id, _) in estructura.nodos() {
                let previo = asignacion.nodos.insert(variable.clone(), id);
                let resultado = cuantificar_oraculo(
                    indice + 1,
                    ligaduras,
                    guarda,
                    cuerpo,
                    universal,
                    estructura,
                    asignacion,
                );
                restaurar_asignacion_oraculo(&mut asignacion.nodos, variable, previo);
                let valor = resultado?;
                if universal && !valor {
                    return Ok(false);
                }
                if !universal && valor {
                    return Ok(true);
                }
            }
        }
        SorteVar::Arista => {
            for (id, _) in estructura.aristas() {
                let previo = asignacion.aristas.insert(variable.clone(), id);
                let resultado = cuantificar_oraculo(
                    indice + 1,
                    ligaduras,
                    guarda,
                    cuerpo,
                    universal,
                    estructura,
                    asignacion,
                );
                restaurar_asignacion_oraculo(&mut asignacion.aristas, variable, previo);
                let valor = resultado?;
                if universal && !valor {
                    return Ok(false);
                }
                if !universal && valor {
                    return Ok(true);
                }
            }
        }
    }
    Ok(universal)
}
