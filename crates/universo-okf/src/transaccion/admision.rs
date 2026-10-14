//! Contexto privado para replay de una admisión ya evaluada.
//!
//! Guarda valores capturados, no paths para reabrir ni una marca booleana de
//! admisión. La persistencia/wire y el binding a la transacción se añaden en
//! otra unidad; quien reconstruya un contexto debe volver a llamar
//! [`ContextoAdmision::validar_base`] y evaluar de nuevo la propuesta.

use std::fmt;
use std::sync::Arc;

use universo_core::Delta;

use crate::delta::{ErrorDeltaJson, parsear_delta_json_v1, serializar_delta_json_v1};

use super::captura::{ConfigRawCapturada, Snapshot};
use super::journal::RawTreeDigest;
use super::limites::PRESUPUESTO_BYTES_RAW;
use super::snapshot::{
    EntradaSnapshot, SnapshotToken, calcular_digest_raw_tree, calcular_token_snapshot,
    validar_presupuesto_configuracion,
};

const MAX_DELTA_BYTES: usize = PRESUPUESTO_BYTES_RAW;
const MAX_METADATA_BYTES: usize = 4 * 1024;

/// Valores exactos suficientes para reconstruir y revalidar la base de una
/// admisión, sin conservar identidad filesystem ni rutas externas.
pub(crate) struct ContextoAdmision {
    entradas_base: Vec<EntradaSnapshot>,
    configuracion: ConfigRawCapturada,
    delta_json_v1: String,
    version_motor: Arc<str>,
    policy_id: Arc<str>,
    base_token: SnapshotToken,
    base_raw_tree_digest: RawTreeDigest,
}

impl ContextoAdmision {
    /// Copia la evidencia raw del snapshot y conserva el delta en su forma
    /// canónica versionada. Los buffers de configuración siguen compartidos
    /// por `Arc`; no se guardan rutas externas.
    pub(crate) fn capturar(
        snapshot: &Snapshot,
        delta: &Delta,
    ) -> Result<Self, ErrorContextoAdmision> {
        Self::capturar_con_limites(snapshot, delta, MAX_DELTA_BYTES, MAX_METADATA_BYTES)
    }

    fn capturar_con_limites(
        snapshot: &Snapshot,
        delta: &Delta,
        limite_delta: usize,
        limite_metadata: usize,
    ) -> Result<Self, ErrorContextoAdmision> {
        validar_limite_metadata(
            snapshot.version_motor(),
            snapshot.policy_id(),
            limite_metadata,
        )?;

        // La Delta tipada ya está materializada; la cuota limita el valor que
        // conserva el contexto, no promete evitar la asignación del codec.
        let delta_json_v1 = serializar_delta_json_v1(delta).map_err(mapear_delta)?;
        validar_limite_delta(delta_json_v1.len(), limite_delta)?;

        let contexto = Self {
            entradas_base: snapshot.entradas().to_vec(),
            configuracion: snapshot.configuracion_raw_capturada(),
            delta_json_v1,
            version_motor: Arc::from(snapshot.version_motor()),
            policy_id: Arc::from(snapshot.policy_id()),
            base_token: snapshot.token(),
            base_raw_tree_digest: snapshot.raw_tree_digest().clone(),
        };

        contexto.validar_base()?;
        Ok(contexto)
    }

    /// Revalida cuotas, Delta canónica y las identidades raw desde los valores
    /// retenidos. No confía en una marca de admisión previamente guardada.
    pub(crate) fn validar_base(&self) -> Result<(), ErrorContextoAdmision> {
        validar_limite_metadata(&self.version_motor, &self.policy_id, MAX_METADATA_BYTES)?;
        validar_limite_delta(self.delta_json_v1.len(), MAX_DELTA_BYTES)?;
        validar_presupuesto_configuracion(
            self.configuracion.manifiesto_raw(),
            self.configuracion.invariantes_raw(),
            PRESUPUESTO_BYTES_RAW,
        )
        .map_err(|_| ErrorContextoAdmision::BaseInconsistente)?;
        self.delta()?;

        let digest = calcular_digest_raw_tree(&self.entradas_base)
            .map_err(|_| ErrorContextoAdmision::BaseInconsistente)?;
        let token = calcular_token_snapshot(
            &self.version_motor,
            &self.policy_id,
            self.configuracion.manifiesto_raw(),
            self.configuracion.invariantes_raw(),
            &self.entradas_base,
        )
        .map_err(|_| ErrorContextoAdmision::BaseInconsistente)?;
        if digest != self.base_raw_tree_digest || token != self.base_token {
            return Err(ErrorContextoAdmision::BaseInconsistente);
        }
        Ok(())
    }

