//! Diagnóstico de solo lectura del estado transaccional y recovery explícito.
//!
//! `inspeccionar` no toma el lock ni abre nada para escribir: lista los
//! siblings reservados `.<raiz>.felix.*` junto al root. Es una foto que puede
//! quedar vieja si otro escritor actúa en paralelo. `recuperar` toma el lock y
//! ejecuta el mismo recovery v2 que precede a cada apply, sin aplicar nada.
//!
//! Un stage sin journal no se puede probar propio: ni `recuperar` ni el apply
//! lo borran. `inspeccionar` lo informa para que la persona decida.
use super::{
    anclado::duenio_transaccion::DuenioTransaccionAnclado, lock::LockEscritura, raiz::UbicacionRaiz,
};
use std::{fmt, fs, path::Path};

/// Siblings transaccionales presentes junto al root.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EstadoTransaccional {
    /// El root existe (falta, p. ej., si un apply murió entre root→backup y
    /// stage→root; el recovery con journal lo restaura).
    pub raiz_presente: bool,
    pub journal: bool,
    pub contexto: bool,
    pub intent_retiro: bool,
    pub backup: bool,
    /// Nombres de directorios stage (`.<raiz>.felix.stage.<id>`).
    pub stages: Vec<String>,
    /// Otros nombres reservados (temporales u otros), salvo el lockfile.
    pub otros: Vec<String>,
}

impl EstadoTransaccional {
    /// Sin ningún sibling transaccional: no hay nada que recuperar.
    pub fn limpio(&self) -> bool {
        self.raiz_presente
            && !self.journal
            && !self.contexto
            && !self.intent_retiro
            && !self.backup
            && self.stages.is_empty()
            && self.otros.is_empty()
    }

    /// El root no existe y no hay ningún sibling transaccional: no hay nada
    /// que recuperar (p. ej. una ruta equivocada).
    pub fn sin_rastro(&self) -> bool {
        !self.raiz_presente
            && EstadoTransaccional {
                raiz_presente: true,
                ..self.clone()
            }
            .limpio()
    }

    /// Stage sin journal: Felix no puede probar que es propio y bloquea todo
    /// apply hasta que una persona lo revise y lo retire.
    pub fn stage_sin_journal(&self) -> bool {
        !self.journal && !self.stages.is_empty()
    }
}

/// Error redactado de `inspeccionar`/`recuperar`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ErrorEstado(&'static str);

impl ErrorEstado {
    pub fn codigo(&self) -> &'static str {
        self.0
    }
}
impl fmt::Debug for ErrorEstado {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl fmt::Display for ErrorEstado {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ErrorEstado {}

pub fn inspeccionar(raiz: &Path) -> Result<EstadoTransaccional, ErrorEstado> {
    let ubicacion = UbicacionRaiz::nueva(raiz).map_err(|_| ErrorEstado("status_root_invalid"))?;
    let ruta = ubicacion.ruta();
    let (Some(parent), Some(basename)) = (ruta.parent(), ruta.file_name()) else {
        return Err(ErrorEstado("status_root_invalid"));
    };
    // Se compara en bytes: un basename no UTF-8 no debe producir un falso
    // «limpio» por la sustitución de `to_string_lossy`.
    let mut prefijo = b".".to_vec();
    prefijo.extend_from_slice(basename.as_encoded_bytes());
    prefijo.extend_from_slice(b".felix.");
    let mut estado = EstadoTransaccional {
        raiz_presente: fs::symlink_metadata(ruta).is_ok_and(|m| m.is_dir()),
        ..EstadoTransaccional::default()
    };
    for entrada in fs::read_dir(parent).map_err(|_| ErrorEstado("status_io"))? {
        let nombre_os = entrada.map_err(|_| ErrorEstado("status_io"))?.file_name();
        let Some(sufijo) = nombre_os
            .as_encoded_bytes()
            .strip_prefix(prefijo.as_slice())
        else {
            continue;
        };
        let sufijo = String::from_utf8_lossy(sufijo);
        let nombre = nombre_os.to_string_lossy().into_owned();
        match sufijo.as_ref() {
            "lock" => {}
            "journal" => estado.journal = true,
            "admctx" => estado.contexto = true,
            "admctx-retire" => estado.intent_retiro = true,
            "backup" => estado.backup = true,
            _ if sufijo.starts_with("stage.") => estado.stages.push(nombre),
            _ => estado.otros.push(nombre),
        }
    }
    estado.stages.sort();
    estado.otros.sort();
    Ok(estado)
}

pub fn recuperar(raiz: &Path) -> Result<(), ErrorEstado> {
    // Sin root ni rastro no hay nada que recuperar; no se crea un lockfile.
    if inspeccionar(raiz)
        .map_err(|_| ErrorEstado("recover_root_invalid"))?
        .sin_rastro()
    {
        return Err(ErrorEstado("recover_root_absent"));
    }
    let ubicacion = UbicacionRaiz::nueva(raiz).map_err(|_| ErrorEstado("recover_root_invalid"))?;
    let mut lock = LockEscritura::adquirir_en(&ubicacion).map_err(|e| ErrorEstado(e.codigo()))?;
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock)
        .map_err(|_| ErrorEstado("recover_owner_invalid"))?;
    owner
        .recuperar_pendiente_v2()
        .map_err(|_| ErrorEstado("recovery_pending_unverified"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("bundle");
        fs::create_dir_all(root.join("n")).unwrap();
        fs::write(root.join("n/node.md"), "---\ntype: a\n---\n").unwrap();
        (dir, root)
    }

    #[test]
    fn root_sin_siblings_esta_limpio_y_recupera_sin_cambios() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let (_dir, root) = bundle();
        assert!(inspeccionar(&root).unwrap().limpio());
        recuperar(&root).unwrap();
        // El lockfile que deja el recovery no cuenta como estado pendiente.
        assert!(inspeccionar(&root).unwrap().limpio());
    }

