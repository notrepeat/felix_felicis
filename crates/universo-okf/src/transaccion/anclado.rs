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

mod abrir;
mod clasificar;
pub(crate) mod journal_temporal;
mod limpieza;
mod operaciones;
pub(crate) mod renombrar;

pub(super) use clasificar::{TipoEntradaAnclada, clasificar_entrada_sin_seguir};
pub(super) use limpieza::{
    ErrorLimpiezaAnclado, limpiar_stage_anclado, limpiar_stage_anclado_con_limites,
};
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

#[derive(Debug)]
pub(crate) enum ErrorLecturaNombres<E> {
    Directorio(ErrorDirectorioAnclado),
    Visitante(E),
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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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

impl IdentidadDirectorio {
    pub(crate) fn journal_identity(self) -> Option<super::journal::DirectoryIdentity> {
        #[cfg(unix)]
        {
            Some(super::journal::DirectoryIdentity::unix(
                self.dispositivo,
                self.inode,
            ))
        }
        #[cfg(windows)]
        {
            Some(super::journal::DirectoryIdentity::windows(
                self.volumen,
                file_id_hex(&self.id_archivo),
            ))
        }
        #[cfg(not(any(unix, windows)))]
        {
            None
        }
    }
}

#[cfg(any(windows, test))]
fn file_id_hex(bytes: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(HEX[(byte >> 4) as usize] as char);
        hex.push(HEX[(byte & 0x0f) as usize] as char);
    }
    hex
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

/// A read-only root capability that does not depend on filesystem file IDs.
/// Bundle loading only needs safe relative opens, not transaction identities.
pub(crate) struct DirectorioLecturaAnclado {
    raiz: File,
}

impl DirectorioLecturaAnclado {
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
        let parent = abrir_directorio_absoluto_lectura(parent_path)?;
        let raiz =
            abrir::abrir_entrada_relativa(&parent, basename, TipoEntradaAnclada::Directorio)?;
        Ok(Self { raiz })
    }

    /// Opens a canonical directory path one component at a time from the
    /// held root. Symlinks and reparse points are never followed.
    pub(crate) fn abrir_directorio_relativo(
        &self,
        relativa: &Path,
    ) -> Result<File, ErrorDirectorioAnclado> {
        let mut directorio = clonar_handle(&self.raiz)?;
        for componente in relativa.components() {
            let Component::Normal(componente) = componente else {
                return Err(ErrorDirectorioAnclado::ComponenteInvalido);
            };
            directorio = abrir::abrir_entrada_relativa(
                &directorio,
                componente,
                TipoEntradaAnclada::Directorio,
            )?;
        }
        Ok(directorio)
    }

    /// Opens one child directory from an already anchored parent handle.
    pub(crate) fn abrir_directorio_hijo(
        &self,
        padre: &File,
        nombre: &OsStr,
    ) -> Result<File, ErrorDirectorioAnclado> {
        abrir::abrir_entrada_relativa(padre, nombre, TipoEntradaAnclada::Directorio)
    }

    /// Enumerates a directory handle opened by this anchored capability.
    /// Directory identity is deliberately not queried, so unsupported file
    /// IDs on FAT/SMB do not prevent bundle loading. Each name is handed to
    /// the visitor immediately so its caller can reserve input budget before
    /// retaining it.
    pub(crate) fn nombres_directorio_abierto_con<E, F>(
        &self,
        directorio: &mut File,
        mut visitar: F,
    ) -> Result<(), ErrorLecturaNombres<E>>
    where
        F: FnMut(OsString) -> Result<(), E>,
    {
        metadata_directorio_validada(directorio).map_err(ErrorLecturaNombres::Directorio)?;
        let mut entradas = fs_at::read_dir(directorio)
            .map_err(mapear_io)
            .map_err(ErrorLecturaNombres::Directorio)?;
        for entrada in &mut entradas {
            let entrada = entrada
                .map_err(mapear_io)
                .map_err(ErrorLecturaNombres::Directorio)?;
            let nombre = entrada.name();
            if nombre == OsStr::new(".") || nombre == OsStr::new("..") {
                continue;
            }
            visitar(nombre.to_os_string()).map_err(ErrorLecturaNombres::Visitante)?;
        }
        Ok(())
    }

    /// Opens a file by resolving each component from the held root handle.
    /// Neither a symlink nor a reparse point can replace an ancestor during
    /// the walk, and file opens remain nonblocking for special files.
    pub(crate) fn abrir_archivo_regular_relativo(
        &self,
        relativa: &Path,
    ) -> Result<File, ErrorDirectorioAnclado> {
        let mut componentes = relativa.components().peekable();
        let mut directorio = clonar_handle(&self.raiz)?;
        while let Some(Component::Normal(componente)) = componentes.next() {
            let tipo = if componentes.peek().is_none() {
                TipoEntradaAnclada::Archivo
            } else {
                TipoEntradaAnclada::Directorio
            };
            let entrada = abrir::abrir_entrada_relativa(&directorio, componente, tipo)?;
            if tipo == TipoEntradaAnclada::Archivo {
                return Ok(entrada);
            }
            directorio = entrada;
        }
        Err(ErrorDirectorioAnclado::ComponenteInvalido)
    }
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
        let parent = abrir_directorio_absoluto(parent_path, true)?;
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

