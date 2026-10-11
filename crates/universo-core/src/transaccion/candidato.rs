use std::collections::HashSet;

use crate::{AristaId, Atomo, Delta, Estructura, LadoDelta, Sujeto};

use super::aristas;

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
    SnapshotInconsistente,
}

impl std::fmt::Display for ErrorCandidato {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedOperation { lado, clase } => {
                write!(f, "unsupported_operation: {lado} {clase}")
            }
            Self::RemoveNotFound { clase } => write!(f, "remove_not_found: {clase}"),
            Self::SnapshotInconsistente => f.write_str("candidate_error: snapshot_inconsistent"),
        }
    }
}

impl std::error::Error for ErrorCandidato {}

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

/// Construye un candidato aislado; este primer slice solo retira aristas.
pub fn construir_candidato(
    snapshot: &Estructura,
    delta: &Delta,
) -> Result<Candidato, ErrorCandidato> {
    if let Some(atomo) = delta.agregar().first() {
        return Err(ErrorCandidato::UnsupportedOperation {
            lado: LadoDelta::Add,
            clase: clase(atomo),
        });
    }
    for atomo in delta.retirar() {
        if !matches!(
            atomo,
            Atomo::Edge { .. }
                | Atomo::HasLab {
                    sujeto: Sujeto::Arista(_),
                    ..
                }
        ) {
            return Err(ErrorCandidato::UnsupportedOperation {
                lado: LadoDelta::Remove,
                clase: clase(atomo),
            });
        }
    }

    let mut aristas_retiradas = HashSet::new();
    let mut etiquetas_retiradas = Vec::<AristaId>::new();
    for atomo in delta.retirar() {
        match atomo {
            Atomo::Edge { id, .. } => {
                let handle =
                    aristas::resolver(snapshot, id).ok_or(ErrorCandidato::RemoveNotFound {
                        clase: ClaseOperacion::Edge,
                    })?;
                aristas_retiradas.insert(handle);
            }
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id),
                etiqueta,
            } => {
                let handle =
                    aristas::resolver(snapshot, id).ok_or(ErrorCandidato::RemoveNotFound {
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
                etiquetas_retiradas.push(handle);
            }
            _ => unreachable!("las operaciones se validaron antes de resolver handles"),
        }
    }
    if etiquetas_retiradas
        .iter()
        .any(|handle| !aristas_retiradas.contains(handle))
    {
        return Err(ErrorCandidato::UnsupportedOperation {
            lado: LadoDelta::Remove,
            clase: ClaseOperacion::HasLab,
        });
    }

    let mut estructura = snapshot.clonar_para_candidato();
    let handles: Vec<_> = aristas_retiradas.into_iter().collect();
    aristas::retirar_lote(&mut estructura, &handles)
        .map_err(|_| ErrorCandidato::SnapshotInconsistente)?;
    Ok(Candidato { estructura })
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
