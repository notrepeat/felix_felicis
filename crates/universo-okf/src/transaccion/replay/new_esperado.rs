//! Expectativa pura del inventario raw `new` ligado a un journal.
//!
//! Este valor no acredita un objeto instalado, no conserva handles y no es
//! una capability para recovery. El caller aún debe observar y comparar el
//! objeto filesystem bajo el lock transaccional.

use std::collections::BTreeMap;
use std::fmt;

use universo_core::{BundlePath, Estructura, ResultadoPropuesta};

use crate::serializar::{ErrorSerializarCandidato, ParentOverride, serializar_candidato};

use super::super::admision::ContextoAdmision;
use super::super::journal::{ArtifactFingerprint, DirectoryIdentity, Journal, RawTreeDigest};
use super::super::snapshot::{EntradaSnapshot, ErrorSnapshot, calcular_digest_raw_tree};
use super::super::stage::{ErrorPlanStaging, inventario_objetivo_para_replay, planificar_staging};
use super::{ErrorReplay, reproponer};

/// Datos raw esperados ligados a los metadatos del journal recibido.
///
/// Su contenido sólo describe la transición esperada. No afirma la identidad
/// ni el estado de la raíz, backup o stage observados.
pub(crate) struct NewEsperado {
    transaction_id: String,
    parent: DirectoryIdentity,
    old: ArtifactFingerprint,
    new: ArtifactFingerprint,
    entradas: Vec<EntradaSnapshot>,
    digest: RawTreeDigest,
}

impl NewEsperado {
    pub(crate) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(crate) fn parent(&self) -> &DirectoryIdentity {
        &self.parent
    }

    pub(crate) fn old(&self) -> &ArtifactFingerprint {
        &self.old
    }

    pub(crate) fn new_fingerprint(&self) -> &ArtifactFingerprint {
        &self.new
    }

    pub(crate) fn entradas(&self) -> &[EntradaSnapshot] {
        &self.entradas
    }

    pub(crate) fn digest(&self) -> &RawTreeDigest {
        &self.digest
    }
}

impl fmt::Debug for NewEsperado {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NewEsperado { .. }")
    }
}

/// Error fijo y redactado al reconstruir la expectativa `new`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorNewEsperado {
    Replay(ErrorReplay),
    PropuestaNoAdmitida,
    SnapshotIncompatible,
    Serializacion(ErrorSerializarCandidato),
    Plan(ErrorPlanStaging),
    Inventario(ErrorPlanStaging),
    Snapshot(ErrorSnapshot),
    HuellaOldNoCoincide,
    HuellaNewNoCoincide,
}

impl ErrorNewEsperado {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::Replay(error) => error.codigo(),
            Self::PropuestaNoAdmitida => "new_proposal_not_admitted",
            Self::SnapshotIncompatible => "candidate_snapshot_incompatible",
            Self::Serializacion(error) => error.codigo(),
            Self::Plan(error) | Self::Inventario(error) => error.codigo(),
            Self::Snapshot(error) => error.codigo(),
            Self::HuellaOldNoCoincide => "new_old_fingerprint_mismatch",
            Self::HuellaNewNoCoincide => "new_expected_fingerprint_mismatch",
        }
    }
}