    /// Abre solo el parent de la ubicación con una capability estrictamente
    /// de lectura. No abre ni inspecciona la raíz; el observador de recovery
    /// resuelve cada artefacto desde este handle después de validar el journal.
    pub(crate) fn parent_solo_lectura_desde_ubicacion(
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
        let parent = abrir_directorio_absoluto(parent_path, false)?;
        let identidad_parent = metadata_directorio(&parent)?;

        Ok(Self {
            parent,
            basename: basename.to_os_string(),
            raiz: None,
            identidad_parent,
            identidad_raiz: None,
            permite_escritura: false,
            puede_crear_stage_sibling: false,
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
    let directorio = abrir::abrir_entrada_relativa(parent, nombre, TipoEntradaAnclada::Directorio)?;
    metadata_directorio(&directorio)?;
    Ok(directorio)
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
    let opciones = opciones_directorio();
    #[cfg(windows)]
    let opciones = {
        use fs_at::os::windows::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
            FILE_TRAVERSE,
        };
        let mut opciones = opciones;
        opciones.desired_access(
            FILE_READ_ATTRIBUTES
                | FILE_LIST_DIRECTORY
                | FILE_TRAVERSE
                | FILE_ADD_FILE
                | FILE_ADD_SUBDIRECTORY,
        );
        opciones
    };
    opciones
}

fn opciones_parent_stage() -> OpenOptionsAt {
    let opciones = opciones_directorio();
    #[cfg(windows)]
    let opciones = {
        use fs_at::os::windows::OpenOptionsExt;
        let mut opciones = opciones;
        opciones.desired_access(derechos_parent_transaccional());
        opciones
    };
    opciones
}

#[cfg(windows)]
fn derechos_parent_transaccional() -> u32 {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
        FILE_TRAVERSE,
    };

    FILE_READ_ATTRIBUTES
        | FILE_LIST_DIRECTORY
        | FILE_TRAVERSE
        | FILE_ADD_FILE
        | FILE_ADD_SUBDIRECTORY
}

fn abrir_directorio_absoluto(
    ruta: &Path,
    parent_transaccional: bool,
) -> Result<File, ErrorDirectorioAnclado> {
    #[cfg(windows)]
    // Si el parent es raíz de volumen/share y su ACL niega estos derechos, se
    // propaga el error; no se reintenta con una ruta sin anclar.
    let volumen = raiz_parent_windows(ruta)?;

    let mut nombres = Vec::new();
    for componente in ruta.components() {
        match componente {
            Component::Prefix(_) | Component::RootDir => continue,
            Component::Normal(nombre) => nombres.push(nombre.to_os_string()),
            Component::CurDir => return Err(ErrorDirectorioAnclado::TipoNoAdmitido),
            Component::ParentDir => return Err(ErrorDirectorioAnclado::TipoNoAdmitido),
        }
    }

    #[cfg(unix)]
    let mut actual = abrir_directorio_por_ruta(Path::new("/"), parent_transaccional)?;
    #[cfg(windows)]
    let mut actual =
        abrir_directorio_por_ruta(&volumen, parent_transaccional && nombres.is_empty())?;

    for (indice, nombre) in nombres.iter().enumerate() {
        #[cfg(unix)]
        {
            let _ = indice;
            let _ = parent_transaccional;
            // `fs_at::open_dir_at` does not guarantee O_NONBLOCK on Unix. A
            // path component replaced with a FIFO must fail without waiting
            // for a writer, so use the shared openat primitive with the full
            // no-follow, nonblocking directory flags.
            actual =
                abrir::abrir_entrada_relativa(&actual, nombre, TipoEntradaAnclada::Directorio)?;
            metadata_directorio(&actual)?;
        }
        #[cfg(not(unix))]
        {
            let opciones = if parent_transaccional && indice + 1 == nombres.len() {
                opciones_parent_stage()
            } else {
                opciones_directorio()
            };
            // Windows uses fs_at's handle-relative open with follow(false),
            // which opens reparse points themselves for rejection by the
            // metadata check and never follows a substituted junction.
            actual = abrir_directorio_hijo_con_opciones(&actual, nombre, &opciones)?;
        }
    }
    Ok(actual)
}

fn abrir_directorio_absoluto_lectura(ruta: &Path) -> Result<File, ErrorDirectorioAnclado> {
    #[cfg(windows)]
    let volumen = raiz_parent_windows(ruta)?;

    #[cfg(unix)]
    let mut actual = abrir_directorio_por_ruta(Path::new("/"), false)?;
    #[cfg(windows)]
    let mut actual = abrir_directorio_por_ruta(&volumen, false)?;

    for componente in ruta.components() {
        match componente {
            Component::Prefix(_) | Component::RootDir => continue,
            Component::Normal(nombre) => {
                actual =
                    abrir::abrir_entrada_relativa(&actual, nombre, TipoEntradaAnclada::Directorio)?;
            }
            Component::CurDir | Component::ParentDir => {
                return Err(ErrorDirectorioAnclado::TipoNoAdmitido);
            }
        }
    }
    Ok(actual)
}

#[cfg(windows)]
fn raiz_parent_windows(ruta: &Path) -> Result<PathBuf, ErrorDirectorioAnclado> {
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
    ) || !matches!(componentes.next(), Some(Component::RootDir))
    {
        return Err(ErrorDirectorioAnclado::TipoNoAdmitido);
    }
    let mut raiz = PathBuf::from(prefijo.as_os_str());
    raiz.push("\\");
    Ok(raiz)
}

