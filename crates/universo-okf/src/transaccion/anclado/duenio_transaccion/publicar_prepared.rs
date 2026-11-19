//! Publicación inicial ContextV2 → JournalV2 Prepared sin renombrar el root.
use super::{
    DuenioTransaccionAnclado, ResultadoOwnerPostcheck, context_journal_v2::CuerpoContextoJournalV2,
    observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo as Presencia,
    preparar_apply::PreparacionApply,
};
use crate::transaccion::{
    admision::envelope::v2::EnvelopeContextoAdmisionV2,
    anclado::contexto_admision::ReciboContextoPublicado,
    captura::{Snapshot, capturar_snapshot_soportado_con_cas_anclado},
    journal::{ArtifactFingerprint, JournalState, selector::JournalPersistido, v2::JournalV2},
    stage::StageAnclado,
};

/// Estado poseído incluso cuando una publicación sólo tuvo efecto parcial.
/// No es por sí mismo autoridad para renombrar root.
pub(super) struct EstadoPublicacionPrepared<'owner, 'lock> {
    pub(super) owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    pub(super) snapshot: Snapshot,
    pub(super) envelope: EnvelopeContextoAdmisionV2,
    pub(super) stage: StageAnclado,
    pub(super) journal: JournalV2,
    pub(super) journal_id: Option<super::super::journal_temporal::IdentidadArchivoJournal>,
    pub(super) contexto_publicado: Option<ReciboContextoPublicado>,
    pub(super) resultado_journal: Option<Box<ResultadoOwnerPostcheck<CuerpoContextoJournalV2>>>,
}
/// Ambos canónicos se confirmaron y el root continúa siendo Old.
#[must_use]
pub(crate) struct PreparacionPublicada<'owner, 'lock> {
    pub(super) estado: EstadoPublicacionPrepared<'owner, 'lock>,
}
/// Retiene snapshot, stage, envelope y resultados de storage; nunca limpia Drop.
pub(crate) struct ErrorPublicacionPrepared<'owner, 'lock> {
    pub(super) estado: Box<EstadoPublicacionPrepared<'owner, 'lock>>,
}
impl std::fmt::Debug for ErrorPublicacionPrepared<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("prepared_publication_unverified")
    }
}
impl<'owner, 'lock> PreparacionApply<'owner, 'lock> {
    pub(crate) fn publicar_prepared(
        self: Box<Self>,
    ) -> Result<PreparacionPublicada<'owner, 'lock>, ErrorPublicacionPrepared<'owner, 'lock>> {
        self.publicar_prepared_con_hook(|_| {})
    }
    fn publicar_prepared_con_hook(
        self: Box<Self>,
        hook: impl Fn(u8),
    ) -> Result<PreparacionPublicada<'owner, 'lock>, ErrorPublicacionPrepared<'owner, 'lock>> {
        // El efecto ya lo tomó el apply para el historial; publicar no lo usa.
        let Self {
            owner,
            snapshot,
            contexto,
            stage,
            journal,
            efecto: _,
        } = *self;
        let envelope = EnvelopeContextoAdmisionV2::ligar(contexto, &journal);
        let mut estado = EstadoPublicacionPrepared {
            owner,
            snapshot,
            envelope,
            stage,
            journal,
            journal_id: None,
            contexto_publicado: None,
            resultado_journal: None,
        };
        let result = estado.publicar_cuerpo(hook);
        // El postcheck domina incluso si el cuerpo produjo un efecto ambiguo.
        if estado.owner.parent_revalidado().is_err() || result.is_err() {
            Err(ErrorPublicacionPrepared {
                estado: Box::new(estado),
            })
        } else {
            Ok(PreparacionPublicada { estado })
        }
    }
}
impl EstadoPublicacionPrepared<'_, '_> {
    fn publicar_cuerpo(&mut self, hook: impl Fn(u8)) -> Result<(), ()> {
        self.validar_artefactos()?;
        self.validar_slots(false, false, false)?;
        if self.journal.state() != JournalState::Prepared {
            return Err(());
        }
        self.envelope
            .calcular_new_esperado(&self.journal)
            .map_err(|_| ())?;
        let temporal = self
            .owner
            .contexto_admision()
            .map_err(|_| ())?
            .crear_temporal_v2(&self.envelope)
            .map_err(|_| ())?
            .verificar()
            .map_err(|_| ())?;
        hook(0);
        let recibo = temporal.publicar().map_err(|_| ())?;
        self.contexto_publicado = Some(recibo);
        hook(1);
        self.validar_artefactos()?;
        self.validar_slots(true, false, false)?;
        let identidad_temporal = {
            let temporal = self
                .owner
                .crear_temporal_journal_v2(&self.journal)
                .map_err(|_| ())?;
            // El recibo conserva owner hasta destruirse; la validación completa
            // se repite tras soltar el préstamo y antes del intento canónico.
            temporal.identidad
        };
        hook(2);
        self.validar_artefactos()?;
        self.validar_slots(true, false, true)?;
        // El temporal original ya fue creado/validado. La reapertura compara
        // su identidad y registro y reconstruye el recibo, sin otra escritura.
        let temporal = self
            .owner
            .reabrir_temporal_prepared(&self.journal, identidad_temporal)
            .map_err(|_| ())?;
        let outcome = temporal.publicar_con_contexto(self.contexto_publicado.as_ref().ok_or(())?);
        let confirmado = matches!(&outcome,ResultadoOwnerPostcheck::PostcheckVerificado {cuerpo} if cuerpo.publicacion_confirmada());
        self.resultado_journal = Some(Box::new(outcome));
        if !confirmado {
            return Err(());
        }
        hook(3);
        self.validar_artefactos()?;
        self.validar_slots(true, true, false)?;
        self.journal_id = Some(identidad_temporal);
        let observado = self
            .owner
            .observar_journal_versionado()
            .map_err(|_| ())?
            .ok_or(())?;
        if observado.journal() != &JournalPersistido::V2(self.journal.clone())
            || observado.identidad() != identidad_temporal
        {
            return Err(());
        }
        let contexto = self
            .owner
            .observar_contexto_versionado()
            .map_err(|_| ())?
            .ok_or(())?;
        self.contexto_publicado
            .as_ref()
            .ok_or(())?
            .validar_observacion_v2(
                &self.owner.contexto_admision().map_err(|_| ())?,
                &contexto,
                &self.journal,
            )
            .map_err(|_| ())?;
        self.owner.parent_revalidado().map_err(|_| ())?;
        Ok(())
    }
    pub(super) fn validar_artefactos(&mut self) -> Result<(), ()> {
        self.owner.parent_revalidado().map_err(|_| ())?;
        let root = self.owner.abrir_root_lectura().map_err(|_| ())?.ok_or(())?;
        let actual = capturar_snapshot_soportado_con_cas_anclado(
            &root,
            self.snapshot.configuracion_raw_capturada(),
            self.snapshot.token(),
        )
        .map_err(|_| ())?;
        let root_id = actual
            .identidad_root()
            .and_then(|id| id.journal_identity())
            .ok_or(())?;
        if ArtifactFingerprint::new(root_id, actual.raw_tree_digest().clone())
            != *self.journal.old()
            || self.stage.fingerprint_raw().map_err(|_| ())? != *self.journal.new_fingerprint()
        {
            return Err(());
        }
        let raw = self
            .owner
            .observar_siblings_recovery_v2_raw(&self.journal)
            .map_err(|_| ())?;
        if raw.root() != Some(self.journal.old())
            || raw.backup().is_some()
            || raw.stage() != Some(self.journal.new_fingerprint())
        {
            return Err(());
        }
        self.owner.parent_revalidado().map_err(|_| ())?;
        Ok(())
    }
    pub(super) fn validar_slots(
        &mut self,
        context: bool,
        journal: bool,
        journal_tmp: bool,
    ) -> Result<(), ()> {
        let slots = self.owner.observar_siblings_nuevo_flujo().map_err(|_| ())?;
        let expected = |present| {
            if present {
                Presencia::Presente
            } else {
                Presencia::Ausente
            }
        };
        if slots.contexto() != expected(context)
            || slots.journal() != expected(journal)
            || slots.journal_temporal() != expected(journal_tmp)
            || [
                slots.contexto_temporal(),
                slots.intent_retiro(),
                slots.intent_retiro_temporal(),
            ]
            .iter()
            .any(|p| *p != Presencia::Ausente)
        {
            return Err(());
        }
        Ok(())
    }
}

