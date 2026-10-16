//! Publicación no-replace del contexto y confirmación desde el parent retenido.

use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::sync::Arc;

use super::super::archivo_temporal::DatosArchivoTemporal;
use super::super::{IdentidadDirectorio, ScopeFilesystem};
use super::verificacion::ContextoTemporalVerificado;
use super::{ContextoAdmisionAnclado, ErrorContextoAdmisionAnclado};

/// Recibo de storage confirmado, sin borrow del lock ni autoridad de recovery.
#[must_use = "el recibo solo vincula datos al owner que retuvo el lock"]
pub(super) struct ReciboContextoPublicado {
    _autoridad: Arc<()>,
    identidad_parent: IdentidadDirectorio,
    scope_parent: ScopeFilesystem,
    datos: DatosArchivoTemporal,
}

impl fmt::Debug for ReciboContextoPublicado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ReciboContextoPublicado { .. }")
    }
}

impl<'lock> ContextoTemporalVerificado<'lock> {
    /// Renombra una sola vez y confirma los nombres desde el parent retenido.
    ///
    /// Un error después del intento no demuestra que el syscall no tuvo efecto.
    /// El método nunca reintenta ni limpia; conserva el estado observado para
    /// que el caller lo reconcilie bajo el lock.
    pub(super) fn publicar(self) -> Result<ReciboContextoPublicado, ErrorContextoAdmisionAnclado> {
        self.publicar_con(renombrar_no_reemplazar)
    }

    pub(super) fn publicar_con<R>(
        self,
        renombrar: R,
    ) -> Result<ReciboContextoPublicado, ErrorContextoAdmisionAnclado>
    where
        R: FnOnce(&File, &File, &OsStr, &OsStr) -> Result<(), ()>,
    {
        let Self {
            _lease,
            archivo,
            nombre_temporal,
            datos,
        } = self;
        _lease.revalidar_parent_y_lock()?;

        // Reopening proves the source basename still names the verified ID.
        let fuente = super::verificacion::verificar_archivo(&_lease, &nombre_temporal, &datos)?;
        drop(archivo);

        let _resultado_syscall =
            renombrar(&_lease.parent, &fuente, &nombre_temporal, &_lease.nombre);
        drop(fuente);
        if !confirmar_publicacion(&_lease, &nombre_temporal, &datos) {
            return Err(ErrorContextoAdmisionAnclado::PublicationUnverified);
        }

        Ok(ReciboContextoPublicado {
            _autoridad: Arc::clone(&_lease._autoridad),
            identidad_parent: _lease.identidad_parent,
            scope_parent: _lease.scope_parent,
            datos,
        })
    }
}

pub(super) fn renombrar_no_reemplazar(
    parent: &File,
    _fuente: &File,
    _nombre_temporal: &OsStr,
    nombre_canonico: &OsStr,
) -> Result<(), ()> {
    #[cfg(windows)]
    {
        super::super::renombrar_noreplace::renombrar_windows(parent, _fuente, nombre_canonico)
            .map_err(|_| ())
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let nombre_temporal = CString::new(_nombre_temporal.as_bytes()).map_err(|_| ())?;
        let nombre_canonico = CString::new(nombre_canonico.as_bytes()).map_err(|_| ())?;
        super::super::renombrar_noreplace::renombrar_unix(
            parent,
            &nombre_temporal,
            &nombre_canonico,
        )
        .map_err(|_| ())
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
    {
        let _ = (parent, _fuente, _nombre_temporal, nombre_canonico);
        Err(())
    }
}

fn confirmar_publicacion(
    lease: &ContextoAdmisionAnclado<'_>,
    nombre_temporal: &OsStr,
    datos: &DatosArchivoTemporal,
) -> bool {
    if lease.revalidar_parent_y_lock().is_err() {
        return false;
    }
    if super::verificacion::verificar_archivo(lease, &lease.nombre, datos).is_err() {
        return false;
    }
    temporal_ausente(&lease.parent, nombre_temporal)
}

#[cfg(windows)]
fn temporal_ausente(parent: &File, nombre: &OsStr) -> bool {
    use fs_at::OpenOptions as OpenOptionsAt;
    use fs_at::os::windows::OpenOptionsExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;

    let mut opciones = OpenOptionsAt::default();
    opciones.desired_access(FILE_READ_ATTRIBUTES).follow(false);
    matches!(
        opciones.open_at(parent, Path::new(nombre)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

#[cfg(not(windows))]
fn temporal_ausente(parent: &File, nombre: &OsStr) -> bool {
    matches!(
        super::abrir_entrada_relativa(parent, nombre, super::TipoEntradaAnclada::Archivo),
        Err(super::ErrorDirectorioAnclado::EntradaAusente)
    )
}