fn abrir_directorio_por_ruta(
    ruta: &Path,
    parent_transaccional: bool,
) -> Result<File, ErrorDirectorioAnclado> {
    let mut opciones = OpenOptions::new();
    opciones.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let _ = parent_transaccional;
        opciones.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        opciones.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
        if parent_transaccional {
            opciones.access_mode(derechos_parent_transaccional());
        }
    }
    let directorio = opciones.open(ruta).map_err(mapear_io)?;
    metadata_directorio_validada(&directorio)?;
    Ok(directorio)
}

fn clonar_handle(handle: &File) -> Result<File, ErrorDirectorioAnclado> {
    handle.try_clone().map_err(mapear_io)
}

fn metadata_directorio(handle: &File) -> Result<IdentidadDirectorio, ErrorDirectorioAnclado> {
    let metadata = metadata_directorio_validada(handle)?;
    identidad_desde_metadata(handle, &metadata)
}

fn metadata_directorio_validada(
    handle: &File,
) -> Result<std::fs::Metadata, ErrorDirectorioAnclado> {
    let metadata = handle.metadata().map_err(mapear_io)?;
    validar_metadata_directorio_por_metadata(&metadata)?;
    Ok(metadata)
}

/// Obtiene la identidad física del directorio abierto para una deduplicación
/// de recorrido. `None` permite al llamador usar la ruta canónica cuando el
/// filesystem de Windows no implementa `FILE_ID_INFO`.
pub(crate) fn identidad_directorio_para_recorrido(
    ruta: &Path,
) -> Result<Option<IdentidadDirectorio>, ErrorDirectorioAnclado> {
    let handle = abrir_directorio_por_ruta(ruta, false)?;

    #[cfg(unix)]
    {
        let metadata = metadata_directorio_validada(&handle)?;
        identidad_desde_metadata(&handle, &metadata).map(Some)
    }
    #[cfg(windows)]
    {
        identidad_opcional(identidad_windows(&handle), filesystem_sin_file_id)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = handle;
        Ok(None)
    }
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
    _handle: &File,
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
        identidad_windows(_handle).map_err(mapear_io)
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

#[cfg(any(windows, test))]
fn filesystem_sin_file_id(error: &io::Error) -> bool {
    // These Win32 errors indicate that this filesystem/driver does not
    // implement FileIdInfo; ERROR_INVALID_PARAMETER also occurs on FAT/SMB.
    // Other failures must remain visible.
    matches!(error.raw_os_error(), Some(1 | 50 | 87))
}

#[cfg(any(windows, test))]
fn identidad_opcional<T>(
    resultado: io::Result<T>,
    es_no_soportada: impl FnOnce(&io::Error) -> bool,
) -> Result<Option<T>, ErrorDirectorioAnclado> {
    match resultado {
        Ok(identidad) => Ok(Some(identidad)),
        Err(error) if es_no_soportada(&error) => Ok(None),
        Err(error) => Err(mapear_io(error)),
    }
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

#[cfg(test)]
#[path = "anclado/renombrar_observacion_tests.rs"]
mod renombrar_observacion_tests;

#[cfg(test)]
#[path = "anclado/renombrar_operacion_tests.rs"]
mod renombrar_operacion_tests;

#[cfg(test)]
#[path = "anclado/renombrar_test_support.rs"]
mod renombrar_test_support;

#[cfg(test)]
#[path = "anclado/renombrar_destino_tests.rs"]
mod renombrar_destino_tests;

#[cfg(test)]
#[path = "anclado/renombrar_scope_tests.rs"]
mod renombrar_scope_tests;

#[cfg(test)]
#[path = "anclado/renombrar_reparse_tests.rs"]
mod renombrar_reparse_tests;

#[cfg(all(test, unix))]
#[path = "anclado/renombrar_parent_unix_tests.rs"]
mod renombrar_parent_unix_tests;

#[cfg(all(test, windows))]
#[path = "anclado/renombrar_parent_windows_tests.rs"]
mod renombrar_parent_windows_tests;
