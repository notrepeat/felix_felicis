use std::collections::{BTreeMap, BTreeSet};

use crate::{Atomo, BundlePath, Delta, Estructura, NodoId};

use super::{ClaseOperacion, ErrorCandidato};

#[derive(Default)]
struct Solicitud {
    nodo: Option<NodoId>,
    altas: BTreeSet<String>,
    retiros: BTreeSet<String>,
}

pub(super) struct Cambio {
    nodo: NodoId,
    predicado: String,
    vigente: Option<String>,
    retirar: Option<String>,
    agregar: Option<String>,
}

/// Resuelve sujetos y valida todo el parche Ext contra el snapshot sin
/// modificarlo.
pub(super) fn preparar(
    snapshot: &Estructura,
    delta: &Delta,
) -> Result<Vec<Cambio>, ErrorCandidato> {
    let mut solicitudes = BTreeMap::<(String, BundlePath), Solicitud>::new();
    for (lado, atomos) in [(false, delta.agregar()), (true, delta.retirar())] {
        for atomo in atomos {
            let Atomo::Ext {
                predicado,
                sujeto,
                constante,
            } = atomo
            else {
                continue;
            };

            let declarado = snapshot
                .manifiesto()
                .predicado(predicado)
                .is_some_and(|declaracion| declaracion.dominio.contains(constante));
            if !declarado {
                return Err(conflicto_ext());
            }

            let nodo = snapshot.por_path(sujeto).ok_or_else(|| {
                if lado {
                    ErrorCandidato::RemoveNotFound {
                        clase: ClaseOperacion::Ext,
                    }
                } else {
                    conflicto_ext()
                }
            })?;
            let solicitud = solicitudes
                .entry((predicado.clone(), sujeto.clone()))
                .or_default();
            solicitud.nodo = Some(nodo);
            if lado {
                solicitud.retiros.insert(constante.clone());
            } else {
                solicitud.altas.insert(constante.clone());
            }
        }
    }

    let mut cambios = Vec::with_capacity(solicitudes.len());
    for ((predicado, _), solicitud) in solicitudes {
        if solicitud.altas.len() > 1 || solicitud.retiros.len() > 1 {
            return Err(conflicto_ext());
        }
        let nodo = solicitud
            .nodo
            .ok_or(ErrorCandidato::SnapshotInconsistente)?;
        let vigente = snapshot.atributos(nodo).get(&predicado).cloned();
        let retirar = solicitud.retiros.into_iter().next();
        let agregar = solicitud.altas.into_iter().next();

        if retirar
            .as_deref()
            .is_some_and(|valor| vigente.as_deref() != Some(valor))
        {
            return Err(ErrorCandidato::RemoveNotFound {
                clase: ClaseOperacion::Ext,
            });
        }
        if agregar
            .as_deref()
            .is_some_and(|nuevo| vigente.as_deref().is_some_and(|actual| actual != nuevo))
            && retirar.is_none()
        {
            return Err(conflicto_ext());
        }

        cambios.push(Cambio {
            nodo,
            predicado,
            vigente,
            retirar,
            agregar,
        });
    }
    Ok(cambios)
}

/// Aplica al candidato el plan ya validado contra el snapshot.
pub(super) fn aplicar(
    candidato: &mut Estructura,
    cambios: &[Cambio],
) -> Result<(), ErrorCandidato> {
    for cambio in cambios {
        if cambio.retirar.is_some() && cambio.agregar.as_ref() != cambio.vigente.as_ref() {
            candidato.quitar_atributo(cambio.nodo, &cambio.predicado);
        }
        if let Some(nuevo) = &cambio.agregar
            && cambio.vigente.as_deref() != Some(nuevo.as_str())
        {
            candidato
                .asignar_atributo(cambio.nodo, &cambio.predicado, nuevo)
                .map_err(|_| ErrorCandidato::SnapshotInconsistente)?;
        }
    }
    Ok(())
}

fn conflicto_ext() -> ErrorCandidato {
    ErrorCandidato::AddConflict {
        clase: ClaseOperacion::Ext,
    }
}
