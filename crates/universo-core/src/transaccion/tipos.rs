//! Type modifica la función total de tipado sobre identidades existentes.
//! Crear o eliminar identidades no se infiere de un hecho de tipado.
use std::collections::{BTreeMap, BTreeSet};

use super::{ClaseOperacion, ErrorCandidato};
use crate::{Atomo, BundlePath, Delta, Estructura, NodoId, Sorte};

#[derive(Default)]
struct Solicitud {
    agregar: BTreeSet<Sorte>,
    retirar: BTreeSet<Sorte>,
}

pub(super) struct CambiosTipos {
    cambios: Vec<(NodoId, Sorte)>,
    pub(super) conflicto: bool,
}

pub(super) fn preparar(
    snapshot: &Estructura,
    delta: &Delta,
) -> Result<CambiosTipos, ErrorCandidato> {
    let mut solicitudes = BTreeMap::<BundlePath, Solicitud>::new();
    for (retirar, atomos) in [(false, delta.agregar()), (true, delta.retirar())] {
        for atomo in atomos {
            if let Atomo::Type { sujeto, sorte } = atomo {
                let solicitud = solicitudes.entry(sujeto.clone()).or_default();
                if retirar {
                    solicitud.retirar.insert(sorte.clone());
                } else {
                    solicitud.agregar.insert(sorte.clone());
                }
            }
        }
    }
    let mut result = CambiosTipos {
        cambios: Vec::new(),
        conflicto: false,
    };
    for (path, solicitud) in solicitudes {
        let nodo = snapshot.por_path(&path).ok_or_else(|| {
            if !solicitud.retirar.is_empty() {
                ErrorCandidato::RemoveNotFound {
                    clase: ClaseOperacion::Type,
                }
            } else {
                invalido("subject_not_found")
            }
        })?;
        let actual = &snapshot
            .nodo(nodo)
            .ok_or(ErrorCandidato::SnapshotInconsistente)?
            .tipo;
        if solicitud.retirar.iter().any(|sorte| sorte != actual) {
            return Err(ErrorCandidato::RemoveNotFound {
                clase: ClaseOperacion::Type,
            });
        }
        if solicitud
            .agregar
            .iter()
            .any(|sorte| !snapshot.manifiesto().sortes_nodo.contains(sorte))
        {
            return Err(invalido("unknown_node_sort"));
        }
        let mut finales = solicitud.agregar;
        if !solicitud.retirar.contains(actual) {
            finales.insert(actual.clone());
        }
        match finales.len() {
            0 => return Err(invalido("type_replacement_required")),
            1 => result.cambios.push((nodo, finales.pop_first().unwrap())),
            _ => result.conflicto = true,
        }
    }
    Ok(result)
}

pub(super) fn aplicar(
    estructura: &mut Estructura,
    cambios: &CambiosTipos,
) -> Result<(), ErrorCandidato> {
    for (nodo, sorte) in &cambios.cambios {
        estructura
            .reemplazar_tipo_nodo(*nodo, sorte.clone())
            .map_err(|_| invalido("type_replacement_failed"))?;
    }
    Ok(())
}

fn invalido(codigo: &'static str) -> ErrorCandidato {
    ErrorCandidato::InvalidOperation {
        clase: ClaseOperacion::Type,
        codigo,
    }
}
