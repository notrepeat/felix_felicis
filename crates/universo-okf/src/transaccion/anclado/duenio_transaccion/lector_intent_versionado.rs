//! Recolecta un frame RETIRE acotado desde el mismo handle ya abierto.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
use crate::transaccion::anclado::intent_persistido::{
    self, ErrorSelectorIntentRetiro, IntentRetiroPersistidoVersionado,
};
use crate::transaccion::journal::DirectoryIdentity;

use super::super::{
    IdentidadArchivo, IdentidadDirectorio, ScopeFilesystem, identidad_archivo, metadata_es_reparse,
    scope_filesystem,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorLecturaFrameIntent {
    EntryRejected,
    ScopeUnverified,
    ScopeMismatch,
    FrameTooLarge,
    LengthOverflow,
    ReservationFailed,
    ReadIo,
}

impl ErrorLecturaFrameIntent {
    pub(super) const fn codigo(self) -> &'static str {
        match self {
            Self::EntryRejected => "retirement_intent_entry_rejected",
            Self::ScopeUnverified => "retirement_intent_scope_unverified",
            Self::ScopeMismatch => "retirement_intent_scope_mismatch",
            Self::FrameTooLarge => "retirement_intent_frame_too_large",
            Self::LengthOverflow => "retirement_intent_frame_length_overflow",
            Self::ReservationFailed => "retirement_intent_frame_reservation_failed",
            Self::ReadIo => "retirement_intent_read_io",
        }
    }
}

impl fmt::Debug for ErrorLecturaFrameIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorLecturaFrameIntent")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorLecturaFrameIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorLecturaFrameIntent {}

#[cfg(test)]
#[path = "../duenio_transaccion_tests/lector_intent_versionado_tests.rs"]
mod tests;

/// Bytes y observaciones del handle; no incluye owner, ruta ni capacidad.
pub(super) struct FrameIntentLeido {
    pub(super) archivo_original: File,
    pub(super) bytes: Vec<u8>,
    pub(super) identidad_archivo: IdentidadArchivo,
    pub(super) scope: ScopeFilesystem,
}

impl fmt::Debug for FrameIntentLeido {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FrameIntentLeido { .. }")
    }
}

/// Candidato decodificado antes de comparar vínculos persistidos y nativos.
pub(super) struct CandidatoIntentDecodificadoSinVinculo {
    envelope: IntentRetiroPersistidoVersionado,
    archivo_original: File,
    identidad_archivo: IdentidadArchivo,
    scope: ScopeFilesystem,
}

/// Datos cuyo ID, scope persistido y parent del Journal coincidieron.
pub(super) struct CandidatoIntentConVinculosCoincidentes {
    envelope: IntentRetiroPersistidoVersionado,
    archivo_original: File,
    identidad_archivo: IdentidadArchivo,
    scope: ScopeFilesystem,
    parent_journal: DirectoryIdentity,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorIntentVersionadoVinculado {
    Candidato(ErrorCandidatoIntentSinVinculo),
    SourceIdentityUnverified,
    SourceBindingMismatch,
    ParentBindingMismatch,
}

impl ErrorIntentVersionadoVinculado {
    pub(super) const fn codigo(self) -> &'static str {
        match self {
            Self::Candidato(error) => error.codigo(),
            Self::SourceIdentityUnverified => "retirement_intent_identity_unverified",
            Self::SourceBindingMismatch => "retirement_intent_source_binding_mismatch",
            Self::ParentBindingMismatch => "retirement_intent_parent_binding_mismatch",
        }
    }
}

impl fmt::Debug for ErrorIntentVersionadoVinculado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl fmt::Display for ErrorIntentVersionadoVinculado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorIntentVersionadoVinculado {}

