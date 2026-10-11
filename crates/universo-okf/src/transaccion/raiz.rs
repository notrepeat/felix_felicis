//! Identidad canónica validada de una raíz transaccional.
//!
//! Los artefactos sibling se derivan de esta identidad para que distintas
//! rutas al mismo directorio compartan nombres y ubicación.

use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::path::{Component, Path, PathBuf};

/// Fallo de inspección de una raíz, sin conservar detalles sensibles del SO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorRaiz {
    Io,
    EnlaceNoPermitido,
    TipoNoAdmitido,
}

/// Ubicación path-based de una raíz y sus siblings transaccionales.
///
/// El parent se canoniza y la ruta completa se canoniza cuando la raíz existe.
/// Si falta durante recovery (por ejemplo, después de `root -> backup`), solo
/// se conserva un basename prospectivo portable y seguro. Esta inspección no
/// ancla el directorio ni elimina TOCTOU: la raíz abierta debe obtenerse desde
/// esta ubicación después de recovery.
#[derive(Clone)]
pub(crate) struct UbicacionRaiz {
    ruta: PathBuf,
    parent: PathBuf,
    basename: OsString,
}

impl UbicacionRaiz {
    /// Rechaza enlaces observados y canoniza la ruta completa si existe; si
    /// falta, canoniza su parent y valida el basename prospectivo.
    pub(crate) fn nueva(ruta: &Path) -> Result<Self, ErrorRaiz> {
        let absoluta = ruta_absoluta(ruta)?;
        match fs::symlink_metadata(&absoluta) {
            Ok(metadata) => {
                validar_metadata_raiz(&metadata)?;
                validar_componentes_sin_enlaces(&absoluta)?;
                Self::desde_raiz_existente(&absoluta)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Self::desde_raiz_prospectiva(&absoluta)
            }
            Err(_) => Err(ErrorRaiz::Io),
        }
    }

    /// Ruta absoluta de la ubicación, compuesta por parent canónico y basename.
    pub(crate) fn ruta(&self) -> &Path {
        &self.ruta
    }

    /// Obtiene una raíz existente validada desde la ubicación.
    #[allow(dead_code)] // Se consume desde el flujo de recuperación/apply futuro.
    pub(crate) fn validar(&self) -> Result<RaizValidada, ErrorRaiz> {
        validar_directorio_sin_enlace(&self.ruta)?;
        let ruta = fs::canonicalize(&self.ruta).map_err(|_| ErrorRaiz::Io)?;
        validar_directorio_final(&ruta)?;
        Ok(RaizValidada { ruta })
    }

    pub(crate) fn lock(&self) -> RutaLock {
        RutaLock(self.ruta_sibling(".felix.lock"))
    }

    #[allow(dead_code)] // El builder de staging se implementa en la siguiente unidad.
    pub(crate) fn stage(&self, txn_id: &str) -> Result<RutaStage, ErrorRaiz> {
        if txn_id.is_empty()
            || !txn_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(ErrorRaiz::TipoNoAdmitido);
        }

