//! Repositorio de criterio (fase 10): un directorio con `felix.toml` en la raíz
//! que declara sus universos y la versión de motor que espera.
//!
//! ```toml
//! schema = "felix-criterio-v1"
//! motor = ">=0.4, <0.5"
//! por_defecto = "candidatos"
//!
//! [universos.candidatos]
//! dir = "universos/candidatos"   # con manifest.toml, invariants.sx y bundle/
//! escritura_agentes = true
//! ```
//!
//! Cada universo es un bundle con su manifiesto y sus invariantes, en nombres
//! fijos dentro de su directorio. Las rutas son relativas a `felix.toml` y no
//! pueden salir del repositorio.
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Component, Path, PathBuf},
};

/// Nombre del archivo que marca la raíz de un repositorio de criterio.
pub const ARCHIVO: &str = "felix.toml";
pub const SCHEMA: &str = "felix-criterio-v1";
const MAX_ARCHIVO: u64 = 64 * 1024;

/// Por qué no se pudo usar un repositorio de criterio.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorCriterio {
    /// No hay `felix.toml` donde se buscó.
    NoEncontrado,
    /// No se pudo leer `felix.toml`.
    Io,
    /// `felix.toml` no tiene el formato esperado.
    Invalido(String),
    /// Una ruta es absoluta, sube con `..` o sale del repositorio.
    RutaInvalida(String),
    /// El universo pedido no existe.
    UniversoNoEncontrado {
        nombre: String,
        disponibles: Vec<String>,
    },
    /// La versión de `universo` no cumple el rango `motor`.
    VersionMotor { requerida: String, actual: String },
}

impl ErrorCriterio {
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::NoEncontrado => "criterio_not_found",
            Self::Io => "criterio_io",
            Self::Invalido(_) => "criterio_invalid",
            Self::RutaInvalida(_) => "criterio_path_invalid",
            Self::UniversoNoEncontrado { .. } => "universe_not_found",
            Self::VersionMotor { .. } => "engine_version_mismatch",
        }
    }
}

impl fmt::Display for ErrorCriterio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEncontrado => write!(f, "no se encontró {ARCHIVO}"),
            Self::Io => write!(f, "no se pudo leer {ARCHIVO}"),
            Self::Invalido(detalle) => write!(f, "{ARCHIVO} inválido: {detalle}"),
            Self::RutaInvalida(ruta) => {
                write!(
                    f,
                    "ruta inválida en {ARCHIVO} (debe quedar dentro del repositorio): {ruta}"
                )
            }
            Self::UniversoNoEncontrado {
                nombre,
                disponibles,
            } => write!(
                f,
                "no existe el universo {nombre}; disponibles: {}",
                disponibles.join(", ")
            ),
            Self::VersionMotor { requerida, actual } => write!(
                f,
                "el repositorio pide universo {requerida} y este es {actual} (usa --ignorar-version para seguir igual)"
            ),
        }
    }
}

/// Un universo del repositorio: un bundle con su manifiesto y sus reglas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Universo {
    pub nombre: String,
    /// Directorio del universo (raíz del repositorio unida a su `dir`).
    pub dir: PathBuf,
    /// Los agentes pueden escribir en él (fase 10, D1).
    pub escritura_agentes: bool,
}

impl Universo {
    pub fn bundle(&self) -> PathBuf {
        self.dir.join("bundle")
    }
    pub fn manifiesto(&self) -> PathBuf {
        self.dir.join("manifest.toml")
    }
    pub fn invariantes(&self) -> PathBuf {
        self.dir.join("invariants.sx")
    }
}

/// Comparación de un rango de versiones (`>=0.4, <0.5`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operador {
    Mayor,
    MayorIgual,
    Menor,
    MenorIgual,
    Igual,
}

/// Rango de versiones de motor que acepta el repositorio.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangoVersion {
    texto: String,
    condiciones: Vec<(Operador, [u64; 3])>,
}

fn parsear_version(texto: &str) -> Option<[u64; 3]> {
    let partes: Vec<&str> = texto.trim().split('.').collect();
    if partes.is_empty() || partes.len() > 3 {
        return None;
    }
    let mut version = [0; 3];
    for (i, parte) in partes.iter().enumerate() {
        if parte.is_empty() || !parte.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        version[i] = parte.parse().ok()?;
    }
    Some(version)
}

