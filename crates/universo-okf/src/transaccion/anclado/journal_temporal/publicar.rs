use super::*;
use std::ffi::OsStr;

impl JournalTemporalAnclado<'_> {
    /// Publica el temporal fijo sin reemplazar el basename canónico del journal.
    /// La identidad debe provenir de `crear_nuevo`; nombres y rutas no los decide
    /// quien llama. El lock protege a escritores cooperantes en un filesystem local.
    pub(crate) fn publicar(
        &self,
        esperada: IdentidadArchivoJournal,
    ) -> Result<ResultadoPublicacionJournal, ErrorJournalAnclado> {
        if self.raiz_remota {
            return Err(ErrorJournalAnclado::RemoteUnsupported);
        }
        self.validar_parent()?;
        let fuente = match abrir_temporal(&self.parent, &self.nombre) {
            Ok(fuente) => fuente,
            Err(ErrorJournalAnclado::SourceMissing) => {
                return if self.canonico_es(&esperada)? {
                    Ok(ResultadoPublicacionJournal::YaPublicado)
                } else {
                    Err(ErrorJournalAnclado::SourceMissing)
                };
            }
            Err(error) => return Err(error),
        };
        validar_archivo_regular(&fuente)?;
        if identidad_archivo(&fuente)? != esperada {
            return Err(ErrorJournalAnclado::IdentityMismatch);
        }

        match publicar_plataforma(&self.parent, &self.nombre, &self.nombre_journal, &fuente) {
            Ok(()) => self.resultado_despues_de_commit(esperada, |capability, identidad| {
                capability.limpiar_temporal_si_coincide(identidad)
            }),
            Err(error) => self.resolver_error_publicacion(esperada, error),
        }
    }

    fn resultado_despues_de_commit(
        &self,
        esperada: IdentidadArchivoJournal,
        limpiar: impl FnOnce(&Self, IdentidadArchivoJournal) -> EstadoLimpiezaPublicacion,
    ) -> Result<ResultadoPublicacionJournal, ErrorJournalAnclado> {
        self.resultado_despues_de_commit_observando(
            esperada,
            |capability, identidad| capability.canonico_es(identidad),
            limpiar,
        )
    }

    fn resultado_despues_de_commit_observando(
        &self,
        esperada: IdentidadArchivoJournal,
        observar_canonico: impl FnOnce(
            &Self,
            &IdentidadArchivoJournal,
        ) -> Result<bool, ErrorJournalAnclado>,
        limpiar: impl FnOnce(&Self, IdentidadArchivoJournal) -> EstadoLimpiezaPublicacion,
    ) -> Result<ResultadoPublicacionJournal, ErrorJournalAnclado> {
        match observar_canonico(self, &esperada) {
            Ok(true) => resultado_con_limpieza_confirmada(
                limpiar(self, esperada),
                ResultadoPublicacionJournal::Publicado,
            ),
            Ok(false) => Ok(ResultadoPublicacionJournal::Indeterminado),
            Err(error) if es_conflicto_concreto(error) => Err(error),
            Err(_) => Ok(ResultadoPublicacionJournal::Indeterminado),
        }
    }

    fn resolver_error_publicacion(
        &self,
        esperada: IdentidadArchivoJournal,
        error: ErrorJournalAnclado,
    ) -> Result<ResultadoPublicacionJournal, ErrorJournalAnclado> {
        self.resolver_error_publicacion_observando(
            esperada,
            error,
            |capability, identidad| capability.canonico_es(identidad),
            |capability, identidad| capability.temporal_es(identidad),
            |capability, identidad| capability.limpiar_temporal_si_coincide(identidad),
        )
    }

    fn resolver_error_publicacion_observando(
        &self,
        esperada: IdentidadArchivoJournal,
        error_publicacion: ErrorJournalAnclado,
        observar_canonico: impl FnOnce(
            &Self,
            &IdentidadArchivoJournal,
        ) -> Result<bool, ErrorJournalAnclado>,
        observar_temporal: impl FnOnce(
            &Self,
            &IdentidadArchivoJournal,
        ) -> Result<bool, ErrorJournalAnclado>,
        limpiar: impl FnOnce(&Self, IdentidadArchivoJournal) -> EstadoLimpiezaPublicacion,
    ) -> Result<ResultadoPublicacionJournal, ErrorJournalAnclado> {
        let canonico = observar_canonico(self, &esperada);
        if canonico == Ok(true) {
            return resultado_con_limpieza_confirmada(
                limpiar(self, esperada),
                ResultadoPublicacionJournal::YaPublicado,
            );
        }

        // Ambas observaciones son de solo lectura. Se conservan sus errores
        // estables de conflicto; sin prueba de no aplicación, el estado es incierto.
        let temporal = observar_temporal(self, &esperada);
        match canonico {
            Err(error) if es_conflicto_concreto(error) => Err(error),
            Err(_) => Ok(ResultadoPublicacionJournal::Indeterminado),
            Ok(false) => match temporal {
                Ok(true) => Err(error_publicacion),
                Err(error) if es_conflicto_temporal(error) => Err(error),
                _ => Ok(ResultadoPublicacionJournal::Indeterminado),
            },
            Ok(true) => unreachable!("canonical result was handled above"),
        }
    }

    fn temporal_es(&self, esperada: &IdentidadArchivoJournal) -> Result<bool, ErrorJournalAnclado> {
        self.validar_parent()?;
        let archivo = match abrir_temporal(&self.parent, &self.nombre) {
            Ok(archivo) => archivo,
            Err(ErrorJournalAnclado::SourceMissing) => return Ok(false),
            Err(error) => return Err(error),
        };
        validar_archivo_regular(&archivo)?;
        if identidad_archivo(&archivo)? != *esperada {
            return Err(ErrorJournalAnclado::IdentityMismatch);
        }
        Ok(true)
    }

    fn canonico_es(&self, esperada: &IdentidadArchivoJournal) -> Result<bool, ErrorJournalAnclado> {
        self.validar_parent()?;
        let archivo = match abrir_entrada_relativa(
            &self.parent,
            &self.nombre_journal,
            TipoEntradaAnclada::Archivo,
        ) {
            Ok(archivo) => archivo,
            Err(ErrorDirectorioAnclado::EntradaAusente) => return Ok(false),
            Err(error) => return Err(mapear_error_canonico(error)),
        };
        validar_archivo_regular(&archivo).map_err(mapear_error_canonico_validacion)?;
        let identidad = identidad_archivo(&archivo)?;
        if identidad != *esperada {
            return Err(ErrorJournalAnclado::CanonicalConflict);
        }
        Ok(true)
    }

    fn limpiar_temporal_si_coincide(
        &self,
        esperada: IdentidadArchivoJournal,
    ) -> EstadoLimpiezaPublicacion {
        if self.validar_parent().is_err() {
            return EstadoLimpiezaPublicacion::Indeterminada;
        }
        let archivo = match abrir_temporal(&self.parent, &self.nombre) {
            Ok(archivo) => archivo,
            Err(ErrorJournalAnclado::SourceMissing) => return EstadoLimpiezaPublicacion::Completa,
            Err(_) => return EstadoLimpiezaPublicacion::Indeterminada,
        };
        if let Err(error) = validar_archivo_regular(&archivo) {
            return if es_conflicto_temporal(error) {
                EstadoLimpiezaPublicacion::Conflicto(error)
            } else {
                EstadoLimpiezaPublicacion::Indeterminada
            };
        }
        match identidad_archivo(&archivo) {
            Ok(identidad) if identidad == esperada => {}
            Ok(_) => {
                return EstadoLimpiezaPublicacion::Conflicto(ErrorJournalAnclado::IdentityMismatch);
            }
            Err(_) => return EstadoLimpiezaPublicacion::Indeterminada,
        }
        match quitar_temporal(&self.parent, &self.nombre, &archivo) {
            Ok(()) => EstadoLimpiezaPublicacion::Completa,
            Err(_) => EstadoLimpiezaPublicacion::Pendiente,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EstadoLimpiezaPublicacion {
    Completa,
    Pendiente,
    Conflicto(ErrorJournalAnclado),
    Indeterminada,
}

fn resultado_con_limpieza_confirmada(
    limpieza: EstadoLimpiezaPublicacion,
    confirmado: ResultadoPublicacionJournal,
) -> Result<ResultadoPublicacionJournal, ErrorJournalAnclado> {
    match limpieza {
        EstadoLimpiezaPublicacion::Completa => Ok(confirmado),
        EstadoLimpiezaPublicacion::Pendiente => {
            Ok(ResultadoPublicacionJournal::PublicadoTemporalPendiente)
        }
        EstadoLimpiezaPublicacion::Conflicto(error) => Err(error),
        EstadoLimpiezaPublicacion::Indeterminada => Ok(ResultadoPublicacionJournal::Indeterminado),
    }
}

fn es_conflicto_concreto(error: ErrorJournalAnclado) -> bool {
    matches!(
        error,
        ErrorJournalAnclado::CanonicalConflict
            | ErrorJournalAnclado::RegularFileRequired
            | ErrorJournalAnclado::ReparseNotAllowed
    )
}

fn es_conflicto_temporal(error: ErrorJournalAnclado) -> bool {
    matches!(
        error,
        ErrorJournalAnclado::IdentityMismatch
            | ErrorJournalAnclado::RegularFileRequired
            | ErrorJournalAnclado::ReparseNotAllowed
    )
}

#[cfg(windows)]
pub(super) fn raiz_es_remota(ruta: &std::path::Path) -> bool {
    matches!(
        ruta.components().next(),
        Some(std::path::Component::Prefix(prefijo))
            if matches!(
                prefijo.kind(),
                std::path::Prefix::UNC(_, _) | std::path::Prefix::VerbatimUNC(_, _)
            )
    )
}

#[cfg(not(windows))]
pub(super) fn raiz_es_remota(_ruta: &std::path::Path) -> bool {
    false
}

#[cfg(unix)]
fn abrir_temporal(parent: &File, nombre: &OsStr) -> Result<File, ErrorJournalAnclado> {
    abrir_entrada_relativa(parent, nombre, TipoEntradaAnclada::Archivo)
        .map_err(mapear_error_temporal)
}

#[cfg(windows)]
fn abrir_temporal(parent: &File, nombre: &OsStr) -> Result<File, ErrorJournalAnclado> {
    use fs_at::os::windows::OpenOptionsExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_READ_ATTRIBUTES};

    let mut opciones = OpenOptionsAt::default();
    opciones
        .desired_access(FILE_READ_ATTRIBUTES | DELETE)
        .follow(false);
    opciones
        .open_at(parent, Path::new(nombre))
        .map_err(mapear_error_apertura_temporal)
}

#[cfg(not(any(unix, windows)))]
fn abrir_temporal(_parent: &File, _nombre: &OsStr) -> Result<File, ErrorJournalAnclado> {
    Err(ErrorJournalAnclado::Io)
}

fn mapear_error_temporal(error: ErrorDirectorioAnclado) -> ErrorJournalAnclado {
    match error {
        ErrorDirectorioAnclado::EntradaAusente => ErrorJournalAnclado::SourceMissing,
        other => mapear_error_directorio(other),
    }
}

fn mapear_error_canonico(error: ErrorDirectorioAnclado) -> ErrorJournalAnclado {
    match error {
        ErrorDirectorioAnclado::Io => ErrorJournalAnclado::Io,
        ErrorDirectorioAnclado::ComponenteInvalido => ErrorJournalAnclado::Io,
        ErrorDirectorioAnclado::EnlaceNoPermitido => ErrorJournalAnclado::ReparseNotAllowed,
        ErrorDirectorioAnclado::TipoNoAdmitido => ErrorJournalAnclado::RegularFileRequired,
        ErrorDirectorioAnclado::RaizAusente | ErrorDirectorioAnclado::EntradaAusente => {
            ErrorJournalAnclado::Io
        }
        ErrorDirectorioAnclado::EntradaExistente => ErrorJournalAnclado::CanonicalConflict,
    }
}

fn mapear_error_canonico_validacion(error: ErrorJournalAnclado) -> ErrorJournalAnclado {
    match error {
        ErrorJournalAnclado::RegularFileRequired | ErrorJournalAnclado::ReparseNotAllowed => error,
        _ => ErrorJournalAnclado::Io,
    }
}

#[cfg(windows)]
fn mapear_error_apertura_temporal(error: std::io::Error) -> ErrorJournalAnclado {
    use windows_sys::Win32::Foundation::{
        ERROR_CANT_RESOLVE_FILENAME, ERROR_DIRECTORY, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND,
    };

    match error.raw_os_error().map(|code| code as u32) {
        Some(ERROR_FILE_NOT_FOUND) | Some(ERROR_PATH_NOT_FOUND) => {
            ErrorJournalAnclado::SourceMissing
        }
        Some(ERROR_CANT_RESOLVE_FILENAME) => ErrorJournalAnclado::ReparseNotAllowed,
        Some(ERROR_DIRECTORY) => ErrorJournalAnclado::RegularFileRequired,
        _ if error.kind() == std::io::ErrorKind::NotFound => ErrorJournalAnclado::SourceMissing,
        _ => ErrorJournalAnclado::Io,
    }
}

#[cfg(unix)]
fn publicar_plataforma(
    parent: &File,
    fuente: &OsStr,
    canonico: &OsStr,
    _handle_fuente: &File,
) -> Result<(), ErrorJournalAnclado> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let fuente = CString::new(fuente.as_bytes()).map_err(|_| ErrorJournalAnclado::Io)?;
    let canonico = CString::new(canonico.as_bytes()).map_err(|_| ErrorJournalAnclado::Io)?;
    // SAFETY: both validated names are single components relative to the same
    // retained parent handle. Flags 0 never follows a source symlink.
    let resultado = unsafe {
        libc::linkat(
            parent.as_raw_fd(),
            fuente.as_ptr(),
            parent.as_raw_fd(),
            canonico.as_ptr(),
            0,
        )
    };
    if resultado == 0 {
        Ok(())
    } else {
        Err(mapear_error_publicacion_unix(
            std::io::Error::last_os_error(),
        ))
    }
}

