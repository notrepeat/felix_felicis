//! Sello del último estado admitido: detecta cambios hechos fuera de Felix.
//!
//! El sello es un sibling `.<raiz>.felix.seal` con el token de snapshot (árbol
//! raw, manifiesto e invariantes) del último estado que Felix admitió. Lo
//! escribe `aplicar_archivo` tras `Committed` y `sellar` cuando una persona
//! acepta explícitamente el estado actual (sólo si las invariantes se cumplen).
//! Antes de publicar un apply se retira el sello: si el apply se interrumpe,
//! el estado queda «sin sello», nunca con un sello que no le corresponde.
//!
//! `evaluar_guardia` compara sin tomar el lock; es una foto. Detecta, no
//! impide: el lock es advisory y cualquiera puede editar el Markdown.
use super::{
    captura::capturar_snapshot,
    durabilidad::{self, Punto},
    estado,
    historial::{self, Procedencia, Registro, anotar},
    journal::RawTreeDigest,
    lock::LockEscritura,
    raiz::UbicacionRaiz,
    replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1},
    snapshot::SnapshotToken,
};
use std::{
    fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use universo_core::{Delta, ResultadoPropuesta, proponer};

const CABECERA: &str = "felix-seal-v1 ";
const MAX_SELLO_BYTES: u64 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstadoSello {
    /// El estado actual es el último admitido por Felix.
    Coincide,
    /// El bundle, el manifiesto o las invariantes cambiaron fuera de Felix.
    Difiere,
    /// Nunca se selló, o un apply quedó interrumpido.
    Ausente,
}

/// Resultado de la guardia: transacción, invariantes y sello.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Guardia {
    pub transaccion_limpia: bool,
    /// Invariantes fallidas (o conflictos) del estado actual; 0 si se cumplen.
    pub invariantes_fallidas: usize,
    pub sello: EstadoSello,
}

impl Guardia {
    pub fn aprobada(&self) -> bool {
        self.transaccion_limpia
            && self.invariantes_fallidas == 0
            && self.sello == EstadoSello::Coincide
    }
}