/// Compares a fresh relative probe with the native data retained at read time.
fn comparar_sonda_visible_intent(
    candidato: &CandidatoIntentConVinculosCoincidentes,
    sonda: &File,
) -> Result<(), ErrorIntentVersionadoVinculado> {
    let metadata = sonda
        .metadata()
        .map_err(|_| ErrorIntentVersionadoVinculado::SourceIdentityUnverified)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata_es_reparse(&metadata) {
        return Err(ErrorIntentVersionadoVinculado::SourceIdentityUnverified);
    }

    let scope = scope_filesystem(sonda)
        .map_err(|_| ErrorIntentVersionadoVinculado::SourceIdentityUnverified)?;
    let identidad = identidad_archivo(sonda)
        .map_err(|_| ErrorIntentVersionadoVinculado::SourceIdentityUnverified)?;
    if scope != candidato.scope || identidad != candidato.identidad_archivo {
        return Err(ErrorIntentVersionadoVinculado::SourceBindingMismatch);
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorIntentCanonicoVersionado {
    CanonicalUnverified,
    Candidate(ErrorIntentVersionadoVinculado),
}

impl ErrorIntentCanonicoVersionado {
    pub(super) const fn codigo(self) -> &'static str {
        match self {
            Self::CanonicalUnverified => "retirement_intent_canonical_unverified",
            Self::Candidate(error) => error.codigo(),
        }
    }
}

impl fmt::Debug for ErrorIntentCanonicoVersionado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl fmt::Display for ErrorIntentCanonicoVersionado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorIntentCanonicoVersionado {}

/// Opens, binds, and probes one present canonical Intent without an owner.
pub(super) fn leer_intent_canonico_versionado(
    parent: &File,
    nombre: &OsStr,
    scope_esperado: ScopeFilesystem,
    identidad_parent: IdentidadDirectorio,
) -> Result<CandidatoIntentConVinculosCoincidentes, ErrorIntentCanonicoVersionado> {
    let archivo = super::lector_intent_canonico::abrir_intent_canonico(parent, nombre)
        .map_err(|_| ErrorIntentCanonicoVersionado::CanonicalUnverified)?
        .ok_or(ErrorIntentCanonicoVersionado::CanonicalUnverified)?;

    leer_intent_abierto_versionado(parent, nombre, archivo, scope_esperado, identidad_parent)
}

fn leer_intent_abierto_versionado(
    parent: &File,
    nombre: &OsStr,
    archivo: File,
    scope_esperado: ScopeFilesystem,
    identidad_parent: IdentidadDirectorio,
) -> Result<CandidatoIntentConVinculosCoincidentes, ErrorIntentCanonicoVersionado> {
    let candidato = leer_intent_versionado_vinculado(archivo, scope_esperado, identidad_parent)
        .map_err(ErrorIntentCanonicoVersionado::Candidate)?;

    let probe = super::lector_intent_canonico::abrir_intent_canonico(parent, nombre)
        .map_err(|_| ErrorIntentCanonicoVersionado::CanonicalUnverified)?
        .ok_or(ErrorIntentCanonicoVersionado::CanonicalUnverified)?;
    comparar_sonda_visible_intent(&candidato, &probe)
        .map_err(|_| ErrorIntentCanonicoVersionado::CanonicalUnverified)?;
    drop(probe);

    Ok(candidato)
}

/// Owner-bound data result; the retained file is private and is not a snapshot.
pub(super) struct ObservacionIntentVersionada {
    candidato: CandidatoIntentConVinculosCoincidentes,
}

impl ObservacionIntentVersionada {
    /// Presta el registro y su identidad nativa a la clasificación de Retiro A.
    pub(super) fn registro_e_identidad_observados(
        &self,
    ) -> (&IntentRetiroPersistidoVersionado, IdentidadArchivo) {
        (&self.candidato.envelope, self.candidato.identidad_archivo)
    }
}

impl fmt::Debug for ObservacionIntentVersionada {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionIntentVersionada { .. }")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorObservacionIntentVersionada {
    OwnerUnverified,
    CanonicalUnverified,
    Candidate(ErrorIntentVersionadoVinculado),
}

impl ErrorObservacionIntentVersionada {
    pub(super) const fn codigo(self) -> &'static str {
        match self {
            Self::OwnerUnverified => "retirement_intent_owner_unverified",
            Self::CanonicalUnverified => "retirement_intent_canonical_unverified",
            Self::Candidate(error) => error.codigo(),
        }
    }
}

