//! Acceso relativo a handles para raíces transaccionales.
//!
//! `fs_at` 0.2.1 (Apache-2.0) provee las operaciones `*at` de Unix y su
//! equivalente con `NtCreateFile` en Windows. Este módulo valida los handles
//! devueltos y no expone rename ni cleanup genérico; `StageAnclado` posee la
//! única operación de borrado sibling explícita.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io;
#[cfg(windows)]
use std::path::PathBuf;
use std::path::{Component, Path};

use fs_at::OpenOptions as OpenOptionsAt;

use super::raiz::{UbicacionRaiz, validar_basename_prospectivo};

mod clasificar;
mod limpieza;
mod operaciones;

pub(super) use clasificar::{TipoEntradaAnclada, clasificar_entrada_sin_seguir};
pub(super) use limpieza::{ErrorLimpiezaAnclado, limpiar_stage_anclado};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorDirectorioAnclado {
    Io,
    ComponenteInvalido,
    EnlaceNoPermitido,
    TipoNoAdmitido,
    RaizAusente,
    EntradaAusente,
    EntradaExistente,
}

/// Conserva la capability del sibling si falla un paso posterior a mkdir.
pub(crate) struct ErrorCrearStageSibling {
    codigo: ErrorDirectorioAnclado,
    parcial: Option<Box<(DirectorioAnclado, OsString)>>,
}

impl ErrorCrearStageSibling {
    fn nuevo(codigo: ErrorDirectorioAnclado) -> Self {
        Self {
            codigo,
            parcial: None,
        }
    }

    fn con_parcial(
        codigo: ErrorDirectorioAnclado,
        directorio: DirectorioAnclado,
        componente: OsString,
    ) -> Self {
        Self {
            codigo,
            parcial: Some(Box::new((directorio, componente))),
        }
    }

    pub(crate) fn codigo(&self) -> ErrorDirectorioAnclado {
        self.codigo
    }

    pub(crate) fn tomar_parcial(&mut self) -> Option<(DirectorioAnclado, OsString)> {
        self.parcial.take().map(|parcial| *parcial)
    }
}

