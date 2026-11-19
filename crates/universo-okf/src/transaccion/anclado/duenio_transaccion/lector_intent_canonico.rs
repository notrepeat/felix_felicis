//! Lector file-bound del intent canónico; observa datos sin conceder recovery.

use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use super::super::abrir::abrir_entrada_relativa;
use super::super::intent_persistido::{IntentRetiroPersistido, MAX_INTENT_BYTES};
use super::super::{
    ErrorDirectorioAnclado, IdentidadArchivo, IdentidadDirectorio, ScopeFilesystem,
    TipoEntradaAnclada, identidad_archivo, scope_filesystem, validar_componente,
};
use super::observar_siblings_nuevo_flujo::{
    PresenciaSiblingNuevoFlujo, nombre_intent_retiro, observar_sibling,
};
use super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ErrorLecturaIntent {
    Io,
    Discrepancia,
}

pub(super) struct IntentoCanonicoLeido {
    pub(super) archivo: File,
    pub(super) identidad: IdentidadArchivo,
    pub(super) scope: ScopeFilesystem,
    pub(super) registro: IntentRetiroPersistido,
}

/// Abre solo el nombre canónico y conserva ausencia exacta como `None`.
pub(super) fn abrir_intent_canonico(
    parent: &File,
    nombre: &OsStr,
) -> Result<Option<File>, ErrorLecturaIntent> {
    match abrir_entrada_relativa(parent, nombre, TipoEntradaAnclada::Archivo) {
        Ok(archivo) => Ok(Some(archivo)),
        Err(ErrorDirectorioAnclado::EntradaAusente) => Ok(None),
        Err(
            ErrorDirectorioAnclado::EnlaceNoPermitido
            | ErrorDirectorioAnclado::TipoNoAdmitido
            | ErrorDirectorioAnclado::ComponenteInvalido,
        ) => Err(ErrorLecturaIntent::Discrepancia),
        Err(_) => Err(ErrorLecturaIntent::Io),
    }
}

/// Valida un handle abierto y decodifica el único framing canónico existente.
pub(super) fn leer_intent_canonico<F>(
    mut archivo: File,
    scope_esperado: ScopeFilesystem,
    identidad_parent: IdentidadDirectorio,
    vinculo_esperado: Option<(IdentidadArchivo, &IntentRetiroPersistido)>,
    mut sondear_scope: F,
) -> Result<IntentoCanonicoLeido, ErrorLecturaIntent>
where
    F: FnMut(&File) -> Result<ScopeFilesystem, ErrorDirectorioAnclado>,
{
    let metadata = archivo.metadata().map_err(|_| ErrorLecturaIntent::Io)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || super::super::metadata_es_reparse(&metadata)
    {
        return Err(ErrorLecturaIntent::Discrepancia);
    }
    if metadata.len() > u64::try_from(MAX_INTENT_BYTES).map_err(|_| ErrorLecturaIntent::Io)? {
        return Err(ErrorLecturaIntent::Discrepancia);
    }

    let scope = sondear_scope(&archivo).map_err(|_| ErrorLecturaIntent::Io)?;
    if scope != scope_esperado {
        return Err(ErrorLecturaIntent::Discrepancia);
    }
    let identidad = identidad_archivo(&archivo).map_err(|_| ErrorLecturaIntent::Io)?;
    if vinculo_esperado.is_some_and(|(identidad_esperada, _)| identidad != identidad_esperada) {
        return Err(ErrorLecturaIntent::Discrepancia);
    }

    let limite = u64::try_from(MAX_INTENT_BYTES)
        .ok()
        .and_then(|maximo| maximo.checked_add(1))
        .ok_or(ErrorLecturaIntent::Io)?;
    let mut bytes = Vec::new();
    archivo
        .seek(SeekFrom::Start(0))
        .map_err(|_| ErrorLecturaIntent::Io)?;
    (&archivo)
        .take(limite)
        .read_to_end(&mut bytes)
        .map_err(|_| ErrorLecturaIntent::Io)?;
    if bytes.len() > MAX_INTENT_BYTES {
        return Err(ErrorLecturaIntent::Discrepancia);
    }

    let registro =
        IntentRetiroPersistido::decode(&bytes).map_err(|_| ErrorLecturaIntent::Discrepancia)?;
    registro
        .validar_vinculo_archivo_intent(identidad, scope)
        .map_err(|_| ErrorLecturaIntent::Discrepancia)?;
    let parent_journal = identidad_parent
        .journal_identity()
        .ok_or(ErrorLecturaIntent::Discrepancia)?;
    if registro.journal().parent() != &parent_journal
        || vinculo_esperado.is_some_and(|(_, registro_esperado)| &registro != registro_esperado)
    {
        return Err(ErrorLecturaIntent::Discrepancia);
    }

    Ok(IntentoCanonicoLeido {
        archivo,
        identidad,
        scope,
        registro,
    })
}

