use super::*;
use std::io::Read;

impl JournalTemporalAnclado<'_> {
    /// Retira el journal canónico solo si el nombre interno vuelve a identificar
    /// el archivo y el registro v1 esperado. En POSIX, `unlinkat` sigue siendo
    /// una operación por nombre: el lock coordina escritores Felix cooperantes,
    /// pero queda una carrera residual frente a mutaciones externas del namespace.
    pub(crate) fn eliminar_canonico_si_coincide(
        &self,
        identidad_esperada: IdentidadArchivoJournal,
        journal_esperado: &Journal,
    ) -> ResultadoEliminacionJournal {
        self.eliminar_canonico_si_coincide_con_scope(
            identidad_esperada,
            journal_esperado,
            &mut super::scope_filesystem,
            eliminar_entrada_plataforma,
        )
    }

    fn eliminar_canonico_si_coincide_con_scope<S, D>(
        &self,
        identidad_esperada: IdentidadArchivoJournal,
        journal_esperado: &Journal,
        scope_probe: &mut S,
        eliminar: D,
    ) -> ResultadoEliminacionJournal
    where
        S: FnMut(&File) -> Result<super::ScopeFilesystem, super::ErrorDirectorioAnclado>,
        D: FnOnce(&File, &std::ffi::OsStr, &File) -> Result<(), ErrorJournalAnclado>,
    {
        if self.raiz_remota {
            return ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::RemoteUnsupported);
        }
        if let Err(error) = self.validar_parent_con_scope(scope_probe) {
            return ResultadoEliminacionJournal::Conflicto(error);
        }
        let Some(parent_esperado) = self.identidad_parent.journal_identity() else {
            return ResultadoEliminacionJournal::Conflicto(
                ErrorJournalAnclado::ParentIdentityUnavailable,
            );
        };
        if journal_esperado.parent() != &parent_esperado {
            return ResultadoEliminacionJournal::Conflicto(
                ErrorJournalAnclado::ParentIdentityMismatch,
            );
        }
        if let Err(error) = journal_esperado.encode() {
            return ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::InvalidJournal(
                error,
            ));
        }

        let mut archivo =
            match self.abrir_canonico_para_eliminar_con_scope(identidad_esperada, scope_probe) {
                Ok(archivo) => archivo,
                Err(error) => return resultado_error_previo(error),
            };
        let identidad = match identidad_archivo(&archivo) {
            Ok(identidad) => identidad,
            Err(error) => return resultado_error_previo(error),
        };
        if identidad != identidad_esperada {
            return ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::IdentityMismatch);
        }
        if let Err(error) = self.validar_scope_source_con(&archivo, scope_probe) {
            return ResultadoEliminacionJournal::Conflicto(error);
        }

        let mut bytes = Vec::with_capacity(MAX_JOURNAL_BYTES + 1);
        if Read::by_ref(&mut archivo)
            .take((MAX_JOURNAL_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .is_err()
        {
            return ResultadoEliminacionJournal::Indeterminado(ErrorJournalAnclado::Io);
        }
        if bytes.len() > MAX_JOURNAL_BYTES {
            return ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::TooLarge);
        }
        let journal_actual = match self.cargar_journal_leido(identidad, &bytes) {
            Ok((_, journal)) => journal,
            Err(error) => return resultado_error_previo(error),
        };
        if journal_actual != *journal_esperado {
            return ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::JournalMismatch);
        }
        if let Err(error) = self.validar_parent_con_scope(scope_probe) {
            return ResultadoEliminacionJournal::Conflicto(error);
        }

        let intento = eliminar(&self.parent, &self.nombre_journal, &archivo);
        drop(archivo);
        self.confirmar_eliminacion_con_scope(
            intento,
            identidad_esperada,
            journal_esperado,
            scope_probe,
        )
    }

    fn abrir_canonico_para_eliminar_con_scope<S>(
        &self,
        identidad_esperada: IdentidadArchivoJournal,
        scope_probe: &mut S,
    ) -> Result<File, ErrorJournalAnclado>
    where
        S: FnMut(&File) -> Result<super::ScopeFilesystem, super::ErrorDirectorioAnclado>,
    {
        self.validar_parent_con_scope(scope_probe)?;

        #[cfg(windows)]
        {
            use fs_at::OpenOptions as OpenOptionsAt;
            use fs_at::os::windows::OpenOptionsExt;
            use std::path::Path;
            use windows_sys::Win32::Storage::FileSystem::{
                DELETE, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
            };

            let sonda = abrir_entrada_relativa(
                &self.parent,
                &self.nombre_journal,
                TipoEntradaAnclada::Archivo,
            )
            .map_err(mapear_error_apertura)?;
            validar_archivo_regular(&sonda)?;
            if identidad_archivo(&sonda)? != identidad_esperada {
                return Err(ErrorJournalAnclado::IdentityMismatch);
            }
            drop(sonda);

            let mut opciones = OpenOptionsAt::default();
            opciones
                .desired_access(FILE_READ_DATA | FILE_READ_ATTRIBUTES | DELETE)
                .follow(false);
            let archivo = opciones
                .open_at(&self.parent, Path::new(&self.nombre_journal))
                .map_err(mapear_error_apertura_windows)?;
            validar_archivo_regular(&archivo)?;
            if identidad_archivo(&archivo)? != identidad_esperada {
                return Err(ErrorJournalAnclado::IdentityMismatch);
            }
            Ok(archivo)
        }

        #[cfg(unix)]
        {
            let archivo = abrir_entrada_relativa(
                &self.parent,
                &self.nombre_journal,
                TipoEntradaAnclada::Archivo,
            )
            .map_err(mapear_error_apertura)?;
            validar_archivo_regular(&archivo)?;
            if identidad_archivo(&archivo)? != identidad_esperada {
                return Err(ErrorJournalAnclado::IdentityMismatch);
            }
            Ok(archivo)
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = identidad_esperada;
            Err(ErrorJournalAnclado::Io)
        }
    }

    fn confirmar_eliminacion_con_scope<S>(
        &self,
        intento: Result<(), ErrorJournalAnclado>,
        identidad_esperada: IdentidadArchivoJournal,
        journal_esperado: &Journal,
        scope_probe: &mut S,
    ) -> ResultadoEliminacionJournal
    where
        S: FnMut(&File) -> Result<super::ScopeFilesystem, super::ErrorDirectorioAnclado>,
    {
        match self.cargar_canonico_con_scope(scope_probe) {
            Ok(None) => match intento {
                Ok(()) => ResultadoEliminacionJournal::Eliminado,
                Err(_) => ResultadoEliminacionJournal::Ausente,
            },
            Ok(Some((identidad, _))) if identidad != identidad_esperada => {
                ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::IdentityMismatch)
            }
            Ok(Some((_, journal))) if journal != *journal_esperado => {
                ResultadoEliminacionJournal::Conflicto(ErrorJournalAnclado::JournalMismatch)
            }
            Ok(Some(_)) => ResultadoEliminacionJournal::Indeterminado(
                intento.err().unwrap_or(ErrorJournalAnclado::Io),
            ),
            Err(error) => resultado_error_previo(error),
        }
    }
}

