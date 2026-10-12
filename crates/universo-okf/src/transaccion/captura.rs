//! Captura inmutable del bundle y de sus entradas raw de configuración.
//!
//! Esta etapa rechaza enlaces observables y parsea solo desde buffers ya
//! capturados. La captura por ruta conserva el límite TOCTOU de su backend; la
//! variante anclada inventaría desde un handle root y materializa desde buffers
//! raw. Las APIs siguen internas mientras apply no coordina lock y contexto.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(windows)]
use std::fs;

use universo_core::{Estructura, SentenciaFuente, parsear_programa};

use crate::bundle::{directorio_layout_seguro, parsear_bundle_desde_estructura};
use crate::manifiesto_toml::parsear_manifiesto_sin_validar;

use super::anclado::{DirectorioAnclado, IdentidadDirectorio};
use super::inventario::{
    inventariar_root, inventariar_root_anclado, inventariar_root_con_lector, leer_archivo_regular,
    ruta_absoluta_lexica, validar_archivo_sin_enlaces,
};
use super::journal::RawTreeDigest;
use super::lock::LockEscritura;
use super::snapshot::{
    EntradaSnapshot, ErrorSnapshot, SnapshotToken, TipoEntradaSnapshot, calcular_digest_raw_tree,
    calcular_token_snapshot,
};

/// Snapshot completo que conserva las entradas raw y sus vistas parseadas.
/// Sus campos no son mutables desde el exterior y su representación se redacta.
pub(crate) struct Snapshot {
    token: SnapshotToken,
    raw_tree_digest: RawTreeDigest,
    entradas: Vec<EntradaSnapshot>,
    manifiesto_raw: Arc<[u8]>,
    invariantes_raw: Arc<[u8]>,
    version_motor: Arc<str>,
    policy_id: Arc<str>,
    identidad_root: Option<IdentidadDirectorio>,
    ruta_manifiesto_root: Option<String>,
    ruta_invariantes_root: Option<String>,
    estructura: Estructura,
    fuentes_invariantes: Vec<SentenciaFuente>,
}

/// Identidad del bundle calculada sobre buffers capturados, aún sin parsear.
/// Solo la capa de transacción puede compararla y luego materializarla.
pub(crate) struct CapturaSnapshotRaw {
    token: SnapshotToken,
    raw_tree_digest: RawTreeDigest,
    entradas: Vec<EntradaSnapshot>,
    manifiesto_raw: Arc<[u8]>,
    invariantes_raw: Arc<[u8]>,
    version_motor: Arc<str>,
    policy_id: Arc<str>,
    identidad_root: Option<IdentidadDirectorio>,
    ruta_manifiesto_root: Option<String>,
    ruta_invariantes_root: Option<String>,
}

/// Bytes raw de configuración que ya fueron capturados y permanecen
/// inmutables durante el cálculo del token y la materialización.
///
/// Las rutas opcionales son procedencia root-relativa, nunca rutas a reabrir.
/// Si son `None`, los bytes se consideran externos y ya capturados por quien
/// posee la capability de lectura correspondiente.
#[derive(Clone)]
pub(crate) struct ConfigRawCapturada {
    manifiesto_raw: Arc<[u8]>,
    invariantes_raw: Arc<[u8]>,
    ruta_manifiesto_root: Option<String>,
    ruta_invariantes_root: Option<String>,
}

impl ConfigRawCapturada {
    /// Conserva los bytes ya leídos sin guardar rutas que puedan reabrirse.
    pub(crate) fn nueva(
        manifiesto_raw: impl Into<Arc<[u8]>>,
        invariantes_raw: impl Into<Arc<[u8]>>,
        ruta_manifiesto_root: Option<String>,
        ruta_invariantes_root: Option<String>,
    ) -> Self {
        Self {
            manifiesto_raw: manifiesto_raw.into(),
            invariantes_raw: invariantes_raw.into(),
            ruta_manifiesto_root,
            ruta_invariantes_root,
        }
    }
}

impl fmt::Debug for ConfigRawCapturada {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ConfigRawCapturada { .. }")
    }
}

struct FuenteRaw {
    bytes: Arc<[u8]>,
    ruta_root: Option<String>,
}

impl fmt::Debug for CapturaSnapshotRaw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CapturaSnapshotRaw { .. }")
    }
}

impl Snapshot {
    /// Token opaco; la representación solo se revela mediante `to_hex`.
    pub(crate) fn token(&self) -> SnapshotToken {
        self.token
    }

    pub(crate) fn raw_tree_digest(&self) -> &RawTreeDigest {
        &self.raw_tree_digest
    }

    /// Entradas raw del root, ordenadas por bytes UTF-8 de la ruta.
    pub(crate) fn entradas(&self) -> &[EntradaSnapshot] {
        &self.entradas
    }

    /// Bytes exactos del manifiesto capturado.
    pub(crate) fn manifiesto_raw(&self) -> &[u8] {
        &self.manifiesto_raw
    }

    /// Bytes exactos del programa de invariantes capturado.
    pub(crate) fn invariantes_raw(&self) -> &[u8] {
        &self.invariantes_raw
    }

    pub(crate) fn version_motor(&self) -> &str {
        &self.version_motor
    }

    pub(crate) fn policy_id(&self) -> &str {
        &self.policy_id
    }

    pub(crate) fn identidad_root(&self) -> Option<IdentidadDirectorio> {
        self.identidad_root
    }

    pub(crate) fn ruta_manifiesto_root(&self) -> Option<&str> {
        self.ruta_manifiesto_root.as_deref()
    }

    pub(crate) fn ruta_invariantes_root(&self) -> Option<&str> {
        self.ruta_invariantes_root.as_deref()
    }

    /// Estructura del bundle parseada desde los bytes de `entradas`.
    pub(crate) fn estructura(&self) -> &Estructura {
        &self.estructura
    }

    /// Fuentes del programa parseadas desde `invariantes_raw`.
    pub(crate) fn fuentes_invariantes(&self) -> &[SentenciaFuente] {
        &self.fuentes_invariantes
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Snapshot { .. }")
    }
}

/// Lee y captura un root, su manifiesto y su programa de invariantes.
///
/// Toda entrada regular del root se lee una vez a un buffer raw; manifiesto,
/// invariantes y Markdown se parsean desde esos mismos buffers. Las rutas de
/// configuración externas al root también se leen una sola vez y se rechazan
/// cuando algún componente observado es un symlink o reparse point. Apply usa
/// la variante anclada cuando la operación requiere identidad de root estable.
pub(crate) fn capturar_snapshot(
    raiz: &Path,
    ruta_manifiesto: &Path,
    ruta_invariantes: &Path,
    version_motor: &str,
    policy_id: &str,
) -> Result<Snapshot, ErrorSnapshot> {
    materializar(capturar_snapshot_raw(
        raiz,
        ruta_manifiesto,
        ruta_invariantes,
        version_motor,
        policy_id,
    )?)
}

/// Captura, compara el token antes de parsear y materializa únicamente si el
/// snapshot sigue siendo el esperado. Esta variante por ruta es para
/// simulación y tests; apply usa la variante que recibe un root anclado.
pub(crate) fn capturar_snapshot_con_cas(
    raiz: &Path,
    ruta_manifiesto: &Path,
    ruta_invariantes: &Path,
    version_motor: &str,
    policy_id: &str,
    esperado: SnapshotToken,
) -> Result<Snapshot, ErrorSnapshot> {
    let captura = capturar_snapshot_raw(
        raiz,
        ruta_manifiesto,
        ruta_invariantes,
        version_motor,
        policy_id,
    )?;
    super::snapshot::comparar_snapshot_esperado(esperado, captura.token)?;
    materializar(captura)
}

/// Captura el inventario desde un directorio ya abierto y combina sus bytes
/// con configuración raw capturada previamente. No acepta rutas ni reabre el
/// root; manifiesto e invariantes se parsearán después desde estos mismos bytes.
pub(crate) fn capturar_snapshot_raw_anclado(
    raiz: &DirectorioAnclado,
    configuracion: ConfigRawCapturada,
    version_motor: &str,
    policy_id: &str,
) -> Result<CapturaSnapshotRaw, ErrorSnapshot> {
    let captura =
        capturar_snapshot_raw_anclado_sin_validar(raiz, configuracion, version_motor, policy_id)?;
    validar_procedencia(&captura)?;
    Ok(captura)
}

/// Captura desde el handle root, compara CAS antes de validar procedencia o
/// parsear configuración, y materializa desde los buffers ya capturados.
pub(crate) fn capturar_snapshot_con_cas_anclado(
    raiz: &DirectorioAnclado,
    configuracion: ConfigRawCapturada,
    version_motor: &str,
    policy_id: &str,
    esperado: SnapshotToken,
) -> Result<Snapshot, ErrorSnapshot> {
    let captura =
        capturar_snapshot_raw_anclado_sin_validar(raiz, configuracion, version_motor, policy_id)?;
    super::snapshot::comparar_snapshot_esperado(esperado, captura.token)?;
    materializar(captura)
}