pub(super) fn mapear_error_lectura_intent(
    _error: ErrorLecturaIntent,
) -> ErrorDuenioTransaccionAnclado {
    ErrorDuenioTransaccionAnclado::NoVerificable
}

#[cfg(test)]
impl ObservacionIntentCanonico<'_, '_> {
    pub(super) fn leer_nucleo_tipado_para_test(
        &self,
    ) -> Result<IntentoCanonicoLeido, ErrorLecturaIntent> {
        let archivo = self
            ._archivo
            .try_clone()
            .map_err(|_| ErrorLecturaIntent::Io)?;
        leer_intent_canonico(
            archivo,
            self._scope_parent,
            self._identidad_parent,
            Some((self._identidad_intent, &self._registro)),
            scope_filesystem,
        )
    }
}

/// Observación file-bound que retiene el préstamo mutable del owner.
///
/// Los campos son datos de observación; no forman una prueba normal de replay.
#[must_use = "la observación retiene el owner mientras se usa"]
pub(super) struct ObservacionIntentCanonico<'owner, 'lock> {
    _owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    _autoridad: Arc<()>,
    _identidad_parent: IdentidadDirectorio,
    _scope_parent: ScopeFilesystem,
    _identidad_intent: IdentidadArchivo,
    _registro: IntentRetiroPersistido,
    _archivo: File,
}

impl fmt::Debug for ObservacionIntentCanonico<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionIntentCanonico { .. }")
    }
}

