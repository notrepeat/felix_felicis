mod aristas;
mod candidato;
mod conflictos;
mod ext;

pub use candidato::{Candidato, ClaseOperacion, ErrorCandidato, construir_candidato};
pub use conflictos::{
    ClaseConflicto, DiagnosticoConflicto, ResultadoConflictos, detectar_conflictos_funcionales,
};