/// Error redactado.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ErrorSello(&'static str);
impl ErrorSello {
    pub fn codigo(&self) -> &'static str {
        self.0
    }
}
impl fmt::Debug for ErrorSello {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl fmt::Display for ErrorSello {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ErrorSello {}

/// Token del estado actual y número de invariantes/conflictos fallidos con un
/// delta vacío (equivale a `check` sobre el estado capturado).
pub(crate) fn capturar_veredicto(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
) -> Result<(SnapshotToken, usize), ErrorSello> {
    capturar_veredicto_con_digest(raiz, manifiesto, invariantes).map(|(t, f, _)| (t, f))
}

/// Como `capturar_veredicto`, más el digest del árbol raw capturado.
pub(crate) fn capturar_veredicto_con_digest(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
) -> Result<(SnapshotToken, usize, RawTreeDigest), ErrorSello> {
    let snapshot = capturar_snapshot(
        raiz,
        manifiesto,
        invariantes,
        VERSION_MOTOR_V1,
        POLICY_ID_V1,
    )
    .map_err(|e| ErrorSello(e.codigo()))?;
    let fallidas = match proponer(
        snapshot.estructura(),
        &Delta::default(),
        snapshot.fuentes_invariantes(),
    )
    .map_err(|e| ErrorSello(e.codigo()))?
    {
        ResultadoPropuesta::Admitida(_) => 0,
        ResultadoPropuesta::RechazadaInvariantes(fallos) => fallos.len().max(1),
        ResultadoPropuesta::RechazadaConflictos(conflictos) => conflictos.total_conflictos().max(1),
    };
    Ok((
        snapshot.token(),
        fallidas,
        snapshot.raw_tree_digest().clone(),
    ))
}

pub fn evaluar_guardia(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
) -> Result<Guardia, ErrorSello> {
    let ubicacion = UbicacionRaiz::nueva(raiz).map_err(|_| ErrorSello("guard_root_invalid"))?;
    let transaccion_limpia = estado::inspeccionar(raiz)
        .map_err(|e| ErrorSello(e.codigo()))?
        .limpio();
    let (token, invariantes_fallidas) = capturar_veredicto(raiz, manifiesto, invariantes)?;
    let sello = match leer_sello(&ubicacion)? {
        None => EstadoSello::Ausente,
        Some(sellado) if sellado == token => EstadoSello::Coincide,
        Some(_) => EstadoSello::Difiere,
    };
    Ok(Guardia {
        transaccion_limpia,
        invariantes_fallidas,
        sello,
    })
}

/// Acepta el estado actual como admitido: exige transacción limpia e
/// invariantes cumplidas, y escribe el sello bajo el lock de escritor.
pub fn sellar(raiz: &Path, manifiesto: &Path, invariantes: &Path) -> Result<(), ErrorSello> {
    sellar_con(raiz, manifiesto, invariantes, &Procedencia::default()).map(|_| ())
}

/// Como `sellar`, con procedencia. Si había un sello de otro estado (o uno
/// ilegible), el cambio se hizo fuera de Felix: deja en el historial una entrada
/// `kind: "seal"` (de ese sello al nuevo) para que `universo diff` lo diga en
/// vez de ver un hueco. Devuelve si escribió esa entrada.
pub fn sellar_con(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    procedencia: &Procedencia,
) -> Result<bool, ErrorSello> {
    procedencia.validar().map_err(|e| ErrorSello(e.codigo()))?;
    let ubicacion = UbicacionRaiz::nueva(raiz).map_err(|_| ErrorSello("seal_root_invalid"))?;
    let estado = estado::inspeccionar(raiz).map_err(|e| ErrorSello(e.codigo()))?;
    if estado.sin_rastro() {
        return Err(ErrorSello("seal_root_absent"));
    }
    let _lock = LockEscritura::adquirir_en(&ubicacion).map_err(|e| ErrorSello(e.codigo()))?;
    if !estado::inspeccionar(raiz)
        .map_err(|e| ErrorSello(e.codigo()))?
        .limpio()
    {
        return Err(ErrorSello("seal_transaction_pending"));
    }
    let (token, fallidas) = capturar_veredicto(raiz, manifiesto, invariantes)?;
    if fallidas > 0 {
        return Err(ErrorSello("seal_invariants_rejected"));
    }
    // Sólo la ausencia de sello (el primero, o tras un apply interrumpido)
    // no deja rastro. Un sello ilegible es otro estado: tras un merge de Git
    // queda con marcadores de conflicto.
    let (distinto, previo) = match leer_sello(&ubicacion) {
        Ok(None) => (false, None),
        Ok(Some(previo)) => (previo != token, Some(previo)),
        Err(_) => (true, None),
    };
    // Un sello atrasado (un apply que no pudo sellar, o un `seal` anterior que
    // registró pero no escribió el sello) no es un cambio externo: la última
    // entrada del historial ya deja este estado.
    let ya_registrado = historial::leer(raiz)
        .ok()
        .and_then(|h| h.entradas.last().and_then(|e| e.estado_hasta.clone()))
        == Some(token.to_hex());
    let registrar = distinto && !ya_registrado;
    // Primero el historial: si el sello no llega a escribirse, el próximo
    // `seal` encuentra la entrada y no la repite. Si el historial no se puede
    // escribir, no se sella: el sello viejo es la evidencia del cambio
    // externo, y el `seal` queda reintentable.
    let registrado = registrar
        && anotar(
            &ubicacion,
            procedencia,
            Some(token),
            true,
            &Registro {
                delta: None,
                efecto: None,
                desde: previo,
                hasta: Some(token),
                clase: Some("seal"),
            },
        )
        .is_ok();
    if registrar && !registrado {
        return Err(ErrorSello("seal_history_io"));
    }
    escribir_sello(&ubicacion, token).map_err(|_| ErrorSello("seal_io"))?;
    Ok(registrado)
}

fn ruta_sello(ubicacion: &UbicacionRaiz) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let ruta = ubicacion.ruta();
    let parent = ruta.parent()?.to_path_buf();
    let mut nombre = std::ffi::OsString::from(".");
    nombre.push(ruta.file_name()?);
    nombre.push(".felix.seal");
    let sello = parent.join(&nombre);
    nombre.push(".tmp");
    Some((parent.clone(), sello, parent.join(nombre)))
}

pub(crate) fn leer_sello(ubicacion: &UbicacionRaiz) -> Result<Option<SnapshotToken>, ErrorSello> {
    let (_, sello, _) = ruta_sello(ubicacion).ok_or(ErrorSello("seal_root_invalid"))?;
    // Sólo un archivo regular (no un symlink ni un directorio) cuenta como sello.
    match fs::symlink_metadata(&sello) {
        Ok(m) if m.file_type().is_file() => {}
        Ok(_) => return Err(ErrorSello("seal_invalid")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ErrorSello("seal_io")),
    }
    let archivo = match fs::File::open(&sello) {
        Ok(archivo) => archivo,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ErrorSello("seal_io")),
    };
    let mut texto = String::new();
    archivo
        .take(MAX_SELLO_BYTES)
        .read_to_string(&mut texto)
        .map_err(|_| ErrorSello("seal_invalid"))?;
    let hex = texto
        .strip_prefix(CABECERA)
        .and_then(|resto| resto.strip_suffix('\n'))
        .ok_or(ErrorSello("seal_invalid"))?;
    SnapshotToken::from_hex(hex)
        .map(Some)
        .map_err(|_| ErrorSello("seal_invalid"))
}

