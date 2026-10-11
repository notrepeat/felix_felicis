mod aristas;
mod candidato;
mod conflictos;
mod ext;
mod nodos;

pub use candidato::{Candidato, ClaseOperacion, ErrorCandidato, construir_candidato};
pub use conflictos::{
    ClaseConflicto, DiagnosticoConflicto, ResultadoConflictos, detectar_conflictos_funcionales,
};