impl fmt::Debug for ErrorObservacionIntentVersionada {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorObservacionIntentVersionada")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorObservacionIntentVersionada {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorObservacionIntentVersionada {}

struct NombresIntentVersionado {
    canonico: OsString,
    temporales: [OsString; 3],
}

impl NombresIntentVersionado {
    fn desde_owner(
        owner: &super::DuenioTransaccionAnclado<'_>,
    ) -> Result<Self, ErrorObservacionIntentVersionada> {
        let ubicacion = owner._lock.ubicacion();
        let basename = ubicacion
            .ruta()
            .file_name()
            .ok_or(ErrorObservacionIntentVersionada::OwnerUnverified)?;
        let canonico = super::observar_siblings_nuevo_flujo::nombre_intent_retiro(basename)
            .map_err(|_| ErrorObservacionIntentVersionada::OwnerUnverified)?;
        let intent_temporal =
            crate::transaccion::anclado::contexto_admision::nombre_temporal_sibling(&canonico)
                .map_err(|_| ErrorObservacionIntentVersionada::OwnerUnverified)?;
        let contexto = crate::transaccion::anclado::contexto_admision::nombre_canonico(basename)
            .map_err(|_| ErrorObservacionIntentVersionada::OwnerUnverified)?;
        let contexto_temporal =
            crate::transaccion::anclado::contexto_admision::nombre_temporal_sibling(&contexto)
                .map_err(|_| ErrorObservacionIntentVersionada::OwnerUnverified)?;
        let journal_temporal = ubicacion.nombre_journal_temporal();

        Ok(Self {
            canonico,
            temporales: [journal_temporal, contexto_temporal, intent_temporal],
        })
    }

    fn temporales_os_str(&self) -> [&OsStr; 3] {
        [
            self.temporales[0].as_os_str(),
            self.temporales[1].as_os_str(),
            self.temporales[2].as_os_str(),
        ]
    }
}

impl super::DuenioTransaccionAnclado<'_> {
    /// Observes a present Intent or two exact absences with owner revalidation.
    pub(super) fn observar_intent_versionado(
        &mut self,
    ) -> Result<Option<ObservacionIntentVersionada>, ErrorObservacionIntentVersionada> {
        self.observar_intent_versionado_con_hooks(|| {}, || {})
    }

    #[cfg(test)]
    pub(super) fn observar_intent_versionado_con_hook_prueba<F>(
        &mut self,
        after_read: F,
    ) -> Result<Option<ObservacionIntentVersionada>, ErrorObservacionIntentVersionada>
    where
        F: FnOnce(),
    {
        self.observar_intent_versionado_con_hooks(|| {}, after_read)
    }

    #[cfg(test)]
    pub(super) fn observar_intent_versionado_despues_primera_ausencia_prueba<F>(
        &mut self,
        after_first_absence: F,
    ) -> Result<Option<ObservacionIntentVersionada>, ErrorObservacionIntentVersionada>
    where
        F: FnOnce(),
    {
        self.observar_intent_versionado_con_hooks(after_first_absence, || {})
    }

    fn observar_intent_versionado_con_hooks<F, G>(
        &mut self,
        after_first_absence: F,
        after_read: G,
    ) -> Result<Option<ObservacionIntentVersionada>, ErrorObservacionIntentVersionada>
    where
        F: FnOnce(),
        G: FnOnce(),
    {
        let nombres = NombresIntentVersionado::desde_owner(self)?;
        let lectura = {
            let parent = self
                .parent_revalidado()
                .map_err(|_| ErrorObservacionIntentVersionada::OwnerUnverified)?;
            validar_temporales_intent(self, &parent, &nombres)?;

            match super::lector_intent_canonico::abrir_intent_canonico(&parent, &nombres.canonico) {
                Ok(Some(archivo)) => self
                    .leer_intent_versionado_presente(&parent, &nombres, archivo)
                    .map(Some),
                Ok(None) => {
                    drop(parent);
                    after_first_absence();
                    self.confirmar_segunda_ausencia(&nombres)
                }
                Err(_) => Err(ErrorObservacionIntentVersionada::CanonicalUnverified),
            }
        };
        after_read();

        let postcheck = self.revalidar_owner_y_temporales();
        finalizar_con_postcheck(lectura, postcheck)
    }