impl fmt::Debug for ErrorCrearStageSibling {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ErrorCrearStageSibling([redacted])")
    }
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
    permite_escritura: bool,
    puede_crear_stage_sibling: bool,
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
            permite_escritura: false,
            puede_crear_stage_sibling: true,
        })
    }

    pub(crate) fn identidad_parent(&self) -> IdentidadDirectorio {
        self.identidad_parent
    }

    pub(crate) fn identidad_raiz(&self) -> Option<IdentidadDirectorio> {
        self.identidad_raiz
    }

    /// Duplica los handles sin resolver nuevamente la ubicación por ruta.
    ///
    /// Los clones comparten cursor de enumeración de directorio en `fs_at`;
    /// inventarios basados en clones de la misma raíz deben ejecutarse en
    /// serie bajo el lock del escritor, no concurrentemente.
    pub(crate) fn clonar(&self) -> Result<Self, ErrorDirectorioAnclado> {
        Ok(Self {
            parent: clonar_handle(&self.parent)?,
            basename: self.basename.clone(),
            raiz: self.raiz.as_ref().map(clonar_handle).transpose()?,
            identidad_parent: self.identidad_parent,
            identidad_raiz: self.identidad_raiz,
            permite_escritura: self.permite_escritura,
            puede_crear_stage_sibling: self.puede_crear_stage_sibling,
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
            permite_escritura: false,
            puede_crear_stage_sibling: self.puede_crear_stage_sibling,
        })
    }

    /// Crea un sibling con nombre aleatorio relativo al parent ya anclado. El
    /// identificador solo admite bytes portables generados por CSPRNG. La
    /// creación y la apertura-capacidad posterior se ejecutan bajo
    /// `LockEscritura`; la identidad del handle abierto se compara con la del
    /// handle devuelto por `mkdir_at`. En Unix, `fs_at::mkdir_at` hace
    /// `mkdirat` y `openat` separados. El lock serializa escritores Felix
    /// cooperantes; no excluye procesos externos que modifiquen el parent.
    pub(crate) fn crear_stage_sibling(
        &self,
        stage_id: &str,
    ) -> Result<(Self, OsString), ErrorCrearStageSibling> {
        self.crear_stage_sibling_con(stage_id, abrir_directorio_hijo_stage)
    }

    pub(super) fn crear_stage_sibling_con<F>(
        &self,
        stage_id: &str,
        mut abrir: F,
    ) -> Result<(Self, OsString), ErrorCrearStageSibling>
    where
        F: FnMut(&File, &OsStr, &File) -> Result<File, ErrorDirectorioAnclado>,
    {
        if !self.puede_crear_stage_sibling {
            return Err(ErrorCrearStageSibling::nuevo(
                ErrorDirectorioAnclado::TipoNoAdmitido,
            ));
        }
        if stage_id.is_empty()
            || !stage_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(ErrorCrearStageSibling::nuevo(
                ErrorDirectorioAnclado::ComponenteInvalido,
            ));
        }
        if self.raiz.is_none() {
            return Err(ErrorCrearStageSibling::nuevo(
                ErrorDirectorioAnclado::RaizAusente,
            ));
        }

        let mut componente = OsString::from(".");
        componente.push(&self.basename);
        componente.push(".felix.stage.");
        componente.push(stage_id);
        validar_componente(&componente).map_err(ErrorCrearStageSibling::nuevo)?;

        let identidad_parent =
            metadata_directorio(&self.parent).map_err(ErrorCrearStageSibling::nuevo)?;
        if identidad_parent != self.identidad_parent {
            return Err(ErrorCrearStageSibling::nuevo(ErrorDirectorioAnclado::Io));
        }

        // Clona antes de mkdir para que no haya ninguna operación fallible de
        // ownership del parent después de crear el sibling.
        let parent_stage = clonar_handle(&self.parent).map_err(ErrorCrearStageSibling::nuevo)?;

        let mut opciones = OpenOptionsAt::default();
        opciones.create_new(true);
        let creado = opciones
            .mkdir_at(&self.parent, Path::new(&componente))
            .map_err(mapear_error_creacion)
            .map_err(ErrorCrearStageSibling::nuevo)?;
        let identidad_creado = match identidad_handle_creado(&creado) {
            Ok(identidad) => Some(identidad),
            Err(error) => {
                let parcial = Self {
                    parent: parent_stage,
                    basename: componente.clone(),
                    raiz: Some(creado),
                    identidad_parent,
                    identidad_raiz: None,
                    permite_escritura: false,
                    puede_crear_stage_sibling: false,
                };
                return Err(ErrorCrearStageSibling::con_parcial(
                    error, parcial, componente,
                ));
            }
        };
        // `mkdir_at` returns a handle without all directory-add rights needed
        // by nested stage writes. Open the same sibling relative to the held
        // parent and fail closed unless its identity matches the created fd.
        let directorio = match abrir(&self.parent, &componente, &creado) {
            Ok(directorio) => directorio,
            Err(error) => {
                let parcial = Self {
                    parent: parent_stage,
                    basename: componente.clone(),
                    raiz: Some(creado),
                    identidad_parent,
                    identidad_raiz: identidad_creado,
                    permite_escritura: false,
                    puede_crear_stage_sibling: false,
                };
                return Err(ErrorCrearStageSibling::con_parcial(
                    error, parcial, componente,
                ));
            }
        };
        let identidad_raiz = match metadata_directorio(&directorio) {
            Ok(identidad) if Some(identidad) == identidad_creado => identidad,
            Ok(_) => {
                let parcial = Self {
                    parent: parent_stage,
                    basename: componente.clone(),
                    raiz: Some(creado),
                    identidad_parent,
                    identidad_raiz: identidad_creado,
                    permite_escritura: false,
                    puede_crear_stage_sibling: false,
                };
                return Err(ErrorCrearStageSibling::con_parcial(
                    ErrorDirectorioAnclado::Io,
                    parcial,
                    componente,
                ));
            }
            Err(error) => {
                let parcial = Self {
                    parent: parent_stage,
                    basename: componente.clone(),
                    raiz: Some(creado),
                    identidad_parent,
                    identidad_raiz: identidad_creado,
                    permite_escritura: false,
                    puede_crear_stage_sibling: false,
                };
                return Err(ErrorCrearStageSibling::con_parcial(
                    error, parcial, componente,
                ));
            }
        };
        let stage = Self {
            parent: parent_stage,
            basename: componente.clone(),
            raiz: Some(directorio),
            identidad_parent,
            identidad_raiz: Some(identidad_raiz),
            permite_escritura: true,
            puede_crear_stage_sibling: false,
        };
        Ok((stage, componente))
    }

    pub(crate) fn componente(&self) -> &OsStr {
        &self.basename
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

pub(super) fn abrir_directorio_hijo_stage(
    parent: &File,
    nombre: &OsStr,
    creado: &File,
) -> Result<File, ErrorDirectorioAnclado> {
    let nombre = validar_componente(nombre)?;
    let identidad_creado = identidad_handle_creado(creado)?;
    let directorio = abrir_directorio_hijo_stage_existente(parent, nombre)?;
    let identidad_reabierto = metadata_directorio(&directorio)?;
    if identidad_reabierto != identidad_creado {
        return Err(ErrorDirectorioAnclado::Io);
    }
    Ok(directorio)
}

pub(super) fn abrir_directorio_hijo_stage_existente(
    parent: &File,
    nombre: &OsStr,
) -> Result<File, ErrorDirectorioAnclado> {
    let nombre = validar_componente(nombre)?;
    let directorio = opciones_directorio_stage()
        .open_dir_at(parent, Path::new(nombre))
        .map_err(mapear_io)?;
    metadata_directorio(&directorio)?;
    Ok(directorio)
}

fn identidad_handle_creado(handle: &File) -> Result<IdentidadDirectorio, ErrorDirectorioAnclado> {
    #[cfg(unix)]
    {
        metadata_directorio(handle)
    }
    #[cfg(windows)]
    {
        // fs_at's atomic Windows mkdir handle omits FILE_READ_ATTRIBUTES, so
        // query the object ID directly before opening a write-capable handle.
        identidad_windows(handle).map_err(mapear_io)
    }
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
    opciones
}

fn opciones_directorio_stage() -> OpenOptionsAt {
    let mut opciones = opciones_directorio();
    #[cfg(windows)]
    {
        use fs_at::os::windows::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
            FILE_TRAVERSE,
        };
        opciones.desired_access(
            FILE_READ_ATTRIBUTES
                | FILE_LIST_DIRECTORY
                | FILE_TRAVERSE
                | FILE_ADD_FILE
                | FILE_ADD_SUBDIRECTORY,
        );
    }
    opciones
}

