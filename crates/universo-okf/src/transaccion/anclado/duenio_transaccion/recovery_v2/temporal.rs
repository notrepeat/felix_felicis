//! Un TMP sucesor nunca es autoridad para promover el Journal canónico.
use super::*;
use crate::transaccion::admision::envelope::selector::EnvelopeAdmisionVersionada;
use crate::transaccion::journal::selector::JournalPersistido;
use crate::transaccion::anclado::duenio_transaccion::observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo::{Ausente, Presente};

impl DuenioTransaccionAnclado<'_> {
    pub(in crate::transaccion::anclado::duenio_transaccion) fn reconciliar_sucesor_pendiente_v2(
        &mut self,
    ) -> Result<bool, ErrorRecoveryV2> {
        self.parent_revalidado().map_err(|_| ErrorRecoveryV2)?;
        let result = self.reconciliar_sucesor_pendiente_v2_interno();
        self.parent_revalidado().map_err(|_| ErrorRecoveryV2)?;
        result
    }

    fn reconciliar_sucesor_pendiente_v2_interno(&mut self) -> Result<bool, ErrorRecoveryV2> {
        let slots = self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorRecoveryV2)?;
        if slots.journal_temporal() == Ausente {
            return Ok(false);
        }
        if slots.journal() != Presente
            || slots.contexto() != Presente
            || slots.contexto_temporal() != Ausente
            || slots.intent_retiro() != Ausente
            || slots.intent_retiro_temporal() != Ausente
        {
            return Err(ErrorRecoveryV2);
        }
        let original = self
            .observar_journal_versionado()
            .map_err(|_| ErrorRecoveryV2)?
            .ok_or(ErrorRecoveryV2)?;
        let JournalPersistido::V2(journal) = original.journal() else {
            return Err(ErrorRecoveryV2);
        };
        let contexto = self
            .observar_contexto_versionado()
            .map_err(|_| ErrorRecoveryV2)?
            .ok_or(ErrorRecoveryV2)?;
        let EnvelopeAdmisionVersionada::V2(envelope) = contexto.envelope_versionado() else {
            return Err(ErrorRecoveryV2);
        };
        let replay = envelope
            .calcular_new_esperado(journal)
            .map_err(|_| ErrorRecoveryV2)?;
        if journal.old().identity() == journal.new_fingerprint().identity() {
            return Err(ErrorRecoveryV2);
        }
        let raw = self
            .observar_siblings_recovery_v2_raw(journal)
            .map_err(|_| ErrorRecoveryV2)?;
        let expected = ArtifactFingerprint::new(
            journal.new_fingerprint().identity().clone(),
            replay.digest().clone(),
        );
        let inv = RecoveryInventory {
            root: clasificar(raw.root(), journal, false, false),
            backup: clasificar(raw.backup(), journal, true, false),
            stage: clasificar(raw.stage(), journal, false, true),
            root_new_validated: raw.root() == Some(&expected),
        };
        if decidir(journal.state(), inv) == AccionRecoveryV2::FailClosed {
            return Err(ErrorRecoveryV2);
        }
        let contexto_fresco = self
            .observar_contexto_versionado()
            .map_err(|_| ErrorRecoveryV2)?
            .ok_or(ErrorRecoveryV2)?;
        if contexto.evidencia_raw_retiro() != contexto_fresco.evidencia_raw_retiro() {
            return Err(ErrorRecoveryV2);
        }
        if self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorRecoveryV2)?
            != slots
        {
            return Err(ErrorRecoveryV2);
        }
        {
            let lease = self
                .journal_temporal_estricto()
                .map_err(|_| ErrorRecoveryV2)?;
            let recibo = lease
                .observar_sucesor_existente_v2(&original)
                .map_err(|_| ErrorRecoveryV2)?;
            lease
                .descartar_sucesor_v2(recibo)
                .map_err(|_| ErrorRecoveryV2)?;
        }
        let canonico = self
            .observar_journal_versionado()
            .map_err(|_| ErrorRecoveryV2)?
            .ok_or(ErrorRecoveryV2)?;
        let contexto_final = self
            .observar_contexto_versionado()
            .map_err(|_| ErrorRecoveryV2)?
            .ok_or(ErrorRecoveryV2)?;
        if canonico.identidad() != original.identidad()
            || canonico.journal() != original.journal()
            || contexto_final.evidencia_raw_retiro() != contexto.evidencia_raw_retiro()
        {
            return Err(ErrorRecoveryV2);
        }
        let _normal_final = self.observar_normal_v2().map_err(|_| ErrorRecoveryV2)?;
        Ok(true)
    }
}