/// Abre el root desde la ubicación protegida por el lock y ejecuta el CAS
/// anclado sin adquirir ni liberar el guard.
pub(crate) fn capturar_snapshot_con_cas_bajo_lock(
    lock: &mut LockEscritura,
    configuracion: ConfigRawCapturada,
    version_motor: &str,
    policy_id: &str,
    esperado: SnapshotToken,
) -> Result<Snapshot, ErrorSnapshot> {
    let raiz =
        DirectorioAnclado::desde_ubicacion(lock.ubicacion()).map_err(|_| ErrorSnapshot::Io)?;
    capturar_snapshot_con_cas_anclado(&raiz, configuracion, version_motor, policy_id, esperado)
}

fn capturar_snapshot_raw_anclado_sin_validar(
    raiz: &DirectorioAnclado,
    configuracion: ConfigRawCapturada,
    version_motor: &str,
    policy_id: &str,
) -> Result<CapturaSnapshotRaw, ErrorSnapshot> {
    let entradas = inventariar_root_anclado(raiz.clonar().map_err(|_| ErrorSnapshot::Io)?)?;
    let identidad_root = raiz.identidad_raiz().ok_or(ErrorSnapshot::RaizInvalida)?;
    let raw_tree_digest = calcular_digest_raw_tree(&entradas)?;
    let token = calcular_token_snapshot(
        version_motor,
        policy_id,
        &configuracion.manifiesto_raw,
        &configuracion.invariantes_raw,
        &entradas,
    )?;

    Ok(CapturaSnapshotRaw {
        token,
        raw_tree_digest,
        entradas,
        manifiesto_raw: configuracion.manifiesto_raw,
        invariantes_raw: configuracion.invariantes_raw,
        version_motor: Arc::from(version_motor),
        policy_id: Arc::from(policy_id),
        identidad_root: Some(identidad_root),
        ruta_manifiesto_root: configuracion.ruta_manifiesto_root,
        ruta_invariantes_root: configuracion.ruta_invariantes_root,
    })
}

/// Lee las entradas y configura la identidad solo desde sus buffers raw. Debe
/// seguir privada para la captura por ruta usada en simulación y tests.
fn capturar_snapshot_raw(
    raiz: &Path,
    ruta_manifiesto: &Path,
    ruta_invariantes: &Path,
    version_motor: &str,
    policy_id: &str,
) -> Result<CapturaSnapshotRaw, ErrorSnapshot> {
    let raiz = ruta_absoluta_lexica(raiz)?;
    let entradas = inventariar_root(&raiz)?;
    capturar_snapshot_raw_con_entradas(
        raiz,
        ruta_manifiesto,
        ruta_invariantes,
        version_motor,
        policy_id,
        entradas,
        leer_archivo_regular,
    )
}

fn capturar_snapshot_con_lector<F>(
    raiz: &Path,
    ruta_manifiesto: &Path,
    ruta_invariantes: &Path,
    version_motor: &str,
    policy_id: &str,
    leer: F,
) -> Result<Snapshot, ErrorSnapshot>
where
    F: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    materializar(capturar_snapshot_raw_con_lector(
        raiz,
        ruta_manifiesto,
        ruta_invariantes,
        version_motor,
        policy_id,
        leer,
    )?)
}

fn capturar_snapshot_raw_con_lector<F>(
    raiz: &Path,
    ruta_manifiesto: &Path,
    ruta_invariantes: &Path,
    version_motor: &str,
    policy_id: &str,
    mut leer: F,
) -> Result<CapturaSnapshotRaw, ErrorSnapshot>
where
    F: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let raiz = ruta_absoluta_lexica(raiz)?;
    let entradas = inventariar_root_con_lector(&raiz, &mut leer)?;
    capturar_snapshot_raw_con_entradas(
        raiz,
        ruta_manifiesto,
        ruta_invariantes,
        version_motor,
        policy_id,
        entradas,
        leer,
    )
}

fn capturar_snapshot_raw_con_entradas<F>(
    raiz: PathBuf,
    ruta_manifiesto: &Path,
    ruta_invariantes: &Path,
    version_motor: &str,
    policy_id: &str,
    entradas: Vec<EntradaSnapshot>,
    mut leer: F,
) -> Result<CapturaSnapshotRaw, ErrorSnapshot>
where
    F: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let mut cache_externa = HashMap::new();
    let manifiesto = bytes_fuente(
        &raiz,
        ruta_manifiesto,
        &entradas,
        &mut cache_externa,
        &mut leer,
    )?;
    let invariantes = bytes_fuente(
        &raiz,
        ruta_invariantes,
        &entradas,
        &mut cache_externa,
        &mut leer,
    )?;

    let token = calcular_token_snapshot(
        version_motor,
        policy_id,
        &manifiesto.bytes,
        &invariantes.bytes,
        &entradas,
    )?;
    let raw_tree_digest = calcular_digest_raw_tree(&entradas)?;

    Ok(CapturaSnapshotRaw {
        token,
        raw_tree_digest,
        entradas,
        manifiesto_raw: manifiesto.bytes,
        invariantes_raw: invariantes.bytes,
        version_motor: Arc::from(version_motor),
        policy_id: Arc::from(policy_id),
        identidad_root: None,
        ruta_manifiesto_root: manifiesto.ruta_root,
        ruta_invariantes_root: invariantes.ruta_root,
    })
}

/// Reinterpreta el inventario ya recapturado de un sibling con la misma
/// configuración raw e identidad de motor/política que originaron `original`.
/// No vuelve a leer root ni rutas de configuración. La recaptura del inventario
/// sigue siendo path-based: esto no ancla el objeto filesystem ni elimina TOCTOU.
pub(crate) fn materializar_desde_entradas(
    original: &Snapshot,
    entradas: Vec<EntradaSnapshot>,
) -> Result<Snapshot, ErrorSnapshot> {
    let raw_tree_digest = calcular_digest_raw_tree(&entradas)?;
    let token = calcular_token_snapshot(
        original.version_motor(),
        original.policy_id(),
        original.manifiesto_raw(),
        original.invariantes_raw(),
        &entradas,
    )?;
    materializar(CapturaSnapshotRaw {
        token,
        raw_tree_digest,
        entradas,
        manifiesto_raw: Arc::clone(&original.manifiesto_raw),
        invariantes_raw: Arc::clone(&original.invariantes_raw),
        version_motor: Arc::clone(&original.version_motor),
        policy_id: Arc::clone(&original.policy_id),
        identidad_root: original.identidad_root,
        ruta_manifiesto_root: original.ruta_manifiesto_root.clone(),
        ruta_invariantes_root: original.ruta_invariantes_root.clone(),
    })
}

/// Parsea únicamente los buffers que ya forman la identidad raw. No vuelve a
/// consultar el filesystem, así que el modelo y token describen los mismos
/// bytes incluso si los archivos cambian después de la captura.
fn materializar(raw: CapturaSnapshotRaw) -> Result<Snapshot, ErrorSnapshot> {
    materializar_con_expansor(raw, archivos_markdown)
}

fn materializar_con_expansor(
    raw: CapturaSnapshotRaw,
    expandir_archivos: impl FnOnce(
        &[EntradaSnapshot],
        &[String],
        &[String],
    ) -> Result<Vec<(String, String)>, ErrorSnapshot>,
) -> Result<Snapshot, ErrorSnapshot> {
    let CapturaSnapshotRaw {
        token,
        raw_tree_digest,
        entradas,
        manifiesto_raw,
        invariantes_raw,
        version_motor,
        policy_id,
        identidad_root,
        ruta_manifiesto_root,
        ruta_invariantes_root,
    } = raw;
    validar_procedencia_campos(
        &entradas,
        &manifiesto_raw,
        &invariantes_raw,
        ruta_manifiesto_root.as_deref(),
        ruta_invariantes_root.as_deref(),
    )?;
    let manifiesto_texto =
        std::str::from_utf8(&manifiesto_raw).map_err(|_| ErrorSnapshot::ManifiestoInvalido)?;
    let manifiesto = parsear_manifiesto_sin_validar(manifiesto_texto)
        .map_err(|_| ErrorSnapshot::ManifiestoInvalido)?;
    let estructura =
        Estructura::nueva(manifiesto).map_err(|_| ErrorSnapshot::ManifiestoInvalido)?;
    let archivos = expandir_archivos(
        &entradas,
        &estructura.manifiesto().layout.directorios,
        &estructura.manifiesto().layout.ignorados,
    )?;
    let estructura = parsear_bundle_desde_estructura(&archivos, estructura)
        .map_err(|_| ErrorSnapshot::BundleInvalido)?;

    let invariantes_texto =
        std::str::from_utf8(&invariantes_raw).map_err(|_| ErrorSnapshot::InvariantesInvalidas)?;
    let fuentes_invariantes =
        parsear_programa(invariantes_texto).map_err(|_| ErrorSnapshot::InvariantesInvalidas)?;
    Ok(Snapshot {
        token,
        raw_tree_digest,
        entradas,
        manifiesto_raw,
        invariantes_raw,
        version_motor,
        policy_id,
        identidad_root,
        ruta_manifiesto_root,
        ruta_invariantes_root,
        estructura,
        fuentes_invariantes,
    })
}

