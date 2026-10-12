use std::collections::{BTreeMap, HashMap, HashSet};

use crate::{
    AristaId, Atomo, BundlePath, Delta, Estructura, Etiqueta, IdArista, LadoDelta, Sorte, Sujeto,
};

use super::{aristas, ext, nodos};

/// Clase pública de operación, sin incluir identidades ni valores del átomo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaseOperacion {
    Type,
    Edge,
    Parent,
    HasLab,
    Ext,
}

impl std::fmt::Display for ClaseOperacion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nombre = match self {
            Self::Type => "Type",
            Self::Edge => "Edge",
            Self::Parent => "Parent",
            Self::HasLab => "HasLab",
            Self::Ext => "Ext",
        };
        f.write_str(nombre)
    }
}

/// Rechazo técnico redactado al construir un candidato.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorCandidato {
    UnsupportedOperation {
        lado: LadoDelta,
        clase: ClaseOperacion,
    },
    RemoveNotFound {
        clase: ClaseOperacion,
    },
    AddConflict {
        clase: ClaseOperacion,
    },
    InvalidOperation {
        clase: ClaseOperacion,
        codigo: &'static str,
    },
    SnapshotInconsistente,
}

impl std::fmt::Display for ErrorCandidato {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedOperation { lado, clase } => {
                write!(f, "unsupported_operation: {lado} {clase}")
            }
            Self::RemoveNotFound { clase } => write!(f, "remove_not_found: {clase}"),
            Self::AddConflict { clase } => write!(f, "add_conflict: {clase}"),
            Self::InvalidOperation { clase, codigo } => write!(f, "{codigo}: {clase}"),
            Self::SnapshotInconsistente => f.write_str("candidate_error: snapshot_inconsistent"),
        }
    }
}

impl std::error::Error for ErrorCandidato {}

impl ErrorCandidato {
    /// Código estable del error sin valores del delta o del snapshot.
    pub const fn codigo(&self) -> &'static str {
        match self {
            Self::UnsupportedOperation { .. } => "unsupported_operation",
            Self::RemoveNotFound { .. } => "remove_not_found",
            Self::AddConflict { .. } => "add_conflict",
            Self::InvalidOperation { codigo, .. } => codigo,
            Self::SnapshotInconsistente => "snapshot_inconsistent",
        }
    }

    /// Clase de operación asociada, cuando el error se refiere a una sola.
    pub const fn clase(&self) -> Option<ClaseOperacion> {
        match self {
            Self::UnsupportedOperation { clase, .. }
            | Self::RemoveNotFound { clase }
            | Self::AddConflict { clase }
            | Self::InvalidOperation { clase, .. } => Some(*clase),
            Self::SnapshotInconsistente => None,
        }
    }
}

/// Estructura aislada construida desde un snapshot sin modificarlo.
pub struct Candidato {
    estructura: Estructura,
}

impl std::fmt::Debug for Candidato {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Candidato")
            .field("node_count", &self.estructura.len_nodos())
            .field("edge_count", &self.estructura.len_aristas())
            .finish()
    }
}

impl Candidato {
    /// Vista de solo lectura del candidato.
    pub fn estructura(&self) -> &Estructura {
        &self.estructura
    }

    /// Consume el candidato para continuar una transacción.
    pub fn into_estructura(self) -> Estructura {
        self.estructura
    }
}