fn resultado_error_previo(error: ErrorJournalAnclado) -> ResultadoEliminacionJournal {
    match error {
        ErrorJournalAnclado::SourceMissing => ResultadoEliminacionJournal::Ausente,
        ErrorJournalAnclado::Io => ResultadoEliminacionJournal::Indeterminado(error),
        otro => ResultadoEliminacionJournal::Conflicto(otro),
    }
}

fn mapear_error_apertura(error: ErrorDirectorioAnclado) -> ErrorJournalAnclado {
    match error {
        ErrorDirectorioAnclado::EntradaAusente => ErrorJournalAnclado::SourceMissing,
        otro => mapear_error_directorio(otro),
    }
}

#[cfg(windows)]
fn mapear_error_apertura_windows(error: std::io::Error) -> ErrorJournalAnclado {
    use windows_sys::Win32::Foundation::{
        ERROR_CANT_RESOLVE_FILENAME, ERROR_DIRECTORY, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND,
    };

    match error.raw_os_error().map(|codigo| codigo as u32) {
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
fn eliminar_entrada_plataforma(
    parent: &File,
    nombre: &std::ffi::OsStr,
    _archivo: &File,
) -> Result<(), ErrorJournalAnclado> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let nombre = CString::new(nombre.as_bytes()).map_err(|_| ErrorJournalAnclado::Io)?;
    // SAFETY: the validated canonical component is relative to the retained
    // parent. Its identity and record were checked through a no-follow handle;
    // only cooperative writers are serialized, so external renames can still
    // race this name-based unlinkat.
    let resultado = unsafe { libc::unlinkat(parent.as_raw_fd(), nombre.as_ptr(), 0) };
    if resultado == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENOENT) => Err(ErrorJournalAnclado::SourceMissing),
        Some(libc::ELOOP) => Err(ErrorJournalAnclado::ReparseNotAllowed),
        Some(libc::EISDIR) | Some(libc::ENOTDIR) => Err(ErrorJournalAnclado::RegularFileRequired),
        _ => Err(ErrorJournalAnclado::Io),
    }
}

#[cfg(windows)]
fn eliminar_entrada_plataforma(
    _parent: &File,
    _nombre: &std::ffi::OsStr,
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
    // SAFETY: `archivo` is the no-follow handle re-opened with DELETE access
    // and checked for its expected identity and canonical journal contents.
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
fn eliminar_entrada_plataforma(
    _parent: &File,
    _nombre: &std::ffi::OsStr,
    _archivo: &File,
) -> Result<(), ErrorJournalAnclado> {
    Err(ErrorJournalAnclado::Io)
}

#[cfg(test)]
#[path = "eliminar_tests.rs"]
mod tests;
