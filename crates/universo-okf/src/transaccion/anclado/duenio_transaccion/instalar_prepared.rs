//! Forward del root bajo el owner único: Old→backup, stage→root.
use super::{
    DuenioTransaccionAnclado, ResultadoOwnerPostcheck,
    publicar_prepared::{EstadoPublicacionPrepared, PreparacionPublicada},
};
use crate::transaccion::{
    anclado::journal_temporal::{
        EstadoPublicacionSucesorV2, EvidenciaSucesorJournalV2,
        versionado::ObservacionJournalPersistido,
    },
    journal::{ArtifactFingerprint, DirectoryIdentity, JournalState, selector::JournalPersistido},
};
use std::ffi::OsStr;

#[must_use]
pub(crate) struct InstalacionPreparada<'owner, 'lock> {
    pub(super) estado: EstadoPublicacionPrepared<'owner, 'lock>,
}
pub(crate) struct ErrorInstalacionPrepared<'owner, 'lock> {
    pub(super) estado: Box<EstadoPublicacionPrepared<'owner, 'lock>>,
    original: Option<Box<ObservacionJournalPersistido>>,
    sucesor: Option<Box<EvidenciaSucesorJournalV2>>,
}
impl std::fmt::Debug for ErrorInstalacionPrepared<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("prepared_installation_unverified")
    }
}
struct EvidenciaAvance {
    original: Option<Box<ObservacionJournalPersistido>>,
    sucesor: Option<Box<EvidenciaSucesorJournalV2>>,
}
impl<'owner, 'lock> PreparacionPublicada<'owner, 'lock> {
    pub(crate) fn instalar(
        self,
    ) -> Result<InstalacionPreparada<'owner, 'lock>, ErrorInstalacionPrepared<'owner, 'lock>> {
        self.instalar_con_hook(|_| {})
    }
    fn instalar_con_hook(
        self,
        hook: impl Fn(u8),
    ) -> Result<InstalacionPreparada<'owner, 'lock>, ErrorInstalacionPrepared<'owner, 'lock>> {
        let mut estado = self.estado;
        let mut evidencia = EvidenciaAvance {
            original: None,
            sucesor: None,
        };
        let result = estado.instalar_cuerpo(&hook, &mut evidencia);
        if estado.owner.parent_revalidado().is_err() || result.is_err() {
            Err(ErrorInstalacionPrepared {
                estado: Box::new(estado),
                original: evidencia.original,
                sucesor: evidencia.sucesor,
            })
        } else {
            Ok(InstalacionPreparada { estado })
        }
    }
}
impl EstadoPublicacionPrepared<'_, '_> {
    fn instalar_cuerpo(
        &mut self,
        hook: &impl Fn(u8),
        evidencia: &mut EvidenciaAvance,
    ) -> Result<(), ()> {
        if self.journal.state() != JournalState::Prepared {
            return Err(());
        }
        self.validar_artefactos()?;
        self.validar_slots(true, true, false)?;
        self.validar_journal_y_contexto()?;
        let old = self.journal.old().clone();
        let new = self.journal.new_fingerprint().clone();
        let location = self.owner._lock.ubicacion().clone();
        let root = location.ruta().file_name().ok_or(())?;
        let backup_path = location.backup();
        let backup = backup_path.as_path().file_name().ok_or(())?;
        self.owner
            .mover_sibling_v2(root, backup, self.journal.old().identity())?;
        hook(1);
        self.validar_fila(None, Some(&old), Some(&new))?;
        self.avanzar_journal_instalacion(JournalState::BackupReady, evidencia)?;
        hook(2);
        self.validar_fila(None, Some(&old), Some(&new))?;
        self.owner.mover_sibling_v2(
            self.stage.componente(),
            root,
            self.journal.new_fingerprint().identity(),
        )?;
        hook(3);
        self.validar_fila(Some(&new), Some(&old), None)?;
        self.avanzar_journal_instalacion(JournalState::Installed, evidencia)?;
        hook(4);
        self.validar_fila(Some(&new), Some(&old), None)?;
        self.owner.parent_revalidado().map_err(|_| ())?;
        Ok(())
    }
    fn validar_fila(
        &mut self,
        root: Option<&ArtifactFingerprint>,
        backup: Option<&ArtifactFingerprint>,
        stage: Option<&ArtifactFingerprint>,
    ) -> Result<(), ()> {
        self.validar_slots(true, true, false)?;
        self.validar_journal_y_contexto()?;
        let raw = self
            .owner
            .observar_siblings_recovery_v2_raw(&self.journal)
            .map_err(|_| ())?;
        if raw.root() != root || raw.backup() != backup || raw.stage() != stage {
            return Err(());
        }
        self.owner.parent_revalidado().map_err(|_| ())?;
        Ok(())
    }
    fn validar_journal_y_contexto(&mut self) -> Result<(), ()> {
        let original = self
            .owner
            .observar_journal_versionado()
            .map_err(|_| ())?
            .ok_or(())?;
        if original.journal() != &JournalPersistido::V2(self.journal.clone())
            || Some(original.identidad()) != self.journal_id
        {
            return Err(());
        }
        let context = self
            .owner
            .observar_contexto_versionado()
            .map_err(|_| ())?
            .ok_or(())?;
        self.contexto_publicado
            .as_ref()
            .ok_or(())?
            .validar_observacion_v2(
                &self.owner.contexto_admision().map_err(|_| ())?,
                &context,
                &self.journal,
            )
            .map_err(|_| ())?;
        self.envelope
            .calcular_new_esperado(&self.journal)
            .map_err(|_| ())?;
        self.owner.parent_revalidado().map_err(|_| ())?;
        Ok(())
    }
    fn avanzar_journal_instalacion(
        &mut self,
        next: JournalState,
        evidencia: &mut EvidenciaAvance,
    ) -> Result<(), ()> {
        let original = self
            .owner
            .observar_journal_versionado()
            .map_err(|_| ())?
            .ok_or(())?;
        if original.journal() != &JournalPersistido::V2(self.journal.clone())
            || Some(original.identidad()) != self.journal_id
        {
            return Err(());
        }
        let recibo = match self.owner.preparar_sucesor_journal_v2(&original) {
            ResultadoOwnerPostcheck::PostcheckVerificado { cuerpo: Ok(recibo) } => recibo,
            ResultadoOwnerPostcheck::PostcheckNoVerificado {
                cuerpo: Ok(recibo), ..
            } => {
                evidencia.sucesor = Some(Box::new(recibo.into_evidencia()));
                evidencia.original = Some(Box::new(original));
                return Err(());
            }
            _ => {
                evidencia.original = Some(Box::new(original));
                return Err(());
            }
        };
        let id = recibo.identidad_temporal();
        let result = (|| {
            self.validar_journal_y_contexto()?;
            self.validar_slots(true, true, true)?;
            let raw = self
                .owner
                .observar_siblings_recovery_v2_raw(&self.journal)
                .map_err(|_| ())?;
            let expected = match next {
                JournalState::BackupReady => {
                    raw.root().is_none()
                        && raw.backup() == Some(self.journal.old())
                        && raw.stage() == Some(self.journal.new_fingerprint())
                }
                JournalState::Installed => {
                    raw.root() == Some(self.journal.new_fingerprint())
                        && raw.backup() == Some(self.journal.old())
                        && raw.stage().is_none()
                }
                _ => false,
            };
            if !expected {
                return Err(());
            }
            match self.owner.publicar_sucesor_journal_v2(&recibo) {
                ResultadoOwnerPostcheck::PostcheckVerificado {
                    cuerpo: Ok(EstadoPublicacionSucesorV2::Confirmada),
                } => {}
                _ => return Err(()),
            }
            self.journal.advance(next).map_err(|_| ())?;
            self.journal_id = Some(id);
            self.validar_journal_y_contexto()?;
            self.validar_slots(true, true, false)?;
            Ok(())
        })();
        evidencia.sucesor = Some(Box::new(recibo.into_evidencia()));
        evidencia.original = Some(Box::new(original));
        if self.owner.parent_revalidado().is_err() {
            return Err(());
        }
        result
    }
}
impl DuenioTransaccionAnclado<'_> {
    pub(super) fn mover_sibling_v2(
        &mut self,
        source: &OsStr,
        target: &OsStr,
        expected: &DirectoryIdentity,
    ) -> Result<(), ()> {
        let source_anchor = self
            .abrir_sibling_lectura(source)
            .map_err(|_| ())?
            .ok_or(())?;
        let identity = source_anchor.identidad_raiz.ok_or(())?;
        if identity.journal_identity().as_ref() != Some(expected) {
            return Err(());
        }
        let mut espacio = self.espacio_transaccional_retenido()?;
        let mut sibling = espacio
            .observar_directorio_sibling(source, identity)
            .map_err(|_| ())?;
        let result = espacio
            .renombrar_directorio(&mut sibling, target)
            .map_err(|_| ());
        drop(espacio);
        self.parent_revalidado().map_err(|_| ())?;
        result
    }
}