impl<'lock> DuenioTransaccionAnclado<'lock> {
    /// Lee y vincula el intent canónico desde el parent retenido por el owner.
    ///
    /// Las comprobaciones son secuenciales. No eliminan entradas ni constituyen
    /// una garantía frente a cambios concurrentes de procesos externos.
    pub(super) fn observar_intent_canonico<'owner>(
        &'owner mut self,
    ) -> Result<ObservacionIntentCanonico<'owner, 'lock>, ErrorDuenioTransaccionAnclado> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let ubicacion = self._lock.ubicacion();
        let basename = ubicacion
            .ruta()
            .file_name()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        validar_componente(basename).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

        let nombre_intent = nombre_intent_retiro(basename)?;
        let nombre_intent_temporal =
            super::super::contexto_admision::nombre_temporal_sibling(&nombre_intent)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let nombre_journal_temporal = ubicacion.nombre_journal_temporal();
        validar_componente(&nombre_journal_temporal)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let nombre_contexto = super::super::contexto_admision::nombre_canonico(basename)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let nombre_contexto_temporal =
            super::super::contexto_admision::nombre_temporal_sibling(&nombre_contexto)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

        validar_temporales_ausentes(
            &parent,
            self.scope_parent,
            [
                &nombre_intent_temporal,
                &nombre_journal_temporal,
                &nombre_contexto_temporal,
            ],
        )?;

        let archivo = abrir_entrada_relativa(&parent, &nombre_intent, TipoEntradaAnclada::Archivo)
            .map_err(mapear_error)?;
        let (identidad_intent, scope_intent) = validar_archivo_intent(&archivo, self.scope_parent)?;
        let limite = u64::try_from(MAX_INTENT_BYTES)
            .ok()
            .and_then(|maximo| maximo.checked_add(1))
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let mut bytes = Vec::with_capacity(MAX_INTENT_BYTES + 1);
        (&archivo)
            .take(limite)
            .read_to_end(&mut bytes)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        if bytes.len() > MAX_INTENT_BYTES {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        let registro = IntentRetiroPersistido::decode(&bytes)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        registro
            .validar_vinculo_archivo_intent(identidad_intent, scope_intent)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

        let parent_final = self
            .parent_revalidado()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        validar_temporales_ausentes(
            &parent_final,
            self.scope_parent,
            [
                &nombre_intent_temporal,
                &nombre_journal_temporal,
                &nombre_contexto_temporal,
            ],
        )?;
        let identidad_parent_journal = self
            .identidad_parent
            .journal_identity()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        if registro.journal().parent() != &identidad_parent_journal {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        let archivo_final =
            abrir_entrada_relativa(&parent_final, &nombre_intent, TipoEntradaAnclada::Archivo)
                .map_err(mapear_error)?;
        let (identidad_final, scope_final) =
            validar_archivo_intent(&archivo_final, self.scope_parent)?;
        if identidad_final != identidad_intent || scope_final != scope_intent {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        let autoridad = Arc::clone(&self._autoridad);
        let identidad_parent = self.identidad_parent;
        let scope_parent = self.scope_parent;
        Ok(ObservacionIntentCanonico {
            _owner: self,
            _autoridad: autoridad,
            _identidad_parent: identidad_parent,
            _scope_parent: scope_parent,
            _identidad_intent: identidad_intent,
            _registro: registro,
            _archivo: archivo_final,
        })
    }
}

impl<'owner, 'lock> ObservacionIntentCanonico<'owner, 'lock> {
    /// Consume la observación y obtiene un lease de lectura del intent actual.
    /// El lease retiene el archivo final; no autoriza replay ni retiro.
    pub(super) fn abrir_lease_retiro(
        self,
    ) -> Result<
        Option<super::intent_retiro::LeaseIntentRetiro<'owner, 'lock>>,
        ErrorDuenioTransaccionAnclado,
    > {
        let Self {
            _owner,
            _autoridad,
            _identidad_parent,
            _scope_parent,
            _identidad_intent,
            _registro,
            _archivo,
        } = self;

        super::intent_retiro::abrir_lease_retiro(
            _owner,
            _autoridad,
            _identidad_parent,
            _scope_parent,
            _identidad_intent,
            _registro,
            _archivo,
        )
    }

    /// Consume la observación y vuelve a leer el nombre bajo el mismo owner.
    /// La lectura nueva se compara por datos; el handle retenido no congela bytes.
    /// El consumidor de reconciliación se integrará en una unidad posterior.
    #[allow(dead_code)]
    pub(super) fn revalidar(self) -> Result<Self, ErrorDuenioTransaccionAnclado> {
        let Self {
            _owner: owner,
            _autoridad: autoridad_original,
            _identidad_parent: identidad_parent_original,
            _scope_parent: scope_parent_original,
            _identidad_intent: identidad_intent_original,
            _registro: registro_original,
            _archivo: archivo_original,
        } = self;

        if !Arc::ptr_eq(&autoridad_original, &owner._autoridad)
            || identidad_parent_original != owner.identidad_parent
            || scope_parent_original != owner.scope_parent
        {
            drop(archivo_original);
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        let observacion_nueva = owner.observar_intent_canonico()?;
        if observacion_nueva._identidad_intent != identidad_intent_original
            || observacion_nueva._registro != registro_original
            || observacion_nueva._identidad_parent != identidad_parent_original
            || observacion_nueva._scope_parent != scope_parent_original
            || !Arc::ptr_eq(&observacion_nueva._autoridad, &autoridad_original)
        {
            drop(archivo_original);
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        drop(archivo_original);
        Ok(observacion_nueva)
    }
}

pub(super) fn validar_temporales_ausentes(
    parent: &File,
    scope_parent: ScopeFilesystem,
    nombres: [&OsStr; 3],
) -> Result<(), ErrorDuenioTransaccionAnclado> {
    for nombre in nombres {
        if observar_sibling(parent, nombre, scope_parent)? != PresenciaSiblingNuevoFlujo::Ausente {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }
    }
    Ok(())
}

pub(super) fn validar_archivo_intent(
    archivo: &File,
    scope_esperado: ScopeFilesystem,
) -> Result<(IdentidadArchivo, ScopeFilesystem), ErrorDuenioTransaccionAnclado> {
    let metadata = archivo
        .metadata()
        .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || super::super::metadata_es_reparse(&metadata)
    {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }
    let scope = scope_filesystem(archivo).map_err(mapear_error)?;
    if scope != scope_esperado {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }
    let identidad =
        identidad_archivo(archivo).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    Ok((identidad, scope))
}

fn mapear_error(_error: ErrorDirectorioAnclado) -> ErrorDuenioTransaccionAnclado {
    ErrorDuenioTransaccionAnclado::NoVerificable
}

/// Datos de una lectura secuencial del intent, Journal, contexto y siblings.
///
/// Retiene el owner y el intent observado. No es prueba normal de replay ni
/// autoriza recovery, cleanup o aplicación.
// El consumidor que usará este dato todavía no se integra.
#[must_use = "la observación conserva datos file-bound y el owner"]
#[allow(dead_code)]
pub(super) struct ObservacionIntentConReplayYInventario<'owner, 'lock> {
    _intent: ObservacionIntentCanonico<'owner, 'lock>,
    _identidad_journal: super::super::journal_temporal::IdentidadArchivoJournal,
    _journal: crate::transaccion::journal::Journal,
    _evidencia_contexto: crate::transaccion::anclado::contexto_admision::EvidenciaLecturaContexto,
    _huella_replay: crate::transaccion::journal::ArtifactFingerprint,
    _inventario: crate::transaccion::journal::recovery::RecoveryInventory,
}

impl fmt::Debug for ObservacionIntentConReplayYInventario<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionIntentConReplayYInventario { .. }")
    }
}

impl<'owner, 'lock> ObservacionIntentCanonico<'owner, 'lock> {
    /// Revalida el intent y observa la fila A sin emitir prueba normal.
    ///
    /// Cada lectura está ligada al parent retenido y al lock cooperativo. La
    /// secuencia no es un snapshot atómico frente a procesos externos.
    // El consumidor que usará este dato todavía no se integra.
    #[allow(dead_code)]
    pub(super) fn observar_reconciliacion_a(
        self,
    ) -> Result<ObservacionIntentConReplayYInventario<'owner, 'lock>, ErrorDuenioTransaccionAnclado>
    {
        let observacion_inicial = self.revalidar()?;
        let (identidad_journal, journal, evidencia_contexto, huella_replay, inventario) = {
            let registro = &observacion_inicial._registro;
            let owner = &mut *observacion_inicial._owner;
            validar_gate_reconciliacion_a(owner)?;

            let (identidad_journal, journal) = {
                let lease = owner
                    .journal_temporal()
                    .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
                lease
                    .cargar_canonico()
                    .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?
                    .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?
            };
            registro
                .validar_vinculo_journal(identidad_journal.identidad_archivo(), &journal)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

            let (envelope, evidencia_contexto) = {
                let lease = owner
                    .contexto_admision()
                    .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
                lease
                    .leer_canonico_con_evidencia()
                    .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?
                    .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?
            };
            registro
                .validar_vinculo_contexto(
                    evidencia_contexto.identidad_archivo,
                    evidencia_contexto.longitud,
                    evidencia_contexto.sha256,
                )
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

            let replay = envelope
                .validar_replay_soportado(&journal)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
            if replay.new_fingerprint() != journal.new_fingerprint() {
                return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
            }
            let huella_replay = replay.new_fingerprint().clone();

            let inventario = owner
                .observar_inventario_recovery_con_journal_datos(&journal)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
            use crate::transaccion::journal::recovery::ArtifactState;
            // New exige igualdad de identidad nativa y digest raw completos.
            // El inventario observado no convierte root_new_validated en true.
            if inventario.root != ArtifactState::New
                || inventario.backup != ArtifactState::Absent
                || inventario.stage != ArtifactState::Absent
                || inventario.root_new_validated
            {
                return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
            }
            validar_gate_reconciliacion_a(owner)?;

            (
                identidad_journal,
                journal,
                evidencia_contexto,
                huella_replay,
                inventario,
            )
        };

        let intent_final = observacion_inicial.revalidar()?;
        validar_gate_reconciliacion_a(&mut *intent_final._owner)?;

        Ok(ObservacionIntentConReplayYInventario {
            _intent: intent_final,
            _identidad_journal: identidad_journal,
            _journal: journal,
            _evidencia_contexto: evidencia_contexto,
            _huella_replay: huella_replay,
            _inventario: inventario,
        })
    }
}

fn validar_gate_reconciliacion_a(
    owner: &mut DuenioTransaccionAnclado<'_>,
) -> Result<(), ErrorDuenioTransaccionAnclado> {
    let siblings = owner.observar_siblings_nuevo_flujo()?;
    use PresenciaSiblingNuevoFlujo::{Ausente, Presente};
    if siblings.journal() != Presente
        || siblings.contexto() != Presente
        || siblings.intent_retiro() != Presente
        || siblings.journal_temporal() != Ausente
        || siblings.contexto_temporal() != Ausente
        || siblings.intent_retiro_temporal() != Ausente
    {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }
    Ok(())
}

/// Observación de replay para intent presente, Journal canónico ausente y
/// Contexto canónico presente. El Journal retenido solo es dato del intent.
#[must_use = "la observación conserva el intent file-bound y el owner"]
// El consumidor futuro de esta observación aún no forma parte del corte B.
#[allow(dead_code)]
pub(super) struct ObservacionIntentConReplayYInventarioSinJournal<'owner, 'lock> {
    _intent: ObservacionIntentCanonico<'owner, 'lock>,
    _journal: crate::transaccion::journal::Journal,
    _evidencia_contexto: crate::transaccion::anclado::contexto_admision::EvidenciaLecturaContexto,
    _huella_replay: crate::transaccion::journal::ArtifactFingerprint,
    _inventario: crate::transaccion::journal::recovery::RecoveryInventory,
}

impl fmt::Debug for ObservacionIntentConReplayYInventarioSinJournal<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionIntentConReplayYInventarioSinJournal { .. }")
    }
}

