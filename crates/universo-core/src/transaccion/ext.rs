use std::collections::{BTreeMap, BTreeSet};

use crate::{Atomo, BundlePath, Delta, Estructura, NodoId};

use super::{ClaseOperacion, ErrorCandidato};

#[derive(Default)]
struct Solicitud {
    nodo: Option<NodoId>,
    altas: BTreeSet<String>,
    retiros: BTreeSet<String>,
}

#[derive(Default)]
struct Fallos {
    alta: bool,
    retiro: bool,
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
    let mut fallos = Fallos::default();
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
                marcar_fallo(&mut fallos, lado);
                continue;
            }

            let Some(nodo) = snapshot.por_path(sujeto) else {
                marcar_fallo(&mut fallos, lado);
                continue;
            };
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
        if solicitud.altas.len() > 1 {
            fallos.alta = true;
        }
        if solicitud.retiros.len() > 1 {
            fallos.retiro = true;
        }
        let nodo = solicitud
            .nodo
            .ok_or(ErrorCandidato::SnapshotInconsistente)?;
        let vigente = snapshot.atributos(nodo).get(&predicado).cloned();
        let retiro_coincide = solicitud_retiros_coincide(&solicitud, vigente.as_deref());
        let tiene_retiro_vigente = solicitud
            .retiros
            .iter()
            .any(|valor| vigente.as_deref() == Some(valor.as_str()));
        let retirar = solicitud.retiros.iter().next().cloned();
        let agregar = solicitud.altas.iter().next().cloned();

        if !retiro_coincide {
            fallos.retiro = true;
        }
        if agregar
            .as_deref()
            .is_some_and(|nuevo| vigente.as_deref().is_some_and(|actual| actual != nuevo))
            && !tiene_retiro_vigente
        {
            fallos.alta = true;
        }

        cambios.push(Cambio {
            nodo,
            predicado,
            vigente,
            retirar,
            agregar,
        });
    }

    if fallos.alta {
        return Err(conflicto_ext());
    }
    if fallos.retiro {
        return Err(remove_not_found_ext());
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

fn remove_not_found_ext() -> ErrorCandidato {
    ErrorCandidato::RemoveNotFound {
        clase: ClaseOperacion::Ext,
    }
}

fn marcar_fallo(fallos: &mut Fallos, lado_retiro: bool) {
    if lado_retiro {
        fallos.retiro = true;
    } else {
        fallos.alta = true;
    }
}

fn solicitud_retiros_coincide(solicitud: &Solicitud, vigente: Option<&str>) -> bool {
    solicitud.retiros.is_empty()
        || (solicitud.retiros.len() == 1
            && vigente.is_some_and(|valor| solicitud.retiros.contains(valor)))
}
