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
/// El parent debe existir y se canoniza; el basename se conserva exactamente
/// como componente final de la ruta solicitada. La raíz puede faltar durante
/// recovery (por ejemplo, después de `root -> backup`). Esta inspección no
/// ancla el directorio ni elimina TOCTOU: la raíz abierta debe obtenerse desde
/// esta ubicación después de recovery.
#[derive(Clone)]
pub(crate) struct UbicacionRaiz {
    ruta: PathBuf,
    parent: PathBuf,
    basename: OsString,
}

impl UbicacionRaiz {
    /// Valida el parent sin seguir enlaces, canoniza ese parent y permite que
    /// la raíz todavía no exista.
    pub(crate) fn nueva(ruta: &Path) -> Result<Self, ErrorRaiz> {
        let absoluta = ruta_absoluta(ruta)?;
        let (parent_solicitado, basename) = separar_parent_y_basename(&absoluta)?;
        validar_componentes_sin_enlaces(&parent_solicitado)?;

        let parent = fs::canonicalize(&parent_solicitado).map_err(|_| ErrorRaiz::Io)?;
        validar_directorio_sin_enlace(&parent)?;
        let ruta = parent.join(&basename);
        validar_raiz_si_existe(&ruta)?;

        Ok(Self {
            ruta,
            parent,
            basename,
        })
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

fn validar_raiz_si_existe(ruta: &Path) -> Result<(), ErrorRaiz> {
    match fs::symlink_metadata(ruta) {
        Ok(metadata) if metadata_es_enlace_o_reparse(&metadata) => {
            Err(ErrorRaiz::EnlaceNoPermitido)
        }
        Ok(metadata) if !metadata.is_dir() => Err(ErrorRaiz::TipoNoAdmitido),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ErrorRaiz::Io),
    }
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

#[allow(dead_code)]
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
}