/// Construye un candidato aislado con retiros y altas `Edge`, `HasLab`,
/// `Parent` y `Ext`.
pub fn construir_candidato(
    snapshot: &Estructura,
    delta: &Delta,
) -> Result<Candidato, ErrorCandidato> {
    for atomo in delta.agregar() {
        if !matches!(
            atomo,
            Atomo::Edge { .. } | Atomo::Ext { .. } | Atomo::HasLab { .. } | Atomo::Parent { .. }
        ) {
            return Err(ErrorCandidato::UnsupportedOperation {
                lado: LadoDelta::Add,
                clase: clase(atomo),
            });
        }
    }
    for atomo in delta.retirar() {
        if !matches!(
            atomo,
            Atomo::Edge { .. } | Atomo::Ext { .. } | Atomo::HasLab { .. } | Atomo::Parent { .. }
        ) {
            return Err(ErrorCandidato::UnsupportedOperation {
                lado: LadoDelta::Remove,
                clase: clase(atomo),
            });
        }
    }

    // Las operaciones Ext y sus handles se resuelven contra el mismo
    // snapshot antes de modificar el candidato aislado.
    let cambios_ext = ext::preparar(snapshot, delta)?;
    let cambios_nodos = nodos::preparar(snapshot, delta)?;

    let mut aristas_retiradas = HashSet::new();
    let mut etiquetas_retiradas = HashMap::<AristaId, Etiqueta>::new();
    let mut indice_resolucion = None;
    for atomo in delta.retirar() {
        match atomo {
            Atomo::Edge { id, .. } => {
                let indice = indice_resolucion
                    .get_or_insert_with(|| aristas::IndiceResolucion::nuevo(snapshot));
                let handle =
                    indice
                        .resolver(snapshot, id)
                        .ok_or(ErrorCandidato::RemoveNotFound {
                            clase: ClaseOperacion::Edge,
                        })?;
                aristas_retiradas.insert(handle);
            }
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id),
                etiqueta,
            } => {
                let indice = indice_resolucion
                    .get_or_insert_with(|| aristas::IndiceResolucion::nuevo(snapshot));
                let handle =
                    indice
                        .resolver(snapshot, id)
                        .ok_or(ErrorCandidato::RemoveNotFound {
                            clase: ClaseOperacion::HasLab,
                        })?;
                if snapshot
                    .arista(handle)
                    .is_none_or(|arista| arista.etiqueta != *etiqueta)
                {
                    return Err(ErrorCandidato::RemoveNotFound {
                        clase: ClaseOperacion::HasLab,
                    });
                }
                etiquetas_retiradas.insert(handle, etiqueta.clone());
            }
            Atomo::Ext { .. } | Atomo::Parent { .. } => {}
            Atomo::HasLab {
                sujeto: Sujeto::Nodo(_),
                ..
            } => {}
            _ => unreachable!("las operaciones se validaron antes de resolver handles"),
        }
    }
    let mut estructura = snapshot.clonar_para_candidato();
    let handles: Vec<_> = aristas_retiradas.into_iter().collect();
    aristas::retirar_lote(&mut estructura, &handles)
        .map_err(|_| ErrorCandidato::SnapshotInconsistente)?;
    aplicar_altas(&mut estructura, delta.agregar(), &etiquetas_retiradas)?;
    ext::aplicar(&mut estructura, &cambios_ext)?;
    nodos::aplicar(&mut estructura, &cambios_nodos)?;
    if cambios_nodos.conflicto_haslab {
        return Err(ErrorCandidato::AddConflict {
            clase: ClaseOperacion::HasLab,
        });
    }
    if cambios_nodos.conflicto_parent {
        return Err(ErrorCandidato::AddConflict {
            clase: ClaseOperacion::Parent,
        });
    }
    Ok(Candidato { estructura })
}

/// Construye solo la topología Edge final requerida por el detector de
/// conflictos. Las etiquetas provienen de una capa lógica separada: cada
/// arista realmente nueva recibe una etiqueta vacía sintética para reutilizar
/// la misma validación e inserción de aristas que el constructor normal.
pub(super) fn construir_topologia_aristas(
    snapshot: &Estructura,
    delta: &Delta,
) -> Result<Candidato, ErrorCandidato> {
    construir_topologia_aristas_observando(snapshot, delta, || {}, || {})
}