    /// Parsea y vuelve a canonizar el valor retenido antes de usarlo.
    pub(crate) fn delta(&self) -> Result<Delta, ErrorContextoAdmision> {
        let delta = parsear_delta_json_v1(&self.delta_json_v1).map_err(mapear_delta)?;
        let canonico = serializar_delta_json_v1(&delta).map_err(mapear_delta)?;
        if canonico != self.delta_json_v1 {
            return Err(ErrorContextoAdmision::DeltaInvalido);
        }
        Ok(delta)
    }

    pub(crate) fn entradas_base(&self) -> Vec<EntradaSnapshot> {
        self.entradas_base.clone()
    }

    pub(crate) fn configuracion(&self) -> ConfigRawCapturada {
        self.configuracion.clone()
    }

    pub(crate) fn delta_json_v1(&self) -> &str {
        &self.delta_json_v1
    }

    pub(crate) fn version_motor(&self) -> &str {
        &self.version_motor
    }

    pub(crate) fn policy_id(&self) -> &str {
        &self.policy_id
    }

    pub(crate) fn base_token(&self) -> SnapshotToken {
        self.base_token
    }

    pub(crate) fn base_raw_tree_digest(&self) -> &RawTreeDigest {
        &self.base_raw_tree_digest
    }
}

fn validar_limite_metadata(
    version_motor: &str,
    policy_id: &str,
    limite: usize,
) -> Result<(), ErrorContextoAdmision> {
    version_motor
        .len()
        .checked_add(policy_id.len())
        .filter(|bytes| *bytes <= limite)
        .map(|_| ())
        .ok_or(ErrorContextoAdmision::MetadataExcedeLimite)
}

fn validar_limite_delta(bytes: usize, limite: usize) -> Result<(), ErrorContextoAdmision> {
    if bytes <= limite {
        Ok(())
    } else {
        Err(ErrorContextoAdmision::DeltaExcedeLimite)
    }
}

fn mapear_delta(_: ErrorDeltaJson) -> ErrorContextoAdmision {
    ErrorContextoAdmision::DeltaInvalido
}

/// Error fijo; nunca conserva ni muestra configuración, átomos, paths o bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorContextoAdmision {
    DeltaInvalido,
    DeltaExcedeLimite,
    MetadataExcedeLimite,
    BaseInconsistente,
}

impl ErrorContextoAdmision {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::DeltaInvalido => "admission_delta_invalid",
            Self::DeltaExcedeLimite => "admission_delta_limit",
            Self::MetadataExcedeLimite => "admission_metadata_limit",
            Self::BaseInconsistente => "admission_base_invalid",
        }
    }
}

impl fmt::Debug for ErrorContextoAdmision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorContextoAdmision")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorContextoAdmision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorContextoAdmision {}

impl fmt::Debug for ContextoAdmision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ContextoAdmision { .. }")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::captura::materializar_snapshot_desde_entradas;
    use crate::transaccion::snapshot::EntradaSnapshot;
    use universo_core::{Atomo, Delta};

    const MANIFIESTO: &str = r#"version = "admission-test/1"

[nodos]
sortes = ["a", "Retirado", "Patrón"]

[nodos.disco]
"Patrón" = "pattern"