impl<'owner, 'lock> ObservacionIntentCanonico<'owner, 'lock> {
    /// Observa replay con el Journal Committed guardado en el intent como dato.
    /// No infiere por qué falta el Journal canónico ni entrega autoridad.
    // El consumidor de reconciliación no se integra en esta unidad.
    #[allow(dead_code)]
    pub(super) fn observar_reconciliacion_b_sin_journal(
        self,
    ) -> Result<
        ObservacionIntentConReplayYInventarioSinJournal<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    > {
        let observacion_inicial = self.revalidar()?;
        let (journal, evidencia_contexto, huella_replay, inventario) = {
            let registro = &observacion_inicial._registro;
            let owner = &mut *observacion_inicial._owner;
            validar_gate_reconciliacion_b(owner)?;

            let journal = registro.journal().clone();
            let parent_esperado = owner
                .identidad_parent
                .journal_identity()
                .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
            if journal.state() != crate::transaccion::journal::JournalState::Committed
                || journal.parent() != &parent_esperado
            {
                return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
            }

            let (envelope, evidencia_contexto) = {
                let lease = owner
                    .contexto_admision()
                    .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
                lease
                    .leer_canonico_con_evidencia()
                    .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?
                    .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?
            };
            registro
                .validar_vinculo_contexto(
                    evidencia_contexto.identidad_archivo,
                    evidencia_contexto.longitud,
                    evidencia_contexto.sha256,
                )
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

            let replay = envelope
                .validar_replay_soportado(&journal)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
            if replay.new_fingerprint() != journal.new_fingerprint() {
                return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
            }
            let huella_replay = replay.new_fingerprint().clone();

            let inventario = owner
                .observar_inventario_recovery_con_journal_datos(&journal)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
            use crate::transaccion::journal::recovery::ArtifactState;
            if inventario.root != ArtifactState::New
                || inventario.backup != ArtifactState::Absent
                || inventario.stage != ArtifactState::Absent
                || inventario.root_new_validated
            {
                return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
            }
            validar_gate_reconciliacion_b(owner)?;

            (journal, evidencia_contexto, huella_replay, inventario)
        };

        let intent_final = observacion_inicial.revalidar()?;
        validar_gate_reconciliacion_b(&mut *intent_final._owner)?;

        Ok(ObservacionIntentConReplayYInventarioSinJournal {
            _intent: intent_final,
            _journal: journal,
            _evidencia_contexto: evidencia_contexto,
            _huella_replay: huella_replay,
            _inventario: inventario,
        })
    }
}

