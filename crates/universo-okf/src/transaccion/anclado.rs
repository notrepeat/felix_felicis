//! Acceso relativo a handles para raíces transaccionales.
//!
//! `fs_at` 0.2.1 (Apache-2.0) provee las operaciones `*at` de Unix y su
//! equivalente con `NtCreateFile` en Windows. Este módulo valida los handles
//! devueltos; no expone rename, unlink ni cleanup.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io;
#[cfg(windows)]
use std::path::PathBuf;
use std::path::{Component, Path};

use fs_at::OpenOptions as OpenOptionsAt;

use super::raiz::{UbicacionRaiz, validar_basename_prospectivo};

mod operaciones;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorDirectorioAnclado {
    Io,
    ComponenteInvalido,
    EnlaceNoPermitido,
    TipoNoAdmitido,
    RaizAusente,
    EntradaAusente,
}

/// Identidad obtenida de metadata de un handle abierto, no de una ruta.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdentidadDirectorio {
    #[cfg(unix)]
    dispositivo: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volumen: u64,
    #[cfg(windows)]
    id_archivo: [u8; 16],
}

impl fmt::Debug for IdentidadDirectorio {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IdentidadDirectorio([redacted])")
    }
}

/// Mantiene abiertos el parent canónico y, si existe, su directorio raíz hijo.
///
/// Cuando recovery deja la raíz ausente, el handle de parent se conserva y
/// `abrir_raiz` vuelve a resolver el basename desde ese handle.
pub(crate) struct DirectorioAnclado {
    parent: File,
    basename: OsString,
    raiz: Option<File>,
    identidad_parent: IdentidadDirectorio,
    identidad_raiz: Option<IdentidadDirectorio>,
}

/// Entrada de lectura clasificada desde el handle devuelto por fs_at.
pub(crate) enum EntradaLecturaAnclada {
    Directorio(DirectorioAnclado),
    Archivo(File),
}

impl DirectorioAnclado {
    pub(crate) fn desde_ubicacion(
        ubicacion: &UbicacionRaiz,
    ) -> Result<Self, ErrorDirectorioAnclado> {
        let ruta = ubicacion.ruta();
        let basename = ruta
            .file_name()
            .ok_or(ErrorDirectorioAnclado::ComponenteInvalido)?;
        validar_componente(basename)?;
        let parent_path = ruta
            .parent()
            .ok_or(ErrorDirectorioAnclado::TipoNoAdmitido)?;
        let parent = abrir_directorio_absoluto(parent_path)?;
        let identidad_parent = metadata_directorio(&parent)?;
        let raiz = match abrir_directorio_hijo(&parent, basename) {
            Ok(directorio) => Some(directorio),
            Err(ErrorDirectorioAnclado::EntradaAusente) => None,
            Err(error) => return Err(error),
        };
        let identidad_raiz = raiz.as_ref().map(metadata_directorio).transpose()?;

        Ok(Self {
            parent,
            basename: basename.to_os_string(),
            raiz,
            identidad_parent,
            identidad_raiz,
        })
    }

    pub(crate) fn identidad_parent(&self) -> IdentidadDirectorio {
        self.identidad_parent
    }

    pub(crate) fn identidad_raiz(&self) -> Option<IdentidadDirectorio> {
        self.identidad_raiz
    }

    /// Duplica los handles sin resolver nuevamente la ubicación por ruta.
    pub(crate) fn clonar(&self) -> Result<Self, ErrorDirectorioAnclado> {
        Ok(Self {
            parent: clonar_handle(&self.parent)?,
            basename: self.basename.clone(),
            raiz: self.raiz.as_ref().map(clonar_handle).transpose()?,
            identidad_parent: self.identidad_parent,
            identidad_raiz: self.identidad_raiz,
        })
    }

    /// Reabre la raíz después de recovery usando el parent ya anclado.
    pub(crate) fn abrir_raiz(&self) -> Result<Self, ErrorDirectorioAnclado> {
        let raiz = abrir_directorio_hijo(&self.parent, &self.basename)?;
        let identidad_raiz = metadata_directorio(&raiz)?;
        Ok(Self {
            parent: clonar_handle(&self.parent)?,
            basename: self.basename.clone(),
            raiz: Some(raiz),
            identidad_parent: self.identidad_parent,
            identidad_raiz: Some(identidad_raiz),
        })
    }
}

fn validar_componente(nombre: &OsStr) -> Result<&OsStr, ErrorDirectorioAnclado> {
    validar_basename_prospectivo(nombre).map_err(|_| ErrorDirectorioAnclado::ComponenteInvalido)?;
    Ok(nombre)
}

fn abrir_directorio_hijo(parent: &File, nombre: &OsStr) -> Result<File, ErrorDirectorioAnclado> {
    let opciones = opciones_directorio();
    abrir_directorio_hijo_con_opciones(parent, nombre, &opciones)
}

fn abrir_directorio_hijo_lectura(
    parent: &File,
    nombre: &OsStr,
) -> Result<File, ErrorDirectorioAnclado> {
    let mut opciones = OpenOptionsAt::default();
    opciones.read(true).follow(false);
    abrir_directorio_hijo_con_opciones(parent, nombre, &opciones)
}

