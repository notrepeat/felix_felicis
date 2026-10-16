//! Escritura y lectura del temporal singleton del journal bajo el lock.

use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;

use fs_at::{OpenOptions as OpenOptionsAt, OpenOptionsWriteMode};

use super::abrir::abrir_entrada_relativa;
use super::{
    DirectorioAnclado, ErrorDirectorioAnclado, IdentidadDirectorio, TipoEntradaAnclada,
    metadata_directorio, validar_componente,
};
use crate::transaccion::journal::{Journal, JournalError, MAX_JOURNAL_BYTES};
use crate::transaccion::lock::LockEscritura;

use super::IdentidadArchivo;

/// Error estable y redactado para operaciones sobre archivos del journal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorJournalAnclado {
    Io,
    Exists,
    TooLarge,
    ReparseNotAllowed,
    RegularFileRequired,
    ParentChanged,
    SourceMissing,
    IdentityMismatch,
    JournalMismatch,
    CanonicalConflict,
    RemoteUnsupported,
    ParentIdentityUnavailable,
    ParentIdentityMismatch,
    ParentScopeUnverified,
    InvalidJournal(JournalError),
}

impl ErrorJournalAnclado {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::Io => "journal_io",
            Self::Exists => "journal_exists",
            Self::TooLarge => "journal_too_large",
            Self::ReparseNotAllowed => "journal_reparse_not_allowed",
            Self::RegularFileRequired => "journal_regular_file_required",
            Self::ParentChanged => "journal_parent_changed",
            Self::SourceMissing => "journal_source_missing",
            Self::IdentityMismatch => "journal_identity_mismatch",
            Self::JournalMismatch => "journal_record_mismatch",
            Self::CanonicalConflict => "journal_canonical_conflict",
            Self::RemoteUnsupported => "journal_remote_unsupported",
            Self::ParentIdentityUnavailable => "journal_parent_identity_unavailable",
            Self::ParentIdentityMismatch => "journal_parent_identity_mismatch",
            Self::ParentScopeUnverified => "journal_parent_scope_unverified",
            Self::InvalidJournal(error) => error.code(),
        }
    }
}