fn validar_procedencia(raw: &CapturaSnapshotRaw) -> Result<(), ErrorSnapshot> {
    validar_procedencia_campos(
        &raw.entradas,
        &raw.manifiesto_raw,
        &raw.invariantes_raw,
        raw.ruta_manifiesto_root.as_deref(),
        raw.ruta_invariantes_root.as_deref(),
    )
}

fn validar_procedencia_campos(
    entradas: &[EntradaSnapshot],
    manifiesto_raw: &[u8],
    invariantes_raw: &[u8],
    ruta_manifiesto_root: Option<&str>,
    ruta_invariantes_root: Option<&str>,
) -> Result<(), ErrorSnapshot> {
    if ruta_manifiesto_root.is_some() && ruta_manifiesto_root == ruta_invariantes_root {
        return Err(ErrorSnapshot::BundleInvalido);
    }
    for (ruta, bytes) in [
        (ruta_manifiesto_root, manifiesto_raw),
        (ruta_invariantes_root, invariantes_raw),
    ] {
        if let Some(ruta) = ruta {
            let coincide = entradas.iter().any(|entrada| {
                entrada.ruta() == ruta
                    && entrada.tipo() == TipoEntradaSnapshot::Archivo
                    && entrada.bytes() == Some(bytes)
            });
            if !coincide {
                return Err(ErrorSnapshot::BundleInvalido);
            }
        }
    }
    Ok(())
}

fn bytes_fuente<F>(
    raiz: &Path,
    ruta: &Path,
    entradas: &[EntradaSnapshot],
    cache_externa: &mut HashMap<PathBuf, Arc<[u8]>>,
    leer: &mut F,
) -> Result<FuenteRaw, ErrorSnapshot>
where
    F: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let absoluta = ruta_absoluta_lexica(ruta)?;
    let relativa_lexica = absoluta
        .starts_with(raiz)
        .then(|| ruta_relativa(raiz, &absoluta))
        .transpose()?;
    if let Some(relativa) = relativa_lexica.as_deref()
        && let Some(bytes) = bytes_entrada(entradas, relativa)
    {
        return Ok(FuenteRaw {
            bytes,
            ruta_root: Some(relativa.to_owned()),
        });
    }

    #[cfg(windows)]
    {
        // `Path::starts_with` compara componentes con caso exacto incluso en
        // Windows. Resuelve la ruta solo después de rechazar reparse points,
        // y usa el spelling real devuelto por el sistema únicamente para
        // localizar el buffer ya leído; el token conserva la ruta inventariada.
        validar_archivo_sin_enlaces(&absoluta)?;
        let raiz_real = fs::canonicalize(raiz).map_err(|_| ErrorSnapshot::Io)?;
        let archivo_real = fs::canonicalize(&absoluta).map_err(|_| ErrorSnapshot::Io)?;
        if archivo_real.starts_with(&raiz_real) {
            let relativa = ruta_relativa(&raiz_real, &archivo_real)?;
            let bytes = bytes_entrada(entradas, &relativa).ok_or(ErrorSnapshot::Io)?;
            return Ok(FuenteRaw {
                bytes,
                ruta_root: Some(relativa),
            });
        }
    }

    #[cfg(not(windows))]
    if relativa_lexica.is_some() {
        return Err(ErrorSnapshot::Io);
    }

    #[cfg(not(windows))]
    validar_archivo_sin_enlaces(&absoluta)?;
    if let Some(bytes) = cache_externa.get(&absoluta) {
        return Ok(FuenteRaw {
            bytes: Arc::clone(bytes),
            ruta_root: None,
        });
    }
    let bytes: Arc<[u8]> = Arc::from(leer(&absoluta)?);
    cache_externa.insert(absoluta, Arc::clone(&bytes));
    Ok(FuenteRaw {
        bytes,
        ruta_root: None,
    })
}

fn ruta_relativa(raiz: &Path, absoluta: &Path) -> Result<String, ErrorSnapshot> {
    absoluta
        .strip_prefix(raiz)
        .map_err(|_| ErrorSnapshot::RutaNoRepresentable)?
        .to_str()
        .ok_or(ErrorSnapshot::RutaNoUtf8)
        .map(|ruta| ruta.replace('\\', "/"))
}

fn bytes_entrada(entradas: &[EntradaSnapshot], relativa: &str) -> Option<Arc<[u8]>> {
    entradas
        .iter()
        .find(|entrada| {
            entrada.ruta() == relativa && entrada.tipo() == TipoEntradaSnapshot::Archivo
        })
        .and_then(EntradaSnapshot::bytes)
        .map(Arc::from)
}

fn archivos_markdown(
    entradas: &[EntradaSnapshot],
    directorios: &[String],
    ignorados: &[String],
) -> Result<Vec<(String, String)>, ErrorSnapshot> {
    archivos_markdown_observando(entradas, directorios, ignorados, || {})
}

fn archivos_markdown_observando(
    entradas: &[EntradaSnapshot],
    directorios: &[String],
    ignorados: &[String],
    mut observar_trabajo: impl FnMut(),
) -> Result<Vec<(String, String)>, ErrorSnapshot> {
    let mut directorios_existentes: Vec<&str> = Vec::new();
    let mut nombres_ignorados: Vec<&str> = ignorados.iter().map(String::as_str).collect();
    nombres_ignorados.sort_unstable();
    nombres_ignorados.dedup();

    for entrada in entradas {
        observar_trabajo();
        if entrada.tipo() == TipoEntradaSnapshot::Directorio {
            directorios_existentes.push(entrada.ruta());
        }
    }
    directorios_existentes.sort_unstable();
    for directorio in directorios {
        observar_trabajo();
        if !directorio_layout_seguro(directorio)
            || !contiene_nombre_ordenado(&directorios_existentes, directorio, &mut observar_trabajo)
        {
            return Err(ErrorSnapshot::BundleInvalido);
        }
    }

    // Las claves toman prestadas las rutas del layout y los valores guardan
    // solo índices del inventario capturado. Cada archivo se indexa bajo cada
    // ancestro declarado, conservando la inclusión recursiva del layout.
    let mut indices_por_directorio: HashMap<&str, Vec<usize>> = HashMap::new();
    for directorio in directorios {
        indices_por_directorio
            .entry(directorio.as_str())
            .or_default();
    }
    for (indice, entrada) in entradas.iter().enumerate() {
        observar_trabajo();
        if entrada.tipo() != TipoEntradaSnapshot::Archivo
            || Path::new(entrada.ruta())
                .extension()
                .and_then(|ext| ext.to_str())
                != Some("md")
        {
            continue;
        }
        let nombre = entrada.ruta().rsplit('/').next().unwrap_or(entrada.ruta());
        if contiene_nombre_ordenado(&nombres_ignorados, nombre, &mut observar_trabajo) {
            continue;
        }
        for (fin_ancestro, _) in entrada.ruta().match_indices('/') {
            observar_trabajo();
            let ancestro = &entrada.ruta()[..fin_ancestro];
            if let Some(indices) = indices_por_directorio.get_mut(ancestro) {
                indices.push(indice);
            }
        }
    }

    let mut archivos = Vec::new();
    for directorio in directorios {
        observar_trabajo();
        if let Some(indices) = indices_por_directorio.get(directorio.as_str()) {
            for &indice in indices {
                observar_trabajo();
                let entrada = &entradas[indice];
                let bytes = entrada.bytes().ok_or(ErrorSnapshot::BundleInvalido)?;
                let texto =
                    std::str::from_utf8(bytes).map_err(|_| ErrorSnapshot::BundleInvalido)?;
                archivos.push((entrada.ruta().to_owned(), texto.to_owned()));
            }
        }
    }
    Ok(archivos)
}

