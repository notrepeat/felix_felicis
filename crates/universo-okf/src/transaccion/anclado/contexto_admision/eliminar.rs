//! Retiro único del Context canónico con verificación raw y confirmación.

use std::ffi::OsStr;
use std::fs::File;

use super::super::journal_temporal;
use super::{
    ContextoAdmisionAnclado, ErrorContextoAdmisionAnclado, EvidenciaLecturaContexto,
    ResultadoEliminacionContextoAnclado,
};

impl ContextoAdmisionAnclado<'_> {
    /// Intenta una vez retirar el nombre canónico y observa su estado después.
    ///
    /// El resultado no atribuye causalidad frente a mutaciones externas de
    /// bytes o namespace; el handle y su digest no congelan bytes observados.
    /// No retira el Journal ni el intent.
    #[allow(dead_code)] // El consumidor privado B→C aún no está integrado.
    pub(in crate::transaccion::anclado) fn eliminar_canonico(
        &self,
        evidencia: &EvidenciaLecturaContexto,
    ) -> ResultadoEliminacionContextoAnclado {
        self.eliminar_canonico_con(evidencia, eliminar_nombre_plataforma)
    }

    pub(super) fn eliminar_canonico_con<D>(
        &self,
        evidencia: &EvidenciaLecturaContexto,
        eliminar: D,
    ) -> ResultadoEliminacionContextoAnclado
    where
        D: FnOnce(&File, &OsStr, &File) -> Result<(), ErrorContextoAdmisionAnclado>,
    {
        if journal_temporal::raiz_es_remota(self._lock.raiz_canonica()) {
            return ResultadoEliminacionContextoAnclado::Conflicto(
                ErrorContextoAdmisionAnclado::RemoteUnsupported,
            );
        }
        if let Err(error) = self.revalidar_parent_y_lock() {
            return ResultadoEliminacionContextoAnclado::Conflicto(error);
        }

        let archivo = match super::verificacion::verificar_canonico_opcional(self, evidencia) {
            Ok(Some(archivo)) => archivo,
            Ok(None) => {
                return match self.revalidar_parent_y_lock() {
                    Ok(()) => ResultadoEliminacionContextoAnclado::Ausente,
                    Err(error) => ResultadoEliminacionContextoAnclado::Conflicto(error),
                };
            }
            Err(error) => return clasificar_error_previo(error),
        };

        if let Err(error) = self.revalidar_parent_y_lock() {
            return ResultadoEliminacionContextoAnclado::Conflicto(error);
        }
        let intento = eliminar(&self.parent, &self.nombre, &archivo);
        drop(archivo);

        if let Err(error) = self.revalidar_parent_y_lock() {
            return ResultadoEliminacionContextoAnclado::Indeterminado(error);
        }
        let confirmacion = match super::verificacion::verificar_canonico_opcional(self, evidencia) {
            Ok(None) => Ok(false),
            Ok(Some(archivo)) => {
                drop(archivo);
                Ok(true)
            }
            Err(error) => Err(error),
        };
        if let Err(error) = self.revalidar_parent_y_lock() {
            return ResultadoEliminacionContextoAnclado::Indeterminado(error);
        }

        match confirmacion {
            Ok(false) if intento.is_ok() => ResultadoEliminacionContextoAnclado::Eliminado,
            Ok(false) => ResultadoEliminacionContextoAnclado::Ausente,
            Ok(true) => ResultadoEliminacionContextoAnclado::Indeterminado(
                ErrorContextoAdmisionAnclado::RemovalUnverified,
            ),
            Err(error) => clasificar_error_confirmacion(error),
        }
    }
}

fn clasificar_error_previo(
    error: ErrorContextoAdmisionAnclado,
) -> ResultadoEliminacionContextoAnclado {
    if error == ErrorContextoAdmisionAnclado::Io {
        ResultadoEliminacionContextoAnclado::Indeterminado(error)
    } else {
        ResultadoEliminacionContextoAnclado::Conflicto(error)
    }
}

fn clasificar_error_confirmacion(
    error: ErrorContextoAdmisionAnclado,
) -> ResultadoEliminacionContextoAnclado {
    if error == ErrorContextoAdmisionAnclado::Io {
        ResultadoEliminacionContextoAnclado::Indeterminado(error)
    } else {
        ResultadoEliminacionContextoAnclado::Conflicto(error)
    }
}

#[cfg(unix)]
fn eliminar_nombre_plataforma(
    parent: &File,
    nombre: &OsStr,
    _archivo: &File,
) -> Result<(), ErrorContextoAdmisionAnclado> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let nombre = CString::new(nombre.as_bytes()).map_err(|_| ErrorContextoAdmisionAnclado::Io)?;
    // SAFETY: the validated component is relative to the retained parent; the
    // no-follow source handle was checked, but external name races can remain.
    let resultado = unsafe { libc::unlinkat(parent.as_raw_fd(), nombre.as_ptr(), 0) };
    if resultado == 0 {
        Ok(())
    } else {
        Err(ErrorContextoAdmisionAnclado::Io)
    }
}

#[cfg(windows)]
fn eliminar_nombre_plataforma(
    _parent: &File,
    _nombre: &OsStr,
    archivo: &File,
) -> Result<(), ErrorContextoAdmisionAnclado> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Wdk::Storage::FileSystem::{
        FILE_DISPOSITION_INFORMATION, FileDispositionInformation, NtSetInformationFile,
    };
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    let mut disposicion = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
    // SAFETY: verification opened this no-follow File with DELETE access and
    // checked its native identity, scope, length and raw digest.
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
        Err(ErrorContextoAdmisionAnclado::Io)
    }
}

#[cfg(not(any(unix, windows)))]
fn eliminar_nombre_plataforma(
    _parent: &File,
    _nombre: &OsStr,
    _archivo: &File,
) -> Result<(), ErrorContextoAdmisionAnclado> {
    Err(ErrorContextoAdmisionAnclado::Io)
}
