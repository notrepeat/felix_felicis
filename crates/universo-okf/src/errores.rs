//! Errores del adaptador OKF: forma del documento (`ErrorDocumento`),
//! listas de una entrada (`ErrorEntrada`), carga del bundle (`ErrorBundle`,
//! `Regla`), escritura (`ErrorEscritura`) y manifiesto en disco
//! (`ErrorManifiestoArchivo`).

use std::fmt;

/// Error al partir un documento OKF en frontmatter + cuerpo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorDocumento {
    /// El texto no empieza con una línea `---` exacta.
    FrontmatterAusente,
    /// Se abrió el frontmatter pero no se encontró la línea `---` de cierre.
    FrontmatterSinCierre,
    /// Una línea del frontmatter no es ni el inicio de una entrada
    /// (`clave:`) ni una continuación de la entrada en curso (vacía,
    /// indentada o iniciada con `-`).
    FrontmatterMalformado { linea: usize },
}

impl fmt::Display for ErrorDocumento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorDocumento::FrontmatterAusente => {
                write!(
                    f,
                    "el documento no tiene frontmatter: no empieza con una línea `---`"
                )
            }
            ErrorDocumento::FrontmatterSinCierre => {
                write!(f, "el frontmatter no tiene línea de cierre `---`")
            }
            ErrorDocumento::FrontmatterMalformado { linea } => write!(
                f,
                "frontmatter malformado en la línea {linea}: se esperaba el inicio de una entrada \
                 (`clave:`) o la continuación de la entrada en curso"
            ),
        }
    }
}

impl std::error::Error for ErrorDocumento {}

/// Error al interpretar el contenido de una entrada de frontmatter como
/// lista de wikilinks o lista de mappings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorEntrada {
    /// Un ítem no cumple la forma `[[slug]]`. Lleva el ítem tal como quedó
    /// tras recortar comillas.
    LinkMalformado(String),
    /// Un mapping viene en forma flow (`- { ... }`), no admitida porque el
    /// campo de etiqueta puede contener comas.
    FlowNoAdmitido,
    /// La lista de mappings trae un valor escalar en la propia línea de la
    /// clave (`ten: algo`): la lista va en bloque debajo de la clave.
    ValorEnLineaDeClave,
    /// El mapping no trae el campo de etiqueta, o viene vacío.
    EtiquetaAusente,
    /// El mapping no trae el campo de destino.
    DestinoAusente,
}

impl fmt::Display for ErrorEntrada {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorEntrada::LinkMalformado(texto) => {
                write!(f, "wikilink malformado: «{texto}»")
            }
            ErrorEntrada::FlowNoAdmitido => {
                write!(
                    f,
                    "no se admite la forma flow (`- {{ ... }}`) en esta lista"
                )
            }
            ErrorEntrada::ValorEnLineaDeClave => {
                write!(
                    f,
                    "la lista de mappings va en bloque debajo de la clave; hay un valor en la linea de la clave"
                )
            }
            ErrorEntrada::EtiquetaAusente => {
                write!(f, "falta el campo de etiqueta, o está vacío")
            }
            ErrorEntrada::DestinoAusente => {
                write!(f, "falta el campo de destino")
            }
        }
    }
}

impl std::error::Error for ErrorEntrada {}

/// Motivo de rechazo de un archivo o entrada durante la carga de un bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Regla {
    /// El documento no tiene frontmatter.
    FrontmatterAusente,
    /// Una línea del frontmatter no es ni el inicio de una entrada ni una
    /// continuación de la entrada en curso.
    FrontmatterMalformado,
    /// El frontmatter no tiene línea de cierre `---`.
    FrontmatterSinCierre,
    /// Falta la entrada `type` del frontmatter.
    TypeAusente,
    /// El valor de `type` no resuelve a ninguna sorte de nodo declarada.
    TypeDesconocido,
    /// Falta la entrada `title` del frontmatter.
    TitleAusente,
    /// Un ítem de una lista de wikilinks o de mappings está malformado.
    LinkMalformado,
    /// Un `[[wikilink]]` no resuelve a ningún nodo cargado.
    TargetColgante,
    /// Un `[[wikilink]]` resuelve a más de un nodo cargado.
    StemAmbiguo,
    /// Dos archivos colisionan en mayúsculas/minúsculas de su path.
    ColisionDeCase,
    /// El path del archivo no es un `BundlePath` válido.
    PathInvalido,
    /// Un mapping de arista no trae el campo de etiqueta, o viene vacío.
    EtiquetaAusente,
    /// Un mapping de arista viene en forma flow, no admitida.
    FlowNoAdmitido,
    /// Una lista de mappings trae un valor escalar en la línea de la clave.
    ValorEnLineaDeClave,
    /// Un mapping de arista no trae el campo de destino.
    DestinoAusente,
    /// Una constante de un predicado de extensión no pertenece a su
    /// dominio cerrado.
    ConstanteFueraDeDominio,
    /// El archivo no es UTF-8 válido.
    NoUtf8,
    /// Error de E/S al leer el archivo.
    Io,
    /// Error al construir la `Estructura` (manifiesto inválido u operación
    /// rechazada).
    Estructura,
}

