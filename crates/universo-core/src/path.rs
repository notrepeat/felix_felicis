//! `BundlePath`: identidad de nodo. Normalización relativa a la raíz del
//! bundle, separador `/`, sin extensión `.md`, bytes conservados (spec
//! §3.2). Los segmentos también deben ser representables en Windows.

pub(crate) const LIMITE_SEGMENTO_UTF16: usize = 255;
const LIMITE_STEM_BUNDLE_UTF16: usize = LIMITE_SEGMENTO_UTF16 - ".md.tmp".len();

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
    /// Un segmento termina en punto, que Windows elimina al resolver rutas.
    SegmentoTerminaEnPunto,
    /// Un segmento termina en espacio, que Windows elimina al resolver rutas.
    SegmentoTerminaEnEspacio,
    /// Un segmento es un nombre de dispositivo reservado por DOS/Windows.
    DispositivoReservado,
    /// Un segmento contiene un carácter que Windows no admite en nombres.
    CaracterNoPortable(char),
    /// Un segmento excede el límite indicado en unidades UTF-16.
    SegmentoDemasiadoLargo { limite_utf16: usize },
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
            ErrorPath::SegmentoTerminaEnPunto => {
                write!(f, "un segmento termina en punto y no es portable a Windows")
            }
            ErrorPath::SegmentoTerminaEnEspacio => {
                write!(
                    f,
                    "un segmento termina en espacio y no es portable a Windows"
                )
            }
            ErrorPath::DispositivoReservado => {
                write!(
                    f,
                    "un segmento usa un nombre de dispositivo reservado por Windows"
                )
            }
            ErrorPath::CaracterNoPortable(c) => {
                write!(
                    f,
                    "la ruta contiene el carácter no portable a Windows {c:?}"
                )
            }
            ErrorPath::SegmentoDemasiadoLargo { limite_utf16 } => write!(
                f,
                "un segmento excede el máximo portable de {limite_utf16} unidades UTF-16"
            ),
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