/// Escribe el sello de forma atómica y durable (temporal + fsync + rename +
/// fsync del parent). El llamador retiene el lock de escritor.
pub(crate) fn escribir_sello(ubicacion: &UbicacionRaiz, token: SnapshotToken) -> io::Result<()> {
    let (parent, sello, temporal) =
        ruta_sello(ubicacion).ok_or_else(|| io::Error::other("seal"))?;
    let _ = fs::remove_file(&temporal);
    let mut archivo = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporal)?;
    archivo.write_all(format!("{CABECERA}{}\n", token.to_hex()).as_bytes())?;
    durabilidad::sincronizar_archivo(&archivo, Punto::Sello)?;
    drop(archivo);
    fs::rename(&temporal, &sello)?;
    sincronizar_parent(&parent)
}

/// Retira el sello (si existe) y hace durable el retiro.
pub(crate) fn borrar_sello(ubicacion: &UbicacionRaiz) -> io::Result<()> {
    let (parent, sello, _) = ruta_sello(ubicacion).ok_or_else(|| io::Error::other("seal"))?;
    match fs::remove_file(&sello) {
        Ok(()) => sincronizar_parent(&parent),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

pub(crate) fn sincronizar_parent(parent: &Path) -> io::Result<()> {
    // Windows no abre directorios con `File::open`; ahí el fsync de directorio
    // es un no-op documentado (ver `durabilidad`).
    #[cfg(unix)]
    {
        let directorio = fs::File::open(parent)?;
        durabilidad::sincronizar_directorio(&directorio, Punto::Sello)
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        manifest: PathBuf,
        invariants: PathBuf,
    }

    fn fixture(regla: &str) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("bundle");
        fs::create_dir_all(root.join("n")).unwrap();
        fs::write(
            root.join("n/node.md"),
            "---\ntype: a\ntitle: Node\n---\nbody\n",
        )
        .unwrap();
        let manifest = dir.path().join("manifest.toml");
        fs::write(
            &manifest,
            "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n",
        )
        .unwrap();
        let invariants = dir.path().join("invariants.sx");
        fs::write(&invariants, regla).unwrap();
        Fixture {
            _dir: dir,
            root,
            manifest,
            invariants,
        }
    }

    fn guardia(f: &Fixture) -> Guardia {
        evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap()
    }

    #[test]
    fn sellar_y_editar_a_mano_se_detecta() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let f = fixture("(Type @n/node a)\n");
        assert_eq!(guardia(&f).sello, EstadoSello::Ausente);
        assert!(!guardia(&f).aprobada());
        sellar(&f.root, &f.manifest, &f.invariants).unwrap();
        assert!(guardia(&f).aprobada());
        fs::write(
            f.root.join("n/node.md"),
            "---\ntype: a\ntitle: Otro\n---\nbody\n",
        )
        .unwrap();
        let g = guardia(&f);
        assert_eq!(g.sello, EstadoSello::Difiere);
        assert_eq!(g.invariantes_fallidas, 0);
        assert!(!g.aprobada());
    }

    #[test]
    fn cambiar_las_reglas_fuera_de_felix_tambien_se_detecta() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let f = fixture("(Type @n/node a)\n");
        sellar(&f.root, &f.manifest, &f.invariants).unwrap();
        fs::write(&f.invariants, "; otra versión\n(Type @n/node a)\n").unwrap();
        assert_eq!(guardia(&f).sello, EstadoSello::Difiere);
    }

    #[test]
    fn no_se_sella_un_estado_que_viola_las_invariantes() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let f = fixture("(Type @n/node b)\n");
        assert_eq!(
            sellar(&f.root, &f.manifest, &f.invariants)
                .unwrap_err()
                .codigo(),
            "seal_invariants_rejected"
        );
        let g = guardia(&f);
        assert!(g.invariantes_fallidas > 0);
        assert_eq!(g.sello, EstadoSello::Ausente);
    }

    #[test]
    fn no_se_sella_con_transaccion_pendiente() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let f = fixture("(Type @n/node a)\n");
        fs::create_dir(f.root.parent().unwrap().join(".bundle.felix.stage.01")).unwrap();
        assert_eq!(
            sellar(&f.root, &f.manifest, &f.invariants)
                .unwrap_err()
                .codigo(),
            "seal_transaction_pending"
        );
        assert!(!guardia(&f).transaccion_limpia);
    }

    #[test]
    fn temporal_del_sello_huerfano_no_bloquea_seal_ni_guard() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let f = fixture("(Type @n/node a)\n");
        fs::write(
            f.root.parent().unwrap().join(".bundle.felix.seal.tmp"),
            "a medias",
        )
        .unwrap();
        assert!(guardia(&f).transaccion_limpia);
        sellar(&f.root, &f.manifest, &f.invariants).unwrap();
        assert!(guardia(&f).aprobada());
    }

    #[cfg(unix)]
    #[test]
    fn sello_que_no_es_archivo_regular_es_invalido() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let f = fixture("(Type @n/node a)\n");
        sellar(&f.root, &f.manifest, &f.invariants).unwrap();
        let parent = f.root.parent().unwrap();
        fs::rename(parent.join(".bundle.felix.seal"), parent.join("copia")).unwrap();
        std::os::unix::fs::symlink(parent.join("copia"), parent.join(".bundle.felix.seal"))
            .unwrap();
        assert_eq!(
            evaluar_guardia(&f.root, &f.manifest, &f.invariants)
                .unwrap_err()
                .codigo(),
            "seal_invalid"
        );
    }

    #[test]
    fn sello_malformado_es_error_y_no_coincide() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let f = fixture("(Type @n/node a)\n");
        fs::write(
            f.root.parent().unwrap().join(".bundle.felix.seal"),
            "basura",
        )
        .unwrap();
        assert_eq!(
            evaluar_guardia(&f.root, &f.manifest, &f.invariants)
                .unwrap_err()
                .codigo(),
            "seal_invalid"
        );
    }
}