[layout]
directorios = ["knowledge"]
sin_iota = []
ignorados = []
"#;

    fn snapshot() -> Snapshot {
        let manifiesto = MANIFIESTO.as_bytes();
        let invariantes = b"\n";
        let entradas = vec![
            EntradaSnapshot::archivo("manifest.toml", manifiesto),
            EntradaSnapshot::archivo("invariants.smt2", invariantes.as_slice()),
            EntradaSnapshot::directorio("knowledge"),
            EntradaSnapshot::archivo(
                "knowledge/a.md",
                b"---\ntype: a\ntitle: A\n---\n".as_slice(),
            ),
        ];
        let configuracion = ConfigRawCapturada::nueva(
            manifiesto,
            invariantes.as_slice(),
            Some("manifest.toml".to_owned()),
            Some("invariants.smt2".to_owned()),
        )
        .unwrap();
        materializar_snapshot_desde_entradas(
            entradas,
            configuracion,
            "engine-test/1",
            "policy-test/1",
        )
        .unwrap()
    }

    #[test]
    fn captura_contexto_raw_canónico_y_redactado() {
        let snapshot = snapshot();
        let delta = Delta::nuevo([Atomo::parsear("Type(b, a)").unwrap()], []).unwrap();

        let contexto = ContextoAdmision::capturar(&snapshot, &delta).unwrap();

        assert_eq!(contexto.delta().unwrap(), delta);
        assert_eq!(
            contexto.delta_json_v1(),
            serializar_delta_json_v1(&delta).unwrap()
        );
        assert_eq!(contexto.entradas_base().len(), snapshot.entradas().len());
        assert_eq!(
            contexto.configuracion().manifiesto_raw(),
            MANIFIESTO.as_bytes()
        );
        assert_eq!(contexto.configuracion().invariantes_raw(), b"\n");
        assert_eq!(
            contexto.configuracion().ruta_manifiesto_root(),
            Some("manifest.toml")
        );
        assert_eq!(
            contexto.configuracion().ruta_invariantes_root(),
            Some("invariants.smt2")
        );
        assert_eq!(contexto.version_motor(), "engine-test/1");
        assert_eq!(contexto.policy_id(), "policy-test/1");
        assert_eq!(contexto.base_token(), snapshot.token());
        assert!(contexto.base_raw_tree_digest() == snapshot.raw_tree_digest());
        contexto.validar_base().unwrap();

        let debug = format!("{contexto:?}");
        assert!(!debug.contains("manifest.toml"));
        assert!(!debug.contains("engine-test/1"));
        assert!(!debug.contains("policy-test/1"));
    }

    #[test]
    fn captura_contexto_aplica_cuotas_sin_exponer_contenidos() {
        let snapshot = snapshot();
        let delta = Delta::nuevo([Atomo::parsear("Type(secreto, a)").unwrap()], []).unwrap();

        let error =
            ContextoAdmision::capturar_con_limites(&snapshot, &delta, 1, MAX_METADATA_BYTES)
                .unwrap_err();
        assert_eq!(error, ErrorContextoAdmision::DeltaExcedeLimite);
        assert!(!format!("{error:?}").contains("secreto"));

        let error = ContextoAdmision::capturar_con_limites(&snapshot, &delta, MAX_DELTA_BYTES, 1)
            .unwrap_err();
        assert_eq!(error, ErrorContextoAdmision::MetadataExcedeLimite);
        assert!(!format!("{error:?}").contains("engine-test/1"));
        assert!(!error.to_string().contains("policy-test/1"));
    }

    #[test]
    fn cuotas_privadas_aceptan_el_limite_y_rechazan_un_byte_mas() {
        let metadata_en_limite = "x".repeat(MAX_METADATA_BYTES);
        let metadata_excedida = format!("{metadata_en_limite}x");
        assert!(validar_limite_metadata(&metadata_en_limite, "", MAX_METADATA_BYTES).is_ok());
        assert_eq!(
            validar_limite_metadata(&metadata_excedida, "", MAX_METADATA_BYTES),
            Err(ErrorContextoAdmision::MetadataExcedeLimite)
        );
        assert!(validar_limite_delta(MAX_DELTA_BYTES, MAX_DELTA_BYTES).is_ok());
        assert_eq!(
            validar_limite_delta(MAX_DELTA_BYTES + 1, MAX_DELTA_BYTES),
            Err(ErrorContextoAdmision::DeltaExcedeLimite)
        );
    }

    #[test]
    fn revalidacion_rechaza_delta_no_canonica_y_base_alterada() {
        let snapshot = snapshot();
        let mut contexto = ContextoAdmision::capturar(&snapshot, &Delta::default()).unwrap();

        contexto.delta_json_v1.insert(0, ' ');
        assert!(matches!(
            contexto.delta(),
            Err(ErrorContextoAdmision::DeltaInvalido)
        ));

        let mut contexto = ContextoAdmision::capturar(&snapshot, &Delta::default()).unwrap();
        contexto.entradas_base[3] = EntradaSnapshot::archivo(
            "knowledge/a.md",
            b"---\ntype: a\ntitle: Alterado\n---\n".as_slice(),
        );
        assert!(matches!(
            contexto.validar_base(),
            Err(ErrorContextoAdmision::BaseInconsistente)
        ));
    }
}