#[path = "journal_temporal/eliminar.rs"]
mod eliminar;
#[path = "journal_temporal/publicar.rs"]
mod publicar;
#[path = "journal_temporal/reemplazar.rs"]
mod reemplazar;

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResultadoPublicacionJournal {
    PrecondicionFallida(ErrorJournalAnclado),
    NoAplicado(ErrorJournalAnclado),
    Confirmado {
        limpieza: LimpiezaConfirmadaJournal,
    },
    Indeterminado {
        causa: CausaIndeterminacionPublicacionJournal,
    },
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResultadoEliminacionJournal {
    Ausente,
    Eliminado,
    Conflicto(ErrorJournalAnclado),
    Indeterminado(ErrorJournalAnclado),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LimpiezaConfirmadaJournal {
    Completa,
    Pendiente,
    Conflicto(ErrorJournalAnclado),
    Indeterminada,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CausaIndeterminacionPublicacionJournal {
    CanonicoAusenteTrasExito,
    CanonicoNoVerificable(ErrorJournalAnclado),
    EstadoPosteriorNoDemostrable {
        error_publicacion: ErrorJournalAnclado,
        canonico: Result<bool, ErrorJournalAnclado>,
        temporal: Option<Result<bool, ErrorJournalAnclado>>,
    },
    ReemplazoNoDemostrable {
        intento: EstadoIntentoReemplazoJournal,
        canonico: EstadoObservacionReemplazoJournal,
        temporal: EstadoObservacionReemplazoJournal,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EstadoIntentoReemplazoJournal {
    NoInvocado,
    Exito,
    Error(ErrorJournalAnclado),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EstadoObservacionReemplazoJournal {
    Ausente,
    CoincideAnterior,
    CoincideCandidato,
    IdentidadDistinta,
    RegistroDistinto,
    NoVerificable(ErrorJournalAnclado),
}

impl fmt::Debug for ErrorJournalAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorJournalAnclado")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorJournalAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorJournalAnclado {}

/// File identity captured from an opened handle. Its representation is secret.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdentidadArchivoJournal(IdentidadArchivo);

impl IdentidadArchivoJournal {
    /// Copia los datos de identidad capturados del handle del journal.
    pub(super) fn identidad_archivo(self) -> IdentidadArchivo {
        self.0
    }
}

impl fmt::Debug for IdentidadArchivoJournal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IdentidadArchivoJournal([redacted])")
    }
}

/// Capability limitada al temporal determinista de la raíz asociada al lock.
/// No acepta rutas ni nombres; el parent handle permanece abierto durante su vida.
/// El modelo asume cooperación bajo `LockEscritura`: un cambio externo del nombre
/// del parent no se re-resuelve ni se considera protegido por el lock.
pub(crate) struct JournalTemporalAnclado<'lock> {
    _lock: &'lock mut LockEscritura,
    parent: File,
    identidad_parent: IdentidadDirectorio,
    nombre: OsString,
    nombre_journal: OsString,
    raiz_remota: bool,
    _autoridad: Arc<()>,
    _vida_lock: PhantomData<&'lock LockEscritura>,
}

impl<'lock> JournalTemporalAnclado<'lock> {
    pub(crate) fn desde_lock(lock: &'lock mut LockEscritura) -> Result<Self, ErrorJournalAnclado> {
        let ancla = DirectorioAnclado::desde_ubicacion(lock.ubicacion())
            .map_err(mapear_error_directorio)?;
        let identidad_parent = ancla.identidad_parent();
        let autoridad = lock.autoridad();
        desde_parent_retenido(lock, ancla.parent, identidad_parent, autoridad)
    }

    /// Crea el singleton de forma exclusiva, escribe y sincroniza el archivo.
    /// Si falla escritura o sincronización, intenta retirar solo la identidad
    /// creada y devuelve el error original. El temporal puede permanecer si no
    /// se demuestra esa identidad o la retirada falla.
    pub(crate) fn crear_nuevo(
        &self,
        bytes: &[u8],
    ) -> Result<IdentidadArchivoJournal, ErrorJournalAnclado> {
        self.crear_nuevo_con(
            bytes,
            |archivo, bytes| {
                archivo
                    .write_all(bytes)
                    .map_err(|_| ErrorJournalAnclado::Io)
            },
            |archivo| archivo.sync_all().map_err(|_| ErrorJournalAnclado::Io),
        )
    }

    fn crear_nuevo_con(
        &self,
        bytes: &[u8],
        escribir: impl FnOnce(&mut File, &[u8]) -> Result<(), ErrorJournalAnclado>,
        sincronizar: impl FnOnce(&File) -> Result<(), ErrorJournalAnclado>,
    ) -> Result<IdentidadArchivoJournal, ErrorJournalAnclado> {
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(ErrorJournalAnclado::TooLarge);
        }
        self.validar_parent()?;

        let mut opciones = OpenOptionsAt::default();
        opciones
            .write(OpenOptionsWriteMode::Write)
            .create_new(true)
            .follow(false);
        #[cfg(unix)]
        {
            use fs_at::os::unix::OpenOptionsExt;
            opciones.mode(0o600);
        }
        #[cfg(windows)]
        {
            use fs_at::os::windows::OpenOptionsExt;
            opciones.desired_access(acceso_temporal_creacion());
        }
        let mut archivo = opciones
            .open_at(&self.parent, Path::new(&self.nombre))
            .map_err(mapear_error_io)?;
        validar_archivo_regular(&archivo)?;
        let identidad = identidad_archivo(&archivo)?;
        if let Err(error) = escribir(&mut archivo, bytes).and_then(|()| sincronizar(&archivo)) {
            drop(archivo);
            let _ = self.limpiar_temporal_si_coincide(identidad);
            return Err(error);
        }
        drop(archivo);
        Ok(identidad)
    }

    /// Abre el singleton respecto al parent retenido, sin seguir enlaces, y lee
    /// como máximo `MAX_JOURNAL_BYTES + 1` bytes antes de decidir el límite.
    pub(crate) fn abrir_y_leer(
        &self,
    ) -> Result<(IdentidadArchivoJournal, Vec<u8>), ErrorJournalAnclado> {
        self.validar_parent()?;
        let archivo =
            abrir_entrada_relativa(&self.parent, &self.nombre, TipoEntradaAnclada::Archivo)
                .map_err(|error| match error {
                    ErrorDirectorioAnclado::EntradaAusente => ErrorJournalAnclado::SourceMissing,
                    otro => mapear_error_directorio(otro),
                })?;
        validar_archivo_regular(&archivo)?;
        let identidad = identidad_archivo(&archivo)?;
        let mut bytes = Vec::with_capacity(MAX_JOURNAL_BYTES + 1);
        archivo
            .take((MAX_JOURNAL_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ErrorJournalAnclado::Io)?;
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(ErrorJournalAnclado::TooLarge);
        }
        Ok((identidad, bytes))
    }

    /// Lee el journal canónico relativo al parent retenido. Solo una ausencia
    /// observada del nombre devuelve `None`; toda otra condición falla cerrada.
    pub(crate) fn abrir_y_leer_canonico(
        &self,
    ) -> Result<Option<(IdentidadArchivoJournal, Vec<u8>)>, ErrorJournalAnclado> {
        self.validar_parent()?;
        let archivo = match abrir_entrada_relativa(
            &self.parent,
            &self.nombre_journal,
            TipoEntradaAnclada::Archivo,
        ) {
            Ok(archivo) => archivo,
            Err(ErrorDirectorioAnclado::EntradaAusente) => return Ok(None),
            Err(error) => return Err(mapear_error_directorio(error)),
        };
        validar_archivo_regular(&archivo)?;
        let identidad = identidad_archivo(&archivo)?;
        let mut bytes = Vec::with_capacity(MAX_JOURNAL_BYTES + 1);
        archivo
            .take((MAX_JOURNAL_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ErrorJournalAnclado::Io)?;
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(ErrorJournalAnclado::TooLarge);
        }
        Ok(Some((identidad, bytes)))
    }

    /// Carga el journal canónico y lo liga a la identidad del parent retenido.
    /// Solo la ausencia del nombre devuelve `None`; bytes inválidos o una
    /// identidad distinta fallan cerrados con errores estables y redactados.
    pub(crate) fn cargar_canonico(
        &self,
    ) -> Result<Option<(IdentidadArchivoJournal, Journal)>, ErrorJournalAnclado> {
        let Some((identidad_archivo, bytes)) = self.abrir_y_leer_canonico()? else {
            return Ok(None);
        };
        self.cargar_journal_leido(identidad_archivo, &bytes)
            .map(Some)
    }

    /// Carga el temporal fijo y lo liga a la identidad del parent retenido.
    /// La ausencia es un error de precondición; la identidad y el journal
    /// corresponden al mismo handle y a los mismos bytes leídos.
    pub(crate) fn cargar_temporal(
        &self,
    ) -> Result<(IdentidadArchivoJournal, Journal), ErrorJournalAnclado> {
        let (identidad_archivo, bytes) = self.abrir_y_leer()?;
        self.cargar_journal_leido(identidad_archivo, &bytes)
    }

    fn cargar_journal_leido(
        &self,
        identidad_archivo: IdentidadArchivoJournal,
        bytes: &[u8],
    ) -> Result<(IdentidadArchivoJournal, Journal), ErrorJournalAnclado> {
        let journal = Journal::parse(bytes).map_err(ErrorJournalAnclado::InvalidJournal)?;
        let parent_esperado = self
            .identidad_parent
            .journal_identity()
            .ok_or(ErrorJournalAnclado::ParentIdentityUnavailable)?;
        if journal.parent() != &parent_esperado {
            return Err(ErrorJournalAnclado::ParentIdentityMismatch);
        }
        Ok((identidad_archivo, journal))
    }

    fn validar_parent(&self) -> Result<(), ErrorJournalAnclado> {
        let identidad = metadata_directorio(&self.parent).map_err(mapear_error_directorio)?;
        if identidad != self.identidad_parent {
            return Err(ErrorJournalAnclado::ParentChanged);
        }
        Ok(())
    }
}

pub(super) fn desde_parent_retenido<'lock>(
    lock: &'lock mut LockEscritura,
    parent: File,
    identidad_parent: IdentidadDirectorio,
    autoridad: Arc<()>,
) -> Result<JournalTemporalAnclado<'lock>, ErrorJournalAnclado> {
    let ubicacion = lock.ubicacion();
    let nombre = ubicacion.nombre_journal_temporal();
    validar_componente(&nombre).map_err(mapear_error_directorio)?;
    let ruta_journal = ubicacion.journal();
    let nombre_journal = ruta_journal
        .as_path()
        .file_name()
        .ok_or(ErrorJournalAnclado::Io)?
        .to_os_string();
    validar_componente(&nombre_journal).map_err(mapear_error_directorio)?;
    let raiz_remota = publicar::raiz_es_remota(lock.raiz_canonica());
    Ok(JournalTemporalAnclado {
        _lock: lock,
        parent,
        identidad_parent,
        nombre,
        nombre_journal,
        raiz_remota,
        _autoridad: autoridad,
        _vida_lock: PhantomData,
    })
}

