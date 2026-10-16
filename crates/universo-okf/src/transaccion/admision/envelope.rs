//! Envoltorio privado que liga un contexto de admisión a un journal.
//!
//! El vínculo compara metadatos inmutables y omite `JournalState`, que cambia
//! durante la transacción. Validarlo reproduce la propuesta; no observa el
//! filesystem, autentica los datos ni concede autoridad de recovery.

use std::fmt;

use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, Journal};
use crate::transaccion::replay::new_esperado::{
    ErrorNewEsperado, NewEsperado, calcular_new_esperado,
};

use super::ContextoAdmision;

#[allow(dead_code)]
pub(crate) mod codec;

/// Contexto raw asociado por consistencia a las identidades de un journal.
///
/// Es un valor en memoria sin rutas, handles ni lifecycle de publicación.
pub(crate) struct EnvelopeContextoAdmision {
    vinculo: VinculoJournal,
    contexto: ContextoAdmision,
}

impl EnvelopeContextoAdmision {
    /// Copia el vínculo inmutable del journal; la consistencia se comprueba al
    /// validar contra el journal y volver a reproducir el contexto.
    pub(crate) fn ligar(contexto: ContextoAdmision, journal: &Journal) -> Self {
        Self {
            vinculo: VinculoJournal::capturar(journal),
            contexto,
        }
    }

    /// Comprueba el vínculo completo y vuelve a calcular la expectativa raw.
    /// El resultado `NewEsperado` sigue siendo dato, no una capability.
    pub(crate) fn validar_replay(
        &self,
        journal: &Journal,
        motor_soportado: &str,
        policy_soportada: &str,
    ) -> Result<NewEsperado, ErrorEnvelopeAdmision> {
        if !self.vinculo.coincide(journal) {
            return Err(ErrorEnvelopeAdmision::VinculoJournalNoCoincide);
        }

        calcular_new_esperado(&self.contexto, journal, motor_soportado, policy_soportada)
            .map_err(ErrorEnvelopeAdmision::Replay)
    }
}

impl fmt::Debug for EnvelopeContextoAdmision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EnvelopeContextoAdmision { .. }")
    }
}

#[derive(Clone, PartialEq, Eq)]
struct VinculoJournal {
    transaction_id: String,
    parent: DirectoryIdentity,
    old: ArtifactFingerprint,
    new: ArtifactFingerprint,
}

impl VinculoJournal {
    fn capturar(journal: &Journal) -> Self {
        Self {
            transaction_id: journal.transaction_id().to_owned(),
            parent: journal.parent().clone(),
            old: journal.old().clone(),
            new: journal.new_fingerprint().clone(),
        }
    }

    fn coincide(&self, journal: &Journal) -> bool {
        self.transaction_id == journal.transaction_id()
            && self.parent == *journal.parent()
            && self.old == *journal.old()
            && self.new == *journal.new_fingerprint()
    }
}

/// Fallo fijo y redactado al comprobar el envelope y reproducir su contexto.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorEnvelopeAdmision {
    VinculoJournalNoCoincide,
    Replay(ErrorNewEsperado),
}

impl ErrorEnvelopeAdmision {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::VinculoJournalNoCoincide => "admission_envelope_journal_mismatch",
            Self::Replay(error) => error.codigo(),
        }
    }
}