fn segmento_es_dispositivo_reservado(segmento: &str) -> bool {
    let base = segmento.split('.').next().unwrap_or(segmento);
    if ["CON", "PRN", "AUX", "NUL"]
        .iter()
        .any(|reservado| base.eq_ignore_ascii_case(reservado))
    {
        return true;
    }

    let puerto_numerado = |prefijo: &str| {
        base.get(..prefijo.len())
            .is_some_and(|parte| parte.eq_ignore_ascii_case(prefijo))
            && base.get(prefijo.len()..).is_some_and(|sufijo| {
                matches!(
                    sufijo,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
    };
    puerto_numerado("COM") || puerto_numerado("LPT")
}

fn caracter_no_portable(c: char) -> bool {
    matches!(c, '<' | '>' | '|' | '?' | '*')
}

pub(crate) fn clave_case_ruta(ruta: &str) -> String {
    // El upcase completo colapsa pares como `Σ`/`ς` y `ᾀ`/`ᾈ`, que el
    // lowercase de Rust deja distintos. Es una clave conservadora de
    // portabilidad, no una equivalencia exacta de las reglas de Windows.
    // Algunas expansiones (por ejemplo `ß` → `SS`) pueden sobre-rechazar
    // aliases que un filesystem distinguiría; nunca resuelve identidad.
    ruta.chars().flat_map(char::to_uppercase).collect()
}

pub(crate) fn validar_segmento_portable_windows(
    segmento: &str,
    limite_utf16: usize,
) -> Result<(), ErrorPath> {
    if segmento
        .encode_utf16()
        .take(limite_utf16.saturating_add(1))
        .count()
        > limite_utf16
    {
        return Err(ErrorPath::SegmentoDemasiadoLargo { limite_utf16 });
    }
    if segmento.ends_with('.') {
        return Err(ErrorPath::SegmentoTerminaEnPunto);
    }
    if segmento.ends_with(' ') {
        return Err(ErrorPath::SegmentoTerminaEnEspacio);
    }
    if let Some(c) = segmento.chars().find(|c| caracter_no_portable(*c)) {
        return Err(ErrorPath::CaracterNoPortable(c));
    }
    if let Some(c) = segmento
        .chars()
        .find(|c| c.is_control() || matches!(c, '"' | ':'))
    {
        return Err(ErrorPath::CaracterProhibido(c));
    }
    if segmento_es_dispositivo_reservado(segmento) {
        return Err(ErrorPath::DispositivoReservado);
    }
    Ok(())
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
        let mut segmentos = ruta.split('/').peekable();
        while let Some(segmento) = segmentos.next() {
            if segmento.is_empty() {
                return Err(ErrorPath::SegmentoVacio);
            }
            if segmento == "." || segmento == ".." {
                return Err(ErrorPath::SegmentoRelativo);
            }
            // La hoja se escribe como `<stem>.md.tmp`; reserva esas siete
            // unidades para no rebasar el máximo Windows de 255 unidades.
            let limite_utf16 = if segmentos.peek().is_none() {
                LIMITE_STEM_BUNDLE_UTF16
            } else {
                LIMITE_SEGMENTO_UTF16
            };
            validar_segmento_portable_windows(segmento, limite_utf16)?;
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

    /// Forma conservadora en mayúsculas de la ruta, solo para detectar
    /// colisiones portables; puede sobre-rechazar expansiones Unicode y nunca
    /// se usa para resolver identidad.
    pub fn clave_case(&self) -> String {
        clave_case_ruta(&self.0)
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
            "RULES/X"
        );
    }

    #[test]
    fn clave_case_detecta_pares_unicode_y_preserva_el_near_miss_de_i_con_punto() {
        let sigma = BundlePath::nuevo("Σ").unwrap();
        let sigma_final = BundlePath::nuevo("ς").unwrap();
        assert_eq!(sigma.clave_case(), sigma_final.clave_case());

        let alfa_con_iota = BundlePath::nuevo("ᾀ").unwrap();
        let alfa_mayuscula_con_iota = BundlePath::nuevo("ᾈ").unwrap();
        assert_eq!(
            alfa_con_iota.clave_case(),
            alfa_mayuscula_con_iota.clave_case()
        );

        let i_mayuscula_punto = BundlePath::nuevo("İ").unwrap();
        let i_mas_punto = BundlePath::nuevo("i\u{307}").unwrap();
        assert_ne!(i_mayuscula_punto.clave_case(), i_mas_punto.clave_case());
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
    fn rechaza_segmentos_no_portables_a_windows() {
        for (caso, esperado) in [
            ("a/b<c", ErrorPath::CaracterNoPortable('<')),
            ("a/b>c", ErrorPath::CaracterNoPortable('>')),
            ("a/b|c", ErrorPath::CaracterNoPortable('|')),
            ("a/b?c", ErrorPath::CaracterNoPortable('?')),
            ("a/b*c", ErrorPath::CaracterNoPortable('*')),
            ("a/b.", ErrorPath::SegmentoTerminaEnPunto),
            ("a/b ", ErrorPath::SegmentoTerminaEnEspacio),
            ("CON", ErrorPath::DispositivoReservado),
            ("a/prn.txt", ErrorPath::DispositivoReservado),
            ("AUX.log", ErrorPath::DispositivoReservado),
            ("n/NuL.data", ErrorPath::DispositivoReservado),
            ("COM1", ErrorPath::DispositivoReservado),
            ("com9.log", ErrorPath::DispositivoReservado),
            ("com¹", ErrorPath::DispositivoReservado),
            ("COM².txt", ErrorPath::DispositivoReservado),
            ("LPT1", ErrorPath::DispositivoReservado),
            ("lpt9.data", ErrorPath::DispositivoReservado),
            ("LPT³.log", ErrorPath::DispositivoReservado),
        ] {
            assert_eq!(BundlePath::nuevo(caso).unwrap_err(), esperado, "{caso:?}");
        }
        assert_eq!(
            BundlePath::desde_archivo("n\\NUL.md").unwrap_err(),
            ErrorPath::DispositivoReservado
        );
        assert_eq!(
            BundlePath::desde_archivo("n\\COM1.guide.md").unwrap_err(),
            ErrorPath::DispositivoReservado
        );
        assert_eq!(
            BundlePath::desde_archivo("n\\COM¹.guide.md").unwrap_err(),
            ErrorPath::DispositivoReservado
        );
    }

    #[test]
    fn acepta_segmentos_cercanos_no_reservados() {
        for caso in [
            "docs/report.v1",
            "docs/COM0",
            "docs/COM10",
            "docs/COM⁴",
            "docs/LPT²extra",
            "docs/LPT0",
            "docs/conifer",
            "docs/auxiliary",
        ] {
            assert_eq!(BundlePath::nuevo(caso).unwrap().como_str(), caso);
        }
        assert_eq!(
            BundlePath::desde_archivo("docs\\report.v1.md")
                .unwrap()
                .como_str(),
            "docs/report.v1"
        );
    }

    #[test]
    fn limita_longitud_por_segmento_en_unidades_utf16() {
        let ascii_255 = "a".repeat(255);
        let ascii_256 = "a".repeat(256);
        assert!(BundlePath::nuevo(&format!("{ascii_255}/x")).is_ok());
        assert_eq!(
            BundlePath::nuevo(&format!("{ascii_256}/x")).unwrap_err(),
            ErrorPath::SegmentoDemasiadoLargo { limite_utf16: 255 }
        );

        let astrales_255 = format!("{}a", "😀".repeat(127));
        let astrales_256 = "😀".repeat(128);
        assert_eq!(astrales_255.encode_utf16().count(), 255);
        assert_eq!(astrales_256.encode_utf16().count(), 256);
        assert!(BundlePath::nuevo(&format!("{astrales_255}/x")).is_ok());
        assert_eq!(
            BundlePath::nuevo(&format!("{astrales_256}/x")).unwrap_err(),
            ErrorPath::SegmentoDemasiadoLargo { limite_utf16: 255 }
        );

        let stem_248 = "a".repeat(248);
        let stem_249 = "a".repeat(249);
        assert!(BundlePath::nuevo(&stem_248).is_ok());
        assert_eq!(
            BundlePath::nuevo(&stem_249).unwrap_err(),
            ErrorPath::SegmentoDemasiadoLargo { limite_utf16: 248 }
        );

        let stem_astral_248 = format!("{}aa", "😀".repeat(123));
        let stem_astral_249 = format!("{}a", "😀".repeat(124));
        assert_eq!(stem_astral_248.encode_utf16().count(), 248);
        assert_eq!(stem_astral_249.encode_utf16().count(), 249);
        assert!(BundlePath::nuevo(&stem_astral_248).is_ok());
        assert_eq!(
            BundlePath::nuevo(&stem_astral_249).unwrap_err(),
            ErrorPath::SegmentoDemasiadoLargo { limite_utf16: 248 }
        );
    }
    #[test]
    fn directorio_y_display() {
        let p = BundlePath::nuevo("a/b/c").unwrap();
        assert_eq!(p.directorio(), Some("a/b"));
        assert_eq!(BundlePath::nuevo("c").unwrap().directorio(), None);
        assert_eq!(p.to_string(), "a/b/c");
    }
}
