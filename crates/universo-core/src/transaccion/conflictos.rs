use std::collections::{HashMap, HashSet};

use crate::{Atomo, BundlePath, Delta, Estructura, Etiqueta, Sorte, Sujeto};

use super::aristas;
use super::candidato::{ErrorCandidato, construir_topologia_aristas, identidad_arista};

/// Clase de conflicto funcional detectada sobre el estado lógico final.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaseConflicto {
    Type,
    HasLab,
    Parent,
}

impl std::fmt::Display for ClaseConflicto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nombre = match self {
            Self::Type => "Type",
            Self::HasLab => "HasLab",
            Self::Parent => "Parent",
        };
        f.write_str(nombre)
    }
}

/// Conteo agregado de conflictos de una clase, sin identidades ni valores.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticoConflicto {
    clase: ClaseConflicto,
    cantidad: usize,
}

impl DiagnosticoConflicto {
    pub fn clase(&self) -> ClaseConflicto {
        self.clase
    }

    /// Cantidad de claves funcionales con más de un valor final distinto.
    pub fn cantidad(&self) -> usize {
        self.cantidad
    }
}

impl std::fmt::Display for DiagnosticoConflicto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.clase, self.cantidad)
    }
}

/// Resultado redactado de la detección de conflictos funcionales.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResultadoConflictos {
    diagnosticos: Vec<DiagnosticoConflicto>,
}

impl ResultadoConflictos {
    /// Diagnósticos en orden estable de clase, con solo clase y conteo.
    pub fn diagnosticos(&self) -> &[DiagnosticoConflicto] {
        &self.diagnosticos
    }

    pub fn hay_conflictos(&self) -> bool {
        !self.diagnosticos.is_empty()
    }

    /// Total de claves funcionales conflictivas.
    pub fn total_conflictos(&self) -> usize {
        self.diagnosticos
            .iter()
            .map(DiagnosticoConflicto::cantidad)
            .sum()
    }
}

impl std::fmt::Display for ResultadoConflictos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.diagnosticos.is_empty() {
            return f.write_str("sin_conflictos_funcionales");
        }
        f.write_str("conflictos_funcionales")?;
        for diagnostico in &self.diagnosticos {
            write!(f, "; {diagnostico}")?;
        }
        Ok(())
    }
}

/// Detecta conflictos funcionales sobre `snapshot - remove + add`.
///
/// La topología `Edge` final se normaliza con el constructor de candidato para
/// conservar la identidad por handle tras compactar ordinales. Las etiquetas
/// se agrupan aparte por identidad final, combinando las etiquetas de handles
/// supervivientes con altas válidas para esa topología; las altas idénticas no
/// duplican valores. Los errores técnicos de topología se devuelven redactados
/// en vez de ocultarse. `Type`, `Parent` y `HasLab` de nodo se superponen sobre
/// el snapshot usando retiros exactos y altas. No evalúa invariantes ni
/// aplica/publica el candidato. `Ext` se omite porque su validación pertenece
/// al procesamiento de candidato de otra etapa.
///
/// Este detector no valida `HasLab` huérfanos ni exige que una arista nueva
/// tenga una etiqueta suministrada por el parche. La validación completa del
/// parche corresponde a `construir_candidato` y al flujo transaccional de
/// `proponer`.
///
/// No se informa conflicto de incidencia en v1: `IdArista` ya contiene
/// origen, tipo, destino y ordinal, y `Atomo`/`Delta::nuevo` rechazan una
/// identidad de arista inconsistente con esos campos antes de esta llamada.
pub fn detectar_conflictos_funcionales(
    snapshot: &Estructura,
    delta: &Delta,
) -> Result<ResultadoConflictos, ErrorCandidato> {
    let mut tipos = HashMap::<BundlePath, HashSet<Sorte>>::new();
    let mut etiquetas = HashMap::<Sujeto, HashSet<Etiqueta>>::new();
    let mut padres = HashMap::<BundlePath, HashSet<BundlePath>>::new();

    let candidato = construir_topologia_aristas(snapshot, delta)?;
    let topologia_final = candidato.estructura();
    let finales = snapshot
        .atomos()
        .into_iter()
        .filter(es_hecho_funcional_de_nodo)
        .filter(|atomo| delta.retirar().binary_search(atomo).is_err())
        .chain(
            delta
                .agregar()
                .iter()
                .filter(|atomo| es_hecho_funcional_de_nodo(atomo))
                .cloned(),
        );

    for (handle, arista_snapshot) in snapshot.aristas() {
        if topologia_final.arista(handle).is_none() {
            continue;
        }
        let identidad_snapshot =
            identidad_arista(snapshot, handle).ok_or(ErrorCandidato::SnapshotInconsistente)?;
        let identidad_final = identidad_arista(topologia_final, handle)
            .ok_or(ErrorCandidato::SnapshotInconsistente)?;
        let etiqueta_snapshot = Atomo::HasLab {
            sujeto: Sujeto::Arista(identidad_snapshot),
            etiqueta: arista_snapshot.etiqueta.clone(),
        };
        if delta.retirar().binary_search(&etiqueta_snapshot).is_err() {
            etiquetas
                .entry(Sujeto::Arista(identidad_final))
                .or_default()
                .insert(arista_snapshot.etiqueta.clone());
        }
    }

    for atomo in delta.agregar() {
        let Atomo::HasLab {
            sujeto: Sujeto::Arista(identidad),
            etiqueta,
        } = atomo
        else {
            continue;
        };
        let Some(handle) = aristas::resolver(topologia_final, identidad) else {
            continue;
        };
        let identidad_final = identidad_arista(topologia_final, handle)
            .ok_or(ErrorCandidato::SnapshotInconsistente)?;
        etiquetas
            .entry(Sujeto::Arista(identidad_final))
            .or_default()
            .insert(etiqueta.clone());
    }

    for atomo in finales {
        match atomo {
            Atomo::Type { sujeto, sorte } => tipos.entry(sujeto).or_default().insert(sorte),
            Atomo::HasLab { sujeto, etiqueta } => {
                etiquetas.entry(sujeto).or_default().insert(etiqueta)
            }
            Atomo::Parent { hijo, padre } => padres.entry(hijo).or_default().insert(padre),
            Atomo::Edge { .. } | Atomo::Ext { .. } => false,
        };
    }

    let mut diagnosticos = Vec::new();
    agregar_diagnostico(&mut diagnosticos, ClaseConflicto::Type, &tipos);
    agregar_diagnostico(&mut diagnosticos, ClaseConflicto::HasLab, &etiquetas);
    agregar_diagnostico(&mut diagnosticos, ClaseConflicto::Parent, &padres);
    Ok(ResultadoConflictos { diagnosticos })
}

fn es_hecho_funcional_de_nodo(atomo: &Atomo) -> bool {
    matches!(
        atomo,
        Atomo::Type { .. }
            | Atomo::Parent { .. }
            | Atomo::HasLab {
                sujeto: Sujeto::Nodo(_),
                ..
            }
    )
}

fn agregar_diagnostico<K, V>(
    diagnosticos: &mut Vec<DiagnosticoConflicto>,
    clase: ClaseConflicto,
    valores_por_clave: &HashMap<K, HashSet<V>>,
) {
    let cantidad = valores_por_clave
        .values()
        .filter(|valores| valores.len() > 1)
        .count();
    if cantidad > 0 {
        diagnosticos.push(DiagnosticoConflicto { clase, cantidad });
    }
}
