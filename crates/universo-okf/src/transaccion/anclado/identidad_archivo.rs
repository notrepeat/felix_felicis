//! Identidad física obtenida desde un handle de archivo abierto.

use std::fmt;
use std::fs::File;
use std::io;
#[cfg(not(any(unix, windows)))]
use std::marker::PhantomData;

/// Identidad de un archivo basada en el objeto del filesystem, no en su ruta.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdentidadArchivo {
    #[cfg(unix)]
    dispositivo: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volumen: u64,
    #[cfg(windows)]
    id_archivo: [u8; 16],
    #[cfg(not(any(unix, windows)))]
    _unsupported: PhantomData<()>,
}

impl fmt::Debug for IdentidadArchivo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IdentidadArchivo([redacted])")
    }
}

pub(crate) fn identidad_archivo(handle: &File) -> io::Result<IdentidadArchivo> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let metadata = handle.metadata()?;
        Ok(IdentidadArchivo {
            dispositivo: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        let identidad = super::identidad_windows(handle)?;
        Ok(IdentidadArchivo {
            volumen: identidad.volumen,
            id_archivo: identidad.id_archivo,
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = handle;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "identidad de archivo no admitida en esta plataforma",
        ))
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn identidad_coincide_entre_clones_y_reaperturas_del_mismo_archivo() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("private-file-name-canary");
        fs::write(&path, b"private file contents canary").unwrap();

        let opened = File::open(&path).unwrap();
        let cloned = opened.try_clone().unwrap();
        let reopened = File::open(&path).unwrap();

        let expected = identidad_archivo(&opened).unwrap();
        assert_eq!(expected, identidad_archivo(&cloned).unwrap());
        assert_eq!(expected, identidad_archivo(&reopened).unwrap());
    }

    #[test]
    fn identidad_distingue_archivos_con_contenido_igual_y_redacta_debug() {
        let temp = tempfile::tempdir().unwrap();
        let first_path = temp.path().join("first-private-file-canary");
        let second_path = temp.path().join("second-private-file-canary");
        let contents = b"equal private file contents canary";
        fs::write(&first_path, contents).unwrap();
        fs::write(&second_path, contents).unwrap();

        let first = File::open(&first_path).unwrap();
        let second = File::open(&second_path).unwrap();
        let first_identity = identidad_archivo(&first).unwrap();
        let second_identity = identidad_archivo(&second).unwrap();

        assert_ne!(first_identity, second_identity);
        let debug = format!("{first_identity:?}");
        assert_eq!(debug, "IdentidadArchivo([redacted])");
        assert!(!debug.contains(first_path.to_string_lossy().as_ref()));
        assert!(!debug.contains(second_path.to_string_lossy().as_ref()));
        assert!(!debug.contains("equal private file contents canary"));
    }
}
