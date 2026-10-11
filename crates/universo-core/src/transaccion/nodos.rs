use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::{Atomo, BundlePath, Delta, Estructura, Etiqueta, NodoId};

use super::{ClaseOperacion, ErrorCandidato};

#[derive(Default)]
struct SolicitudEtiqueta {
    nodo: Option<NodoId>,
    agregar: BTreeSet<Etiqueta>,
    retirar: BTreeSet<Etiqueta>,
}

#[derive(Default)]
struct SolicitudPadre {
    hijo: Option<NodoId>,
    agregar: BTreeSet<BundlePath>,
    retirar: BTreeSet<BundlePath>,
}

#[derive(Default)]
struct Fallos {
    agregar_haslab: bool,
    retirar_haslab: bool,
    agregar_parent: bool,
    retirar_parent: bool,
    invalidez_tecnica: Option<ErrorCandidato>,
}

#[derive(Default)]
pub(super) struct CambiosNodos {
    etiquetas: Vec<(NodoId, Etiqueta)>,
    retirar_padres: Vec<NodoId>,
    agregar_padres: Vec<(NodoId, NodoId)>,
    pub(super) conflicto_haslab: bool,
    pub(super) conflicto_parent: bool,
}

/// Resuelve los cambios funcionales de nodo contra el snapshot sin mutarlo.
pub(super) fn preparar(
    snapshot: &Estructura,
    delta: &Delta,
) -> Result<CambiosNodos, ErrorCandidato> {
    let mut etiquetas = BTreeMap::<BundlePath, SolicitudEtiqueta>::new();
    let mut padres = BTreeMap::<BundlePath, SolicitudPadre>::new();
    let mut fallos = Fallos::default();

    for (es_retiro, atomos) in [(false, delta.agregar()), (true, delta.retirar())] {
        for atomo in atomos {
            match atomo {
                Atomo::HasLab {
                    sujeto: crate::Sujeto::Nodo(path),
                    etiqueta,
                } => {
                    let Some(nodo) = snapshot.por_path(path) else {
                        if es_retiro {
                            fallos.retirar_haslab = true;
                        } else {
                            fallos.invalidez_tecnica.get_or_insert(
                                ErrorCandidato::InvalidOperation {
                                    clase: ClaseOperacion::HasLab,
                                    codigo: "subject_not_found",
                                },
                            );
                        }
                        continue;
                    };
                    if !es_retiro && etiqueta.como_str().contains(['\n', '\r']) {
                        fallos
                            .invalidez_tecnica
                            .get_or_insert(ErrorCandidato::InvalidOperation {
                                clase: ClaseOperacion::HasLab,
                                codigo: "label_contains_line_break",
                            });
                    }
                    let solicitud = etiquetas.entry(path.clone()).or_default();
                    solicitud.nodo = Some(nodo);
                    if es_retiro {
                        solicitud.retirar.insert(etiqueta.clone());
                    } else {
                        solicitud.agregar.insert(etiqueta.clone());
                    }
                }
                Atomo::Parent { hijo, padre } => {
                    let Some(nodo_hijo) = snapshot.por_path(hijo) else {
                        if es_retiro {
                            fallos.retirar_parent = true;
                        } else {
                            fallos.invalidez_tecnica.get_or_insert(
                                ErrorCandidato::InvalidOperation {
                                    clase: ClaseOperacion::Parent,
                                    codigo: "child_reference_not_found",
                                },
                            );
                        }
                        continue;
                    };
                    if snapshot.por_path(padre).is_none() {
                        if es_retiro {
                            fallos.retirar_parent = true;
                        } else {
                            fallos.invalidez_tecnica.get_or_insert(
                                ErrorCandidato::InvalidOperation {
                                    clase: ClaseOperacion::Parent,
                                    codigo: "parent_reference_not_found",
                                },
                            );
                        }
                        continue;
                    }
                    let solicitud = padres.entry(hijo.clone()).or_default();
                    solicitud.hijo = Some(nodo_hijo);
                    if es_retiro {
                        solicitud.retirar.insert(padre.clone());
                    } else {
                        solicitud.agregar.insert(padre.clone());
                    }
                }
                _ => {}
            }
        }
    }

    let mut cambios = CambiosNodos::default();
    for solicitud in etiquetas.values() {
        if solicitud.agregar.len() > 1 {
            fallos.agregar_haslab = true;
        }
        if solicitud.retirar.len() > 1 {
            fallos.retirar_haslab = true;
        }
        let nodo = solicitud
            .nodo
            .ok_or(ErrorCandidato::SnapshotInconsistente)?;
        let etiqueta_actual = &snapshot
            .nodo(nodo)
            .ok_or(ErrorCandidato::SnapshotInconsistente)?
            .etiqueta;
        let retiro_exacto =
            solicitud.retirar.len() == 1 && solicitud.retirar.first() == Some(etiqueta_actual);
        if !solicitud.retirar.is_empty() && !retiro_exacto {
            fallos.retirar_haslab = true;
        }

        if let Some(etiqueta_nueva) = solicitud.agregar.first() {
            if etiqueta_nueva != etiqueta_actual {
                if etiqueta_nueva.como_str().contains(['\n', '\r']) || !retiro_exacto {
                    fallos.agregar_haslab = true;
                } else {
                    cambios.etiquetas.push((nodo, etiqueta_nueva.clone()));
                }
            }
        } else if retiro_exacto {
            // Lambda es total: la etiqueta requiere un reemplazo explícito.
            fallos
                .invalidez_tecnica
                .get_or_insert(ErrorCandidato::InvalidOperation {
                    clase: ClaseOperacion::HasLab,
                    codigo: "label_replacement_required",
                });
        }
    }

    let mut padres_finales = BTreeMap::<BundlePath, BundlePath>::new();
    for (hijo, nodo) in snapshot.nodos() {
        if let Some(padre) = snapshot.padre(hijo) {
            let hijo_path = &nodo.path;
            let padre_path = &snapshot
                .nodo(padre)
                .ok_or(ErrorCandidato::SnapshotInconsistente)?
                .path;
            padres_finales.insert(hijo_path.clone(), padre_path.clone());
        }
    }

    for (hijo_path, solicitud) in padres {
        if solicitud.agregar.len() > 1 {
            fallos.agregar_parent = true;
        }
        if solicitud.retirar.len() > 1 {
            fallos.retirar_parent = true;
        }
        let hijo = solicitud
            .hijo
            .ok_or(ErrorCandidato::SnapshotInconsistente)?;
        let padre_actual = padres_finales.get(&hijo_path);
        let retiro_exacto =
            solicitud.retirar.len() == 1 && solicitud.retirar.first() == padre_actual;
        if !solicitud.retirar.is_empty() && !retiro_exacto {
            fallos.retirar_parent = true;
        }

        if let Some(padre_nuevo) = solicitud.agregar.first() {
            if Some(padre_nuevo) == padre_actual {
                continue;
            }
            if padre_nuevo == &hijo_path {
                fallos
                    .invalidez_tecnica
                    .get_or_insert(ErrorCandidato::InvalidOperation {
                        clase: ClaseOperacion::Parent,
                        codigo: "self_parent",
                    });
                continue;
            }
            if padre_actual.is_some() && !retiro_exacto {
                fallos.agregar_parent = true;
                continue;
            }
            let Some(nodo_padre) = snapshot.por_path(padre_nuevo) else {
                fallos
                    .invalidez_tecnica
                    .get_or_insert(ErrorCandidato::InvalidOperation {
                        clase: ClaseOperacion::Parent,
                        codigo: "parent_reference_not_found",
                    });
                continue;
            };
            if padre_actual.is_some() {
                cambios.retirar_padres.push(hijo);
            }
            cambios.agregar_padres.push((hijo, nodo_padre));
            padres_finales.insert(hijo_path, padre_nuevo.clone());
        } else if retiro_exacto {
            cambios.retirar_padres.push(hijo);
            padres_finales.remove(&hijo_path);
        }
    }

    if !cambios.agregar_padres.is_empty() && tiene_ciclo(&padres_finales) {
        fallos
            .invalidez_tecnica
            .get_or_insert(ErrorCandidato::InvalidOperation {
                clase: ClaseOperacion::Parent,
                codigo: "parent_cycle",
            });
    }
    devolver_fallo(&fallos)?;
    cambios.conflicto_haslab = fallos.agregar_haslab;
    cambios.conflicto_parent = fallos.agregar_parent;
    Ok(cambios)
}

