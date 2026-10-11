use std::fmt;

use crate::{
    ErrorEvaluacion, Estructura, SentenciaFuente, evaluar,
    invariante::expansion::{ErrorExpansion, expandir_sentencias_fuente},
    veredicto::ProcedenciaExplicacion,
};

use super::{
    Candidato, ClaseOperacion, ErrorCandidato, ResultadoConflictos, construir_candidato,
    detectar_conflictos_funcionales,
};
use crate::Delta;

/// Procedencia pública de una instancia falsa, sin fórmula ni sustituciones.
pub type ProcedenciaInvariante = ProcedenciaExplicacion;

/// Resultado funcional de proponer un delta sobre un snapshot.
pub enum ResultadoPropuesta {
    /// El candidato satisface todas las sentencias recibidas.
    Admitida(Box<Candidato>),
    /// La combinación final viola una o más claves funcionales.
    RechazadaConflictos(ResultadoConflictos),
    /// Una o más instancias de invariantes son falsas en el candidato.
    RechazadaInvariantes(Vec<ProcedenciaInvariante>),
}

impl fmt::Debug for ResultadoPropuesta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admitida(candidato) => f.debug_tuple("Admitida").field(candidato).finish(),
            Self::RechazadaConflictos(resultado) => f
                .debug_tuple("RechazadaConflictos")
                .field(resultado)
                .finish(),
            Self::RechazadaInvariantes(procedencias) => f
                .debug_tuple("RechazadaInvariantes")
                .field(procedencias)
                .finish(),
        }
    }
}

impl fmt::Display for ResultadoPropuesta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admitida(_) => f.write_str("admitida"),
            Self::RechazadaConflictos(resultado) => write!(f, "rechazada: {resultado}"),
            Self::RechazadaInvariantes(procedencias) => write!(
                f,
                "rechazada_por_invariantes: {} instancia(s) falsa(s)",
                procedencias.len()
            ),
        }
    }
}

/// Etapa que produjo un error técnico de propuesta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EtapaErrorPropuesta {
    Candidato,
    Expansion,
    Evaluacion,
}

impl EtapaErrorPropuesta {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Candidato => "candidate",
            Self::Expansion => "expansion",
            Self::Evaluacion => "evaluation",
        }
    }
}

/// Error técnico con etapa, código y clase opcional, sin causa sensible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorPropuesta {
    etapa: EtapaErrorPropuesta,
    codigo: &'static str,
    clase: Option<ClaseOperacion>,
}

impl ErrorPropuesta {
    fn nuevo(
        etapa: EtapaErrorPropuesta,
        codigo: &'static str,
        clase: Option<ClaseOperacion>,
    ) -> Self {
        Self {
            etapa,
            codigo,
            clase,
        }
    }

    pub const fn etapa(&self) -> EtapaErrorPropuesta {
        self.etapa
    }

    pub const fn codigo(&self) -> &'static str {
        self.codigo
    }

    pub const fn clase(&self) -> Option<ClaseOperacion> {
        self.clase
    }
}

impl fmt::Display for ErrorPropuesta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_error: {}", self.etapa.as_str(), self.codigo)?;
        if let Some(clase) = self.clase {
            write!(f, " ({clase})")?;
        }
        Ok(())
    }
}

impl std::error::Error for ErrorPropuesta {}