    fn leer_intent_versionado_presente(
        &self,
        parent: &File,
        nombres: &NombresIntentVersionado,
        archivo: File,
    ) -> Result<ObservacionIntentVersionada, ErrorObservacionIntentVersionada> {
        let candidato = leer_intent_abierto_versionado(
            parent,
            &nombres.canonico,
            archivo,
            self.scope_parent,
            self.identidad_parent,
        )
        .map_err(mapear_error_canonico_intent)?;
        Ok(ObservacionIntentVersionada { candidato })
    }

    fn confirmar_segunda_ausencia(
        &self,
        nombres: &NombresIntentVersionado,
    ) -> Result<Option<ObservacionIntentVersionada>, ErrorObservacionIntentVersionada> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionIntentVersionada::OwnerUnverified)?;
        validar_temporales_intent(self, &parent, nombres)?;

        match super::lector_intent_canonico::abrir_intent_canonico(&parent, &nombres.canonico) {
            Ok(None) => Ok(None),
            Ok(Some(archivo)) => {
                drop(archivo);
                Err(ErrorObservacionIntentVersionada::CanonicalUnverified)
            }
            Err(_) => Err(ErrorObservacionIntentVersionada::CanonicalUnverified),
        }
    }

    fn revalidar_owner_y_temporales(&self) -> Result<(), ErrorObservacionIntentVersionada> {
        let nombres = NombresIntentVersionado::desde_owner(self)?;
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionIntentVersionada::OwnerUnverified)?;
        let validacion = validar_temporales_intent(self, &parent, &nombres);
        drop(parent);
        validacion
    }
}

fn validar_temporales_intent(
    owner: &super::DuenioTransaccionAnclado<'_>,
    parent: &File,
    nombres: &NombresIntentVersionado,
) -> Result<(), ErrorObservacionIntentVersionada> {
    super::lector_intent_canonico::validar_temporales_ausentes(
        parent,
        owner.scope_parent,
        nombres.temporales_os_str(),
    )
    .map_err(|_| ErrorObservacionIntentVersionada::OwnerUnverified)
}

fn mapear_error_canonico_intent(
    error: ErrorIntentCanonicoVersionado,
) -> ErrorObservacionIntentVersionada {
    match error {
        ErrorIntentCanonicoVersionado::CanonicalUnverified => {
            ErrorObservacionIntentVersionada::CanonicalUnverified
        }
        ErrorIntentCanonicoVersionado::Candidate(error) => {
            ErrorObservacionIntentVersionada::Candidate(error)
        }
    }
}

/// Decodes, then compares the captured native source and expected parent data.
pub(super) fn leer_intent_versionado_vinculado(
    archivo: File,
    scope_esperado: ScopeFilesystem,
    identidad_parent: IdentidadDirectorio,
) -> Result<CandidatoIntentConVinculosCoincidentes, ErrorIntentVersionadoVinculado> {
    let candidato = leer_candidato_intent_versionado(archivo, scope_esperado)
        .map_err(ErrorIntentVersionadoVinculado::Candidato)?;
    vincular_candidato(candidato, identidad_parent)
}