/// Aplica el plan validado al candidato; primero quita relaciones para que
/// varias reasignaciones válidas no creen ciclos transitorios.
pub(super) fn aplicar(
    candidato: &mut Estructura,
    cambios: &CambiosNodos,
) -> Result<(), ErrorCandidato> {
    for &hijo in &cambios.retirar_padres {
        candidato
            .quitar_padre(hijo)
            .map_err(|_| ErrorCandidato::SnapshotInconsistente)?;
    }
    for &(hijo, padre) in &cambios.agregar_padres {
        candidato
            .asignar_padre(hijo, padre)
            .map_err(|_| ErrorCandidato::InvalidOperation {
                clase: ClaseOperacion::Parent,
                codigo: "parent_assignment_failed",
            })?;
    }
    for (nodo, etiqueta) in &cambios.etiquetas {
        candidato
            .reemplazar_etiqueta_nodo(*nodo, etiqueta.clone())
            .map_err(|_| ErrorCandidato::SnapshotInconsistente)?;
    }
    Ok(())
}

fn devolver_fallo(fallos: &Fallos) -> Result<(), ErrorCandidato> {
    if fallos.retirar_haslab {
        return Err(ErrorCandidato::RemoveNotFound {
            clase: ClaseOperacion::HasLab,
        });
    }
    if fallos.retirar_parent {
        return Err(ErrorCandidato::RemoveNotFound {
            clase: ClaseOperacion::Parent,
        });
    }
    if let Some(error) = &fallos.invalidez_tecnica {
        return Err(error.clone());
    }
    Ok(())
}

fn tiene_ciclo(padres: &BTreeMap<BundlePath, BundlePath>) -> bool {
    let mut terminados = HashSet::new();
    for inicio in padres.keys() {
        let mut camino = HashSet::new();
        let mut actual = Some(inicio.clone());
        while let Some(nodo) = actual {
            if terminados.contains(&nodo) {
                break;
            }
            if !camino.insert(nodo.clone()) {
                return true;
            }
            actual = padres.get(&nodo).cloned();
        }
        terminados.extend(camino);
    }
    false
}
