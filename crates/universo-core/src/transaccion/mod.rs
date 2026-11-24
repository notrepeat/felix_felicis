mod aristas;
mod candidato;
mod conflictos;
mod ext;
mod nodos;
mod notas;
pub use notas::padre_legado;
mod propuesta;
mod tipos;

pub use candidato::{Candidato, ClaseOperacion, ErrorCandidato, construir_candidato};
pub use conflictos::{
    ClaseConflicto, DiagnosticoConflicto, ResultadoConflictos, detectar_conflictos_funcionales,
};
pub use propuesta::{
    ErrorPropuesta, EtapaErrorPropuesta, ProcedenciaInvariante, ResultadoPropuesta, TestigosFuente,
    explicar_propuesta, proponer, testigo_propuesta, testigos_de_expansion, testigos_propuesta,
};
