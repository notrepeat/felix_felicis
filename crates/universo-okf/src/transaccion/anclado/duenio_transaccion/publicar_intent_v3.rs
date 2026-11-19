//! Publica una vez el Intent autocontenido desde NormalV2 Committed verificado.
//! Un error puede conservar TMP o un Intent publicado: nunca limpia ni reintenta.
use super::{
    DuenioTransaccionAnclado,
    observar_siblings_nuevo_flujo::{
        PresenciaSiblingNuevoFlujo as Presencia, nombre_intent_retiro,
    },
};
use crate::transaccion::admision::envelope::selector::EnvelopeAdmisionVersionada;
use crate::transaccion::anclado::contexto_admision::ObservacionContextoVersionada;
use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
use crate::transaccion::anclado::intent_persistido_v2::IntentRetiroPersistidoV2;
use crate::transaccion::anclado::intent_persistido_v3::{
    IntentRetiroPersistidoV3, archivo::leer_archivo,
};
use crate::transaccion::anclado::journal_temporal::versionado::ObservacionJournalPersistido;
use crate::transaccion::anclado::{
    archivo_temporal, contexto_admision, identidad_archivo, scope_filesystem,
};
use crate::transaccion::journal::{
    ArtifactFingerprint, JournalState, selector::JournalPersistido, v2::JournalV2,
};
use sha2::{Digest, Sha256};
use std::{ffi::OsStr, fs::File, io::Read};

#[derive(Debug)]
pub(crate) struct ErrorPublicacionIntentV3;

struct Captura {
    journal: JournalV2,
    contexto: Vec<u8>,
    id_journal: IdentidadArchivoPersistida,
    id_contexto: IdentidadArchivoPersistida,
    _journal: ObservacionJournalPersistido,
    _contexto: ObservacionContextoVersionada,
}
impl Captura {
    fn coincide(&self, other: &Self) -> bool {
        self.journal == other.journal
            && self.contexto == other.contexto
            && self.id_journal == other.id_journal
            && self.id_contexto == other.id_contexto
    }
}