    #[test]
    fn stage_huerfano_se_informa_y_recover_falla_cerrado_sin_borrarlo() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let (dir, root) = bundle();
        let stage = dir.path().join(".bundle.felix.stage.0123abcd");
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("canary"), b"x").unwrap();
        fs::write(dir.path().join(".bundle.felix.journal.tmp"), b"t").unwrap();
        let estado = inspeccionar(&root).unwrap();
        assert!(!estado.limpio());
        assert!(estado.stage_sin_journal());
        assert_eq!(
            estado.stages,
            vec![".bundle.felix.stage.0123abcd".to_string()]
        );
        assert_eq!(estado.otros, vec![".bundle.felix.journal.tmp".to_string()]);
        assert_eq!(
            recuperar(&root).unwrap_err().codigo(),
            "recovery_pending_unverified"
        );
        assert_eq!(fs::read(stage.join("canary")).unwrap(), b"x");
    }

    #[test]
    fn root_ausente_con_backup_se_informa() {
        let (dir, root) = bundle();
        fs::rename(&root, dir.path().join(".bundle.felix.backup")).unwrap();
        let estado = inspeccionar(&root).unwrap();
        assert!(!estado.raiz_presente);
        assert!(estado.backup);
        assert!(!estado.limpio());
    }

    #[test]
    fn root_inexistente_sin_rastro_no_crea_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("nada");
        let estado = inspeccionar(&root).unwrap();
        assert!(estado.sin_rastro());
        assert!(!estado.limpio());
        assert_eq!(
            recuperar(&root).unwrap_err().codigo(),
            "recover_root_absent"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn basename_no_utf8_se_compara_en_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().unwrap();
        let basename = std::ffi::OsStr::from_bytes(b"b\xff");
        let root = dir.path().join(basename);
        if fs::create_dir(&root).is_err() {
            return; // El filesystem no admite nombres no UTF-8.
        }
        let stage = std::ffi::OsStr::from_bytes(b".b\xff.felix.stage.01");
        fs::create_dir(dir.path().join(stage)).unwrap();
        let estado = inspeccionar(&root).unwrap();
        assert_eq!(estado.stages.len(), 1);
        assert!(!estado.limpio());
    }

    #[test]
    fn otro_bundle_en_el_mismo_parent_no_se_mezcla() {
        let (dir, root) = bundle();
        fs::create_dir(dir.path().join(".otro.felix.stage.ff")).unwrap();
        fs::create_dir(dir.path().join(".bundle.felix.backup")).unwrap();
        let estado = inspeccionar(&root).unwrap();
        assert!(estado.backup);
        assert!(estado.stages.is_empty());
    }
}