#[cfg(test)]
#[path = "instalar_prepared_tests.rs"]
mod tests;

impl DuenioTransaccionAnclado<'_> {
    pub(super) fn espacio_transaccional_retenido(
        &mut self,
    ) -> Result<super::super::renombrar::EspacioTransaccionalAnclado<'_>, ()> {
        let parent = self.parent_revalidado().map_err(|_| ())?;
        super::super::renombrar::EspacioTransaccionalAnclado::desde_parent_retenido(
            self._lock,
            parent,
            self.identidad_parent,
            self.scope_parent,
        )
        .map_err(|_| ())
    }
}

pub(crate) enum ErrorFinalizacionPrepared<'owner, 'lock> {
    Precondicion(Box<InstalacionPreparada<'owner, 'lock>>),
    Confirmacion(Box<super::recovery_v2::forward::ErrorConfirmacionV2>),
}
impl std::fmt::Debug for ErrorFinalizacionPrepared<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("prepared_finalization_unverified")
    }
}
impl<'owner, 'lock> InstalacionPreparada<'owner, 'lock> {
    pub(crate) fn finalizar(mut self) -> Result<(), ErrorFinalizacionPrepared<'owner, 'lock>> {
        let old = self.estado.journal.old().clone();
        let new = self.estado.journal.new_fingerprint().clone();
        if self.estado.journal.state() != JournalState::Installed
            || self
                .estado
                .validar_fila(Some(&new), Some(&old), None)
                .is_err()
        {
            return Err(ErrorFinalizacionPrepared::Precondicion(Box::new(self)));
        }
        // Las lecturas Context retenidas por la publicación inicial ya no son
        // necesarias: el dispatcher vuelve a abrirlas. Cerrarlas antes del
        // delete es indispensable en Windows; sus bytes siguen en envelope.
        self.estado.resultado_journal = None;
        self.estado
            .owner
            .reanudar_confirmacion_v2()
            .map_err(|e| ErrorFinalizacionPrepared::Confirmacion(Box::new(e)))
    }
}