fn construir_topologia_aristas_observando(
    snapshot: &Estructura,
    delta: &Delta,
    mut observar_arista: impl FnMut(),
    mut observar_busqueda: impl FnMut(),
) -> Result<Candidato, ErrorCandidato> {
    let retiros = delta
        .retirar()
        .iter()
        .filter(|atomo| matches!(atomo, Atomo::Edge { .. }))
        .cloned()
        .collect::<Vec<_>>();
    let delta_retiros = Delta::nuevo(std::iter::empty(), retiros)
        .expect("filtrar operaciones Edge conserva el delta canónico");
    let mut candidato = construir_candidato(snapshot, &delta_retiros)?;

    let mut altas_topologicas = Vec::new();
    let mut indice = None;
    for atomo in delta.agregar() {
        let Atomo::Edge { id, .. } = atomo else {
            continue;
        };
        let indice = indice.get_or_insert_with(|| {
            aristas::IndiceResolucion::nuevo_observando(candidato.estructura(), || {
                observar_arista()
            })
        });
        // Las altas Edge que ya resuelven tras los retiros son no-op. Omitirlas
        // evita que una etiqueta sintética sustituya la del handle existente.
        if indice
            .resolver_observando(candidato.estructura(), id, &mut observar_busqueda)
            .is_some()
        {
            continue;
        }
        altas_topologicas.push(atomo.clone());
        altas_topologicas.push(Atomo::HasLab {
            sujeto: Sujeto::Arista(id.clone()),
            etiqueta: Etiqueta::nuevo(""),
        });
    }

    aplicar_altas_con_indice(
        &mut candidato.estructura,
        &altas_topologicas,
        &HashMap::new(),
        &mut indice,
        observar_arista,
        observar_busqueda,
    )?;
    Ok(candidato)
}

type GrupoArista = (BundlePath, Sorte, BundlePath);

fn aplicar_altas(
    estructura: &mut Estructura,
    atomos: &[Atomo],
    etiquetas_retiradas: &HashMap<AristaId, Etiqueta>,
) -> Result<(), ErrorCandidato> {
    let mut indice = None;
    aplicar_altas_con_indice(
        estructura,
        atomos,
        etiquetas_retiradas,
        &mut indice,
        || {},
        || {},
    )
}

