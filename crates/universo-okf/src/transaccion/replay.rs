//! Reproducción pura de una propuesta desde su contexto de admisión.
//!
//! El resultado reconstruye la base y vuelve a evaluar el delta, pero no
//! demuestra el estado instalado del filesystem ni enlaza la propuesta a un
//! journal.

#[allow(dead_code)]
pub(crate) mod new_esperado;
pub(crate) mod semantica_archivada;

use std::fmt;

use universo_core::{Delta, EtapaErrorPropuesta, ResultadoPropuesta, proponer};

use super::admision::{ContextoAdmision, ErrorContextoAdmision};
use super::captura::{Snapshot, materializar_snapshot_desde_entradas};
use super::snapshot::ErrorSnapshot;

/// Resultado funcional de volver a proponer contra la base raw reconstruida.
pub(crate) struct PropuestaReproducida {
    pub(crate) snapshot: Snapshot,
    pub(crate) delta: Delta,
    pub(crate) resultado: ResultadoPropuesta,
}

/// Error fijo de replay; nunca conserva el texto del delta ni errores internos.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorReplay {
    Contexto(ErrorContextoAdmision),
    Snapshot(ErrorSnapshot),
    MotorPoliticaNoCoinciden,
    PropuestaCandidato,
    PropuestaExpansion,
    PropuestaEvaluacion,
}

impl ErrorReplay {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::Contexto(error) => error.codigo(),
            Self::Snapshot(error) => error.codigo(),
            Self::MotorPoliticaNoCoinciden => "replay_engine_policy_mismatch",
            Self::PropuestaCandidato => "replay_proposal_candidate_failed",
            Self::PropuestaExpansion => "replay_proposal_expansion_failed",
            Self::PropuestaEvaluacion => "replay_proposal_evaluation_failed",
        }
    }
}

impl fmt::Debug for ErrorReplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorReplay").field(&self.codigo()).finish()
    }
}

impl fmt::Display for ErrorReplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorReplay {}