fn vincular_candidato(
    candidato: CandidatoIntentDecodificadoSinVinculo,
    identidad_parent: IdentidadDirectorio,
) -> Result<CandidatoIntentConVinculosCoincidentes, ErrorIntentVersionadoVinculado> {
    let parent_journal = identidad_parent
        .journal_identity()
        .ok_or(ErrorIntentVersionadoVinculado::ParentBindingMismatch)?;

    match &candidato.envelope {
        IntentRetiroPersistidoVersionado::V1(intent) => {
            intent
                .validar_vinculo_archivo_intent(candidato.identidad_archivo, candidato.scope)
                .map_err(|_| ErrorIntentVersionadoVinculado::SourceBindingMismatch)?;
            if intent.journal().parent() != &parent_journal {
                return Err(ErrorIntentVersionadoVinculado::ParentBindingMismatch);
            }
        }
        IntentRetiroPersistidoVersionado::V2(intent) => {
            let identity = IdentidadArchivoPersistida::try_from(candidato.identidad_archivo)
                .map_err(|_| ErrorIntentVersionadoVinculado::SourceIdentityUnverified)?;
            intent
                .validar_vinculo_archivo_intent_datos(&identity, candidato.scope.0)
                .map_err(|_| ErrorIntentVersionadoVinculado::SourceBindingMismatch)?;
            if intent.journal().parent() != &parent_journal {
                return Err(ErrorIntentVersionadoVinculado::ParentBindingMismatch);
            }
        }
    }

    let CandidatoIntentDecodificadoSinVinculo {
        envelope,
        archivo_original,
        identidad_archivo,
        scope,
    } = candidato;
    Ok(CandidatoIntentConVinculosCoincidentes {
        envelope,
        archivo_original,
        identidad_archivo,
        scope,
        parent_journal,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorCandidatoIntentSinVinculo {
    Frame(ErrorLecturaFrameIntent),
    Selector(ErrorSelectorIntentRetiro),
}

impl ErrorCandidatoIntentSinVinculo {
    pub(super) const fn codigo(self) -> &'static str {
        match self {
            Self::Frame(error) => error.codigo(),
            Self::Selector(error) => error.codigo(),
        }
    }
}

impl fmt::Debug for ErrorCandidatoIntentSinVinculo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl fmt::Display for ErrorCandidatoIntentSinVinculo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorCandidatoIntentSinVinculo {}

/// Lee un frame acotado y decodifica su versión sin comprobar vínculos.
pub(super) fn leer_candidato_intent_versionado(
    archivo: File,
    scope_esperado: ScopeFilesystem,
) -> Result<CandidatoIntentDecodificadoSinVinculo, ErrorCandidatoIntentSinVinculo> {
    leer_candidato_con_hook(
        archivo,
        scope_esperado,
        intent_persistido::cuota_maxima_frame_bytes(),
        || {},
    )
}

#[cfg(test)]
pub(super) fn leer_candidato_intent_versionado_con_cap_y_hook<F>(
    archivo: File,
    scope_esperado: ScopeFilesystem,
    cap_prueba: usize,
    despues_metadata: F,
) -> Result<CandidatoIntentDecodificadoSinVinculo, ErrorCandidatoIntentSinVinculo>
where
    F: FnOnce(),
{
    leer_candidato_con_hook(archivo, scope_esperado, cap_prueba, despues_metadata)
}

fn leer_candidato_con_hook<F>(
    archivo: File,
    scope_esperado: ScopeFilesystem,
    cap_prueba: usize,
    despues_metadata: F,
) -> Result<CandidatoIntentDecodificadoSinVinculo, ErrorCandidatoIntentSinVinculo>
where
    F: FnOnce(),
{
    let cap = cap_prueba.min(intent_persistido::cuota_maxima_frame_bytes());
    let frame = leer_con_hook(archivo, scope_esperado, cap, despues_metadata)
        .map_err(ErrorCandidatoIntentSinVinculo::Frame)?;
    let envelope = intent_persistido::decodificar_versionado(&frame.bytes)
        .map_err(ErrorCandidatoIntentSinVinculo::Selector)?;

    Ok(CandidatoIntentDecodificadoSinVinculo {
        envelope,
        archivo_original: frame.archivo_original,
        identidad_archivo: frame.identidad_archivo,
        scope: frame.scope,
    })
}

pub(super) fn leer_frame_intent(
    archivo: File,
    scope_esperado: ScopeFilesystem,
    cap: usize,
) -> Result<FrameIntentLeido, ErrorLecturaFrameIntent> {
    leer_con_hook(archivo, scope_esperado, cap, || {})
}

#[cfg(test)]
pub(super) fn leer_frame_intent_con_hook<F>(
    archivo: File,
    scope_esperado: ScopeFilesystem,
    cap: usize,
    despues_metadata: F,
) -> Result<FrameIntentLeido, ErrorLecturaFrameIntent>
where
    F: FnOnce(),
{
    leer_con_hook(archivo, scope_esperado, cap, despues_metadata)
}

fn leer_con_hook<F>(
    mut archivo: File,
    scope_esperado: ScopeFilesystem,
    cap: usize,
    despues_metadata: F,
) -> Result<FrameIntentLeido, ErrorLecturaFrameIntent>
where
    F: FnOnce(),
{
    let cap_mas_uno = cap
        .checked_add(1)
        .ok_or(ErrorLecturaFrameIntent::LengthOverflow)?;
    let cap_mas_uno_u64 =
        u64::try_from(cap_mas_uno).map_err(|_| ErrorLecturaFrameIntent::LengthOverflow)?;
    let cap_u64 = u64::try_from(cap).map_err(|_| ErrorLecturaFrameIntent::LengthOverflow)?;

    let metadata = archivo
        .metadata()
        .map_err(|_| ErrorLecturaFrameIntent::ReadIo)?;
    if metadata.file_type().is_symlink() || metadata_es_reparse(&metadata) || !metadata.is_file() {
        return Err(ErrorLecturaFrameIntent::EntryRejected);
    }
    if metadata.len() > cap_u64 {
        return Err(ErrorLecturaFrameIntent::FrameTooLarge);
    }

    let identidad_archivo =
        identidad_archivo(&archivo).map_err(|_| ErrorLecturaFrameIntent::ReadIo)?;
    let scope = scope_filesystem(&archivo).map_err(|_| ErrorLecturaFrameIntent::ScopeUnverified)?;
    if scope != scope_esperado {
        return Err(ErrorLecturaFrameIntent::ScopeMismatch);
    }

    despues_metadata();
    seek_inicio(&mut archivo)?;
    let bytes = leer_acotado(&mut archivo, cap, cap_mas_uno_u64)?;

    Ok(FrameIntentLeido {
        archivo_original: archivo,
        bytes,
        identidad_archivo,
        scope,
    })
}

fn seek_inicio(archivo: &mut File) -> Result<(), ErrorLecturaFrameIntent> {
    loop {
        match archivo.seek(SeekFrom::Start(0)) {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorLecturaFrameIntent::ReadIo),
        }
    }
}

fn leer_acotado<R: Read>(
    reader: &mut R,
    cap: usize,
    cap_mas_uno_u64: u64,
) -> Result<Vec<u8>, ErrorLecturaFrameIntent> {
    let mut bounded = reader.take(cap_mas_uno_u64);
    let mut bytes = Vec::new();
    let mut fragmento = [0_u8; 512];

    loop {
        let leidos = match bounded.read(&mut fragmento) {
            Ok(0) => break,
            Ok(leidos) => leidos,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ErrorLecturaFrameIntent::ReadIo),
        };
        let largo_siguiente = bytes
            .len()
            .checked_add(leidos)
            .ok_or(ErrorLecturaFrameIntent::LengthOverflow)?;
        if largo_siguiente > cap {
            return Err(ErrorLecturaFrameIntent::FrameTooLarge);
        }
        bytes
            .try_reserve(leidos)
            .map_err(|_| ErrorLecturaFrameIntent::ReservationFailed)?;
        bytes.extend_from_slice(&fragmento[..leidos]);
    }
    Ok(bytes)
}

fn finalizar_con_postcheck<T, E>(
    resultado: Result<T, E>,
    postcheck: Result<(), E>,
) -> Result<T, E> {
    postcheck?;
    resultado
}