        let mut sufijo = OsString::from(".felix.stage.");
        sufijo.push(txn_id);
        Ok(RutaStage(self.ruta_sibling_os(&sufijo)))
    }

    #[allow(dead_code)] // El publicador/recovery consumirá esta ruta tipada.
    pub(crate) fn backup(&self) -> RutaBackup {
        RutaBackup(self.ruta_sibling(".felix.backup"))
    }

    #[allow(dead_code)] // El publicador/recovery consumirá esta ruta tipada.
    pub(crate) fn journal(&self) -> RutaJournal {
        RutaJournal(self.ruta_sibling(".felix.journal"))
    }

    fn ruta_sibling(&self, sufijo: &str) -> PathBuf {
        self.ruta_sibling_os(std::ffi::OsStr::new(sufijo))
    }

    fn ruta_sibling_os(&self, sufijo: &std::ffi::OsStr) -> PathBuf {
        let mut nombre = OsString::from(".");
        nombre.push(&self.basename);
        nombre.push(sufijo);
        self.parent.join(nombre)
    }

    fn desde_raiz_existente(ruta: &Path) -> Result<Self, ErrorRaiz> {
        let ruta = fs::canonicalize(ruta).map_err(|_| ErrorRaiz::Io)?;
        validar_directorio_final(&ruta)?;
        let (parent, basename) = separar_parent_y_basename(&ruta)?;
        Ok(Self {
            ruta,
            parent,
            basename,
        })
    }

    fn desde_raiz_prospectiva(ruta: &Path) -> Result<Self, ErrorRaiz> {
        let (parent_solicitado, basename) = separar_parent_y_basename(ruta)?;
        validar_componentes_sin_enlaces(&parent_solicitado)?;

        let parent = fs::canonicalize(&parent_solicitado).map_err(|_| ErrorRaiz::Io)?;
        validar_directorio_sin_enlace(&parent)?;
        let ruta = parent.join(&basename);

        // La canonización del parent puede hacer visible un destino que no
        // estaba visible a través de un alias de parent en la primera stat.
        match fs::symlink_metadata(&ruta) {
            Ok(metadata) => {
                validar_metadata_raiz(&metadata)?;
                Self::desde_raiz_existente(&ruta)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                validar_basename_prospectivo(&basename)?;
                Ok(Self {
                    ruta,
                    parent,
                    basename,
                })
            }
            Err(_) => Err(ErrorRaiz::Io),
        }
    }
}

/// Rutas sibling tienen tipos distintos para impedir que consumidores mezclen
/// lock, staging, backup y journal accidentalmente.
pub(crate) struct RutaLock(PathBuf);
#[allow(dead_code)]
pub(crate) struct RutaStage(PathBuf);
#[allow(dead_code)]
pub(crate) struct RutaBackup(PathBuf);
#[allow(dead_code)]
pub(crate) struct RutaJournal(PathBuf);

impl RutaLock {
    pub(super) fn as_path(&self) -> &Path {
        &self.0
    }
}

#[allow(dead_code)]
impl RutaStage {
    pub(super) fn as_path(&self) -> &Path {
        &self.0
    }
}

#[allow(dead_code)]
impl RutaBackup {
    pub(super) fn as_path(&self) -> &Path {
        &self.0
    }
}

#[allow(dead_code)]
impl RutaJournal {
    pub(super) fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Raíz existente, inspeccionada sin seguir enlaces y canonizada una vez.
/// Sigue siendo una validación por ruta; el anclaje por handle se crea después
/// de recovery cuando el flujo transaccional lo integre.
#[allow(dead_code)]
pub(crate) struct RaizValidada {
    ruta: PathBuf,
}

#[allow(dead_code)]
impl RaizValidada {
    /// Ruta canónica validada.
    pub(crate) fn ruta(&self) -> &Path {
        &self.ruta
    }
}

fn separar_parent_y_basename(ruta: &Path) -> Result<(PathBuf, OsString), ErrorRaiz> {
    let componentes = ruta.components().collect::<Vec<_>>();
    let indice_basename = componentes
        .iter()
        .rposition(|componente| !matches!(componente, Component::CurDir))
        .ok_or(ErrorRaiz::TipoNoAdmitido)?;
    let basename = match componentes[indice_basename] {
        Component::Normal(nombre) => nombre.to_os_string(),
        _ => return Err(ErrorRaiz::TipoNoAdmitido),
    };

    let mut parent = PathBuf::new();
    for componente in &componentes[..indice_basename] {
        parent.push(componente.as_os_str());
    }
    if parent.as_os_str().is_empty() {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }
    Ok((parent, basename))
}

fn validar_metadata_raiz(metadata: &Metadata) -> Result<(), ErrorRaiz> {
    if metadata_es_enlace_o_reparse(metadata) {
        return Err(ErrorRaiz::EnlaceNoPermitido);
    }
    if !metadata.is_dir() {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }
    Ok(())
}

pub(super) fn validar_basename_prospectivo(basename: &std::ffi::OsStr) -> Result<(), ErrorRaiz> {
    let nombre = basename.to_str().ok_or(ErrorRaiz::TipoNoAdmitido)?;
    if nombre.is_empty()
        || matches!(nombre, "." | "..")
        || nombre.chars().any(|caracter| {
            caracter.is_control()
                || matches!(
                    caracter,
                    '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'
                )
        })
        || es_nombre_dispositivo_windows(nombre)
        || parece_alias_83_windows(nombre)
    {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }

    #[cfg(windows)]
    if nombre.ends_with(['.', ' ']) {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }

    Ok(())
}

fn es_nombre_dispositivo_windows(nombre: &str) -> bool {
    let stem = nombre
        .split('.')
        .next()
        .unwrap_or(nombre)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "CLOCK$"
            | "CONIN$"
            | "CONOUT$"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
            | "COM¹"
            | "COM²"
            | "COM³"
            | "LPT¹"
            | "LPT²"
            | "LPT³"
    )
}

