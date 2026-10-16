//! Prueba fresca y estricta para comenzar el retiro experimental.

use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use sha2::{Digest, Sha256};

use crate::transaccion::anclado::duenio_transaccion::observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo;
use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::recovery::ArtifactState;

use super::{
    ErrorPruebaReplayRootCoincidente, IntentTemporalEscrito, IntentTemporalVerificado,
    PruebaReplayAdmitidoRootCoincidente,
};

/// Retiene la prueba validada y su mismo owner/lock; no autoriza borrados.
pub(super) struct PruebaInicioRetiroContexto<'owner, 'lock> {
    _prueba: PruebaReplayAdmitidoRootCoincidente<'owner, 'lock>,
}

impl fmt::Debug for PruebaInicioRetiroContexto<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PruebaInicioRetiroContexto { .. }")
    }
}

impl<'owner, 'lock> PruebaReplayAdmitidoRootCoincidente<'owner, 'lock> {
    /// Revalida la prueba y requiere la fila Committed con root New, sin
    /// backup ni stage. No crea archivos ni devuelve un permiso de cleanup.
    pub(super) fn preparar_retiro_contexto(
        self,
    ) -> Result<PruebaInicioRetiroContexto<'owner, 'lock>, ErrorPruebaReplayRootCoincidente> {
        let prueba = self.revalidar()?;
        if prueba.journal.state() != JournalState::Committed {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let (journal_observado, inventario) = prueba
            ._owner
            .observar_inventario_recovery_anclado()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        if journal_observado != prueba.journal
            || inventario.root_new_validated
            || inventario.root != ArtifactState::New
            || inventario.backup != ArtifactState::Absent
            || inventario.stage != ArtifactState::Absent
        {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        Ok(PruebaInicioRetiroContexto { _prueba: prueba })
    }
}

impl<'owner, 'lock> PruebaInicioRetiroContexto<'owner, 'lock> {
    /// Crea y sincroniza el temporal exacto. Cualquier fallo posterior a
    /// `create_new` conserva el archivo y no implica que la operación no tuvo efecto.
    pub(super) fn escribir_intent_temporal(
        self,
    ) -> Result<IntentTemporalEscrito<'owner, 'lock>, ErrorPruebaReplayRootCoincidente> {
        if super::super::super::journal_temporal::raiz_es_remota(
            self._prueba._owner._lock.raiz_canonica(),
        ) {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let prueba = self._prueba.preparar_retiro_contexto()?._prueba;
        let nombre_temporal = {
            let ubicacion = prueba._owner._lock.ubicacion();
            let basename = ubicacion
                .ruta()
                .file_name()
                .ok_or(ErrorPruebaReplayRootCoincidente)?;
            let canonico =
                super::super::observar_siblings_nuevo_flujo::nombre_intent_retiro(basename)
                    .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
            super::super::super::contexto_admision::nombre_temporal_sibling(&canonico)
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?
        };

        let parent = prueba
            ._owner
            .parent_revalidado()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        let vacio = super::super::super::archivo_temporal::crear_vacio(&parent, &nombre_temporal)
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        let identidad_intent = vacio.identidad();
        let evidencia = &prueba.evidencia_contexto;
        let frame_esperado = super::super::super::intent_persistido::codificar_nuevo(
            &prueba.journal,
            identidad_intent,
            prueba.identidad_archivo_journal.identidad_archivo(),
            evidencia.identidad_archivo,
            prueba.scope_parent.0,
            evidencia.longitud,
            evidencia.sha256,
        )
        .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        let datos = vacio
            .escribir(&frame_esperado)
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        drop(parent);

        Ok(IntentTemporalEscrito {
            _inicio: PruebaInicioRetiroContexto { _prueba: prueba },
            datos,
            frame_esperado,
        })
    }
}

/// Revalidación puntual completa mientras el intent temporal está presente.
/// Retiene el mismo proof y owner, pero no habilita publicación ni cleanup.
pub(super) struct ObservacionIntentTemporalCompleta<'owner, 'lock> {
    _verificado: IntentTemporalVerificado<'owner, 'lock>,
}

impl fmt::Debug for ObservacionIntentTemporalCompleta<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionIntentTemporalCompleta { .. }")
    }
}

impl<'owner, 'lock> IntentTemporalVerificado<'owner, 'lock> {
    /// Cierra el handle anterior y vuelve a comprobar todos los datos y nombres
    /// requeridos para una observación Committed con intent temporal exacto.
    /// El resultado sigue siendo puntual y no autoriza una mutación.
    pub(super) fn revalidar_completo(
        self,
    ) -> Result<ObservacionIntentTemporalCompleta<'owner, 'lock>, ErrorPruebaReplayRootCoincidente>
    {
        let IntentTemporalVerificado {
            mut _escrito,
            _archivo,
        } = self;
        drop(_archivo);

        {
            let prueba = &mut _escrito._inicio._prueba;
            prueba.comprobar_vinculo_owner()?;
            let siblings = prueba
                ._owner
                .observar_siblings_nuevo_flujo()
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
            if siblings.journal() != PresenciaSiblingNuevoFlujo::Presente
                || siblings.journal_temporal() != PresenciaSiblingNuevoFlujo::Ausente
                || siblings.contexto() != PresenciaSiblingNuevoFlujo::Presente
                || siblings.contexto_temporal() != PresenciaSiblingNuevoFlujo::Ausente
                || siblings.intent_retiro() != PresenciaSiblingNuevoFlujo::Ausente
                || siblings.intent_retiro_temporal() != PresenciaSiblingNuevoFlujo::Presente
            {
                return Err(ErrorPruebaReplayRootCoincidente);
            }
        }

        let mut verificado = _escrito.verificar_temporal()?;
        {
            let prueba = &mut verificado._escrito._inicio._prueba;
            prueba.comprobar_artefactos()?;
            if prueba.journal.state() != JournalState::Committed {
                return Err(ErrorPruebaReplayRootCoincidente);
            }

            let (journal_observado, inventario) = prueba
                ._owner
                .observar_inventario_recovery_anclado()
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
            if journal_observado != prueba.journal
                || inventario.root_new_validated
                || inventario.root != ArtifactState::New
                || inventario.backup != ArtifactState::Absent
                || inventario.stage != ArtifactState::Absent
            {
                return Err(ErrorPruebaReplayRootCoincidente);
            }
        }

        Ok(ObservacionIntentTemporalCompleta {
            _verificado: verificado,
        })
    }
}

impl<'owner, 'lock> IntentTemporalEscrito<'owner, 'lock> {
    /// Reabre el nombre temporal exacto desde el parent retenido y contrasta
    /// el mismo handle con la identidad y el frame que escribió el creador.
    /// No repite el replay ni entrega autoridad de publicación o cleanup.
    pub(super) fn verificar_temporal(
        self,
    ) -> Result<IntentTemporalVerificado<'owner, 'lock>, ErrorPruebaReplayRootCoincidente> {
        let prueba = &self._inicio._prueba;
        let parent = prueba
            ._owner
            .parent_revalidado()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        let nombre_temporal = {
            let ubicacion = prueba._owner._lock.ubicacion();
            let basename = ubicacion
                .ruta()
                .file_name()
                .ok_or(ErrorPruebaReplayRootCoincidente)?;
            let canonico =
                super::super::observar_siblings_nuevo_flujo::nombre_intent_retiro(basename)
                    .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
            super::super::super::contexto_admision::nombre_temporal_sibling(&canonico)
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?
        };
        let archivo = super::super::super::abrir::abrir_entrada_relativa(
            &parent,
            &nombre_temporal,
            super::super::super::TipoEntradaAnclada::Archivo,
        )
        .map_err(|_| ErrorPruebaReplayRootCoincidente)?;

        let archivo = self.verificar_handle_temporal(archivo)?;
        Ok(IntentTemporalVerificado {
            _escrito: self,
            _archivo: archivo,
        })
    }