impl RangoVersion {
    pub fn parsear(texto: &str) -> Option<Self> {
        let mut condiciones = Vec::new();
        for parte in texto.split(',') {
            let parte = parte.trim();
            let (operador, resto) = [
                (">=", Operador::MayorIgual),
                ("<=", Operador::MenorIgual),
                (">", Operador::Mayor),
                ("<", Operador::Menor),
                ("=", Operador::Igual),
            ]
            .into_iter()
            .find_map(|(prefijo, op)| parte.strip_prefix(prefijo).map(|r| (op, r)))?;
            condiciones.push((operador, parsear_version(resto)?));
        }
        Some(Self {
            texto: texto.trim().to_string(),
            condiciones,
        })
    }

    /// ¿Cumple `version` (`X.Y.Z`) todas las condiciones?
    pub fn admite(&self, version: &str) -> bool {
        let Some(v) = parsear_version(version) else {
            return false;
        };
        self.condiciones.iter().all(|(op, limite)| match op {
            Operador::Mayor => v > *limite,
            Operador::MayorIgual => v >= *limite,
            Operador::Menor => v < *limite,
            Operador::MenorIgual => v <= *limite,
            Operador::Igual => v == *limite,
        })
    }

    pub fn texto(&self) -> &str {
        &self.texto
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchivoUniverso {
    dir: String,
    #[serde(default)]
    escritura_agentes: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Archivo {
    schema: String,
    motor: String,
    por_defecto: String,
    universos: BTreeMap<String, ArchivoUniverso>,
}

/// Repositorio de criterio cargado desde su `felix.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoCriterio {
    pub raiz: PathBuf,
    pub motor: RangoVersion,
    pub por_defecto: String,
    pub universos: BTreeMap<String, Universo>,
}

fn nombre_valido(nombre: &str) -> bool {
    let mut bytes = nombre.bytes();
    nombre.len() <= 64
        && bytes
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Una ruta relativa que no sube ni es absoluta.
fn ruta_relativa_segura(ruta: &str) -> bool {
    let ruta = Path::new(ruta);
    !ruta.as_os_str().is_empty()
        && ruta
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        && ruta.components().any(|c| matches!(c, Component::Normal(_)))
}

impl RepoCriterio {
    /// Interpreta el contenido de `felix.toml`, con `raiz` como su directorio.
    pub fn parsear(raiz: &Path, texto: &str) -> Result<Self, ErrorCriterio> {
        let archivo: Archivo =
            toml::from_str(texto).map_err(|e| ErrorCriterio::Invalido(e.message().to_string()))?;
        if archivo.schema != SCHEMA {
            return Err(ErrorCriterio::Invalido(format!(
                "schema {} no soportado (se espera {SCHEMA})",
                archivo.schema
            )));
        }
        let motor = RangoVersion::parsear(&archivo.motor).ok_or_else(|| {
            ErrorCriterio::Invalido(format!("motor {} no es un rango", archivo.motor))
        })?;
        if archivo.universos.is_empty() {
            return Err(ErrorCriterio::Invalido("no declara universos".into()));
        }
        let mut universos = BTreeMap::new();
        for (nombre, universo) in archivo.universos {
            if !nombre_valido(&nombre) {
                return Err(ErrorCriterio::Invalido(format!(
                    "nombre de universo inválido: {nombre}"
                )));
            }
            if !ruta_relativa_segura(&universo.dir) {
                return Err(ErrorCriterio::RutaInvalida(universo.dir));
            }
            universos.insert(
                nombre.clone(),
                Universo {
                    nombre,
                    dir: raiz.join(&universo.dir),
                    escritura_agentes: universo.escritura_agentes,
                },
            );
        }
        if !universos.contains_key(&archivo.por_defecto) {
            return Err(ErrorCriterio::Invalido(format!(
                "por_defecto {} no es un universo declarado",
                archivo.por_defecto
            )));
        }
        Ok(Self {
            raiz: raiz.to_path_buf(),
            motor,
            por_defecto: archivo.por_defecto,
            universos,
        })
    }

    /// Lee `<raiz>/felix.toml`.
    pub fn cargar(raiz: &Path) -> Result<Self, ErrorCriterio> {
        let ruta = raiz.join(ARCHIVO);
        let metadatos = fs::metadata(&ruta).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ErrorCriterio::NoEncontrado,
            _ => ErrorCriterio::Io,
        })?;
        if !metadatos.is_file() || metadatos.len() > MAX_ARCHIVO {
            return Err(ErrorCriterio::Io);
        }
        let texto = fs::read_to_string(&ruta).map_err(|_| ErrorCriterio::Io)?;
        Self::parsear(raiz, &texto)
    }

    /// El universo `nombre`, o el `por_defecto`. Si su directorio existe, debe
    /// quedar dentro del repositorio también tras resolver enlaces.
    pub fn universo(&self, nombre: Option<&str>) -> Result<&Universo, ErrorCriterio> {
        let nombre = nombre.unwrap_or(&self.por_defecto);
        let universo =
            self.universos
                .get(nombre)
                .ok_or_else(|| ErrorCriterio::UniversoNoEncontrado {
                    nombre: nombre.to_string(),
                    disponibles: self.universos.keys().cloned().collect(),
                })?;
        // El directorio y lo que hay dentro (bundle, manifiesto, invariantes)
        // no pueden salir del repositorio, tampoco por un enlace. Lo que aún
        // no existe se comprueba al usarlo: el comando fallará al abrirlo.
        if let Ok(raiz) = self.raiz.canonicalize() {
            for ruta in [
                universo.dir.clone(),
                universo.bundle(),
                universo.manifiesto(),
                universo.invariantes(),
            ] {
                if let Ok(real) = ruta.canonicalize() {
                    if !real.starts_with(&raiz) {
                        return Err(ErrorCriterio::RutaInvalida(ruta.display().to_string()));
                    }
                }
            }
        }
        Ok(universo)
    }

    /// Error si la versión del motor no está en el rango del repositorio.
    pub fn comprobar_motor(&self, version: &str) -> Result<(), ErrorCriterio> {
        if self.motor.admite(version) {
            Ok(())
        } else {
            Err(ErrorCriterio::VersionMotor {
                requerida: self.motor.texto().to_string(),
                actual: version.to_string(),
            })
        }
    }
}

/// El directorio más cercano, desde `desde` hacia arriba, que contiene
/// `felix.toml`.
pub fn buscar_hacia_arriba(desde: &Path) -> Option<PathBuf> {
    desde
        .ancestors()
        .find(|dir| dir.join(ARCHIVO).is_file())
        .map(Path::to_path_buf)
}

/// Rango de motor que escribe `init` para la versión `actual` (`X.Y.Z`): la
/// misma menor, que en `0.x` puede cambiar contratos.
pub fn rango_para(actual: &str) -> String {
    let [mayor, menor, _] = parsear_version(actual).unwrap_or([0, 0, 0]);
    if mayor == 0 {
        format!(">={mayor}.{menor}, <{mayor}.{}", menor + 1)
    } else {
        format!(">={mayor}.{menor}, <{}.0", mayor + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALIDO: &str = r#"
schema = "felix-criterio-v1"
motor = ">=0.4, <0.5"
por_defecto = "candidatos"

[universos.candidatos]
dir = "universos/candidatos"
escritura_agentes = true

[universos.vigente]
dir = "universos/vigente"
"#;

    #[test]
    fn interpreta_universos_y_resuelve_el_por_defecto() {
        let repo = RepoCriterio::parsear(Path::new("/r"), VALIDO).unwrap();
        let candidatos = repo.universo(None).unwrap();
        assert_eq!(candidatos.nombre, "candidatos");
        assert!(candidatos.escritura_agentes);
        assert_eq!(
            candidatos.bundle(),
            Path::new("/r/universos/candidatos/bundle")
        );
        let vigente = repo.universo(Some("vigente")).unwrap();
        assert!(!vigente.escritura_agentes);
        assert_eq!(
            vigente.invariantes(),
            Path::new("/r/universos/vigente/invariants.sx")
        );
        assert_eq!(
            repo.universo(Some("axiomas")).unwrap_err(),
            ErrorCriterio::UniversoNoEncontrado {
                nombre: "axiomas".into(),
                disponibles: vec!["candidatos".into(), "vigente".into()],
            }
        );
    }

    #[test]
    fn rechaza_rutas_que_salen_del_repositorio_y_formas_invalidas() {
        for dir in ["../fuera", "/abs", "a/../../b", "", "."] {
            let texto = VALIDO.replace("universos/vigente", dir);
            assert_eq!(
                RepoCriterio::parsear(Path::new("/r"), &texto)
                    .unwrap_err()
                    .codigo(),
                "criterio_path_invalid",
                "{dir}"
            );
        }
        for (de, a) in [
            ("felix-criterio-v1", "otro"),
            ("por_defecto = \"candidatos\"", "por_defecto = \"nada\""),
            ("[universos.vigente]", "[universos.Vigente]"),
            (">=0.4, <0.5", "cualquiera"),
            ("escritura_agentes = true", "escritura = true"),
        ] {
            let texto = VALIDO.replace(de, a);
            assert_eq!(
                RepoCriterio::parsear(Path::new("/r"), &texto)
                    .unwrap_err()
                    .codigo(),
                "criterio_invalid",
                "{a}"
            );
        }
    }

    #[test]
    fn rango_de_version_del_motor() {
        let rango = RangoVersion::parsear(">=0.4, <0.5").unwrap();
        assert!(rango.admite("0.4.0") && rango.admite("0.4.9"));
        assert!(!rango.admite("0.3.9") && !rango.admite("0.5.0") && !rango.admite("x"));
        assert!(RangoVersion::parsear("=1.2.3").unwrap().admite("1.2.3"));
        assert!(RangoVersion::parsear(">=0.4, 0.5").is_none());
        let repo = RepoCriterio::parsear(Path::new("/r"), VALIDO).unwrap();
        assert_eq!(
            repo.comprobar_motor("0.3.0").unwrap_err().codigo(),
            "engine_version_mismatch"
        );
        assert!(repo.comprobar_motor("0.4.2").is_ok());
        assert_eq!(rango_para("0.3.0"), ">=0.3, <0.4");
        assert_eq!(rango_para("1.2.0"), ">=1.2, <2.0");
    }

    #[test]
    fn busca_felix_toml_hacia_arriba_y_no_sigue_un_universo_fuera() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path().join("repo");
        let hondo = raiz.join("universos/candidatos/bundle/notas");
        fs::create_dir_all(&hondo).unwrap();
        assert_eq!(buscar_hacia_arriba(&hondo), None);
        assert_eq!(
            RepoCriterio::cargar(&raiz).unwrap_err(),
            ErrorCriterio::NoEncontrado
        );
        fs::write(raiz.join(ARCHIVO), VALIDO).unwrap();
        assert_eq!(buscar_hacia_arriba(&hondo).as_deref(), Some(raiz.as_path()));
        assert!(RepoCriterio::cargar(&raiz).unwrap().universo(None).is_ok());
        #[cfg(unix)]
        {
            let fuera = dir.path().join("fuera");
            fs::create_dir_all(&fuera).unwrap();
            fs::create_dir_all(raiz.join("universos")).unwrap();
            std::os::unix::fs::symlink(&fuera, raiz.join("universos/vigente")).unwrap();
            let repo = RepoCriterio::cargar(&raiz).unwrap();
            assert_eq!(
                repo.universo(Some("vigente")).unwrap_err().codigo(),
                "criterio_path_invalid"
            );
        }
    }
}

#[cfg(all(test, unix))]
mod tests_enlaces {
    use super::*;

    #[test]
    fn un_bundle_o_manifiesto_enlazado_fuera_del_repositorio_se_rechaza() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path().join("repo");
        let fuera = dir.path().join("fuera");
        fs::create_dir_all(raiz.join("u/c")).unwrap();
        fs::create_dir_all(&fuera).unwrap();
        fs::write(
            raiz.join(ARCHIVO),
            "schema = \"felix-criterio-v1\"\nmotor = \">=0.0\"\npor_defecto = \"c\"\n[universos.c]\ndir = \"u/c\"\n",
        )
        .unwrap();
        let repo = RepoCriterio::cargar(&raiz).unwrap();
        assert!(repo.universo(None).is_ok());
        std::os::unix::fs::symlink(&fuera, raiz.join("u/c/bundle")).unwrap();
        assert_eq!(
            repo.universo(None).unwrap_err().codigo(),
            "criterio_path_invalid"
        );
        fs::remove_file(raiz.join("u/c/bundle")).unwrap();
        fs::write(fuera.join("m.toml"), "").unwrap();
        std::os::unix::fs::symlink(fuera.join("m.toml"), raiz.join("u/c/manifest.toml")).unwrap();
        assert_eq!(
            repo.universo(None).unwrap_err().codigo(),
            "criterio_path_invalid"
        );
    }
}