fn aplicar_altas_con_indice(
    estructura: &mut Estructura,
    atomos: &[Atomo],
    etiquetas_retiradas: &HashMap<AristaId, Etiqueta>,
    indice: &mut Option<aristas::IndiceResolucion>,
    mut observar_arista: impl FnMut(),
    mut observar_busqueda: impl FnMut(),
) -> Result<(), ErrorCandidato> {
    let mut grupos = BTreeMap::<GrupoArista, Vec<&Atomo>>::new();
    let mut etiquetas = HashMap::<IdArista, Vec<&Etiqueta>>::new();
    let mut identidades_edge = HashSet::<IdArista>::new();

    for atomo in atomos {
        match atomo {
            Atomo::Edge {
                id,
                origen,
                destino,
                sorte,
            } => {
                if id.origen != *origen || id.destino != *destino || id.sorte != *sorte {
                    return Err(ErrorCandidato::AddConflict {
                        clase: ClaseOperacion::Edge,
                    });
                }
                if !identidades_edge.insert(id.clone()) {
                    return Err(ErrorCandidato::AddConflict {
                        clase: ClaseOperacion::Edge,
                    });
                }
                grupos
                    .entry((origen.clone(), sorte.clone(), destino.clone()))
                    .or_default()
                    .push(atomo);
            }
            Atomo::Ext { .. } | Atomo::Parent { .. } => continue,
            Atomo::HasLab {
                sujeto: Sujeto::Nodo(_),
                ..
            } => continue,
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id),
                etiqueta,
            } => etiquetas.entry(id.clone()).or_default().push(etiqueta),
            _ => unreachable!("las altas se validaron antes de agrupar"),
        }
    }

    let identidades_reemplazo: HashSet<_> = etiquetas_retiradas
        .keys()
        .filter_map(|handle| identidad_arista(estructura, *handle))
        .collect();
    let mut etiquetas_vigentes = HashMap::<IdArista, Etiqueta>::new();
    for (handle, arista) in estructura.aristas() {
        let identidad =
            identidad_arista(estructura, handle).ok_or(ErrorCandidato::SnapshotInconsistente)?;
        etiquetas_vigentes.insert(identidad, arista.etiqueta.clone());
    }
    if etiquetas.keys().any(|identidad| {
        !identidades_edge.contains(identidad)
            && !identidades_reemplazo.contains(identidad)
            && !etiquetas_vigentes.contains_key(identidad)
    }) {
        return Err(ErrorCandidato::InvalidOperation {
            clase: ClaseOperacion::HasLab,
            codigo: "label_orphan",
        });
    }

    for (identidad, etiquetas_nuevas) in &etiquetas {
        if identidades_edge.contains(identidad) || identidades_reemplazo.contains(identidad) {
            continue;
        }
        if etiquetas_nuevas.len() != 1
            || etiquetas_vigentes.get(identidad) != etiquetas_nuevas.first().copied()
        {
            return Err(ErrorCandidato::AddConflict {
                clase: ClaseOperacion::HasLab,
            });
        }
    }

    for ((origen_path, sorte, destino_path), mut altas) in grupos {
        // Atomo se ordena por su texto canónico, donde #10 precede a #2.
        // El contrato operativo exige ordinal numérico.
        altas.sort_by_key(|atomo| match atomo {
            Atomo::Edge { id, .. } => id.ordinal,
            _ => unreachable!("el grupo solo contiene altas Edge"),
        });

        let origen = estructura
            .por_path(&origen_path)
            .ok_or(ErrorCandidato::AddConflict {
                clase: ClaseOperacion::Edge,
            })?;
        let destino = estructura
            .por_path(&destino_path)
            .ok_or(ErrorCandidato::AddConflict {
                clase: ClaseOperacion::Edge,
            })?;
        let indice = indice.get_or_insert_with(|| {
            aristas::IndiceResolucion::nuevo_observando(estructura, &mut observar_arista)
        });
        let supervivientes =
            indice.cantidad_grupo_observando(origen, &sorte, destino, &mut observar_busqueda);
        let mut nuevas = 0usize;

        for atomo in altas {
            let Atomo::Edge { id, .. } = atomo else {
                unreachable!("el grupo solo contiene altas Edge")
            };
            let etiquetas_edge = etiquetas.get(id).ok_or(ErrorCandidato::InvalidOperation {
                clase: ClaseOperacion::HasLab,
                codigo: "label_missing",
            })?;
            if etiquetas_edge.is_empty() {
                return Err(ErrorCandidato::InvalidOperation {
                    clase: ClaseOperacion::HasLab,
                    codigo: "label_missing",
                });
            }
            if etiquetas_edge.len() != 1 {
                return Err(ErrorCandidato::AddConflict {
                    clase: ClaseOperacion::HasLab,
                });
            }
            let etiqueta = etiquetas_edge[0];
            if etiqueta.como_str().contains(['\n', '\r']) {
                return Err(ErrorCandidato::InvalidOperation {
                    clase: ClaseOperacion::HasLab,
                    codigo: "label_contains_line_break",
                });
            }

            let ordinal = usize::try_from(id.ordinal).map_err(|_| ErrorCandidato::AddConflict {
                clase: ClaseOperacion::Edge,
            })?;
            if ordinal < supervivientes {
                let existente = indice
                    .resolver_grupo_observando(origen, &sorte, destino, id.ordinal, || {
                        observar_busqueda()
                    })
                    .ok_or(ErrorCandidato::AddConflict {
                        clase: ClaseOperacion::Edge,
                    })?;
                if estructura
                    .arista(existente)
                    .is_none_or(|arista| arista.etiqueta != *etiqueta)
                {
                    return Err(ErrorCandidato::AddConflict {
                        clase: ClaseOperacion::HasLab,
                    });
                }
                continue;
            }

            let esperado =
                supervivientes
                    .checked_add(nuevas)
                    .ok_or(ErrorCandidato::AddConflict {
                        clase: ClaseOperacion::Edge,
                    })?;
            if ordinal != esperado {
                return Err(ErrorCandidato::AddConflict {
                    clase: ClaseOperacion::Edge,
                });
            }
            let handle = estructura
                .insertar_arista(origen, destino, sorte.clone(), (*etiqueta).clone())
                .map_err(|_| ErrorCandidato::AddConflict {
                    clase: ClaseOperacion::Edge,
                })?;
            indice.registrar(estructura, handle);
            nuevas += 1;
        }
    }

    let mut reemplazos = Vec::new();
    for (handle, etiqueta_anterior) in etiquetas_retiradas {
        if estructura.arista(*handle).is_none() {
            continue;
        }
        let identidad_final =
            identidad_arista(estructura, *handle).ok_or(ErrorCandidato::SnapshotInconsistente)?;
        let etiquetas_nuevas =
            etiquetas
                .get(&identidad_final)
                .ok_or(ErrorCandidato::InvalidOperation {
                    clase: ClaseOperacion::HasLab,
                    codigo: "label_replacement_required",
                })?;
        if etiquetas_nuevas.is_empty() {
            return Err(ErrorCandidato::InvalidOperation {
                clase: ClaseOperacion::HasLab,
                codigo: "label_replacement_required",
            });
        }
        if etiquetas_nuevas.len() != 1 {
            return Err(ErrorCandidato::AddConflict {
                clase: ClaseOperacion::HasLab,
            });
        }
        let etiqueta_nueva = etiquetas_nuevas[0];
        if etiqueta_nueva.como_str().contains(['\n', '\r']) {
            return Err(ErrorCandidato::InvalidOperation {
                clase: ClaseOperacion::HasLab,
                codigo: "label_contains_line_break",
            });
        }
        if estructura
            .arista(*handle)
            .is_none_or(|arista| arista.etiqueta != *etiqueta_anterior)
        {
            return Err(ErrorCandidato::SnapshotInconsistente);
        }
        reemplazos.push((*handle, (*etiqueta_nueva).clone()));
    }

    for (handle, etiqueta) in reemplazos {
        estructura
            .reemplazar_etiqueta_arista(handle, etiqueta)
            .map_err(|_| ErrorCandidato::InvalidOperation {
                clase: ClaseOperacion::HasLab,
                codigo: "label_replacement_failed",
            })?;
    }
    Ok(())
}