fn validar_gate_reconciliacion_b(
    owner: &mut DuenioTransaccionAnclado<'_>,
) -> Result<(), ErrorDuenioTransaccionAnclado> {
    let siblings = owner.observar_siblings_nuevo_flujo()?;
    use PresenciaSiblingNuevoFlujo::{Ausente, Presente};
    if siblings.journal() != Ausente
        || siblings.contexto() != Presente
        || siblings.intent_retiro() != Presente
        || siblings.journal_temporal() != Ausente
        || siblings.contexto_temporal() != Ausente
        || siblings.intent_retiro_temporal() != Ausente
    {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }
    Ok(())
}

/// Observación de intent presente con Journal y Context canónicos ausentes.
/// Conserva solo el intent; no infiere la causa de las ausencias.
#[must_use = "la observación conserva el intent file-bound y el owner"]
#[allow(dead_code)]
pub(super) struct ObservacionIntentConJournalYContextoAusentes<'owner, 'lock> {
    _intent: ObservacionIntentCanonico<'owner, 'lock>,
}

impl fmt::Debug for ObservacionIntentConJournalYContextoAusentes<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionIntentConJournalYContextoAusentes { .. }")
    }
}

impl<'owner, 'lock> ObservacionIntentCanonico<'owner, 'lock> {
    /// Revalida intent y ausencia de ambos sidecars sin observar su contenido.
    /// El dato no autoriza recovery, cleanup ni aplicación.
    #[allow(dead_code)]
    pub(super) fn observar_reconciliacion_c_sin_sidecars(
        self,
    ) -> Result<
        ObservacionIntentConJournalYContextoAusentes<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    > {
        let intent_inicial = self.revalidar()?;
        validar_gate_reconciliacion_c(&mut *intent_inicial._owner)?;

        let intent_final = intent_inicial.revalidar()?;
        validar_gate_reconciliacion_c(&mut *intent_final._owner)?;

        Ok(ObservacionIntentConJournalYContextoAusentes {
            _intent: intent_final,
        })
    }
}