fn abrir_directorio_hijo_con_opciones(
    parent: &File,
    nombre: &OsStr,
    opciones: &OpenOptionsAt,
) -> Result<File, ErrorDirectorioAnclado> {
    let directorio = opciones
        .open_dir_at(parent, Path::new(nombre))
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ErrorDirectorioAnclado::EntradaAusente
            } else {
                mapear_io(error)
            }
        })?;
    metadata_directorio(&directorio)?;
    Ok(directorio)
}

fn opciones_directorio() -> OpenOptionsAt {
    let mut opciones = OpenOptionsAt::default();
    opciones.read(true).follow(false);
    #[cfg(windows)]
    {
        use fs_at::os::windows::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_TRAVERSE,
        };
        opciones.desired_access(FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_TRAVERSE);
    }
    opciones
}

fn abrir_directorio_absoluto(ruta: &Path) -> Result<File, ErrorDirectorioAnclado> {
    #[cfg(unix)]
    let mut actual = abrir_directorio_por_ruta(Path::new("/"))?;

    #[cfg(windows)]
    let mut actual = {
        let mut componentes = ruta.components();
        let prefijo = match componentes.next() {
            Some(Component::Prefix(prefijo)) => prefijo,
            _ => return Err(ErrorDirectorioAnclado::TipoNoAdmitido),
        };
        if !matches!(
            prefijo.kind(),
            std::path::Prefix::Disk(_)
                | std::path::Prefix::VerbatimDisk(_)
                | std::path::Prefix::UNC(_, _)
                | std::path::Prefix::VerbatimUNC(_, _)
        ) {
            return Err(ErrorDirectorioAnclado::TipoNoAdmitido);
        }
        if !matches!(componentes.next(), Some(Component::RootDir)) {
            return Err(ErrorDirectorioAnclado::TipoNoAdmitido);
        }
        let mut volumen = PathBuf::from(prefijo.as_os_str());
        volumen.push("\\");
        abrir_directorio_por_ruta(&volumen)?
    };

    for componente in ruta.components() {
        match componente {
            Component::Prefix(_) | Component::RootDir => continue,
            Component::Normal(nombre) => {
                actual = abrir_directorio_hijo_lectura(&actual, nombre)?;
            }
            Component::CurDir => return Err(ErrorDirectorioAnclado::TipoNoAdmitido),
            Component::ParentDir => return Err(ErrorDirectorioAnclado::TipoNoAdmitido),
        }
    }
    Ok(actual)
}

fn abrir_directorio_por_ruta(ruta: &Path) -> Result<File, ErrorDirectorioAnclado> {
    let mut opciones = OpenOptions::new();
    opciones.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opciones.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        opciones.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let directorio = opciones.open(ruta).map_err(mapear_io)?;
    metadata_directorio(&directorio)?;
    Ok(directorio)
}

fn clonar_handle(handle: &File) -> Result<File, ErrorDirectorioAnclado> {
    handle.try_clone().map_err(mapear_io)
}

fn metadata_directorio(handle: &File) -> Result<IdentidadDirectorio, ErrorDirectorioAnclado> {
    let metadata = handle.metadata().map_err(mapear_io)?;
    validar_metadata_directorio_por_metadata(&metadata)?;
    identidad_desde_metadata(handle, &metadata)
}

fn validar_metadata_directorio_por_metadata(
    metadata: &std::fs::Metadata,
) -> Result<(), ErrorDirectorioAnclado> {
    if metadata.file_type().is_symlink() || metadata_es_reparse(metadata) {
        return Err(ErrorDirectorioAnclado::EnlaceNoPermitido);
    }
    if !metadata.is_dir() {
        return Err(ErrorDirectorioAnclado::TipoNoAdmitido);
    }
    Ok(())
}

#[cfg(windows)]
fn metadata_es_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_es_reparse(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn identidad_desde_metadata(
    handle: &File,
    _metadata: &std::fs::Metadata,
) -> Result<IdentidadDirectorio, ErrorDirectorioAnclado> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(IdentidadDirectorio {
            dispositivo: _metadata.dev(),
            inode: _metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        identidad_windows(handle).map_err(mapear_io)
    }
}

#[cfg(windows)]
fn identidad_windows(handle: &File) -> io::Result<IdentidadDirectorio> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };

    let mut informacion = FILE_ID_INFO::default();
    // SAFETY: `handle` stays open for this synchronous query, and `informacion`
    // is an aligned, writable buffer sized for FILE_ID_INFO as required by the API.
    let exito = unsafe {
        GetFileInformationByHandleEx(
            handle.as_raw_handle() as HANDLE,
            FileIdInfo,
            (&mut informacion as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if exito == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(IdentidadDirectorio {
        volumen: informacion.VolumeSerialNumber,
        id_archivo: informacion.FileId.Identifier,
    })
}

#[cfg(windows)]
fn filesystem_sin_file_id(error: &io::Error) -> bool {
    // These Win32 errors explicitly indicate that this filesystem/driver does
    // not implement the FileIdInfo query. Other failures must remain visible.
    matches!(error.raw_os_error(), Some(1 | 50))
}

fn mapear_io(error: io::Error) -> ErrorDirectorioAnclado {
    match error.kind() {
        io::ErrorKind::PermissionDenied => ErrorDirectorioAnclado::Io,
        io::ErrorKind::InvalidData | io::ErrorKind::NotADirectory => {
            ErrorDirectorioAnclado::TipoNoAdmitido
        }
        _ => ErrorDirectorioAnclado::Io,
    }
}

#[cfg(test)]
mod tests;