impl fmt::Debug for ErrorNewEsperado {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorNewEsperado")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorNewEsperado {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorNewEsperado {}

/// Resultado del núcleo compartido; no conserva metadatos del Journal.
pub(super) struct InventarioRawRepropuesto {
    pub(super) entradas: Vec<EntradaSnapshot>,
    pub(super) digest: RawTreeDigest,
}

/// Vuelve a evaluar y serializar el delta, calcula el inventario raw completo
/// que produciría staging y verifica sus huellas old/new contra el journal.
///
/// No hace IO ni certifica que el journal o el contexto sean auténticos. El
/// contexto persistido debe validar por separado su envelope de transacción;
/// la observación de identidades filesystem corresponde a otra fase.
pub(crate) fn calcular_new_esperado(
    contexto: &ContextoAdmision,
    journal: &Journal,
    motor_soportado: &str,
    policy_soportada: &str,
) -> Result<NewEsperado, ErrorNewEsperado> {
    if contexto.version_motor() != motor_soportado || contexto.policy_id() != policy_soportada {
        return Err(ErrorNewEsperado::Replay(
            ErrorReplay::MotorPoliticaNoCoinciden,
        ));
    }
    contexto
        .validar_base()
        .map_err(|error| ErrorNewEsperado::Replay(ErrorReplay::Contexto(error)))?;
    let digest_old =
        calcular_digest_raw_tree(&contexto.entradas_base()).map_err(ErrorNewEsperado::Snapshot)?;
    let old = ArtifactFingerprint::new(journal.old().identity().clone(), digest_old);
    if &old != journal.old() {
        return Err(ErrorNewEsperado::HuellaOldNoCoincide);
    }

    let inventario =
        calcular_inventario_raw_repropuesto(contexto, motor_soportado, policy_soportada)?;
    let new = ArtifactFingerprint::new(
        journal.new_fingerprint().identity().clone(),
        inventario.digest.clone(),
    );
    if &new != journal.new_fingerprint() {
        return Err(ErrorNewEsperado::HuellaNewNoCoincide);
    }

    Ok(NewEsperado {
        transaction_id: journal.transaction_id().to_owned(),
        parent: journal.parent().clone(),
        old,
        new,
        entradas: inventario.entradas,
        digest: inventario.digest,
    })
}

/// Reproduce la propuesta y obtiene solo el inventario raw; no enlaza un Journal.
pub(super) fn calcular_inventario_raw_repropuesto(
    contexto: &ContextoAdmision,
    motor_soportado: &str,
    policy_soportada: &str,
) -> Result<InventarioRawRepropuesto, ErrorNewEsperado> {
    let reproducida = reproponer(contexto, motor_soportado, policy_soportada)
        .map_err(ErrorNewEsperado::Replay)?;
    let candidato = match &reproducida.resultado {
        ResultadoPropuesta::Admitida(candidato) => candidato.as_ref(),
        ResultadoPropuesta::RechazadaConflictos(_)
        | ResultadoPropuesta::RechazadaInvariantes(_) => {
            return Err(ErrorNewEsperado::PropuestaNoAdmitida);
        }
    };
    let overrides =
        derivar_parent_overrides(reproducida.snapshot.estructura(), candidato.estructura())?;
    let documentos = serializar_candidato(reproducida.snapshot.estructura(), candidato, &overrides)
        .map_err(ErrorNewEsperado::Serializacion)?;
    let plan =
        planificar_staging(&reproducida.snapshot, &documentos).map_err(ErrorNewEsperado::Plan)?;
    let entradas = inventario_objetivo_para_replay(&plan).map_err(ErrorNewEsperado::Inventario)?;
    let digest = calcular_digest_raw_tree(&entradas).map_err(ErrorNewEsperado::Snapshot)?;

    Ok(InventarioRawRepropuesto { entradas, digest })
}

fn derivar_parent_overrides(
    snapshot: &Estructura,
    candidato: &Estructura,
) -> Result<Vec<ParentOverride>, ErrorNewEsperado> {
    let padres_snapshot = padres_por_path(snapshot);
    let padres_candidato = padres_por_path(candidato);
    if padres_snapshot.keys().ne(padres_candidato.keys()) {
        return Err(ErrorNewEsperado::SnapshotIncompatible);
    }

    Ok(padres_candidato
        .iter()
        .filter(|(hijo, padre)| padres_snapshot.get(*hijo) != Some(*padre))
        .map(|(hijo, padre)| ParentOverride::new(hijo.clone(), padre.clone()))
        .collect())
}

fn padres_por_path(estructura: &Estructura) -> BTreeMap<BundlePath, Option<BundlePath>> {
    estructura
        .nodos()
        .map(|(id, nodo)| {
            let padre = estructura
                .padre(id)
                .and_then(|id_padre| estructura.nodo(id_padre))
                .map(|nodo_padre| nodo_padre.path.clone());
            (nodo.path.clone(), padre)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::transaccion::captura::{ConfigRawCapturada, materializar_snapshot_desde_entradas};
    use crate::transaccion::snapshot::TipoEntradaSnapshot;
    use universo_core::{Atomo, Delta};

    const MANIFIESTO: &str = "version = \"new-proof/1\"\n\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patrón\"]\n\n[layout]\ndirectorios = [\"knowledge\"]\nsin_iota = []\nignorados = []\nparent_key = \"parent\"\n";
    const MOTOR: &str = "engine-test/1";
    const POLICY: &str = "policy-test/1";
    const ROOT_DOC: &[u8] = b"---\ntype: a\ntitle: Root\nparent: \"\"\n---\nroot body\n";
    const SUB_DOC: &[u8] = b"---\ntype: a\ntitle: Sub\n---\nsub body\n";
    const LEAF_DOC: &[u8] = b"---\ntype: a\ntitle: Leaf\nparent: knowledge/index\n---\nleaf body\n";
    const AUX_DOC: &[u8] = b"captured auxiliary bytes";

    fn contexto(invariantes: &[u8], delta: Delta) -> ContextoAdmision {
        let entradas = vec![
            EntradaSnapshot::directorio("knowledge"),
            EntradaSnapshot::directorio("knowledge/empty"),
            EntradaSnapshot::directorio("knowledge/sub"),
            EntradaSnapshot::archivo("knowledge/index.md", ROOT_DOC),
            EntradaSnapshot::archivo("knowledge/note.txt", AUX_DOC),
            EntradaSnapshot::archivo("knowledge/sub/index.md", SUB_DOC),
            EntradaSnapshot::archivo("knowledge/sub/leaf.md", LEAF_DOC),
        ];
        let configuracion =
            ConfigRawCapturada::nueva(MANIFIESTO.as_bytes(), invariantes, None, None).unwrap();
        let snapshot =
            materializar_snapshot_desde_entradas(entradas, configuracion, MOTOR, POLICY).unwrap();
        ContextoAdmision::capturar(&snapshot, &delta).unwrap()
    }

    fn delta_mover_leaf() -> Delta {
        let hijo = BundlePath::nuevo("knowledge/sub/leaf").unwrap();
        Delta::nuevo(
            [Atomo::Parent {
                hijo: hijo.clone(),
                padre: BundlePath::nuevo("knowledge/sub/index").unwrap(),
            }],
            [Atomo::Parent {
                hijo,
                padre: BundlePath::nuevo("knowledge/index").unwrap(),
            }],
        )
        .unwrap()
    }

    fn identidad(seed: u64) -> DirectoryIdentity {
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(seed, format!("{seed:032x}"))
        }
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(seed, seed + 1)
        }
    }

    fn journal(contexto: &ContextoAdmision, digest_new: RawTreeDigest) -> Journal {
        let digest_old = calcular_digest_raw_tree(&contexto.entradas_base()).unwrap();
        Journal::new(
            "0123456789abcdef0123456789abcdef",
            identidad(1),
            ArtifactFingerprint::new(identidad(2), digest_old),
            ArtifactFingerprint::new(identidad(3), digest_new),
        )
        .unwrap()
    }

    fn inventario_esperado(contexto: &ContextoAdmision) -> Vec<EntradaSnapshot> {
        let reproducida = reproponer(contexto, MOTOR, POLICY).unwrap();
        let ResultadoPropuesta::Admitida(candidato) = &reproducida.resultado else {
            panic!("la propuesta fixture debe ser admitida");
        };
        let overrides =
            derivar_parent_overrides(reproducida.snapshot.estructura(), candidato.estructura())
                .unwrap();
        let documentos =
            serializar_candidato(reproducida.snapshot.estructura(), candidato, &overrides).unwrap();
        let plan = planificar_staging(&reproducida.snapshot, &documentos).unwrap();
        inventario_objetivo_para_replay(&plan).unwrap()
    }

    fn digest_con_aux_mutado(contexto: &ContextoAdmision) -> RawTreeDigest {
        let entradas = inventario_esperado(contexto)
            .into_iter()
            .filter(|entrada| entrada.ruta() != "knowledge/note.txt")
            .collect::<Vec<_>>();
        calcular_digest_raw_tree(&entradas).unwrap()
    }

    #[test]
    fn new_esperado_liga_journal_y_conserva_auxiliares_y_directorio_vacio() {
        let contexto = contexto(b"\n", delta_mover_leaf());
        let inventario = inventario_esperado(&contexto);
        let digest = calcular_digest_raw_tree(&inventario).unwrap();
        let journal = journal(&contexto, digest);

        let esperado = calcular_new_esperado(&contexto, &journal, MOTOR, POLICY).unwrap();

        assert_eq!(esperado.transaction_id(), journal.transaction_id());
        assert!(esperado.parent() == journal.parent());
        assert!(esperado.old() == journal.old());
        assert!(esperado.new_fingerprint() == journal.new_fingerprint());
        assert!(
            ArtifactFingerprint::new(
                journal.new_fingerprint().identity().clone(),
                esperado.digest().clone(),
            ) == *journal.new_fingerprint()
        );
        assert!(esperado.entradas().iter().any(|entrada| {
            entrada.ruta() == "knowledge/note.txt"
                && entrada.bytes() == Some(AUX_DOC)
                && entrada.tipo() == TipoEntradaSnapshot::Archivo
        }));
        assert!(esperado.entradas().iter().any(|entrada| {
            entrada.ruta() == "knowledge/empty"
                && entrada.bytes().is_none()
                && entrada.tipo() == TipoEntradaSnapshot::Directorio
        }));
        let leaf = esperado
            .entradas()
            .iter()
            .find(|entrada| entrada.ruta() == "knowledge/sub/leaf.md")
            .and_then(EntradaSnapshot::bytes)
            .unwrap();
        assert!(
            leaf.windows(b"parent: knowledge/sub/index".len())
                .any(|window| { window == b"parent: knowledge/sub/index" })
        );
    }

    #[test]
    fn new_esperado_rechaza_digest_que_omite_auxiliar() {
        let contexto = contexto(b"\n", Delta::nuevo([], []).unwrap());
        let journal = journal(&contexto, digest_con_aux_mutado(&contexto));

        let error = calcular_new_esperado(&contexto, &journal, MOTOR, POLICY).unwrap_err();

        assert_eq!(error.codigo(), "new_expected_fingerprint_mismatch");
    }

    #[test]
    fn new_esperado_rechaza_huella_old_distinta() {
        let contexto = contexto(
            b"(not (Type @knowledge/index a))",
            Delta::nuevo([], []).unwrap(),
        );
        let digest = calcular_digest_raw_tree(&contexto.entradas_base()).unwrap();
        let journal = journal(&contexto, digest);
        let digest_old_incompleto = calcular_digest_raw_tree(
            &contexto
                .entradas_base()
                .into_iter()
                .filter(|entrada| entrada.ruta() != "knowledge/note.txt")
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let journal = Journal::new(
            journal.transaction_id(),
            journal.parent().clone(),
            ArtifactFingerprint::new(identidad(2), digest_old_incompleto),
            journal.new_fingerprint().clone(),
        )
        .unwrap();

        let error = calcular_new_esperado(&contexto, &journal, MOTOR, POLICY).unwrap_err();

        assert_eq!(error.codigo(), "new_old_fingerprint_mismatch");
    }

    #[test]
    fn new_esperado_no_continua_una_propuesta_rechazada() {
        let contexto = contexto(
            b"(not (Type @knowledge/index a))",
            Delta::nuevo([], []).unwrap(),
        );
        let digest = calcular_digest_raw_tree(&contexto.entradas_base()).unwrap();
        let journal = journal(&contexto, digest);

        let error = calcular_new_esperado(&contexto, &journal, MOTOR, POLICY).unwrap_err();

        assert_eq!(error.codigo(), "new_proposal_not_admitted");
    }
}
