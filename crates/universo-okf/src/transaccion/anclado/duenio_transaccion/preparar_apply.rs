//! Prepara bajo un único owner; no publica root, Journal ni Context.
use super::{
    DuenioTransaccionAnclado,
    observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo as Presencia,
};
use crate::transaccion::{
    admision::ContextoAdmision,
    captura::{ConfigRawCapturada, Snapshot, capturar_snapshot_soportado_con_cas_anclado},
    journal::{ArtifactFingerprint, v2::JournalV2},
    snapshot::SnapshotToken,
    stage::{ErrorEscrituraStage, StageAnclado, materializar_staging_anclado, planificar_staging},
};
use universo_core::{Delta, ResultadoPropuesta, proponer};

#[must_use]
pub(crate) struct PreparacionApply<'owner, 'lock> {
    pub(super) owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    pub(super) snapshot: Snapshot,
    pub(super) contexto: ContextoAdmision,
    pub(super) stage: StageAnclado,
    pub(super) journal: JournalV2,
}
pub(crate) enum ResultadoPreparacionApply<'owner, 'lock> {
    Preparada(Box<PreparacionApply<'owner, 'lock>>),
    Rechazada(ResultadoPropuesta),
}
/// Conserva evidencia de efectos parciales; no ejecuta cleanup al destruirse.
pub(crate) struct ErrorPreparacionApply {
    codigo: &'static str,
    pub(super) stage: Option<Box<StageAnclado>>,
    pub(super) escritura: Option<Box<ErrorEscrituraStage>>,
}
impl ErrorPreparacionApply {
    fn nuevo(codigo: &'static str) -> Self {
        Self {
            codigo,
            stage: None,
            escritura: None,
        }
    }
    pub(crate) fn codigo(&self) -> &'static str {
        self.codigo
    }
}
impl std::fmt::Debug for ErrorPreparacionApply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.codigo)
    }
}
impl PreparacionApply<'_, '_> {
    /// Digest del árbol raw del stage verificado: lo que el apply va a instalar.
    pub(crate) fn digest_nuevo(&self) -> crate::transaccion::journal::RawTreeDigest {
        self.journal.new_fingerprint().digest().clone()
    }
}

impl<'lock> DuenioTransaccionAnclado<'lock> {
    /// Retira, bajo el mismo lock, el stage que dejó una preparación fallida.
    /// Sin journal ese stage no se puede probar propio después, y bloquearía
    /// todo apply siguiente (`exigir_stages_ausentes`). Solo limpia si el owner
    /// sigue válido y con la capability anclada; `StageAnclado::limpiar`
    /// revalida parent, handle e identidad. Si no puede, conserva la evidencia
    /// y devuelve `apply_stage_retained`; el código original se pierde solo en
    /// ese caso, porque el stage retenido es lo que el usuario debe atender.
    pub(crate) fn limpiar_stage_de_preparacion_fallida(
        &self,
        mut error: ErrorPreparacionApply,
    ) -> ErrorPreparacionApply {
        let stage = match (error.stage.take(), error.escritura.take()) {
            (Some(stage), escritura) => {
                error.escritura = escritura;
                Some(*stage)
            }
            (None, Some(escritura)) => match escritura.recuperar_stage_anclado() {
                Ok(stage) => Some(stage),
                Err(escritura) => {
                    if escritura.sibling_huerfano_posible() {
                        error.codigo = "apply_stage_retained";
                    }
                    error.escritura = Some(Box::new(escritura));
                    None
                }
            },
            (None, None) => None,
        };
        let Some(stage) = stage else {
            return error;
        };
        if self.parent_revalidado().is_err() {
            error.stage = Some(Box::new(stage));
            error.codigo = "apply_stage_retained";
            return error;
        }
        if let Err(fallo) = stage.limpiar() {
            error.stage = Some(Box::new(fallo.recuperar_stage()));
            error.codigo = "apply_stage_retained";
        }
        error
    }