impl std::fmt::Display for Regla {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nombre = match self {
            Regla::FrontmatterAusente => "FrontmatterAusente",
            Regla::FrontmatterMalformado => "FrontmatterMalformado",
            Regla::FrontmatterSinCierre => "FrontmatterSinCierre",
            Regla::TypeAusente => "TypeAusente",
            Regla::TypeDesconocido => "TypeDesconocido",
            Regla::TitleAusente => "TitleAusente",
            Regla::LinkMalformado => "LinkMalformado",
            Regla::TargetColgante => "TargetColgante",
            Regla::StemAmbiguo => "StemAmbiguo",
            Regla::ColisionDeCase => "ColisionDeCase",
            Regla::PathInvalido => "PathInvalido",
            Regla::EtiquetaAusente => "EtiquetaAusente",
            Regla::FlowNoAdmitido => "FlowNoAdmitido",
            Regla::ValorEnLineaDeClave => "ValorEnLineaDeClave",
            Regla::DestinoAusente => "DestinoAusente",
            Regla::ConstanteFueraDeDominio => "ConstanteFueraDeDominio",
            Regla::NoUtf8 => "NoUtf8",
            Regla::Io => "Io",
            Regla::Estructura => "Estructura",
        };
        write!(f, "{nombre}")
    }
}

/// Error al cargar un bundle: el archivo (si se conoce), la línea (si se
/// conoce), la regla infringida y su detalle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorBundle {
    pub path: Option<String>,
    pub linea: Option<usize>,
    pub regla: Regla,
    pub detalle: String,
}

impl std::fmt::Display for ErrorBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = self.path.as_deref().unwrap_or("<bundle>");
        match self.linea {
            Some(linea) => write!(f, "{path}:{linea}: {} — {}", self.regla, self.detalle),
            None => write!(f, "{path}: {} — {}", self.regla, self.detalle),
        }
    }
}

impl std::error::Error for ErrorBundle {}

/// Error de E/S al serializar un bundle a disco (`serializar_bundle`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorEscritura {
    pub path: String,
    pub detalle: String,
}

impl std::fmt::Display for ErrorEscritura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.detalle)
    }
}

impl std::error::Error for ErrorEscritura {}

/// Error al cargar un `Manifiesto` desde TOML (spec §4.5): E/S al leer el
/// archivo, sintaxis TOML (incluye campos desconocidos, rechazados por
/// `deny_unknown_fields`) o errores de `Manifiesto::validar()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorManifiestoArchivo {
    /// Error de E/S al leer el archivo del manifiesto.
    Io(String),
    /// El texto no es TOML válido, o no respeta el schema esperado (incluye
    /// campos desconocidos).
    Sintaxis(String),
    /// El manifiesto deserializado no pasa `validar()`.
    Validacion(Vec<universo_core::ErrorManifiesto>),
}

impl fmt::Display for ErrorManifiestoArchivo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorManifiestoArchivo::Io(detalle) => {
                write!(f, "error de E/S al leer el manifiesto: {detalle}")
            }
            ErrorManifiestoArchivo::Sintaxis(detalle) => {
                write!(f, "el manifiesto no es TOML válido: {detalle}")
            }
            ErrorManifiestoArchivo::Validacion(errores) => {
                write!(f, "el manifiesto no es válido:")?;
                for error in errores {
                    write!(f, "\n  - {error}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ErrorManifiestoArchivo {}