/// Observación puntual de C que coteja root con el Journal almacenado como dato.
/// No incluye replay, admisión, decisión de recovery ni autoridad de limpieza.
/// Las comprobaciones son secuenciales y no forman un snapshot externo.
#[must_use = "la observación conserva el intent file-bound y el owner"]
#[allow(dead_code)]
pub(super) struct ObservacionIntentConsistenciaRootC<'owner, 'lock> {
    _intent: ObservacionIntentCanonico<'owner, 'lock>,
    _journal: crate::transaccion::journal::Journal,
    _inventario: crate::transaccion::journal::recovery::RecoveryInventory,
}

impl fmt::Debug for ObservacionIntentConsistenciaRootC<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionIntentConsistenciaRootC { .. }")
    }
}

impl<'owner, 'lock> ObservacionIntentConJournalYContextoAusentes<'owner, 'lock> {
    /// Compara el inventario puntual con el descriptor retenido en el intent.
    /// La igualdad de root usa identidad nativa y digest raw completos.
    /// No atribuye la ausencia de sidecars a una operación autorizada.
    #[allow(dead_code)]
    pub(super) fn observar_consistencia_root_c(
        self,
    ) -> Result<ObservacionIntentConsistenciaRootC<'owner, 'lock>, ErrorDuenioTransaccionAnclado>
    {
        let intent_inicial = self._intent.revalidar()?;
        let (journal, inventario) = {
            let registro = &intent_inicial._registro;
            let owner = &mut *intent_inicial._owner;
            validar_gate_reconciliacion_c(owner)?;

            let journal = registro.journal().clone();
            let parent_esperado = owner
                .identidad_parent
                .journal_identity()
                .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
            if journal.state() != crate::transaccion::journal::JournalState::Committed
                || journal.parent() != &parent_esperado
            {
                return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
            }

            let inventario = owner
                .observar_inventario_recovery_con_journal_datos(&journal)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
            use crate::transaccion::journal::recovery::ArtifactState;
            // New requiere identidad nativa y digest raw completos.
            if inventario.root != ArtifactState::New
                || inventario.backup != ArtifactState::Absent
                || inventario.stage != ArtifactState::Absent
                || inventario.root_new_validated
            {
                return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
            }
            validar_gate_reconciliacion_c(owner)?;

            (journal, inventario)
        };

        let intent_final = intent_inicial.revalidar()?;
        validar_gate_reconciliacion_c(&mut *intent_final._owner)?;

        Ok(ObservacionIntentConsistenciaRootC {
            _intent: intent_final,
            _journal: journal,
            _inventario: inventario,
        })
    }
}