impl fmt::Debug for ErrorEnvelopeAdmision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorEnvelopeAdmision")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorEnvelopeAdmision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorEnvelopeAdmision {}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use crate::serializar::serializar_candidato;
    use crate::transaccion::captura::{ConfigRawCapturada, materializar_snapshot_desde_entradas};
    use crate::transaccion::journal::{JournalState, RawTreeDigest};
    use crate::transaccion::replay::reproponer;
    use crate::transaccion::snapshot::{EntradaSnapshot, calcular_digest_raw_tree};
    use crate::transaccion::stage::{inventario_objetivo_para_replay, planificar_staging};
    use universo_core::{Delta, ResultadoPropuesta};

    pub(super) const MOTOR: &str = "engine-envelope/1";
    pub(super) const POLICY: &str = "policy-envelope/1";
    const MANIFIESTO: &str = "version = \"envelope-test/1\"\n\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patrón\"]\n\n[nodos.disco]\n\"Patrón\" = \"pattern\"\n\n[layout]\ndirectorios = [\"knowledge\"]\nsin_iota = []\nignorados = []\nparent_key = \"parent\"\n";
    const DOCUMENTO: &[u8] = b"---\ntype: a\ntitle: Index\nparent: \"\"\n---\nindex\n";

    pub(crate) fn contexto(invariantes: &[u8]) -> ContextoAdmision {
        let entradas = vec![
            EntradaSnapshot::directorio("knowledge"),
            EntradaSnapshot::archivo("knowledge/index.md", DOCUMENTO),
        ];
        let configuracion =
            ConfigRawCapturada::nueva(MANIFIESTO.as_bytes(), invariantes, None, None).unwrap();
        let snapshot =
            materializar_snapshot_desde_entradas(entradas, configuracion, MOTOR, POLICY).unwrap();
        ContextoAdmision::capturar(&snapshot, &Delta::nuevo([], []).unwrap()).unwrap()
    }

    fn digest_new(contexto: &ContextoAdmision) -> RawTreeDigest {
        let reproducida = reproponer(contexto, MOTOR, POLICY).unwrap();
        let ResultadoPropuesta::Admitida(candidato) = &reproducida.resultado else {
            panic!("la propuesta fixture debe ser admitida");
        };
        let documentos =
            serializar_candidato(reproducida.snapshot.estructura(), candidato, &[]).unwrap();
        let plan = planificar_staging(&reproducida.snapshot, &documentos).unwrap();
        let entradas = inventario_objetivo_para_replay(&plan).unwrap();
        calcular_digest_raw_tree(&entradas).unwrap()
    }

    pub(super) fn identidad(seed: u64) -> DirectoryIdentity {
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(seed, format!("{seed:032x}"))
        }
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(seed, seed + 1)
        }
    }

    pub(super) fn fingerprint(id: u64, digest: RawTreeDigest) -> ArtifactFingerprint {
        ArtifactFingerprint::new(identidad(id), digest)
    }

    fn journal(
        transaction_id: &str,
        parent: DirectoryIdentity,
        old: ArtifactFingerprint,
        new: ArtifactFingerprint,
    ) -> Journal {
        Journal::new(transaction_id, parent, old, new).unwrap()
    }

    pub(super) fn journal_valido(contexto: &ContextoAdmision) -> Journal {
        let digest_old = calcular_digest_raw_tree(&contexto.entradas_base()).unwrap();
        journal(
            "0123456789abcdef0123456789abcdef",
            identidad(1),
            fingerprint(2, digest_old),
            fingerprint(3, digest_new(contexto)),
        )
    }

    fn error_vinculo(envelope: &EnvelopeContextoAdmision, journal: &Journal) {
        let error = envelope.validar_replay(journal, MOTOR, POLICY).unwrap_err();
        assert_eq!(error.codigo(), "admission_envelope_journal_mismatch");
    }

    #[test]
    fn liga_replay_a_journal_y_acepta_solo_cambios_de_fase() {
        let contexto = contexto(b"\n");
        let journal = journal_valido(&contexto);
        let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);

        let esperado = envelope.validar_replay(&journal, MOTOR, POLICY).unwrap();
        assert_eq!(esperado.transaction_id(), journal.transaction_id());
        assert!(esperado.parent() == journal.parent());
        assert!(esperado.old() == journal.old());
        assert!(esperado.new_fingerprint() == journal.new_fingerprint());

        let mut sucesor = journal.clone();
        sucesor.advance(JournalState::BackupReady).unwrap();
        envelope.validar_replay(&sucesor, MOTOR, POLICY).unwrap();

        let debug = format!("{envelope:?}");
        assert!(!debug.contains(journal.transaction_id()));
        assert!(!debug.contains("Index"));
    }

    #[test]
    fn rechaza_otro_transaction_id_o_parent() {
        let contexto = contexto(b"\n");
        let journal = journal_valido(&contexto);
        let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);

        error_vinculo(
            &envelope,
            &Journal::new(
                "1123456789abcdef0123456789abcdef",
                journal.parent().clone(),
                journal.old().clone(),
                journal.new_fingerprint().clone(),
            )
            .unwrap(),
        );
        error_vinculo(
            &envelope,
            &Journal::new(
                journal.transaction_id(),
                identidad(9),
                journal.old().clone(),
                journal.new_fingerprint().clone(),
            )
            .unwrap(),
        );
    }

    #[test]
    fn rechaza_cambios_de_identidad_aun_si_el_digest_no_cambia() {
        let contexto = contexto(b"\n");
        let journal = journal_valido(&contexto);
        let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);
        let digest_old = calcular_digest_raw_tree(&envelope.contexto.entradas_base()).unwrap();
        let digest_new = digest_new(&envelope.contexto);

        error_vinculo(
            &envelope,
            &Journal::new(
                journal.transaction_id(),
                journal.parent().clone(),
                fingerprint(8, digest_old),
                journal.new_fingerprint().clone(),
            )
            .unwrap(),
        );
        error_vinculo(
            &envelope,
            &Journal::new(
                journal.transaction_id(),
                journal.parent().clone(),
                journal.old().clone(),
                fingerprint(9, digest_new),
            )
            .unwrap(),
        );
    }

    #[test]
    fn rechaza_cambios_de_digest_old_o_new() {
        let contexto = contexto(b"\n");
        let journal = journal_valido(&contexto);
        let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);
        let digest_distinto = RawTreeDigest::from_hex("f".repeat(64)).unwrap();

        error_vinculo(
            &envelope,
            &Journal::new(
                journal.transaction_id(),
                journal.parent().clone(),
                fingerprint(2, digest_distinto.clone()),
                journal.new_fingerprint().clone(),
            )
            .unwrap(),
        );
        error_vinculo(
            &envelope,
            &Journal::new(
                journal.transaction_id(),
                journal.parent().clone(),
                journal.old().clone(),
                fingerprint(3, digest_distinto),
            )
            .unwrap(),
        );
    }

    #[test]
    fn contexto_cuya_propuesta_se_rechaza_no_produce_new_esperado() {
        let contexto = contexto(b"(not (Type @knowledge/index a))");
        let digest_old = calcular_digest_raw_tree(&contexto.entradas_base()).unwrap();
        let journal = journal(
            "0123456789abcdef0123456789abcdef",
            identidad(1),
            fingerprint(2, digest_old),
            fingerprint(3, RawTreeDigest::from_hex("a".repeat(64)).unwrap()),
        );
        let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);

        let error = envelope
            .validar_replay(&journal, MOTOR, POLICY)
            .unwrap_err();

        assert_eq!(error.codigo(), "new_proposal_not_admitted");
    }

    #[test]
    fn rechaza_motor_o_policy_no_soportados() {
        let contexto = contexto(b"\n");
        let journal = journal_valido(&contexto);
        let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);

        assert_eq!(
            envelope
                .validar_replay(&journal, "engine-future/1", POLICY)
                .unwrap_err()
                .codigo(),
            "replay_engine_policy_mismatch"
        );
        assert_eq!(
            envelope
                .validar_replay(&journal, MOTOR, "policy-future/1")
                .unwrap_err()
                .codigo(),
            "replay_engine_policy_mismatch"
        );
    }
}
