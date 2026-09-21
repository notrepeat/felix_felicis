//! Constructor de bundles sintéticos para tests: acumula archivos en
//! memoria y los escribe en un directorio temporal, byte a byte.

/// Acumula archivos (path relativo con `/`, contenido) y los escribe en un
/// directorio temporal.
pub struct Constructor {
    archivos: Vec<(String, String)>,
}

impl Constructor {
    /// Un constructor sin archivos.
    pub fn nuevo() -> Self {
        Self {
            archivos: Vec::new(),
        }
    }

    /// Añade un archivo (builder). `path` usa `/` como separador.
    pub fn archivo(mut self, path: &str, contenido: &str) -> Self {
        self.archivos
            .push((path.to_string(), contenido.to_string()));
        self
    }

    /// Los archivos acumulados hasta ahora.
    pub fn archivos(&self) -> &[(String, String)] {
        &self.archivos
    }

    /// Escribe todos los archivos acumulados en un directorio temporal
    /// nuevo, creando los subdirectorios que hagan falta. El contenido se
    /// escribe tal cual, sin transformar `\r\n`.
    pub fn escribir(&self) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("no se pudo crear el directorio temporal");
        for (path, contenido) in &self.archivos {
            let destino = dir.path().join(path);
            if let Some(padre) = destino.parent() {
                std::fs::create_dir_all(padre).expect("no se pudo crear el subdirectorio");
            }
            std::fs::write(&destino, contenido.as_bytes()).expect("no se pudo escribir el archivo");
        }
        dir
    }
}

impl Default for Constructor {
    fn default() -> Self {
        Self::nuevo()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escribe_y_relee_archivos_byte_a_byte() {
        let c = Constructor::nuevo()
            .archivo("n/x.md", "contenido x\r\ncon crlf\n")
            .archivo("n/sub/y.md", "contenido y\n");
        let dir = c.escribir();
        let x = std::fs::read(dir.path().join("n/x.md")).unwrap();
        assert_eq!(x, b"contenido x\r\ncon crlf\n");
        let y = std::fs::read(dir.path().join("n/sub/y.md")).unwrap();
        assert_eq!(y, b"contenido y\n");
    }
}