fn validar_gate_reconciliacion_c(
    owner: &mut DuenioTransaccionAnclado<'_>,
) -> Result<(), ErrorDuenioTransaccionAnclado> {
    let siblings = owner.observar_siblings_nuevo_flujo()?;
    use PresenciaSiblingNuevoFlujo::Ausente;
    if siblings.journal() != Ausente
        || siblings.contexto() != Ausente
        || siblings.intent_retiro() != PresenciaSiblingNuevoFlujo::Presente
        || siblings.journal_temporal() != Ausente
        || siblings.contexto_temporal() != Ausente
        || siblings.intent_retiro_temporal() != Ausente
    {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }
    Ok(())
}

impl<'owner, 'lock> ObservacionIntentConsistenciaRootC<'owner, 'lock> {
    /// Retira el intent solo tras reconstruir C desde una observación fresca.
    /// El resultado owner-bound es intermedio y no asegura un estado final.
    #[allow(dead_code)]
    pub(super) fn retirar_intent_tras_consistencia_root_c(
        self,
    ) -> Result<
        super::intent_retiro::ResultadoRetiroIntent<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    > {
        self.retirar_intent_tras_consistencia_root_c_con(|| {})
    }

    #[cfg(test)]
    pub(super) fn retirar_intent_tras_consistencia_root_c_con_prueba<F>(
        self,
        despues_de_c_fresca: F,
    ) -> Result<
        super::intent_retiro::ResultadoRetiroIntent<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    >
    where
        F: FnOnce(),
    {
        self.retirar_intent_tras_consistencia_root_c_con(despues_de_c_fresca)
    }

    fn retirar_intent_tras_consistencia_root_c_con<F>(
        self,
        despues_de_c_fresca: F,
    ) -> Result<
        super::intent_retiro::ResultadoRetiroIntent<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    >
    where
        F: FnOnce(),
    {
        let ObservacionIntentConsistenciaRootC {
            _intent: intent_inicial,
            ..
        } = self;
        let c_fresca = intent_inicial
            .observar_reconciliacion_c_sin_sidecars()?
            .observar_consistencia_root_c()?;

        despues_de_c_fresca();

        let lease = c_fresca
            ._intent
            .abrir_lease_retiro()?
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let resultado = lease.retirar();
        match resultado.estado() {
            super::intent_retiro::EstadoRetiroIntent::Eliminado
            | super::intent_retiro::EstadoRetiroIntent::Ausente => Ok(resultado),
            super::intent_retiro::EstadoRetiroIntent::Conflicto
            | super::intent_retiro::EstadoRetiroIntent::Indeterminado => {
                Err(ErrorDuenioTransaccionAnclado::NoVerificable)
            }
        }
    }
}

impl<'owner, 'lock> ObservacionIntentConReplayYInventario<'owner, 'lock> {
    /// Retira el Journal observado y solo devuelve B si una lectura nueva la valida.
    ///
    /// El resultado del borrado decide si se intenta observar B; no es un recibo
    /// ni concede autoridad para cleanup o apply.
    // El caller productivo se integra en una unidad posterior.
    #[allow(dead_code)]
    pub(super) fn retirar_journal_y_observar_b_sin_journal(
        self,
    ) -> Result<
        ObservacionIntentConReplayYInventarioSinJournal<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    > {
        self.retirar_journal_y_observar_b_sin_journal_con(|| {})
    }