fn parece_alias_83_windows(nombre: &str) -> bool {
    let stem = nombre.split('.').next().unwrap_or(nombre);
    stem.rsplit_once('~').is_some_and(|(_, sufijo)| {
        !sufijo.is_empty() && sufijo.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn ruta_absoluta(ruta: &Path) -> Result<PathBuf, ErrorRaiz> {
    if ruta.as_os_str().is_empty() {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }

    let ruta = if ruta.is_absolute() {
        ruta.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| ErrorRaiz::Io)?
            .join(ruta)
    };

    if !ruta.is_absolute() {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }

    Ok(ruta)
}

/// Inspecciona cada prefijo antes de resolver el siguiente componente. En
/// particular, un symlink antes de `..` se rechaza antes de que `..` pueda
/// cambiar su significado. El recorrido léxico solo se usa para inspeccionar
/// prefijos ya comprobados; la ruta final se canoniza con semántica del SO.
fn validar_componentes_sin_enlaces(ruta: &Path) -> Result<(), ErrorRaiz> {
    let mut recorrido = PathBuf::new();
    let mut profundidad = 0usize;

    for componente in ruta.components() {
        match componente {
            Component::Prefix(prefijo) => recorrido.push(prefijo.as_os_str()),
            Component::RootDir => {
                recorrido.push(componente.as_os_str());
                validar_directorio_sin_enlace(&recorrido)?;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if profundidad > 0 {
                    recorrido.pop();
                    profundidad -= 1;
                }
            }
            Component::Normal(nombre) => {
                recorrido.push(nombre);
                validar_directorio_sin_enlace(&recorrido)?;
                profundidad += 1;
            }
        }
    }
    validar_directorio_sin_enlace(&recorrido)
}

fn validar_directorio_sin_enlace(ruta: &Path) -> Result<(), ErrorRaiz> {
    let metadata = fs::symlink_metadata(ruta).map_err(|_| ErrorRaiz::Io)?;
    if metadata_es_enlace_o_reparse(&metadata) {
        return Err(ErrorRaiz::EnlaceNoPermitido);
    }
    if !metadata.is_dir() {
        return Err(ErrorRaiz::TipoNoAdmitido);
    }
    Ok(())
}

fn validar_directorio_final(ruta: &Path) -> Result<(), ErrorRaiz> {
    validar_directorio_sin_enlace(ruta)
}

pub(crate) fn metadata_es_enlace_o_reparse(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn canoniza_alias_lexico_y_deriva_lock_sibling_estable() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        fs::create_dir(&raiz).unwrap();

        let directa = UbicacionRaiz::nueva(&raiz).unwrap();
        let alias = UbicacionRaiz::nueva(&raiz.join(".")).unwrap();

        assert_eq!(directa.ruta(), alias.ruta());
        assert_eq!(directa.lock().as_path(), alias.lock().as_path());
        assert_eq!(
            directa.lock().as_path().file_name().unwrap(),
            OsString::from(".bundle.felix.lock")
        );
        assert_eq!(
            directa.validar().unwrap().ruta(),
            fs::canonicalize(&raiz).unwrap()
        );
    }

    #[test]
    fn alias_parent_converge_sin_enlaces() {
        let temp = tempdir().unwrap();
        let desvio = temp.path().join("desvio");
        let raiz = temp.path().join("bundle");
        fs::create_dir(&desvio).unwrap();
        fs::create_dir(&raiz).unwrap();

        let directa = UbicacionRaiz::nueva(&raiz).unwrap();
        let alias = UbicacionRaiz::nueva(&desvio.join("..").join("bundle")).unwrap();

        assert_eq!(directa.ruta(), alias.ruta());
        assert_eq!(directa.lock().as_path(), alias.lock().as_path());
    }

    #[test]
    fn ubicacion_permite_root_ausente_y_valida_root_existente() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");

        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();
        assert_eq!(
            ubicacion.ruta(),
            fs::canonicalize(temp.path()).unwrap().join("bundle")
        );
        assert!(!raiz.exists());
        assert!(matches!(ubicacion.validar(), Err(ErrorRaiz::Io)));

        fs::create_dir(&raiz).unwrap();
        assert_eq!(
            UbicacionRaiz::nueva(&raiz).unwrap().ruta(),
            fs::canonicalize(temp.path()).unwrap().join("bundle")
        );
        let root_sin_basename = std::env::current_dir()
            .unwrap()
            .ancestors()
            .last()
            .unwrap()
            .to_path_buf();
        assert!(matches!(
            UbicacionRaiz::nueva(&root_sin_basename),
            Err(ErrorRaiz::TipoNoAdmitido)
        ));

        let archivo = temp.path().join("archivo");
        fs::write(&archivo, b"regular file").unwrap();
        assert!(matches!(
            UbicacionRaiz::nueva(&archivo),
            Err(ErrorRaiz::TipoNoAdmitido)
        ));
    }

    #[test]
    fn siblings_son_disjuntos_y_stage_rechaza_identificadores_inseguros() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();

        let lock = ubicacion.lock();
        let stage = ubicacion.stage("txn-01_a").unwrap();
        let backup = ubicacion.backup();
        let journal = ubicacion.journal();
        let siblings = [
            lock.as_path(),
            stage.as_path(),
            backup.as_path(),
            journal.as_path(),
        ];
        let parent = fs::canonicalize(temp.path()).unwrap();

        for sibling in siblings {
            assert_eq!(sibling.parent(), Some(parent.as_path()));
            assert_ne!(sibling, raiz);
        }
        assert_eq!(lock.as_path().file_name().unwrap(), ".bundle.felix.lock");
        assert_eq!(
            stage.as_path().file_name().unwrap(),
            ".bundle.felix.stage.txn-01_a"
        );
        assert_eq!(
            backup.as_path().file_name().unwrap(),
            ".bundle.felix.backup"
        );
        assert_eq!(
            journal.as_path().file_name().unwrap(),
            ".bundle.felix.journal"
        );
        assert!(ubicacion.stage("").is_err());
        assert!(ubicacion.stage("../outside").is_err());
        assert!(ubicacion.stage("txn/child").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_raiz_symlink_sin_seguirla() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let destino = temp.path().join("destino");
        let alias = temp.path().join("alias");
        fs::create_dir(&destino).unwrap();
        symlink(&destino, &alias).unwrap();

        assert!(matches!(
            UbicacionRaiz::nueva(&alias),
            Err(ErrorRaiz::EnlaceNoPermitido)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn rechaza_basename_prospectivo_que_normaliza_o_reserva() {
        let temp = tempdir().unwrap();

        for nombre in [
            "bundle.",
            "bundle ",
            "CON",
            "CON.txt",
            "COM1",
            "LPT9",
            "BUNDLE~1",
            "bundle:name",
        ] {
            assert!(UbicacionRaiz::nueva(&temp.path().join(nombre)).is_err());
        }
        assert!(UbicacionRaiz::nueva(&temp.path().join("bundle-safe.v1")).is_ok());
    }
}
