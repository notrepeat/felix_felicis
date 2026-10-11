//! Lock advisory single-writer sobre un archivo sibling reutilizable.
//!
//! El guard conserva abierto el handle que posee el lock; descartarlo libera
//! el lock por cierre del handle y el sistema operativo también lo libera al
//! terminar el proceso. El archivo no se trunca, elimina ni reemplaza.
//!
//! El lock es advisory y coordina escritores Felix cooperantes. En rutas UNC o
//! shares SMB, la exclusión depende del servidor y de la conexión de red; esta
//! implementación no promete recuperación del lock tras desconexiones ni una
//! garantía distribuida entre hosts. La prueba de proceso hijo cubre sistemas
//! de archivos locales.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::path::Path;

use super::raiz::{ErrorRaiz, RaizValidada, metadata_es_enlace_o_reparse};

/// Resultado redactado de adquirir el lock de escritor.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ErrorLockEscritura {
    Busy,
    Io,
}

impl ErrorLockEscritura {
    /// Código estable que no revela rutas ni detalles del sistema operativo.
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::Busy => "writer_lock_busy",
            Self::Io => "writer_lock_io",
        }
    }
}

impl fmt::Debug for ErrorLockEscritura {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorLockEscritura")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorLockEscritura {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorLockEscritura {}

/// Guard exclusivo del escritor. Mantiene el handle vivo hasta `drop`.
#[must_use = "el lock se libera cuando se descarta este guard"]
pub struct LockEscritura {
    _archivo_lock: File,
    raiz: RaizValidada,
}

impl LockEscritura {
    /// Adquiere sin espera el lock sibling de la raíz canónica.
    pub fn adquirir(raiz: impl AsRef<Path>) -> Result<Self, ErrorLockEscritura> {
        let raiz = RaizValidada::nueva(raiz.as_ref()).map_err(mapear_error_raiz)?;
        let archivo = abrir_lockfile(&raiz).map_err(|_| ErrorLockEscritura::Io)?;

        match fs4::FileExt::try_lock(&archivo) {
            Ok(()) => Ok(Self {
                _archivo_lock: archivo,
                raiz,
            }),
            Err(fs4::TryLockError::WouldBlock) => Err(ErrorLockEscritura::Busy),
            Err(fs4::TryLockError::Error(_)) => Err(ErrorLockEscritura::Io),
        }
    }

    /// Ruta canónica que determina la identidad y los siblings de este guard.
    pub fn raiz_canonica(&self) -> &Path {
        self.raiz.ruta()
    }
}

impl fmt::Debug for LockEscritura {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LockEscritura { .. }")
    }
}

fn mapear_error_raiz(error: ErrorRaiz) -> ErrorLockEscritura {
    match error {
        ErrorRaiz::Io | ErrorRaiz::EnlaceNoPermitido | ErrorRaiz::TipoNoAdmitido => {
            ErrorLockEscritura::Io
        }
    }
}

fn abrir_lockfile(raiz: &RaizValidada) -> std::io::Result<File> {
    let mut opciones = OpenOptions::new();
    opciones.read(true).write(true).create(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opciones.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        opciones.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }

    let archivo = opciones.open(raiz.ruta_lock())?;
    let metadata = archivo.metadata()?;
    if metadata_es_enlace_o_reparse(&metadata) || !metadata.is_file() {
        return Err(std::io::Error::other("invalid lockfile type"));
    }
    Ok(archivo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn guard_sostiene_y_libera_lock_sin_modificar_lockfile() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        fs::create_dir(&raiz).unwrap();
        let lockfile = temp.path().join(".bundle.felix.lock");
        fs::write(&lockfile, b"reusable-lockfile-canary").unwrap();

        let guard = LockEscritura::adquirir(&raiz).unwrap();
        assert_eq!(guard.raiz_canonica(), fs::canonicalize(&raiz).unwrap());
        assert!(matches!(
            LockEscritura::adquirir(raiz.join(".")),
            Err(ErrorLockEscritura::Busy)
        ));
        drop(guard);

        assert!(lockfile.is_file());
        assert_eq!(fs::read(&lockfile).unwrap(), b"reusable-lockfile-canary");
        drop(LockEscritura::adquirir(&raiz).unwrap());
        assert_eq!(fs::read(&lockfile).unwrap(), b"reusable-lockfile-canary");
    }

    #[test]
    fn los_errores_solo_exponen_codigos_redactados() {
        assert_eq!(ErrorLockEscritura::Busy.codigo(), "writer_lock_busy");
        assert_eq!(ErrorLockEscritura::Io.to_string(), "writer_lock_io");
        assert!(!format!("{:?}", ErrorLockEscritura::Io).contains("C:\\"));
    }
}