pub(super) fn raiz_es_remota(ruta: &Path) -> bool {
    publicar::raiz_es_remota(ruta)
}

#[cfg(windows)]
fn acceso_temporal_creacion() -> u32 {
    use windows_sys::Win32::Foundation::GENERIC_WRITE;
    use windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;

    FILE_READ_ATTRIBUTES | GENERIC_WRITE
}

fn validar_archivo_regular(handle: &File) -> Result<(), ErrorJournalAnclado> {
    let metadata = handle.metadata().map_err(|_| ErrorJournalAnclado::Io)?;
    if metadata.file_type().is_symlink() || super::metadata_es_reparse(&metadata) {
        return Err(ErrorJournalAnclado::ReparseNotAllowed);
    }
    if !metadata.is_file() {
        return Err(ErrorJournalAnclado::RegularFileRequired);
    }
    Ok(())
}

fn identidad_archivo(handle: &File) -> Result<IdentidadArchivoJournal, ErrorJournalAnclado> {
    super::identidad_archivo(handle)
        .map(IdentidadArchivoJournal)
        .map_err(|_| ErrorJournalAnclado::Io)
}

fn mapear_error_directorio(error: ErrorDirectorioAnclado) -> ErrorJournalAnclado {
    match error {
        ErrorDirectorioAnclado::Io
        | ErrorDirectorioAnclado::ComponenteInvalido
        | ErrorDirectorioAnclado::RaizAusente
        | ErrorDirectorioAnclado::EntradaAusente => ErrorJournalAnclado::Io,
        ErrorDirectorioAnclado::EnlaceNoPermitido => ErrorJournalAnclado::ReparseNotAllowed,
        ErrorDirectorioAnclado::TipoNoAdmitido => ErrorJournalAnclado::RegularFileRequired,
        ErrorDirectorioAnclado::EntradaExistente => ErrorJournalAnclado::Exists,
    }
}

fn mapear_error_io(error: std::io::Error) -> ErrorJournalAnclado {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        ErrorJournalAnclado::Exists
    } else {
        ErrorJournalAnclado::Io
    }
}

#[cfg(test)]
#[path = "journal_temporal_tests.rs"]
mod tests;