    /// Devuelve el mismo `File` validado. Un futuro syscall Windows debe usar
    /// ese handle con los derechos adecuados; Linux renombra por nombre y el
    /// caller debe comprobar el binding justo antes, con una carrera externa residual.
    fn verificar_handle_temporal(
        &self,
        mut archivo: File,
    ) -> Result<File, ErrorPruebaReplayRootCoincidente> {
        let prueba = &self._inicio._prueba;

        let metadata = archivo
            .metadata()
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        if metadata.file_type().is_symlink()
            || super::super::super::metadata_es_reparse(&metadata)
            || !metadata.is_file()
            || super::super::super::scope_filesystem(&archivo)
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?
                != prueba.scope_parent
        {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let longitud_esperada = self.frame_esperado.len();
        if longitud_esperada == 0
            || longitud_esperada > super::super::super::intent_persistido::MAX_INTENT_BYTES
            || longitud_esperada != self.datos.longitud
            || metadata.len()
                != u64::try_from(longitud_esperada).map_err(|_| ErrorPruebaReplayRootCoincidente)?
            || super::super::super::identidad_archivo(&archivo)
                .map_err(|_| ErrorPruebaReplayRootCoincidente)?
                != self.datos.identidad_archivo
        {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let digest_esperado: [u8; 32] = Sha256::digest(&self.frame_esperado).into();
        if digest_esperado != self.datos.digest {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        let limite = u64::try_from(longitud_esperada)
            .ok()
            .and_then(|longitud| longitud.checked_add(1))
            .ok_or(ErrorPruebaReplayRootCoincidente)?;
        loop {
            match archivo.seek(SeekFrom::Start(0)) {
                Ok(_) => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(ErrorPruebaReplayRootCoincidente),
            }
        }
        let mut bytes = Vec::with_capacity(longitud_esperada);
        archivo
            .by_ref()
            .take(limite)
            .read_to_end(&mut bytes)
            .map_err(|_| ErrorPruebaReplayRootCoincidente)?;
        let digest_observado: [u8; 32] = Sha256::digest(&bytes).into();
        if bytes.len() != longitud_esperada
            || bytes != self.frame_esperado
            || digest_observado != self.datos.digest
        {
            return Err(ErrorPruebaReplayRootCoincidente);
        }

        Ok(archivo)
    }
}