impl<'lock> DuenioTransaccionAnclado<'lock> {
    fn reabrir_temporal_prepared<'owner>(
        &'owner mut self,
        journal: &JournalV2,
        identidad: super::super::journal_temporal::IdentidadArchivoJournal,
    ) -> Result<super::ReciboTemporalJournalV2<'owner, 'lock>, ()> {
        self.parent_revalidado().map_err(|_| ())?;
        let lease = self.journal_temporal_estricto().map_err(|_| ())?;
        let (actual, bytes) = lease.abrir_y_leer().map_err(|_| ())?;
        if actual != identidad || JournalV2::parse(&bytes).map_err(|_| ())? != *journal {
            return Err(());
        }
        self.parent_revalidado().map_err(|_| ())?;
        Ok(super::ReciboTemporalJournalV2 {
            _owner: self,
            journal: journal.clone(),
            identidad,
        })
    }
}

#[cfg(test)]
#[path = "publicar_prepared_tests.rs"]
mod tests;

/// Evidencia poseída que puede sobrevivir a la liberación del lock del caller.
/// No concede una nueva operación y no incluye referencias al owner.
pub(crate) struct EvidenciaPublicacionPrepared {
    _snapshot: Snapshot,
    _envelope: EnvelopeContextoAdmisionV2,
    _stage: StageAnclado,
    _journal: JournalV2,
    _journal_id: Option<super::super::journal_temporal::IdentidadArchivoJournal>,
    _contexto_publicado: Option<ReciboContextoPublicado>,
    _resultado_journal: Option<Box<ResultadoOwnerPostcheck<CuerpoContextoJournalV2>>>,
}
impl EstadoPublicacionPrepared<'_, '_> {
    pub(super) fn into_evidencia_sin_owner(self) -> EvidenciaPublicacionPrepared {
        let Self {
            owner: _,
            snapshot,
            envelope,
            stage,
            journal,
            journal_id,
            contexto_publicado,
            resultado_journal,
        } = self;
        EvidenciaPublicacionPrepared {
            _snapshot: snapshot,
            _envelope: envelope,
            _stage: stage,
            _journal: journal,
            _journal_id: journal_id,
            _contexto_publicado: contexto_publicado,
            _resultado_journal: resultado_journal,
        }
    }
}
impl ErrorPublicacionPrepared<'_, '_> {
    pub(crate) fn into_evidencia_sin_owner(self) -> EvidenciaPublicacionPrepared {
        self.estado.into_evidencia_sin_owner()
    }
}