#[cfg(unix)]
fn quitar_temporal(
    parent: &File,
    nombre: &OsStr,
    _archivo: &File,
) -> Result<(), ErrorJournalAnclado> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let nombre = CString::new(nombre.as_bytes()).map_err(|_| ErrorJournalAnclado::Io)?;
    // SAFETY: `nombre` is a validated NUL-terminated component and `parent`
    // remains open. The caller re-opened and identity-checked this name first.
    let resultado = unsafe { libc::unlinkat(parent.as_raw_fd(), nombre.as_ptr(), 0) };
    if resultado == 0 {
        Ok(())
    } else {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            Ok(())
        } else {
            Err(ErrorJournalAnclado::Io)
        }
    }
}

#[cfg(unix)]
fn mapear_error_publicacion_unix(error: std::io::Error) -> ErrorJournalAnclado {
    match error.raw_os_error() {
        Some(libc::EEXIST) => ErrorJournalAnclado::CanonicalConflict,
        Some(libc::ELOOP) => ErrorJournalAnclado::ReparseNotAllowed,
        Some(libc::EISDIR | libc::ENOTDIR) => ErrorJournalAnclado::RegularFileRequired,
        _ => ErrorJournalAnclado::Io,
    }
}

#[cfg(windows)]
fn publicar_plataforma(
    parent: &File,
    _fuente: &OsStr,
    canonico: &OsStr,
    handle_fuente: &File,
) -> Result<(), ErrorJournalAnclado> {
    use std::mem::{offset_of, size_of};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Wdk::Storage::FileSystem::{
        FILE_RENAME_INFORMATION, FileRenameInformation, NtSetInformationFile,
    };
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    let utf16: Vec<u16> = canonico.encode_wide().collect();
    let offset = offset_of!(FILE_RENAME_INFORMATION, FileName);
    let nombre_bytes = utf16
        .len()
        .checked_mul(size_of::<u16>())
        .ok_or(ErrorJournalAnclado::Io)?;
    let byte_len = offset
        .checked_add(nombre_bytes)
        .ok_or(ErrorJournalAnclado::Io)?;
    let nombre_bytes_u32 = u32::try_from(nombre_bytes).map_err(|_| ErrorJournalAnclado::Io)?;
    let mut storage = vec![0usize; byte_len.div_ceil(size_of::<usize>())];
    let informacion = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    // SAFETY: storage is usize-aligned and large enough for the fixed header
    // and UTF-16 component. No terminating NUL is part of FileNameLength.
    unsafe {
        (*informacion).Anonymous.ReplaceIfExists = false;
        (*informacion).RootDirectory = parent.as_raw_handle() as HANDLE;
        (*informacion).FileNameLength = nombre_bytes_u32;
        std::ptr::copy_nonoverlapping(
            utf16.as_ptr(),
            storage.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
            utf16.len(),
        );
    }
    // SAFETY: the source was opened relative/no-follow with DELETE access;
    // the retained parent anchors a no-replace rename on the local filesystem.
    let mut estado_io = IO_STATUS_BLOCK::default();
    let estado = unsafe {
        NtSetInformationFile(
            handle_fuente.as_raw_handle() as HANDLE,
            &mut estado_io,
            informacion.cast(),
            u32::try_from(byte_len).map_err(|_| ErrorJournalAnclado::Io)?,
            FileRenameInformation,
        )
    };
    if estado >= 0 {
        return Ok(());
    }
    if estado == 0xC0000035u32 as i32 {
        return Err(ErrorJournalAnclado::CanonicalConflict);
    }
    Err(ErrorJournalAnclado::Io)
}