/// Revalida y vuelve a evaluar una propuesta sin consultar el filesystem.
///
/// Los identificadores soportados son proporcionados por el caller. La
/// coincidencia exacta no autentica el contexto ni da soporte a evaluadores
/// antiguos; solo permite continuar con esta configuración de motor y policy.
/// La comparación precede a la validación y al parseo del contexto.
pub(crate) fn reproponer(
    contexto: &ContextoAdmision,
    motor_soportado: &str,
    policy_soportada: &str,
) -> Result<PropuestaReproducida, ErrorReplay> {
    if contexto.version_motor() != motor_soportado || contexto.policy_id() != policy_soportada {
        return Err(ErrorReplay::MotorPoliticaNoCoinciden);
    }

    contexto.validar_base().map_err(ErrorReplay::Contexto)?;
    let snapshot = materializar_snapshot_desde_entradas(
        contexto.entradas_base(),
        contexto.configuracion(),
        contexto.version_motor(),
        contexto.policy_id(),
    )
    .map_err(ErrorReplay::Snapshot)?;

    if snapshot.token() != contexto.base_token()
        || snapshot.raw_tree_digest() != contexto.base_raw_tree_digest()
    {
        return Err(ErrorReplay::Contexto(
            ErrorContextoAdmision::BaseInconsistente,
        ));
    }

    // `validar_base` ya limita y canoniza el texto retenido; volver a leerlo
    // produce la Delta tipada que consume el motor de propuesta.
    let delta = contexto.delta().map_err(ErrorReplay::Contexto)?;
    let resultado = proponer(
        snapshot.estructura(),
        &delta,
        snapshot.fuentes_invariantes(),
    )
    .map_err(|error| match error.etapa() {
        EtapaErrorPropuesta::Candidato => ErrorReplay::PropuestaCandidato,
        EtapaErrorPropuesta::Expansion => ErrorReplay::PropuestaExpansion,
        EtapaErrorPropuesta::Evaluacion => ErrorReplay::PropuestaEvaluacion,
    })?;

    Ok(PropuestaReproducida {
        snapshot,
        delta,
        resultado,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::admision::ContextoAdmision;
    use crate::transaccion::captura::{ConfigRawCapturada, materializar_snapshot_desde_entradas};
    use crate::transaccion::snapshot::EntradaSnapshot;
    use universo_core::{Atomo, Delta, ResultadoPropuesta};

    const MANIFIESTO: &str = r#"version = "snapshot-test/1"

[nodos]
sortes = ["a", "Retirado", "Patrón"]

[nodos.disco]
"Patrón" = "pattern"

[layout]
directorios = ["knowledge"]
sin_iota = []
ignorados = ["ignored.md"]
"#;
    const DOCUMENTO: &[u8] = b"---\ntype: a\ntitle: A\n---\n";
    const MOTOR: &str = "engine-test/1";
    const POLICY: &str = "policy-test/1";

    fn contexto(invariantes: &[u8], delta: Delta, config_root_relative: bool) -> ContextoAdmision {
        let mut entradas = Vec::new();
        let (ruta_manifiesto_root, ruta_invariantes_root) = if config_root_relative {
            entradas.push(EntradaSnapshot::archivo(
                "manifest.toml",
                MANIFIESTO.as_bytes(),
            ));
            entradas.push(EntradaSnapshot::archivo("invariants.smt2", invariantes));
            (
                Some("manifest.toml".to_owned()),
                Some("invariants.smt2".to_owned()),
            )
        } else {
            (None, None)
        };
        entradas.extend([
            EntradaSnapshot::directorio("knowledge"),
            EntradaSnapshot::archivo("knowledge/a.md", DOCUMENTO),
        ]);

        let configuracion = ConfigRawCapturada::nueva(
            MANIFIESTO.as_bytes(),
            invariantes,
            ruta_manifiesto_root,
            ruta_invariantes_root,
        )
        .unwrap();
        let snapshot =
            materializar_snapshot_desde_entradas(entradas, configuracion, MOTOR, POLICY).unwrap();
        ContextoAdmision::capturar(&snapshot, &delta).unwrap()
    }

    fn delta_vacio() -> Delta {
        Delta::nuevo([], []).unwrap()
    }

    #[test]
    fn reproponer_admite_delta_sin_cambio_desde_configuracion_externa_capturada() {
        let contexto = contexto(b"\n", delta_vacio(), false);

        let reproducida = reproponer(&contexto, MOTOR, POLICY).unwrap();

        assert!(matches!(
            &reproducida.resultado,
            ResultadoPropuesta::Admitida(_)
        ));
        assert!(reproducida.delta == delta_vacio());
        assert_eq!(reproducida.snapshot.token(), contexto.base_token());
        assert!(reproducida.snapshot.raw_tree_digest() == contexto.base_raw_tree_digest());
        assert_eq!(reproducida.snapshot.manifiesto_raw(), MANIFIESTO.as_bytes());
        assert_eq!(reproducida.snapshot.invariantes_raw(), b"\n");
        assert_eq!(reproducida.snapshot.ruta_manifiesto_root(), None);
        assert_eq!(reproducida.snapshot.ruta_invariantes_root(), None);
        assert_eq!(reproducida.snapshot.identidad_root(), None);
    }

    #[test]
    fn reproponer_devuelve_rechazo_de_invariante_como_resultado() {
        let contexto = contexto(b"(not (Type @knowledge/a a))", delta_vacio(), true);

        let reproducida = reproponer(&contexto, MOTOR, POLICY).unwrap();

        assert!(matches!(
            &reproducida.resultado,
            ResultadoPropuesta::RechazadaInvariantes(incumplidas) if !incumplidas.is_empty()
        ));
    }

    #[test]
    fn reproponer_exige_motor_y_policy_exactos_sin_mostrarlos() {
        let contexto = contexto(b"\n", delta_vacio(), false);
        for (motor, policy) in [("engine-test/2", POLICY), (MOTOR, "policy-test/2")] {
            let error = reproponer(&contexto, motor, policy).err().unwrap();
            assert_eq!(error.codigo(), "replay_engine_policy_mismatch");
            assert!(!format!("{error:?} {error}").contains("engine-test"));
            assert!(!format!("{error:?} {error}").contains("policy-test"));
        }
    }

    #[test]
    fn error_de_propuesta_no_expone_el_delta_capturado() {
        let delta = Delta::nuevo(
            [Atomo::parsear("Type(knowledge/PRIVATE_CANARY, a)").unwrap()],
            [],
        )
        .unwrap();
        let contexto = contexto(b"\n", delta, false);

        let error = reproponer(&contexto, MOTOR, POLICY).err().unwrap();

        assert_eq!(error.codigo(), "replay_proposal_candidate_failed");
        assert!(!format!("{error:?} {error}").contains("PRIVATE_CANARY"));
    }
}