/// Construye un candidato aislado, detecta conflictos funcionales y evalúa
/// invariantes expandidos sobre ese candidato. No realiza I/O ni muta el
/// snapshot. Los errores técnicos se reducen a códigos y clases redactados.
pub fn proponer(
    snapshot: &Estructura,
    delta: &Delta,
    invariantes: &[SentenciaFuente],
) -> Result<ResultadoPropuesta, ErrorPropuesta> {
    let candidato = match construir_candidato(snapshot, delta) {
        Ok(candidato) => candidato,
        Err(error) => {
            if requiere_detector(&error) {
                let conflictos =
                    detectar_conflictos_funcionales(snapshot, delta).map_err(error_candidato)?;
                if conflictos.hay_conflictos() {
                    return Ok(ResultadoPropuesta::RechazadaConflictos(conflictos));
                }
            }
            return Err(error_candidato(error));
        }
    };

    let conflictos = detectar_conflictos_funcionales(snapshot, delta).map_err(error_candidato)?;
    if conflictos.hay_conflictos() {
        return Ok(ResultadoPropuesta::RechazadaConflictos(conflictos));
    }

    let expansion = expandir_sentencias_fuente(
        invariantes,
        candidato.estructura().manifiesto(),
        candidato.estructura(),
    )
    .map_err(error_expansion)?;
    let mut falsas = Vec::new();
    for oracion in &expansion.oraciones {
        let satisfecha =
            evaluar(&oracion.formula, candidato.estructura()).map_err(error_evaluacion)?;
        if !satisfecha {
            falsas.push(ProcedenciaInvariante::from(oracion));
        }
    }

    if falsas.is_empty() {
        Ok(ResultadoPropuesta::Admitida(Box::new(candidato)))
    } else {
        Ok(ResultadoPropuesta::RechazadaInvariantes(falsas))
    }
}

fn requiere_detector(error: &ErrorCandidato) -> bool {
    error.codigo() == "add_conflict"
        && matches!(
            error.clase(),
            Some(ClaseOperacion::HasLab | ClaseOperacion::Parent | ClaseOperacion::Edge)
        )
}

fn error_candidato(error: ErrorCandidato) -> ErrorPropuesta {
    ErrorPropuesta::nuevo(
        EtapaErrorPropuesta::Candidato,
        error.codigo(),
        error.clase(),
    )
}

fn error_expansion(error: ErrorExpansion) -> ErrorPropuesta {
    let codigo = match error {
        ErrorExpansion::SentenciaInvalida { .. } => "invalid_invariant_statement",
        ErrorExpansion::DominioPredicadoDesconocido(_) => "unknown_predicate_domain",
        ErrorExpansion::PresupuestoAgotado { .. } => "instance_budget_exhausted",
    };
    ErrorPropuesta::nuevo(EtapaErrorPropuesta::Expansion, codigo, None)
}

fn error_evaluacion(error: ErrorEvaluacion) -> ErrorPropuesta {
    let codigo = match error {
        ErrorEvaluacion::ProfundidadAstExcedida { .. } => "ast_depth_exceeded",
        ErrorEvaluacion::DemasiadasLigaduras { .. } => "binding_limit_exceeded",
        ErrorEvaluacion::PresupuestoAgotado { .. } => "evaluation_budget_exhausted",
        ErrorEvaluacion::PresupuestoEvidenciaAgotado { .. } => "evidence_budget_exhausted",
        ErrorEvaluacion::VariableNoLigada { .. } => "free_variable",
        ErrorEvaluacion::VariableDeSorteIncorrecta { .. } => "variable_sort_mismatch",
        ErrorEvaluacion::VariableRedeclarada(_) => "variable_redeclared",
        ErrorEvaluacion::ConstanteNodoInexistente(_) => "node_constant_missing",
        ErrorEvaluacion::ConstanteAristaInexistente(_) => "edge_constant_missing",
        ErrorEvaluacion::SorteNodoNoDeclarada(_) => "node_type_undeclared",
        ErrorEvaluacion::SorteAristaNoDeclarada(_) => "edge_type_undeclared",
        ErrorEvaluacion::EtiquetaNoEnUso(_) => "label_unavailable",
        ErrorEvaluacion::PredicadoNoDeclarado(_) => "predicate_undeclared",
        ErrorEvaluacion::ConstanteFueraDeDominio { .. } => "extension_value_out_of_domain",
        ErrorEvaluacion::MetaNoInstanciada(_) => "unbound_schema_meta",
        ErrorEvaluacion::NodoLigadoInexistente(_) => "bound_node_missing",
        ErrorEvaluacion::AristaLigadaInexistente(_) => "bound_edge_missing",
    };
    ErrorPropuesta::nuevo(EtapaErrorPropuesta::Evaluacion, codigo, None)
}
