use std::collections::{HashMap, HashSet};

use crate::{Atomo, BundlePath, Delta, Estructura, Etiqueta, Sorte, Sujeto};

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
/// Los retiros solo sustraen el átomo exacto cuando está presente; las altas
/// idénticas a hechos vigentes no duplican valores. La función no construye ni
/// aplica un candidato, no evalúa invariantes, y no clasifica errores de retiro
/// o de soporte de operaciones. `Ext` se omite porque su validación pertenece
/// al procesamiento de candidato.
///
/// No se informa conflicto de incidencia en v1: `IdArista` ya contiene
/// origen, tipo, destino y ordinal, y `Atomo`/`Delta::nuevo` rechazan una
/// identidad de arista inconsistente con esos campos antes de esta llamada.
pub fn detectar_conflictos_funcionales(
    snapshot: &Estructura,
    delta: &Delta,
) -> ResultadoConflictos {
    let mut tipos = HashMap::<BundlePath, HashSet<Sorte>>::new();
    let mut etiquetas = HashMap::<Sujeto, HashSet<Etiqueta>>::new();
    let mut padres = HashMap::<BundlePath, HashSet<BundlePath>>::new();

    let finales = snapshot
        .atomos()
        .into_iter()
        .filter(|atomo| delta.retirar().binary_search(atomo).is_err())
        .chain(delta.agregar().iter().cloned());

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
    ResultadoConflictos { diagnosticos }
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
