//! `BundlePath`: identidad de nodo. Normalización relativa a la raíz del
//! bundle, separador `/`, sin extensión `.md`, bytes conservados (spec
//! §3.2).

/// Identidad de nodo: ruta relativa a la raíz del bundle, normalizada con
/// separador `/`, sin extensión `.md`, con los bytes del nombre
/// conservados (la comparación de igualdad es sensible a mayúsculas).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct BundlePath(String);

/// Motivo de rechazo de un `BundlePath`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ErrorPath {
    /// La ruta está vacía.
    Vacio,
    /// La ruta contiene el separador `\` en vez de `/` (solo aplica a
    /// `BundlePath::nuevo`; `desde_archivo` lo normaliza antes).
    SeparadorInvalido,
    /// La ruta termina en `.md` (solo aplica a `BundlePath::nuevo`).
    ExtensionMd,
    /// La ruta empieza por `/`.
    Absoluto,
    /// Algún segmento entre `/` está vacío (p. ej. `a//b`).
    SegmentoVacio,
    /// Algún segmento es `.` o `..`.
    SegmentoRelativo,
    /// La ruta contiene un carácter prohibido: blanco, de control, o uno
    /// de `# → ( ) , " : [ ]`.
    CaracterProhibido(char),
}

impl std::fmt::Display for ErrorPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorPath::Vacio => write!(f, "la ruta está vacía"),
            ErrorPath::SeparadorInvalido => write!(f, "la ruta contiene el separador '\\'"),
            ErrorPath::ExtensionMd => write!(f, "la ruta termina en '.md'"),
            ErrorPath::Absoluto => write!(f, "la ruta es absoluta"),
            ErrorPath::SegmentoVacio => write!(f, "la ruta tiene un segmento vacío"),
            ErrorPath::SegmentoRelativo => write!(f, "la ruta tiene un segmento '.' o '..'"),
            ErrorPath::CaracterProhibido(c) => {
                write!(f, "la ruta contiene el carácter prohibido {c:?}")
            }
        }
    }
}

impl std::error::Error for ErrorPath {}

fn caracter_prohibido(c: char) -> bool {
    c.is_whitespace()
        || c.is_control()
        || matches!(
            c,
            '#' | '→' | '(' | ')' | ',' | '"' | '\\' | ':' | '[' | ']'
        )
}

impl BundlePath {
    /// Construye un `BundlePath` a partir de una ruta ya normalizada
    /// (separador `/`, sin `.md`). Ver reglas de rechazo en `ErrorPath`.
    pub fn nuevo(ruta: &str) -> Result<Self, ErrorPath> {
        if ruta.is_empty() {
            return Err(ErrorPath::Vacio);
        }
        if ruta.contains('\\') {
            return Err(ErrorPath::SeparadorInvalido);
        }
        if ruta.ends_with(".md") {
            return Err(ErrorPath::ExtensionMd);
        }
        if ruta.starts_with('/') {
            return Err(ErrorPath::Absoluto);
        }
        for segmento in ruta.split('/') {
            if segmento.is_empty() {
                return Err(ErrorPath::SegmentoVacio);
            }
            if segmento == "." || segmento == ".." {
                return Err(ErrorPath::SegmentoRelativo);
            }
        }
        if let Some(c) = ruta.chars().find(|c| caracter_prohibido(*c)) {
            return Err(ErrorPath::CaracterProhibido(c));
        }
        Ok(Self(ruta.to_string()))
    }

    /// Construye un `BundlePath` a partir de una ruta de archivo tal como
    /// aparece en disco: acepta `\` o `/` como separador y quita una
    /// extensión `.md` final, antes de aplicar las mismas reglas que
    /// `nuevo`.
    pub fn desde_archivo(ruta: &str) -> Result<Self, ErrorPath> {
        let normalizada = ruta.replace('\\', "/");
        let sin_extension = normalizada.strip_suffix(".md").unwrap_or(&normalizada);
        Self::nuevo(sin_extension)
    }

    /// La ruta normalizada, con los bytes del nombre conservados.
    pub fn como_str(&self) -> &str {
        &self.0
    }

    /// Último segmento de la ruta.
    pub fn stem(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// Forma en minúsculas de la ruta, solo para detectar colisiones;
    /// nunca para resolver identidad.
    pub fn clave_case(&self) -> String {
        self.0.to_lowercase()
    }

    /// Segmentos previos al último, si los hay.
    pub fn directorio(&self) -> Option<&str> {
        self.0.rfind('/').map(|i| &self.0[..i])
    }
}

impl std::fmt::Display for BundlePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desde_archivo_normaliza_separadores_y_quita_md() {
        let p = BundlePath::desde_archivo("rules\\sub\\x.md").unwrap();
        assert_eq!(p.como_str(), "rules/sub/x");
        assert_eq!(p.stem(), "x");
    }
    #[test]
    fn conserva_bytes_del_nombre() {
        assert_eq!(
            BundlePath::desde_archivo("Rules/X.md").unwrap().como_str(),
            "Rules/X"
        );
        assert_eq!(
            BundlePath::nuevo("Rules/X").unwrap().clave_case(),
            "rules/x"
        );
    }
    #[test]
    fn nuevo_exige_normalizado() {
        assert!(matches!(
            BundlePath::nuevo("a/b.md"),
            Err(ErrorPath::ExtensionMd)
        ));
        assert!(matches!(
            BundlePath::nuevo("a\\b"),
            Err(ErrorPath::SeparadorInvalido)
        ));
    }
    #[test]
    fn rechaza_formas_invalidas() {
        for (caso, esperado) in [
            ("", ErrorPath::Vacio),
            ("/a", ErrorPath::Absoluto),
            ("a//b", ErrorPath::SegmentoVacio),
            ("a/../b", ErrorPath::SegmentoRelativo),
            ("a/./b", ErrorPath::SegmentoRelativo),
            ("a b", ErrorPath::CaracterProhibido(' ')),
            ("a#b", ErrorPath::CaracterProhibido('#')),
            ("a→b", ErrorPath::CaracterProhibido('→')),
            ("a(b", ErrorPath::CaracterProhibido('(')),
            ("a,b", ErrorPath::CaracterProhibido(',')),
            ("a\"b", ErrorPath::CaracterProhibido('"')),
            ("C:/Windows/evil", ErrorPath::CaracterProhibido(':')),
            ("C:evil", ErrorPath::CaracterProhibido(':')),
            ("a/b]]c", ErrorPath::CaracterProhibido(']')),
            ("a/[b", ErrorPath::CaracterProhibido('[')),
        ] {
            assert_eq!(
                BundlePath::nuevo(caso).unwrap_err(),
                esperado,
                "caso {caso:?}"
            );
        }
    }
    #[test]
    fn directorio_y_display() {
        let p = BundlePath::nuevo("a/b/c").unwrap();
        assert_eq!(p.directorio(), Some("a/b"));
        assert_eq!(BundlePath::nuevo("c").unwrap().directorio(), None);
        assert_eq!(p.to_string(), "a/b/c");
    }
}