    pub(crate) fn preparar_apply<'owner>(
        &'owner mut self,
        config: ConfigRawCapturada,
        esperado: SnapshotToken,
        delta: &Delta,
    ) -> Result<ResultadoPreparacionApply<'owner, 'lock>, ErrorPreparacionApply> {
        self.preparar_apply_con_hook(config, esperado, delta, || {})
    }
    pub(crate) fn preparar_apply_con_hook<'owner>(
        &'owner mut self,
        config: ConfigRawCapturada,
        esperado: SnapshotToken,
        delta: &Delta,
        despues_stage: impl FnOnce(),
    ) -> Result<ResultadoPreparacionApply<'owner, 'lock>, ErrorPreparacionApply> {
        self.exigir_namespace_apply_vacio()?;
        self.exigir_stages_ausentes(None)
            .map_err(|_| ErrorPreparacionApply::nuevo("apply_orphan_stage"))?;
        let mut raiz = self
            .abrir_root_lectura()
            .map_err(|_| ErrorPreparacionApply::nuevo("apply_owner_invalid"))?
            .ok_or_else(|| ErrorPreparacionApply::nuevo("apply_root_absent"))?;
        let snapshot = capturar_snapshot_soportado_con_cas_anclado(&raiz, config, esperado)
            .map_err(|e| ErrorPreparacionApply::nuevo(e.codigo()))?;
        let propuesta = proponer(snapshot.estructura(), delta, snapshot.fuentes_invariantes())
            .map_err(|e| ErrorPreparacionApply::nuevo(e.codigo()))?;
        let ResultadoPropuesta::Admitida(candidato) = propuesta else {
            self.exigir_namespace_apply_vacio()?;
            return Ok(ResultadoPreparacionApply::Rechazada(propuesta));
        };
        let contexto = ContextoAdmision::capturar(&snapshot, delta)
            .map_err(|e| ErrorPreparacionApply::nuevo(e.codigo()))?;
        let overrides = crate::transaccion::replay::new_esperado::derivar_parent_overrides(
            snapshot.estructura(),
            candidato.estructura(),
        )
        .map_err(|e| ErrorPreparacionApply::nuevo(e.codigo()))?;
        let documentos =
            crate::serializar::serializar_candidato(snapshot.estructura(), &candidato, &overrides)
                .map_err(|e| ErrorPreparacionApply::nuevo(e.codigo()))?;
        let plan = planificar_staging(&snapshot, &documentos)
            .map_err(|e| ErrorPreparacionApply::nuevo(e.codigo()))?;
        self.exigir_namespace_apply_vacio()?;
        self.exigir_stages_ausentes(None)
            .map_err(|_| ErrorPreparacionApply::nuevo("apply_orphan_stage"))?;
        // Sólo habilita la creación exclusiva de un sibling mediante el parent
        // retenido por este owner. El root continúa siendo de solo lectura.
        raiz.puede_crear_stage_sibling = true;
        let stage = match materializar_staging_anclado(&raiz, &snapshot, &plan, &candidato) {
            Ok(stage) => stage,
            Err(error) => {
                let mut retained = ErrorPreparacionApply::nuevo(error.codigo());
                retained.escritura = Some(Box::new(error));
                if self.parent_revalidado().is_err() {
                    retained.codigo = "apply_owner_invalid";
                }
                return Err(retained);
            }
        };
        despues_stage();
        let result = (|| {
            self.exigir_namespace_apply_vacio()?;
            self.exigir_stages_ausentes(Some(stage.componente()))
                .map_err(|_| ErrorPreparacionApply::nuevo("apply_orphan_stage"))?;
            let root_actual = self
                .abrir_root_lectura()
                .map_err(|_| ErrorPreparacionApply::nuevo("apply_owner_invalid"))?
                .ok_or_else(|| ErrorPreparacionApply::nuevo("apply_root_absent"))?;
            let recapturado = capturar_snapshot_soportado_con_cas_anclado(
                &root_actual,
                snapshot.configuracion_raw_capturada(),
                snapshot.token(),
            )
            .map_err(|e| ErrorPreparacionApply::nuevo(e.codigo()))?;
            if recapturado.identidad_root() != snapshot.identidad_root() {
                return Err(ErrorPreparacionApply::nuevo("apply_root_changed"));
            }
            let parent = self
                .identidad_parent
                .journal_identity()
                .ok_or_else(|| ErrorPreparacionApply::nuevo("apply_identity_invalid"))?;
            let old_id = snapshot
                .identidad_root()
                .and_then(|i| i.journal_identity())
                .ok_or_else(|| ErrorPreparacionApply::nuevo("apply_identity_invalid"))?;
            let receipt = stage
                .recibo()
                .ok_or_else(|| ErrorPreparacionApply::nuevo("apply_stage_invalid"))?;
            let new_id = receipt
                .identidad_journal()
                .cloned()
                .ok_or_else(|| ErrorPreparacionApply::nuevo("apply_stage_invalid"))?;
            JournalV2::new(
                stage.stage_id(),
                parent,
                ArtifactFingerprint::new(old_id, snapshot.raw_tree_digest().clone()),
                ArtifactFingerprint::new(new_id, receipt.raw_tree_digest().clone()),
            )
            .map_err(|_| ErrorPreparacionApply::nuevo("apply_journal_invalid"))
        })();
        let result = if self.parent_revalidado().is_err() {
            Err(ErrorPreparacionApply::nuevo("apply_owner_invalid"))
        } else {
            result
        };
        match result {
            Ok(journal) => Ok(ResultadoPreparacionApply::Preparada(Box::new(
                PreparacionApply {
                    owner: self,
                    snapshot,
                    contexto,
                    stage,
                    journal,
                },
            ))),
            Err(mut error) => {
                error.stage = Some(Box::new(stage));
                Err(error)
            }
        }
    }

    fn exigir_namespace_apply_vacio(&mut self) -> Result<(), ErrorPreparacionApply> {
        self.parent_revalidado()
            .map_err(|_| ErrorPreparacionApply::nuevo("apply_owner_invalid"))?;
        let slots = self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorPreparacionApply::nuevo("apply_namespace_invalid"))?;
        if [
            slots.journal(),
            slots.journal_temporal(),
            slots.contexto(),
            slots.contexto_temporal(),
            slots.intent_retiro(),
            slots.intent_retiro_temporal(),
        ]
        .iter()
        .any(|p| *p != Presencia::Ausente)
        {
            return Err(ErrorPreparacionApply::nuevo("apply_recovery_required"));
        }
        let backup = self._lock.ubicacion().backup();
        let nombre = backup
            .as_path()
            .file_name()
            .ok_or_else(|| ErrorPreparacionApply::nuevo("apply_namespace_invalid"))?;
        if self
            .abrir_sibling_lectura(nombre)
            .map_err(|_| ErrorPreparacionApply::nuevo("apply_namespace_invalid"))?
            .is_some()
        {
            return Err(ErrorPreparacionApply::nuevo("apply_recovery_required"));
        }
        self.parent_revalidado()
            .map_err(|_| ErrorPreparacionApply::nuevo("apply_owner_invalid"))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "preparar_apply_tests.rs"]
pub(super) mod tests;