pub(super) fn identidad_arista(estructura: &Estructura, handle: AristaId) -> Option<IdArista> {
    let arista = estructura.arista(handle)?;
    Some(IdArista {
        origen: estructura.nodo(arista.origen)?.path.clone(),
        sorte: arista.tipo.clone(),
        destino: estructura.nodo(arista.destino)?.path.clone(),
        ordinal: arista.ordinal,
    })
}

fn clase(atomo: &Atomo) -> ClaseOperacion {
    match atomo {
        Atomo::Type { .. } => ClaseOperacion::Type,
        Atomo::Edge { .. } => ClaseOperacion::Edge,
        Atomo::Parent { .. } => ClaseOperacion::Parent,
        Atomo::HasLab { .. } => ClaseOperacion::HasLab,
        Atomo::Ext { .. } => ClaseOperacion::Ext,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BundlePath, manifiesto::tests::base};

    fn path(valor: &str) -> BundlePath {
        BundlePath::nuevo(valor).unwrap()
    }

    fn edge_atom(id: IdArista) -> Atomo {
        Atomo::Edge {
            origen: id.origen.clone(),
            destino: id.destino.clone(),
            sorte: id.sorte.clone(),
            id,
        }
    }

    #[test]
    fn topologia_consulta_altas_con_un_scan_y_busquedas_constantes() {
        let mut snapshot = Estructura::nueva(base()).unwrap();
        let origen = snapshot
            .insertar_nodo(path("n/origen"), Sorte::nuevo("a"), Etiqueta::nuevo("o"))
            .unwrap();
        let destino = snapshot
            .insertar_nodo(path("n/destino"), Sorte::nuevo("a"), Etiqueta::nuevo("d"))
            .unwrap();
        let cantidad = 256u32;
        let mut altas = Vec::new();
        for _ in 0..cantidad {
            let handle = snapshot
                .insertar_arista(origen, destino, Sorte::nuevo("rel"), Etiqueta::nuevo(""))
                .unwrap();
            altas.push(edge_atom(identidad_arista(&snapshot, handle).unwrap()));
        }
        let nueva = IdArista {
            origen: path("n/origen"),
            sorte: Sorte::nuevo("rel"),
            destino: path("n/destino"),
            ordinal: cantidad,
        };
        altas.push(edge_atom(nueva.clone()));
        let delta = Delta::nuevo(altas, std::iter::empty()).unwrap();

        let mut aristas_visitadas = 0;
        let mut busquedas = 0;
        let candidato = construir_topologia_aristas_observando(
            &snapshot,
            &delta,
            || aristas_visitadas += 1,
            || busquedas += 1,
        )
        .unwrap();

        assert_eq!(aristas_visitadas, cantidad as usize);
        assert_eq!(busquedas, cantidad as usize + 2);
        assert_eq!(candidato.estructura().len_aristas(), cantidad as usize + 1);
        assert!(aristas::resolver(candidato.estructura(), &nueva).is_some());
        assert_eq!(snapshot.len_aristas(), cantidad as usize);
    }
}