#[cfg(windows)]
fn quitar_temporal(
    _parent: &File,
    _nombre: &OsStr,
    archivo: &File,
) -> Result<(), ErrorJournalAnclado> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Wdk::Storage::FileSystem::{
        FILE_DISPOSITION_INFORMATION, FileDispositionInformation, NtSetInformationFile,
    };
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    let mut disposicion = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
    // SAFETY: `archivo` is the no-follow handle re-opened and identity-checked
    // by the caller with DELETE access; the aligned struct matches the class.
    let mut estado_io = IO_STATUS_BLOCK::default();
    let estado = unsafe {
        NtSetInformationFile(
            archivo.as_raw_handle() as HANDLE,
            &mut estado_io,
            (&mut disposicion as *mut FILE_DISPOSITION_INFORMATION).cast(),
            size_of::<FILE_DISPOSITION_INFORMATION>() as u32,
            FileDispositionInformation,
        )
    };
    if estado >= 0 {
        Ok(())
    } else {
        Err(ErrorJournalAnclado::Io)
    }
}

#[cfg(not(any(unix, windows)))]
fn publicar_plataforma(
    _parent: &File,
    _fuente: &OsStr,
    _canonico: &OsStr,
    _handle_fuente: &File,
) -> Result<(), ErrorJournalAnclado> {
    Err(ErrorJournalAnclado::Io)
}

#[cfg(test)]
#[path = "publicar_tests.rs"]
mod tests;

#[cfg(not(any(unix, windows)))]
fn quitar_temporal(
    _parent: &File,
    _nombre: &OsStr,
    _archivo: &File,
) -> Result<(), ErrorJournalAnclado> {
    Err(ErrorJournalAnclado::Io)
}