fn contiene_nombre_ordenado(
    nombres: &[&str],
    nombre: &str,
    mut observar_comparacion: impl FnMut(),
) -> bool {
    let mut inicio = 0;
    let mut fin = nombres.len();
    while inicio < fin {
        observar_comparacion();
        let medio = inicio + (fin - inicio) / 2;
        match nombres[medio].cmp(nombre) {
            std::cmp::Ordering::Less => inicio = medio + 1,
            std::cmp::Ordering::Greater => fin = medio,
            std::cmp::Ordering::Equal => return true,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::lock::{ErrorLockEscritura, LockEscritura};
    use crate::transaccion::raiz::UbicacionRaiz;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::tempdir;

    const MANIFIESTO: &str = r#"version = "snapshot-test/1"

[nodos]
sortes = ["a", "Retirado", "Patrón"]

[nodos.disco]
"Patrón" = "pattern"

[layout]
directorios = ["knowledge"]
sin_iota = []
ignorados = ["ignored.md"]
"#;

    #[test]
    fn archivos_markdown_distribuye_por_ancestros_con_crecimiento_acotado() {
        fn techo_log2(valor: usize) -> usize {
            if valor <= 1 {
                0
            } else {
                usize::BITS as usize - (valor - 1).leading_zeros() as usize
            }
        }

        const DIRECTORIOS: usize = 64;
        const IGNORADOS: usize = 4096;
        let nombre_ignorado = format!("ignorado_{:04}.md", IGNORADOS - 1);
        let ignorados: Vec<String> = (0..IGNORADOS)
            .map(|indice| format!("ignorado_{indice:04}.md"))
            .collect();
        let mut entradas = Vec::new();
        for indice in (0..DIRECTORIOS).rev() {
            let directorio = format!("d{indice:03}");
            entradas.push(EntradaSnapshot::archivo(
                format!("{directorio}/z.md"),
                b"z".as_slice(),
            ));
            entradas.push(EntradaSnapshot::archivo(
                format!("{directorio}/a.md"),
                b"a".as_slice(),
            ));
            entradas.push(EntradaSnapshot::archivo(
                format!("{directorio}/{nombre_ignorado}"),
                b"skip".as_slice(),
            ));
            entradas.push(EntradaSnapshot::directorio(directorio));
        }
        let directorios: Vec<String> = (0..DIRECTORIOS)
            .rev()
            .map(|indice| format!("d{indice:03}"))
            .collect();
        assert!(directorios.iter().all(|directorio| {
            directorio_layout_seguro(directorio)
                && entradas.iter().any(|entrada| {
                    entrada.ruta() == directorio
                        && entrada.tipo() == TipoEntradaSnapshot::Directorio
                })
        }));
        let mut operaciones = 0;
        let archivos =
            archivos_markdown_observando(&entradas, &directorios, &ignorados, || operaciones += 1)
                .unwrap();

        let esperados: Vec<String> = directorios
            .iter()
            .flat_map(|directorio| [format!("{directorio}/z.md"), format!("{directorio}/a.md")])
            .collect();
        assert_eq!(
            archivos.iter().map(|(ruta, _)| ruta).collect::<Vec<_>>(),
            esperados.iter().collect::<Vec<_>>()
        );

        let limite = directorios.len()
            + 2 * entradas.len()
            + directorios.len() * techo_log2(DIRECTORIOS + 1)
            + 3 * DIRECTORIOS * techo_log2(IGNORADOS + 1)
            + 6 * DIRECTORIOS;
        assert!(
            operaciones <= limite,
            "{operaciones} comparaciones/visitas exceden la cota {limite}"
        );
    }

    #[test]
    fn captura_reutiliza_buffers_del_root_para_configuracion_y_parsing() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        fs::write(root.path().join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.path().join("invariants.smt2"), "\n").unwrap();
        fs::write(
            root.path().join("knowledge/a.md"),
            "---\ntype: a\ntitle: A\n---\n",
        )
        .unwrap();
        let mut lecturas = HashMap::<PathBuf, usize>::new();
        let snapshot = capturar_snapshot_con_lector(
            root.path(),
            &root.path().join("manifest.toml"),
            &root.path().join("invariants.smt2"),
            "engine-test/1",
            "policy-test/1",
            |path| {
                *lecturas.entry(path.to_path_buf()).or_default() += 1;
                leer_archivo_regular(path)
            },
        )
        .unwrap();

        assert!(lecturas.values().all(|&count| count == 1));
        assert_eq!(lecturas.len(), 3);
        assert_eq!(snapshot.manifiesto_raw(), MANIFIESTO.as_bytes());
        assert_eq!(snapshot.invariantes_raw(), b"\n");
        assert_eq!(snapshot.version_motor(), "engine-test/1");
        assert_eq!(snapshot.policy_id(), "policy-test/1");
        assert_eq!(snapshot.ruta_manifiesto_root(), Some("manifest.toml"));
        assert_eq!(snapshot.ruta_invariantes_root(), Some("invariants.smt2"));
        assert_eq!(snapshot.estructura().nodos().count(), 1);
        assert!(snapshot.fuentes_invariantes().is_empty());
        assert!(!format!("{snapshot:?}").contains("snapshot-test"));
        assert!(!format!("{snapshot:?}").contains("knowledge/a.md"));
    }

    #[test]
    fn captura_raw_se_materializa_desde_los_mismos_buffers_sin_releer() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        let manifiesto = root.path().join("manifest.toml");
        let invariantes = root.path().join("invariants.smt2");
        let documento = root.path().join("knowledge/a.md");
        fs::write(&manifiesto, MANIFIESTO).unwrap();
        fs::write(&invariantes, "\n").unwrap();
        fs::write(&documento, "---\ntype: a\ntitle: A\n---\n").unwrap();

        let raw = capturar_snapshot_raw(
            root.path(),
            &manifiesto,
            &invariantes,
            "engine/1",
            "policy/1",
        )
        .unwrap();
        fs::write(&manifiesto, b"\xffMANIFEST-CANARY").unwrap();
        fs::write(&documento, b"\xffDOCUMENT-CANARY").unwrap();

        let snapshot = materializar(raw).unwrap();
        assert_eq!(snapshot.manifiesto_raw(), MANIFIESTO.as_bytes());
        assert_eq!(snapshot.estructura().nodos().count(), 1);
    }

    #[test]
    fn manifiesto_invalido_no_expande_ni_clona_contenido_markdown() {
        let casos = [
            (
                "duplicados",
                MANIFIESTO.replace(
                    "directorios = [\"knowledge\"]",
                    "directorios = [\"knowledge\", \"knowledge\"]",
                ),
            ),
            (
                "solapados",
                MANIFIESTO.replace(
                    "directorios = [\"knowledge\"]",
                    "directorios = [\"knowledge\", \"knowledge/sub\"]",
                ),
            ),
            (
                "vacio",
                MANIFIESTO.replace("directorios = [\"knowledge\"]", "directorios = []"),
            ),
        ];

        for (caso, manifiesto) in casos {
            let root = tempdir().unwrap();
            fs::create_dir_all(root.path().join("knowledge/sub")).unwrap();
            fs::write(root.path().join("manifest.toml"), manifiesto).unwrap();
            fs::write(root.path().join("invariants.smt2"), "\n").unwrap();
            let canary = format!("CANARY-{caso}-no-expand");
            fs::write(root.path().join("knowledge/canary.md"), &canary).unwrap();

            let raw = capturar_snapshot_raw(
                root.path(),
                &root.path().join("manifest.toml"),
                &root.path().join("invariants.smt2"),
                "engine/1",
                "policy/1",
            )
            .unwrap();
            let mut operaciones_expansion = 0;
            let error = materializar_con_expansor(raw, |entradas, directorios, ignorados| {
                archivos_markdown_observando(entradas, directorios, ignorados, || {
                    operaciones_expansion += 1;
                })
            })
            .unwrap_err();

            assert_eq!(error, ErrorSnapshot::ManifiestoInvalido, "caso: {caso}");
            assert_eq!(operaciones_expansion, 0, "caso: {caso}");
            assert!(!format!("{error:?}").contains(&canary));
            assert!(!error.to_string().contains(&canary));
        }
    }

    #[test]
    fn captura_anclada_coincide_con_path_y_repite_inventario_por_handle() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        let manifest = root.path().join("manifest.toml");
        let invariants = root.path().join("invariants.smt2");
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n").unwrap();
        fs::write(
            root.path().join("knowledge/a.md"),
            "---\ntype: a\ntitle: A\n---\n",
        )
        .unwrap();

        let por_path =
            capturar_snapshot(root.path(), &manifest, &invariants, "engine/1", "policy/1").unwrap();
        let configuracion = ConfigRawCapturada::nueva(
            fs::read(&manifest).unwrap(),
            fs::read(&invariants).unwrap(),
            Some("manifest.toml".to_owned()),
            Some("invariants.smt2".to_owned()),
        );
        let ubicacion = UbicacionRaiz::nueva(root.path()).unwrap();
        let ancla = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();

        for _ in 0..2 {
            let anclado = materializar(
                capturar_snapshot_raw_anclado(
                    &ancla,
                    configuracion.clone(),
                    "engine/1",
                    "policy/1",
                )
                .unwrap(),
            )
            .unwrap();

            assert_eq!(anclado.token(), por_path.token());
            assert!(anclado.identidad_root().is_some());
            assert!(por_path.identidad_root().is_none());
            assert_eq!(anclado.manifiesto_raw(), por_path.manifiesto_raw());
            assert_eq!(anclado.invariantes_raw(), por_path.invariantes_raw());
            assert_eq!(
                anclado
                    .entradas()
                    .iter()
                    .map(EntradaSnapshot::ruta)
                    .collect::<Vec<_>>(),
                por_path
                    .entradas()
                    .iter()
                    .map(EntradaSnapshot::ruta)
                    .collect::<Vec<_>>()
            );
            assert!(
                anclado
                    .entradas()
                    .windows(2)
                    .all(|par| { par[0].ruta().as_bytes() < par[1].ruta().as_bytes() })
            );
            assert_eq!(
                anclado.estructura().atomos(),
                por_path.estructura().atomos()
            );
        }

        let casos_invalidos = [
            ConfigRawCapturada::nueva(
                fs::read(&manifest).unwrap(),
                b"\n".as_slice(),
                Some("manifest.toml".to_owned()),
                Some("manifest.toml".to_owned()),
            ),
            ConfigRawCapturada::nueva(
                b"PRIVATE-CANARY mismatched config".as_slice(),
                b"\n".as_slice(),
                Some("manifest.toml".to_owned()),
                Some("invariants.smt2".to_owned()),
            ),
        ];
        for config in casos_invalidos {
            let error =
                capturar_snapshot_raw_anclado(&ancla, config, "engine/1", "policy/1").unwrap_err();
            assert_eq!(error, ErrorSnapshot::BundleInvalido);
            assert!(!format!("{error:?}").contains("PRIVATE-CANARY"));
            assert!(!error.to_string().contains("manifest.toml"));
        }
    }

    #[test]
    fn captura_anclada_rechaza_procedencia_ausente_o_no_regular_redactada() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        fs::write(root.path().join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.path().join("invariants.smt2"), "\n").unwrap();
        let ubicacion = UbicacionRaiz::nueva(root.path()).unwrap();
        let ancla = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();

        for ruta in ["PRIVATE-CANARY-missing.toml", "knowledge"] {
            let config = ConfigRawCapturada::nueva(
                MANIFIESTO.as_bytes(),
                b"\n".as_slice(),
                Some(ruta.to_owned()),
                None,
            );
            let error =
                capturar_snapshot_raw_anclado(&ancla, config, "engine/1", "policy/1").unwrap_err();
            assert_eq!(error, ErrorSnapshot::BundleInvalido);
            assert_eq!(error.to_string(), "snapshot_bundle_invalid");
            assert!(!format!("{error:?}").contains("PRIVATE-CANARY"));
            assert!(!format!("{error:?}").contains(ruta));
            assert!(!error.to_string().contains("PRIVATE-CANARY"));
            assert!(!error.to_string().contains(ruta));
        }
    }

    #[test]
    fn captura_anclada_usa_config_raw_capturada_aunque_cambien_sus_fuentes() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::write(root.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();
        let source = temp.path().join("config-source");
        fs::create_dir(&source).unwrap();
        let manifest = source.join("manifest.toml");
        let invariants = source.join("invariants.smt2");
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n").unwrap();

        let configuracion = ConfigRawCapturada::nueva(
            fs::read(&manifest).unwrap(),
            fs::read(&invariants).unwrap(),
            None,
            None,
        );
        fs::write(&manifest, b"PRIVATE-CANARY invalid manifest").unwrap();
        fs::write(&invariants, b"PRIVATE-CANARY invalid invariants").unwrap();

        let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
        let snapshot = materializar(
            capturar_snapshot_raw_anclado(
                &DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap(),
                configuracion,
                "engine/1",
                "policy/1",
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(snapshot.manifiesto_raw(), MANIFIESTO.as_bytes());
        assert_eq!(snapshot.invariantes_raw(), b"\n");
        assert_eq!(snapshot.estructura().nodos().count(), 1);
        assert!(!format!("{snapshot:?}").contains("PRIVATE-CANARY"));
    }

    #[test]
    fn captura_anclada_no_relee_config_externa_eliminada() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::write(root.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();
        let source = temp.path().join("PRIVATE-CANARY-config-source");
        fs::create_dir(&source).unwrap();
        let manifest = source.join("manifest.toml");
        let invariants = source.join("invariants.smt2");
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n").unwrap();

        let config = ConfigRawCapturada::nueva(
            fs::read(&manifest).unwrap(),
            fs::read(&invariants).unwrap(),
            None,
            None,
        );
        let debug_config = format!("{config:?}");
        fs::remove_file(&manifest).unwrap();
        fs::remove_file(&invariants).unwrap();
        fs::remove_dir(&source).unwrap();
        assert!(!source.exists());

        let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
        let snapshot = materializar(
            capturar_snapshot_raw_anclado(
                &DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap(),
                config,
                "engine/1",
                "policy/1",
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(snapshot.manifiesto_raw(), MANIFIESTO.as_bytes());
        assert_eq!(snapshot.invariantes_raw(), b"\n");
        assert_eq!(snapshot.estructura().nodos().count(), 1);
        assert!(!debug_config.contains("PRIVATE-CANARY"));
        assert!(!format!("{snapshot:?}").contains("PRIVATE-CANARY"));
    }

    #[test]
    fn snapshot_anclado_conserva_identidad_root_sin_incluirla_en_token() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        let moved = temp.path().join("bundle-old");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::write(root.join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.join("invariants.smt2"), "\n").unwrap();
        fs::write(root.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();
        let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
        let ancla = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();
        let identidad_original = ancla.identidad_raiz().unwrap();
        let config = ConfigRawCapturada::nueva(
            MANIFIESTO.as_bytes(),
            b"\n".as_slice(),
            Some("manifest.toml".to_owned()),
            Some("invariants.smt2".to_owned()),
        );
        let snapshot = materializar(
            capturar_snapshot_raw_anclado(&ancla, config, "engine/1", "policy/1").unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot.identidad_root(), Some(identidad_original));
        drop(ancla);

        fs::rename(&root, &moved).unwrap();
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::write(root.join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.join("invariants.smt2"), "\n").unwrap();
        fs::write(root.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();
        let nueva_ancla = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();
        let nueva_config = ConfigRawCapturada::nueva(
            MANIFIESTO.as_bytes(),
            b"\n".as_slice(),
            Some("manifest.toml".to_owned()),
            Some("invariants.smt2".to_owned()),
        );
        let nuevo_snapshot = materializar(
            capturar_snapshot_raw_anclado(&nueva_ancla, nueva_config, "engine/1", "policy/1")
                .unwrap(),
        )
        .unwrap();

        assert_eq!(
            nueva_ancla.identidad_raiz(),
            nuevo_snapshot.identidad_root()
        );
        assert_ne!(snapshot.identidad_root(), nuevo_snapshot.identidad_root());
        assert_eq!(snapshot.token(), nuevo_snapshot.token());
    }

    #[cfg(unix)]
    #[test]
    fn captura_anclada_no_sigue_un_ancestro_reemplazado_por_enlace() {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let moved = temp.path().join("parent-moved");
        let outside = temp.path().join("outside");
        let root = parent.join("stable").join("bundle");
        let outside_root = outside.join("stable").join("bundle");
        fs::create_dir(&parent).unwrap();
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::write(root.join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.join("invariants.smt2"), "\n").unwrap();
        fs::write(root.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();
        fs::create_dir_all(&outside_root).unwrap();
        fs::write(
            outside_root.join("external-canary.bin"),
            b"EXTERNAL-ROOT-CANARY",
        )
        .unwrap();

        let configuracion = ConfigRawCapturada::nueva(
            fs::read(root.join("manifest.toml")).unwrap(),
            fs::read(root.join("invariants.smt2")).unwrap(),
            Some("manifest.toml".to_owned()),
            Some("invariants.smt2".to_owned()),
        );
        let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
        let ancla = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();

        fs::rename(&parent, &moved).unwrap();
        crear_junction(&parent, &outside);

        let snapshot = materializar(
            capturar_snapshot_raw_anclado(&ancla, configuracion, "engine/1", "policy/1").unwrap(),
        )
        .unwrap();
        let rutas = snapshot
            .entradas()
            .iter()
            .map(EntradaSnapshot::ruta)
            .collect::<Vec<_>>();

        assert!(rutas.contains(&"knowledge"));
        assert!(rutas.contains(&"knowledge/a.md"));
        assert!(!rutas.iter().any(|ruta| ruta.contains("external-canary")));
        assert_eq!(
            fs::read(outside_root.join("external-canary.bin")).unwrap(),
            b"EXTERNAL-ROOT-CANARY"
        );
        assert_eq!(
            fs::read(moved.join("stable/bundle/knowledge/a.md")).unwrap(),
            b"---\ntype: a\ntitle: A\n---\n"
        );
    }

    #[cfg(windows)]
    #[test]
    fn handle_root_impide_reemplazar_su_ancestro_y_preserva_canary() {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let moved = temp.path().join("parent-moved");
        let outside = temp.path().join("outside");
        let root = parent.join("bundle");
        let outside_root = outside.join("bundle");
        fs::create_dir(&parent).unwrap();
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::write(root.join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.join("invariants.smt2"), "\n").unwrap();
        fs::write(root.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();
        fs::create_dir_all(&outside_root).unwrap();
        fs::write(
            outside_root.join("external-canary.bin"),
            b"EXTERNAL-ROOT-CANARY",
        )
        .unwrap();

        let configuracion = ConfigRawCapturada::nueva(
            fs::read(root.join("manifest.toml")).unwrap(),
            fs::read(root.join("invariants.smt2")).unwrap(),
            Some("manifest.toml".to_owned()),
            Some("invariants.smt2".to_owned()),
        );
        let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
        let ancla = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();

        let error = fs::rename(&parent, &moved).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        let snapshot = materializar(
            capturar_snapshot_raw_anclado(&ancla, configuracion, "engine/1", "policy/1").unwrap(),
        )
        .unwrap();
        assert!(
            snapshot
                .entradas()
                .iter()
                .all(|entrada| !entrada.ruta().contains("external-canary"))
        );
        assert_eq!(
            fs::read(outside_root.join("external-canary.bin")).unwrap(),
            b"EXTERNAL-ROOT-CANARY"
        );
    }

    #[test]
    fn cas_stale_precede_parseo_de_manifest_e_invariantes_invalidos() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        let manifiesto = root.path().join("manifest.toml");
        let invariantes = root.path().join("invariants.smt2");
        fs::write(&manifiesto, MANIFIESTO).unwrap();
        fs::write(&invariantes, "\n").unwrap();
        fs::write(
            root.path().join("knowledge/a.md"),
            "---\ntype: a\ntitle: A\n---\n",
        )
        .unwrap();
        let capturar = || {
            capturar_snapshot(
                root.path(),
                &manifiesto,
                &invariantes,
                "engine/1",
                "policy/1",
            )
            .unwrap()
        };
        let esperado = capturar().token();
        assert!(
            capturar_snapshot_con_cas(
                root.path(),
                &manifiesto,
                &invariantes,
                "engine/1",
                "policy/1",
                esperado,
            )
            .is_ok()
        );

        for (ruta, bytes) in [
            (&manifiesto, &b"\xffMANIFEST-CANARY"[..]),
            (&invariantes, &b"\xffINVARIANTS-CANARY"[..]),
        ] {
            fs::write(ruta, bytes).unwrap();
            let error = capturar_snapshot_con_cas(
                root.path(),
                &manifiesto,
                &invariantes,
                "engine/1",
                "policy/1",
                esperado,
            )
            .unwrap_err();
            assert_eq!(error, ErrorSnapshot::Stale);
            assert_eq!(error.codigo(), "stale");
            assert!(!format!("{error:?}").contains("CANARY"));
            assert!(!error.to_string().contains("CANARY"));
            fs::write(
                ruta,
                if ruta == &manifiesto {
                    MANIFIESTO.as_bytes()
                } else {
                    b"\n"
                },
            )
            .unwrap();
        }
    }

    #[test]
    fn cas_anclado_stale_precede_validacion_y_parseo_y_redacta_configuracion() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        let manifest = root.path().join("manifest.toml");
        let invariants = root.path().join("invariants.smt2");
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n").unwrap();
        fs::write(
            root.path().join("knowledge/a.md"),
            "---\ntype: a\ntitle: A\n---\n",
        )
        .unwrap();
        let ubicacion = UbicacionRaiz::nueva(root.path()).unwrap();
        let ancla = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();
        let validada = ConfigRawCapturada::nueva(
            MANIFIESTO.as_bytes(),
            b"\n".as_slice(),
            Some("manifest.toml".to_owned()),
            Some("invariants.smt2".to_owned()),
        );
        let esperado =
            capturar_snapshot_raw_anclado(&ancla, validada.clone(), "engine/1", "policy/1")
                .unwrap()
                .token;
        let snapshot = capturar_snapshot_con_cas_anclado(
            &ancla,
            validada.clone(),
            "engine/1",
            "policy/1",
            esperado,
        )
        .unwrap();
        assert_eq!(snapshot.token(), esperado);
        assert!(snapshot.identidad_root().is_some());

        fs::write(&manifest, b"PRIVATE-MISMATCH-CANARY root bytes changed").unwrap();
        let error =
            capturar_snapshot_con_cas_anclado(&ancla, validada, "engine/1", "policy/1", esperado)
                .unwrap_err();
        assert_eq!(error, ErrorSnapshot::Stale);
        assert_eq!(error.to_string(), "stale");
        assert!(!format!("{error:?}").contains("PRIVATE-MISMATCH-CANARY"));
        assert!(!format!("{error:?}").contains("manifest.toml"));
        assert!(!error.to_string().contains("PRIVATE-MISMATCH-CANARY"));
        assert!(!error.to_string().contains("manifest.toml"));

        for (config, canary) in [
            (
                ConfigRawCapturada::nueva(
                    MANIFIESTO.as_bytes(),
                    b"\n".as_slice(),
                    Some("PRIVATE-CANARY-missing.toml".to_owned()),
                    None,
                ),
                "PRIVATE-CANARY-missing.toml",
            ),
            (
                ConfigRawCapturada::nueva(
                    MANIFIESTO.as_bytes(),
                    b"\n".as_slice(),
                    Some("knowledge".to_owned()),
                    None,
                ),
                "knowledge",
            ),
            (
                ConfigRawCapturada::nueva(
                    b"PRIVATE-CANARY invalid local manifest".as_slice(),
                    b"\n".as_slice(),
                    Some("manifest.toml".to_owned()),
                    Some("invariants.smt2".to_owned()),
                ),
                "PRIVATE-CANARY",
            ),
        ] {
            let error =
                capturar_snapshot_con_cas_anclado(&ancla, config, "engine/1", "policy/1", esperado)
                    .unwrap_err();
            assert_eq!(error, ErrorSnapshot::Stale);
            assert_eq!(error.to_string(), "stale");
            assert!(!format!("{error:?}").contains(canary));
            assert!(!format!("{error:?}").contains("manifest.toml"));
            assert!(!format!("{error:?}").contains("invariants.smt2"));
            assert!(!error.to_string().contains(canary));
            assert!(!error.to_string().contains("manifest.toml"));
            assert!(!error.to_string().contains("invariants.smt2"));
        }

        fs::write(&manifest, MANIFIESTO).unwrap();
        for (manifiesto, invariantes, canary) in [
            (
                b"PRIVATE-MANIFEST-CANARY invalid".as_slice(),
                b"\n".as_slice(),
                "PRIVATE-MANIFEST-CANARY",
            ),
            (
                MANIFIESTO.as_bytes(),
                b"PRIVATE-INVARIANT-CANARY invalid".as_slice(),
                "PRIVATE-INVARIANT-CANARY",
            ),
        ] {
            let invalida = ConfigRawCapturada::nueva(manifiesto, invariantes, None, None);
            let error = capturar_snapshot_con_cas_anclado(
                &ancla, invalida, "engine/1", "policy/1", esperado,
            )
            .unwrap_err();
            assert_eq!(error, ErrorSnapshot::Stale);
            assert_eq!(error.to_string(), "stale");
            assert!(!format!("{error:?}").contains(canary));
            assert!(!error.to_string().contains(canary));
        }
    }

    #[test]
    fn cas_bajo_lock_materializa_snapshot_con_token_esperado() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        fs::write(root.path().join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.path().join("invariants.smt2"), "\n").unwrap();
        fs::write(
            root.path().join("knowledge/a.md"),
            "---\ntype: a\ntitle: A\n---\n",
        )
        .unwrap();
        let configuracion = ConfigRawCapturada::nueva(
            MANIFIESTO.as_bytes(),
            b"\n".as_slice(),
            Some("manifest.toml".to_owned()),
            Some("invariants.smt2".to_owned()),
        );

        let mut lock = LockEscritura::adquirir(root.path()).unwrap();
        let _: fn(
            &mut LockEscritura,
            ConfigRawCapturada,
            &str,
            &str,
            SnapshotToken,
        ) -> Result<Snapshot, ErrorSnapshot> = capturar_snapshot_con_cas_bajo_lock;
        let ancla = DirectorioAnclado::desde_ubicacion(lock.ubicacion()).unwrap();
        let esperado =
            capturar_snapshot_raw_anclado(&ancla, configuracion.clone(), "engine/1", "policy/1")
                .unwrap()
                .token;

        let snapshot = capturar_snapshot_con_cas_bajo_lock(
            &mut lock,
            configuracion,
            "engine/1",
            "policy/1",
            esperado,
        )
        .unwrap();

        assert_eq!(snapshot.token(), esperado);
        assert!(snapshot.identidad_root().is_some());
        assert_eq!(snapshot.manifiesto_raw(), MANIFIESTO.as_bytes());
        assert_eq!(snapshot.invariantes_raw(), b"\n");
        assert_eq!(snapshot.estructura().nodos().count(), 1);
        assert!(snapshot.fuentes_invariantes().is_empty());
        assert_eq!(
            LockEscritura::adquirir(root.path()).unwrap_err(),
            ErrorLockEscritura::Busy,
            "el helper debe dejar el guard retenido por quien lo recibió"
        );
    }

    #[test]
    fn cas_bajo_lock_rechaza_snapshot_stale_tras_otra_epoca_de_escritura() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        fs::write(root.path().join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.path().join("invariants.smt2"), "\n").unwrap();
        let documento = root.path().join("knowledge/a.md");
        fs::write(&documento, "---\ntype: a\ntitle: A\n---\n").unwrap();
        let configuracion = ConfigRawCapturada::nueva(
            MANIFIESTO.as_bytes(),
            b"\n".as_slice(),
            Some("manifest.toml".to_owned()),
            Some("invariants.smt2".to_owned()),
        );

        let esperado = {
            let lock = LockEscritura::adquirir(root.path()).unwrap();
            let ancla = DirectorioAnclado::desde_ubicacion(lock.ubicacion()).unwrap();
            capturar_snapshot_raw_anclado(&ancla, configuracion.clone(), "engine/1", "policy/1")
                .unwrap()
                .token
        };

        {
            let _epoca_mutadora = LockEscritura::adquirir(root.path()).unwrap();
            fs::write(&documento, b"\xffPRIVATE-CANARY invalid markdown").unwrap();
        }

        let contenido_antes = fs::read(&documento).unwrap();
        let mut nombres_antes = fs::read_dir(root.path())
            .unwrap()
            .map(|entrada| entrada.unwrap().file_name())
            .collect::<Vec<_>>();
        nombres_antes.sort();
        let mut lock = LockEscritura::adquirir(root.path()).unwrap();
        let error = capturar_snapshot_con_cas_bajo_lock(
            &mut lock,
            configuracion,
            "engine/1",
            "policy/1",
            esperado,
        )
        .unwrap_err();

        assert_eq!(error, ErrorSnapshot::Stale);
        assert_eq!(error.codigo(), "stale");
        let debug = format!("{error:?}");
        let display = error.to_string();
        for secreto in [
            "PRIVATE-CANARY",
            "knowledge/a.md",
            root.path().to_string_lossy().as_ref(),
        ] {
            assert!(!debug.contains(secreto), "Debug leaked {secreto}");
            assert!(!display.contains(secreto), "Display leaked {secreto}");
        }
        assert_eq!(fs::read(&documento).unwrap(), contenido_antes);
        let mut nombres_despues = fs::read_dir(root.path())
            .unwrap()
            .map(|entrada| entrada.unwrap().file_name())
            .collect::<Vec<_>>();
        nombres_despues.sort();
        assert_eq!(nombres_despues, nombres_antes);
    }

    #[test]
    fn replay_y_cambios_raw_auxiliares_y_directorios_cambian_la_identidad() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        let manifest = temp.path().join("manifest.toml");
        let invariants = temp.path().join("invariants.smt2");
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n").unwrap();
        fs::write(root.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();
        fs::write(root.join("knowledge/ignored.md"), [0xff, 0x00]).unwrap();
        fs::write(root.join("payload.bin"), []).unwrap();

        let capturar =
            || capturar_snapshot(&root, &manifest, &invariants, "engine/1", "policy/1").unwrap();
        let base = capturar();
        let replay = capturar();
        assert_eq!(base.token(), replay.token());
        assert_eq!(base.estructura().nodos().count(), 1);
        assert!(base.fuentes_invariantes().is_empty());
        assert!(base.entradas().iter().any(|entry| {
            entry.ruta() == "knowledge/ignored.md" && entry.bytes() == Some([0xff, 0x00].as_slice())
        }));

        fs::write(root.join("payload.bin"), [0, 255, 1]).unwrap();
        let binary_changed = capturar().token();
        assert_ne!(base.token(), binary_changed);
        fs::write(&manifest, format!("{MANIFIESTO}\n")).unwrap();
        let manifest_changed = capturar().token();
        assert_ne!(binary_changed, manifest_changed);
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n\n").unwrap();
        let invariants_changed = capturar().token();
        assert_ne!(binary_changed, invariants_changed);

        fs::write(&invariants, "\n").unwrap();
        fs::create_dir(root.join("empty-dir")).unwrap();
        assert_ne!(base.token(), capturar().token());
    }

    #[test]
    fn captura_detecta_altas_bajas_renombres_y_cambios_solo_de_case() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        let manifest = temp.path().join("manifest.toml");
        let invariants = temp.path().join("invariants.smt2");
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n").unwrap();
        fs::write(root.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();

        let capturar =
            || capturar_snapshot(&root, &manifest, &invariants, "engine/1", "policy/1").unwrap();
        let base = capturar().token();

        let archivo = root.join("auxiliar.bin");
        fs::write(&archivo, []).unwrap();
        let agregado = capturar().token();
        assert_ne!(
            base, agregado,
            "agregar un archivo vacío debe cambiar el token"
        );
        fs::remove_file(&archivo).unwrap();
        assert_eq!(
            base,
            capturar().token(),
            "eliminar el archivo revierte el token"
        );

        let original = root.join("original.bin");
        let renombrado = root.join("renombrado.bin");
        fs::write(&original, [0, 255]).unwrap();
        let antes_renombrar = capturar().token();
        fs::rename(&original, &renombrado).unwrap();
        assert_ne!(antes_renombrar, capturar().token());
        fs::rename(&renombrado, &original).unwrap();
        assert_eq!(antes_renombrar, capturar().token());

        let mayusculas = root.join("Case.bin");
        let intermedio = root.join("case-temp.bin");
        let minusculas = root.join("case.bin");
        fs::write(&mayusculas, [7]).unwrap();
        let antes_case = capturar().token();
        // El paso intermedio hace que el renombre solo de case funcione también
        // en volúmenes Windows case-insensitive.
        fs::rename(&mayusculas, &intermedio).unwrap();
        fs::rename(&intermedio, &minusculas).unwrap();
        assert_ne!(antes_case, capturar().token());
    }

    #[cfg(windows)]
    #[test]
    fn alias_de_case_para_manifest_e_invariants_reusa_los_buffers_capturados() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        fs::write(root.path().join("manifest.toml"), MANIFIESTO).unwrap();
        fs::write(root.path().join("invariants.smt2"), "\n").unwrap();
        fs::write(
            root.path().join("knowledge/a.md"),
            "---\ntype: a\ntitle: A\n---\n",
        )
        .unwrap();
        // Cambia también el case de los componentes del root: con el root
        // lexical original, cambiar solo el nombre final no prueba el caso en
        // que `starts_with` clasifica la fuente como externa.
        let root_case_alias = PathBuf::from(root.path().to_string_lossy().to_ascii_uppercase());
        let mut lecturas = HashMap::<PathBuf, usize>::new();
        let snapshot = capturar_snapshot_con_lector(
            root.path(),
            &root_case_alias.join("MANIFEST.TOML"),
            &root_case_alias.join("INVARIANTS.SMT2"),
            "engine/1",
            "policy/1",
            |path| {
                let real = fs::canonicalize(path).unwrap();
                *lecturas.entry(real).or_default() += 1;
                leer_archivo_regular(path)
            },
        )
        .unwrap();

        assert_eq!(lecturas.len(), 3);
        assert!(lecturas.values().all(|&count| count == 1));
        assert_eq!(snapshot.manifiesto_raw(), MANIFIESTO.as_bytes());
        assert_eq!(snapshot.invariantes_raw(), b"\n");
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_links_usados_como_ruta_de_manifest_o_invariants() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        let outside = temp.path().join("outside");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::create_dir(&outside).unwrap();
        let manifest_target = outside.join("manifest.toml");
        let invariants_target = outside.join("invariants.smt2");
        let manifest_link = temp.path().join("manifest-link.toml");
        let invariants_link = temp.path().join("invariants-link.smt2");
        fs::write(&manifest_target, MANIFIESTO).unwrap();
        fs::write(&invariants_target, "\n").unwrap();
        symlink(&manifest_target, &manifest_link).unwrap();
        symlink(&invariants_target, &invariants_link).unwrap();

        assert_eq!(
            capturar_snapshot(
                &root,
                &manifest_link,
                &invariants_target,
                "engine/1",
                "policy/1"
            )
            .unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
        assert_eq!(
            capturar_snapshot(
                &root,
                &manifest_target,
                &invariants_link,
                "engine/1",
                "policy/1"
            )
            .unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
    }

    #[cfg(windows)]
    #[test]
    fn rechaza_reparse_points_en_root_y_en_rutas_de_configuracion() {
        use std::os::windows::fs::{symlink_dir, symlink_file};
        use std::process::Command;

        fn junction(enlace: &Path, destino: &Path) {
            if let Err(error) = symlink_dir(destino, enlace) {
                eprintln!("aviso: symlink_dir no disponible ({error}); se usa junction");
                let resultado = Command::new("cmd")
                    .args(["/c", "mklink", "/J"])
                    .arg(enlace)
                    .arg(destino)
                    .output()
                    .expect("cmd debe poder crear junctions");
                assert!(
                    resultado.status.success(),
                    "no se pudo crear junction: {}",
                    String::from_utf8_lossy(&resultado.stderr)
                );
            }
        }

        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        let config_target = temp.path().join("config-target");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::create_dir(&config_target).unwrap();
        let manifest = config_target.join("manifest.toml");
        let invariants = config_target.join("invariants.smt2");
        fs::write(&manifest, MANIFIESTO).unwrap();
        fs::write(&invariants, "\n").unwrap();
        let outside_file = temp.path().join("outside.bin");
        fs::write(&outside_file, b"raw").unwrap();

        let file_link = root.join("file-link.bin");
        match symlink_file(&outside_file, &file_link) {
            Ok(()) => {
                assert_eq!(
                    capturar_snapshot(&root, &manifest, &invariants, "engine/1", "policy/1")
                        .unwrap_err(),
                    ErrorSnapshot::EnlaceNoPermitido
                );
                fs::remove_file(file_link).unwrap();
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    || error.raw_os_error() == Some(1314) =>
            {
                eprintln!(
                    "SKIP explícito: symlink_file no disponible ({error}); este subcaso no cuenta como cobertura"
                );
            }
            Err(error) => panic!("no se pudo crear symlink_file: {error}"),
        }

        let root_link = root.join("directory-link");
        junction(&root_link, &config_target);
        assert_eq!(
            capturar_snapshot(&root, &manifest, &invariants, "engine/1", "policy/1").unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
        fs::remove_dir(root_link).unwrap();

        let alias = temp.path().join("config-alias");
        junction(&alias, &config_target);
        assert_eq!(
            capturar_snapshot(
                &root,
                &alias.join("manifest.toml"),
                &invariants,
                "engine/1",
                "policy/1"
            )
            .unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
        assert_eq!(
            capturar_snapshot(
                &root,
                &manifest,
                &alias.join("invariants.smt2"),
                "engine/1",
                "policy/1"
            )
            .unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
    }

    #[test]
    fn error_de_parseo_no_revela_contenido_raw() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("knowledge")).unwrap();
        let manifest = root.path().join("manifest.toml");
        let invariants = root.path().join("invariants.smt2");
        fs::write(&manifest, "PRIVATE-CANARY invalid manifest").unwrap();
        fs::write(&invariants, "").unwrap();

        let error = capturar_snapshot(root.path(), &manifest, &invariants, "engine/1", "policy/1")
            .unwrap_err();
        assert_eq!(error, ErrorSnapshot::ManifiestoInvalido);
        assert!(!format!("{error:?}").contains("PRIVATE-CANARY"));
        assert!(!error.to_string().contains("PRIVATE-CANARY"));
    }

    #[test]
    #[ignore = "harness runtime; ejecutar en release con --ignored --nocapture"]
    fn runtime_snapshot_mide_bundles_de_1_10_y_100_mib() {
        use std::io::Write;
        use std::time::Instant;

        const MIB: usize = 1024 * 1024;
        const DOCUMENTO: &str = "---\ntype: a\ntitle: A\n---\n";
        const MOTOR: &str = "runtime/1";
        const POLITICA: &str = "policy/1";

        fn escribir_payload(path: &Path, bytes: usize) {
            let mut archivo = fs::File::create(path).unwrap();
            let mut bloque = [0_u8; 64 * 1024];
            for (indice, byte) in bloque.iter_mut().enumerate() {
                *byte = (indice % 251) as u8;
            }
            let mut restantes = bytes;
            while restantes > 0 {
                let tamano = restantes.min(bloque.len());
                archivo.write_all(&bloque[..tamano]).unwrap();
                restantes -= tamano;
            }
        }

        fn bytes_archivos(snapshot: &Snapshot) -> usize {
            snapshot
                .entradas()
                .iter()
                .filter_map(EntradaSnapshot::bytes)
                .map(|bytes| bytes.len())
                .sum()
        }

        fn firma_estructura(snapshot: &Snapshot) -> Vec<String> {
            snapshot
                .estructura()
                .atomos()
                .iter()
                .map(ToString::to_string)
                .collect()
        }

        for mib in [1, 10, 100] {
            let bytes_objetivo = mib * MIB;
            let temporal = tempdir().unwrap();
            let raiz = temporal.path();
            fs::create_dir(raiz.join("knowledge")).unwrap();
            fs::create_dir(raiz.join("aux")).unwrap();
            fs::write(raiz.join("manifest.toml"), MANIFIESTO).unwrap();
            fs::write(raiz.join("invariants.smt2"), "\n").unwrap();
            fs::write(raiz.join("knowledge/a.md"), DOCUMENTO).unwrap();

            let base = MANIFIESTO.len() + 1 + DOCUMENTO.len();
            assert!(bytes_objetivo > base);
            escribir_payload(&raiz.join("aux/payload.bin"), bytes_objetivo - base);

            let inicio = Instant::now();
            let snapshot = capturar_snapshot(
                raiz,
                &raiz.join("manifest.toml"),
                &raiz.join("invariants.smt2"),
                MOTOR,
                POLITICA,
            )
            .unwrap();
            let captura = inicio.elapsed();

            assert_eq!(snapshot.entradas().len(), 6, "inventario y directorios");
            assert_eq!(bytes_archivos(&snapshot), bytes_objetivo, "bytes exactos");
            assert_eq!(snapshot.manifiesto_raw(), MANIFIESTO.as_bytes());
            assert_eq!(snapshot.invariantes_raw(), b"\n");
            assert_eq!(snapshot.estructura().nodos().count(), 1);
            assert_eq!(snapshot.fuentes_invariantes().len(), 0);
            let firma = firma_estructura(&snapshot);
            let token = snapshot.token();

            let inicio_framing = Instant::now();
            let token_recalculado = calcular_token_snapshot(
                MOTOR,
                POLITICA,
                snapshot.manifiesto_raw(),
                snapshot.invariantes_raw(),
                snapshot.entradas(),
            )
            .unwrap();
            let framing = inicio_framing.elapsed();
            assert_eq!(token_recalculado, token, "framing sobre buffers capturados");
            drop(snapshot);

            let inicio_replay = Instant::now();
            let replay = capturar_snapshot(
                raiz,
                &raiz.join("manifest.toml"),
                &raiz.join("invariants.smt2"),
                MOTOR,
                POLITICA,
            )
            .unwrap();
            let duracion_replay = inicio_replay.elapsed();
            assert_eq!(replay.token(), token, "replay del token");
            assert_eq!(bytes_archivos(&replay), bytes_objetivo, "replay de bytes");
            assert_eq!(replay.estructura().nodos().count(), 1);
            assert_eq!(firma_estructura(&replay), firma, "replay de estructura");

            let mb_por_segundo = bytes_objetivo as f64 / captura.as_secs_f64() / 1_000_000.0;
            let porcentaje_framing = framing.as_secs_f64() / captura.as_secs_f64() * 100.0;
            println!(
                "Runtime U4: {mib} MiB, bytes={bytes_objetivo}, captura={:.3} ms, MB/s={mb_por_segundo:.2}, framing={:.3} ms ({porcentaje_framing:.1}% de captura), replay={:.3} ms, token/replay/estructura=OK (orientativo; sin SLA)",
                captura.as_secs_f64() * 1_000.0,
                framing.as_secs_f64() * 1_000.0,
                duracion_replay.as_secs_f64() * 1_000.0,
            );
        }
    }

    #[cfg(unix)]
    fn crear_junction(enlace: &Path, destino: &Path) {
        std::os::unix::fs::symlink(destino, enlace).unwrap();
    }
}