    #[cfg(test)]
    pub(super) fn retirar_journal_y_observar_b_sin_journal_con_prueba<F>(
        self,
        antes_de_retirar: F,
    ) -> Result<
        ObservacionIntentConReplayYInventarioSinJournal<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    >
    where
        F: FnOnce(),
    {
        self.retirar_journal_y_observar_b_sin_journal_con(antes_de_retirar)
    }

    fn retirar_journal_y_observar_b_sin_journal_con<F>(
        self,
        antes_de_retirar: F,
    ) -> Result<
        ObservacionIntentConReplayYInventarioSinJournal<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    >
    where
        F: FnOnce(),
    {
        let ObservacionIntentConReplayYInventario {
            _intent: intent_inicial,
            ..
        } = self;
        let observacion_a_fresca = intent_inicial.observar_reconciliacion_a()?;

        antes_de_retirar();

        let resultado = {
            let owner = &mut *observacion_a_fresca._intent._owner;
            let lease = owner
                .journal_temporal_estricto()
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
            lease.eliminar_canonico_si_coincide(
                observacion_a_fresca._identidad_journal,
                &observacion_a_fresca._journal,
            )
        };

        match resultado {
            super::super::journal_temporal::ResultadoEliminacionJournal::Eliminado
            | super::super::journal_temporal::ResultadoEliminacionJournal::Ausente => {}
            super::super::journal_temporal::ResultadoEliminacionJournal::Conflicto(_)
            | super::super::journal_temporal::ResultadoEliminacionJournal::Indeterminado(_) => {
                return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
            }
        }

        observacion_a_fresca
            ._intent
            .observar_reconciliacion_b_sin_journal()
    }
}

impl<'owner, 'lock> ObservacionIntentConReplayYInventarioSinJournal<'owner, 'lock> {
    /// Retira Context tras una observación B nueva y solo devuelve C fresco.
    /// El intento no demuestra causalidad ni autoriza limpiar el intent.
    #[allow(dead_code)]
    pub(super) fn retirar_contexto_y_observar_c_sin_sidecars(
        self,
    ) -> Result<
        ObservacionIntentConJournalYContextoAusentes<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    > {
        self.retirar_contexto_y_observar_c_sin_sidecars_con(|| {})
    }

    #[cfg(test)]
    pub(super) fn retirar_contexto_y_observar_c_sin_sidecars_con_prueba<F>(
        self,
        despues_de_b_fresca: F,
    ) -> Result<
        ObservacionIntentConJournalYContextoAusentes<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    >
    where
        F: FnOnce(),
    {
        self.retirar_contexto_y_observar_c_sin_sidecars_con(despues_de_b_fresca)
    }

    fn retirar_contexto_y_observar_c_sin_sidecars_con<F>(
        self,
        despues_de_b_fresca: F,
    ) -> Result<
        ObservacionIntentConJournalYContextoAusentes<'owner, 'lock>,
        ErrorDuenioTransaccionAnclado,
    >
    where
        F: FnOnce(),
    {
        let ObservacionIntentConReplayYInventarioSinJournal {
            _intent: intent_inicial,
            ..
        } = self;
        let b_fresca = intent_inicial.observar_reconciliacion_b_sin_journal()?;

        despues_de_b_fresca();

        let ObservacionIntentConReplayYInventarioSinJournal {
            _intent: intent_fresca,
            _evidencia_contexto: evidencia_contexto_fresca,
            ..
        } = b_fresca;
        let resultado = {
            let owner = &mut *intent_fresca._owner;
            let lease = owner
                .contexto_admision()
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
            lease.eliminar_canonico(&evidencia_contexto_fresca)
        };

        match resultado {
            super::super::contexto_admision::ResultadoEliminacionContextoAnclado::Eliminado
            | super::super::contexto_admision::ResultadoEliminacionContextoAnclado::Ausente => {}
            super::super::contexto_admision::ResultadoEliminacionContextoAnclado::Conflicto(_)
            | super::super::contexto_admision::ResultadoEliminacionContextoAnclado::Indeterminado(
                _,
            ) => return Err(ErrorDuenioTransaccionAnclado::NoVerificable),
        }

        intent_fresca.observar_reconciliacion_c_sin_sidecars()
    }
}
