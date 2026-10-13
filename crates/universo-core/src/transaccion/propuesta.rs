use std::fmt;

use crate::evaluador::ContextoPresupuestoEvaluacion;
use crate::{
    ErrorEvaluacion, Estructura, SentenciaFuente,
    invariante::expansion::{ErrorExpansion, expandir_sentencias_fuente},
    veredicto::ProcedenciaExplicacion,
};

use super::{
    Candidato, ClaseOperacion, ErrorCandidato, ResultadoConflictos, construir_candidato,
    detectar_conflictos_funcionales,
};
use crate::Delta;

/// Límite semántico agregado para todas las fórmulas de una propuesta.
const PRESUPUESTO_EVALUACION_PROPUESTA: u64 = 1_000_000;

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
    proponer_con_presupuesto_evaluacion(
        snapshot,
        delta,
        invariantes,
        PRESUPUESTO_EVALUACION_PROPUESTA,
    )
}

fn proponer_con_presupuesto_evaluacion(
    snapshot: &Estructura,
    delta: &Delta,
    invariantes: &[SentenciaFuente],
    limite_evaluacion: u64,
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
    let mut presupuesto = ContextoPresupuestoEvaluacion::nuevo(limite_evaluacion);
    let mut falsas = Vec::new();
    for oracion in &expansion.oraciones {
        let satisfecha = presupuesto
            .evaluar(&oracion.formula, candidato.estructura())
            .map_err(error_evaluacion)?;
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

#[cfg(test)]
mod tests {
    use crate::atomos::{Atomo, Sujeto};
    use crate::delta::Delta;
    use crate::evaluador::evaluar_con_presupuesto;
    use crate::formula::Formula;
    use crate::invariante::expansion::OracionExpandida;
    use crate::invariante::expansion::expandir_sentencias_fuente;
    use crate::manifiesto::{Layout, Manifiesto};
    use crate::path::BundlePath;
    use crate::sexpr::parsear_programa;
    use crate::tipos::{Etiqueta, Sorte};
    use std::collections::BTreeMap;

    use super::{EtapaErrorPropuesta, ResultadoPropuesta, proponer_con_presupuesto_evaluacion};
    use crate::Estructura;

    fn estructura_dos_nodos() -> Estructura {
        let mut estructura = Estructura::nueva(manifiesto_minimo()).unwrap();
        insertar_nodo(&mut estructura, "n/PRIVATE_A", "a", "PRIVATE_LABEL_A");
        insertar_nodo(&mut estructura, "n/PRIVATE_B", "b", "PRIVATE_LABEL_B");
        estructura
    }

    fn estructura_un_nodo() -> Estructura {
        let mut estructura = Estructura::nueva(manifiesto_minimo()).unwrap();
        insertar_nodo(&mut estructura, "n/PRIVATE_A", "a", "PRIVATE_LABEL_A");
        estructura
    }

    fn insertar_nodo(estructura: &mut Estructura, path: &str, sorte: &str, etiqueta: &str) {
        estructura
            .insertar_nodo(
                BundlePath::nuevo(path).unwrap(),
                Sorte::nuevo(sorte),
                Etiqueta::nuevo(etiqueta),
            )
            .unwrap();
    }

    fn manifiesto_minimo() -> Manifiesto {
        Manifiesto {
            version: "minimo/1".into(),
            sortes_nodo: ["a", "b", "Retirado", "Patrón"]
                .into_iter()
                .map(Sorte::nuevo)
                .collect(),
            sortes_nodo_disco: BTreeMap::new(),
            retirado_en_disco: None,
            aristas: Vec::new(),
            predicados: Vec::new(),
            layout: Layout {
                directorios: vec!["n".into()],
                sin_iota: Vec::new(),
                ignorados: Vec::new(),
                parent_key: None,
            },
        }
    }

    fn expansion(texto: &str, estructura: &Estructura) -> Vec<OracionExpandida> {
        let fuentes = parsear_programa(texto).unwrap();
        expandir_sentencias_fuente(&fuentes, estructura.manifiesto(), estructura)
            .unwrap()
            .oraciones
    }

    fn costo_minimo(formula: &Formula, estructura: &Estructura) -> u64 {
        (1..=128)
            .find(|limite| evaluar_con_presupuesto(formula, estructura, *limite).is_ok())
            .expect("la fórmula simple debe caber en el presupuesto de prueba")
    }

    fn delta_vacio() -> Delta {
        Delta::nuevo([], []).unwrap()
    }

    fn error_presupuesto(
        texto: &str,
        estructura: &Estructura,
        limite: u64,
    ) -> super::ErrorPropuesta {
        proponer_con_presupuesto_evaluacion(
            estructura,
            &delta_vacio(),
            &parsear_programa(texto).unwrap(),
            limite,
        )
        .unwrap_err()
    }

    #[test]
    fn presupuesto_agregado_respeta_limite_exacto_y_agota_una_unidad_antes() {
        let estructura = estructura_dos_nodos();
        let texto = "(Type @n/PRIVATE_A a)\n(Type @n/PRIVATE_A a)";
        let oraciones = expansion(texto, &estructura);
        let costo = costo_minimo(&oraciones[0].formula, &estructura);
        let total = costo * oraciones.len() as u64;

        for oracion in &oraciones {
            assert!(evaluar_con_presupuesto(&oracion.formula, &estructura, costo).is_ok());
        }
        assert!(matches!(
            proponer_con_presupuesto_evaluacion(
                &estructura,
                &delta_vacio(),
                &parsear_programa(texto).unwrap(),
                total,
            ),
            Ok(ResultadoPropuesta::Admitida(_))
        ));
        assert!(matches!(
            proponer_con_presupuesto_evaluacion(
                &estructura,
                &delta_vacio(),
                &parsear_programa(texto).unwrap(),
                total + 1,
            ),
            Ok(ResultadoPropuesta::Admitida(_))
        ));

        let error = error_presupuesto(texto, &estructura, total - 1);
        assert_eq!(error.etapa(), EtapaErrorPropuesta::Evaluacion);
        assert_eq!(error.codigo(), "evaluation_budget_exhausted");

        let error = error_presupuesto(texto, &estructura, costo);
        assert_eq!(error.codigo(), "evaluation_budget_exhausted");
    }

    #[test]
    fn falso_primero_y_permutaciones_terminan_en_el_mismo_error_redactado() {
        let estructura = estructura_dos_nodos();
        let falso_primero = "(Type @n/PRIVATE_B a)\n(Type @n/PRIVATE_A a)";
        let verdadero_primero = "(Type @n/PRIVATE_A a)\n(Type @n/PRIVATE_B a)";
        let oraciones = expansion(falso_primero, &estructura);
        let costo = costo_minimo(&oraciones[0].formula, &estructura);
        assert!(!evaluar_con_presupuesto(&oraciones[0].formula, &estructura, costo).unwrap());
        assert!(evaluar_con_presupuesto(&oraciones[1].formula, &estructura, costo).is_ok());

        let primer_error = error_presupuesto(falso_primero, &estructura, costo);
        let segundo_error = error_presupuesto(verdadero_primero, &estructura, costo);
        assert_eq!(primer_error, segundo_error);
        assert_eq!(primer_error.etapa(), EtapaErrorPropuesta::Evaluacion);
        assert_eq!(primer_error.codigo(), "evaluation_budget_exhausted");
        let salida = format!("{primer_error} {primer_error:?}");
        assert!(!salida.contains("PRIVATE_A"));
        assert!(!salida.contains("PRIVATE_B"));
        assert!(!salida.contains("(Type"));
    }

    #[test]
    fn presupuesto_agregado_comparte_costo_entre_oracion_directa_y_expandida() {
        let estructura = estructura_un_nodo();
        let texto = "(Type @n/PRIVATE_A a)\n(esquema ((l Lab)) (HasLab @n/PRIVATE_A l))";
        let oraciones = expansion(texto, &estructura);
        assert_eq!(oraciones.len(), 2);
        assert_eq!(oraciones[0].source_index, 0);
        assert_eq!(oraciones[0].instance_index, None);
        assert_eq!(oraciones[1].source_index, 1);
        assert_eq!(oraciones[1].instance_index, Some(0));

        let costo_maximo = oraciones
            .iter()
            .map(|oracion| costo_minimo(&oracion.formula, &estructura))
            .max()
            .unwrap();
        for oracion in &oraciones {
            assert!(evaluar_con_presupuesto(&oracion.formula, &estructura, costo_maximo).is_ok());
        }

        let error = error_presupuesto(texto, &estructura, costo_maximo);
        assert_eq!(error.etapa(), EtapaErrorPropuesta::Evaluacion);
        assert_eq!(error.codigo(), "evaluation_budget_exhausted");
    }

    #[test]
    fn conflictos_y_validacion_de_expansion_preceden_al_presupuesto_evaluativo() {
        let estructura = estructura_un_nodo();
        let conflictivo = Delta::nuevo(
            [Atomo::HasLab {
                sujeto: Sujeto::Nodo(BundlePath::nuevo("n/PRIVATE_A").unwrap()),
                etiqueta: Etiqueta::nuevo("PRIVATE_LABEL_NEW"),
            }],
            [],
        )
        .unwrap();
        let invalida = parsear_programa("(Type @n/PRIVATE_MISSING a)").unwrap();

        assert!(matches!(
            proponer_con_presupuesto_evaluacion(&estructura, &conflictivo, &invalida, 0),
            Ok(ResultadoPropuesta::RechazadaConflictos(_))
        ));

        let error = proponer_con_presupuesto_evaluacion(&estructura, &delta_vacio(), &invalida, 0)
            .unwrap_err();
        assert_eq!(error.etapa(), EtapaErrorPropuesta::Expansion);
        assert_eq!(error.codigo(), "invalid_invariant_statement");
    }
}