fn opciones_parent_stage() -> OpenOptionsAt {
    let mut opciones = opciones_directorio();
    #[cfg(windows)]
    {
        use fs_at::os::windows::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ADD_SUBDIRECTORY, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_TRAVERSE,
        };
        opciones.desired_access(
            FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_ADD_SUBDIRECTORY,
        );
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

    let mut nombres = Vec::new();
    for componente in ruta.components() {
        match componente {
            Component::Prefix(_) | Component::RootDir => continue,
            Component::Normal(nombre) => nombres.push(nombre.to_os_string()),
            Component::CurDir => return Err(ErrorDirectorioAnclado::TipoNoAdmitido),
            Component::ParentDir => return Err(ErrorDirectorioAnclado::TipoNoAdmitido),
        }
    }
    for (indice, nombre) in nombres.iter().enumerate() {
        let opciones = if indice + 1 == nombres.len() {
            opciones_parent_stage()
        } else {
            let mut opciones = OpenOptionsAt::default();
            opciones.read(true).follow(false);
            opciones
        };
        actual = abrir_directorio_hijo_con_opciones(&actual, nombre, &opciones)?;
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

pub(super) fn mapear_error_creacion(error: io::Error) -> ErrorDirectorioAnclado {
    if error.kind() == io::ErrorKind::AlreadyExists {
        ErrorDirectorioAnclado::EntradaExistente
    } else {
        mapear_io(error)
    }
}

#[cfg(test)]
mod tests;