impl DuenioTransaccionAnclado<'_> {
    pub(crate) fn publicar_intent_v3_desde_committed(
        &mut self,
    ) -> Result<(), ErrorPublicacionIntentV3> {
        self.publicar_intent_v3_con_hooks(|| {}, contexto_admision::renombrar_no_reemplazar)
    }

    fn publicar_intent_v3_con_hooks<H, R>(
        &mut self,
        despues_tmp: H,
        renombrar: R,
    ) -> Result<(), ErrorPublicacionIntentV3>
    where
        H: FnOnce(),
        R: FnOnce(&File, &File, &OsStr, &OsStr) -> Result<(), ()>,
    {
        self.parent_revalidado()
            .map_err(|_| ErrorPublicacionIntentV3)?;
        let result = self.publicar_intent_v3_cuerpo(despues_tmp, renombrar);
        self.parent_revalidado()
            .map_err(|_| ErrorPublicacionIntentV3)?;
        result
    }

    fn publicar_intent_v3_cuerpo<H, R>(
        &mut self,
        despues_tmp: H,
        renombrar: R,
    ) -> Result<(), ErrorPublicacionIntentV3>
    where
        H: FnOnce(),
        R: FnOnce(&File, &File, &OsStr, &OsStr) -> Result<(), ()>,
    {
        if crate::transaccion::anclado::journal_temporal::raiz_es_remota(self._lock.raiz_canonica())
        {
            return Err(ErrorPublicacionIntentV3);
        }
        let inicial = self.capturar_publicacion_v3(false, false)?;
        let basename = self
            ._lock
            .ubicacion()
            .ruta()
            .file_name()
            .ok_or(ErrorPublicacionIntentV3)?;
        let canonico = nombre_intent_retiro(basename).map_err(|_| ErrorPublicacionIntentV3)?;
        let temporal = contexto_admision::nombre_temporal_sibling(&canonico)
            .map_err(|_| ErrorPublicacionIntentV3)?;
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorPublicacionIntentV3)?;
        let vacio = archivo_temporal::crear_vacio(&parent, &temporal)
            .map_err(|_| ErrorPublicacionIntentV3)?;
        let id = IdentidadArchivoPersistida::try_from(vacio.identidad())
            .map_err(|_| ErrorPublicacionIntentV3)?;
        let metadata = IntentRetiroPersistidoV2::desde_datos_persistidos(
            inicial.journal.clone(),
            id,
            inicial.id_journal.clone(),
            inicial.id_contexto.clone(),
            self.scope_parent.0,
            inicial.contexto.len() as u64,
            Sha256::digest(&inicial.contexto).into(),
        )
        .map_err(|_| ErrorPublicacionIntentV3)?;
        let frame = IntentRetiroPersistidoV3::encode(&metadata, &inicial.contexto)
            .map_err(|_| ErrorPublicacionIntentV3)?;
        let datos = vacio
            .escribir(&frame)
            .map_err(|_| ErrorPublicacionIntentV3)?;
        despues_tmp();
        if !inicial.coincide(&self.capturar_publicacion_v3(false, true)?) {
            return Err(ErrorPublicacionIntentV3);
        }
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorPublicacionIntentV3)?;
        #[cfg(windows)]
        let fuente = super::intent_retiro::abrir_final_con_delete(&parent, &temporal)
            .map_err(|_| ErrorPublicacionIntentV3)?;
        #[cfg(not(windows))]
        let fuente = super::lector_intent_canonico::abrir_intent_canonico(&parent, &temporal)
            .map_err(|_| ErrorPublicacionIntentV3)?
            .ok_or(ErrorPublicacionIntentV3)?;
        let fuente = leer_archivo(fuente, self.scope_parent, inicial.journal.parent())
            .map_err(|_| ErrorPublicacionIntentV3)?;
        if identidad_archivo(&fuente.archivo).map_err(|_| ErrorPublicacionIntentV3)?
            != datos.identidad_archivo
            || fuente.intent.metadata() != &metadata
            || fuente.intent.contexto_original() != inicial.contexto
        {
            return Err(ErrorPublicacionIntentV3);
        }
        self.parent_revalidado()
            .map_err(|_| ErrorPublicacionIntentV3)?;
        renombrar(&parent, &fuente.archivo, &temporal, &canonico)
            .map_err(|_| ErrorPublicacionIntentV3)?;
        drop(fuente);
        crate::transaccion::durabilidad::sincronizar_directorio(
            &parent,
            crate::transaccion::durabilidad::Punto::IntentPublicado,
        )
        .map_err(|_| ErrorPublicacionIntentV3)?;
        if !inicial.coincide(&self.capturar_publicacion_v3(true, false)?) {
            return Err(ErrorPublicacionIntentV3);
        }
        let publicado = self
            .observar_intent_v3()
            .map_err(|_| ErrorPublicacionIntentV3)?;
        if publicado.intent.metadata() != &metadata
            || publicado.intent.contexto_original() != inicial.contexto
        {
            return Err(ErrorPublicacionIntentV3);
        }
        Ok(())
    }

    fn capturar_publicacion_v3(
        &mut self,
        intent: bool,
        temporal: bool,
    ) -> Result<Captura, ErrorPublicacionIntentV3> {
        let slots = self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorPublicacionIntentV3)?;
        let presence = |value| {
            if value {
                Presencia::Presente
            } else {
                Presencia::Ausente
            }
        };
        if slots.journal() != Presencia::Presente
            || slots.contexto() != Presencia::Presente
            || slots.journal_temporal() != Presencia::Ausente
            || slots.contexto_temporal() != Presencia::Ausente
            || slots.intent_retiro() != presence(intent)
            || slots.intent_retiro_temporal() != presence(temporal)
        {
            return Err(ErrorPublicacionIntentV3);
        }
        let journal = self
            .observar_journal_versionado()
            .map_err(|_| ErrorPublicacionIntentV3)?
            .ok_or(ErrorPublicacionIntentV3)?;
        let JournalPersistido::V2(registro) = journal.journal() else {
            return Err(ErrorPublicacionIntentV3);
        };
        if registro.state() != JournalState::Committed {
            return Err(ErrorPublicacionIntentV3);
        }
        let contexto = self
            .observar_contexto_versionado()
            .map_err(|_| ErrorPublicacionIntentV3)?
            .ok_or(ErrorPublicacionIntentV3)?;
        let EnvelopeAdmisionVersionada::V2(envelope) = contexto.envelope_versionado() else {
            return Err(ErrorPublicacionIntentV3);
        };
        let replay = envelope
            .calcular_new_esperado(registro)
            .map_err(|_| ErrorPublicacionIntentV3)?;
        let esperado = ArtifactFingerprint::new(
            registro.new_fingerprint().identity().clone(),
            replay.digest().clone(),
        );
        let raw = self
            .observar_siblings_recovery_v2_raw(registro)
            .map_err(|_| ErrorPublicacionIntentV3)?;
        if raw.root() != Some(&esperado) || raw.backup().is_some() || raw.stage().is_some() {
            return Err(ErrorPublicacionIntentV3);
        }
        let bytes = self.leer_contexto_publicacion_v3(&contexto)?;
        Ok(Captura {
            journal: registro.clone(),
            contexto: bytes,
            id_journal: IdentidadArchivoPersistida::try_from(
                journal.identidad().identidad_archivo(),
            )
            .map_err(|_| ErrorPublicacionIntentV3)?,
            id_contexto: IdentidadArchivoPersistida::try_from(
                contexto.identidad_archivo_observado(),
            )
            .map_err(|_| ErrorPublicacionIntentV3)?,
            _journal: journal,
            _contexto: contexto,
        })
    }

    fn leer_contexto_publicacion_v3(
        &self,
        observado: &ObservacionContextoVersionada,
    ) -> Result<Vec<u8>, ErrorPublicacionIntentV3> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorPublicacionIntentV3)?;
        let nombre = contexto_admision::nombre_canonico(
            self._lock
                .ubicacion()
                .ruta()
                .file_name()
                .ok_or(ErrorPublicacionIntentV3)?,
        )
        .map_err(|_| ErrorPublicacionIntentV3)?;
        let mut file = super::lector_intent_canonico::abrir_intent_canonico(&parent, &nombre)
            .map_err(|_| ErrorPublicacionIntentV3)?
            .ok_or(ErrorPublicacionIntentV3)?;
        let (id, len, digest) = observado.evidencia_raw_retiro();
        let verificar = |file: &File| {
            identidad_archivo(file)
                .map_err(|_| ErrorPublicacionIntentV3)
                .and_then(|actual| {
                    if actual != id
                        || scope_filesystem(file).map_err(|_| ErrorPublicacionIntentV3)?
                            != self.scope_parent
                        || file.metadata().map_err(|_| ErrorPublicacionIntentV3)?.len() != len
                    {
                        Err(ErrorPublicacionIntentV3)
                    } else {
                        Ok(())
                    }
                })
        };
        verificar(&file)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(usize::try_from(len).map_err(|_| ErrorPublicacionIntentV3)?)
            .map_err(|_| ErrorPublicacionIntentV3)?;
        (&mut file)
            .take(len.checked_add(1).ok_or(ErrorPublicacionIntentV3)?)
            .read_to_end(&mut bytes)
            .map_err(|_| ErrorPublicacionIntentV3)?;
        verificar(&file)?;
        if bytes.len() as u64 != len || <[u8; 32]>::from(Sha256::digest(&bytes)) != digest {
            return Err(ErrorPublicacionIntentV3);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
#[path = "publicar_intent_v3_tests.rs"]
mod tests;
