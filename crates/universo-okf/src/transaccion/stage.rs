//! Plan puro de staging desde inventario raw, sin raíz ni operaciones de filesystem.
//! En Windows las colisiones usan `CompareStringOrdinal`; en Unix, los paths
//! conservan identidad exacta y sensible a case, incluidos sus bytes Unicode.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use universo_core::BundlePath;

use crate::serializar::DocumentoCandidato;

use super::captura::Snapshot;
use super::raiz::{UbicacionRaiz, metadata_es_enlace_o_reparse};
use super::snapshot::{EntradaSnapshot, TipoEntradaSnapshot};

#[cfg(windows)]
use std::cmp::Ordering;

#[cfg(windows)]
use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
#[cfg(windows)]
use windows_sys::Win32::Globalization::{CSTR_GREATER_THAN, CSTR_LESS_THAN};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorPlanStaging {
    RutaInvalida,
    EntradaDuplicada,
    ColisionCaseInsensitive,
    ConflictoArchivoDirectorio,
    ColisionAuxiliarAdministrado,
    SnapshotInconsistente,
}

impl ErrorPlanStaging {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::RutaInvalida => "stage_path_invalid",
            Self::EntradaDuplicada => "stage_duplicate_entry",
            Self::ColisionCaseInsensitive => "stage_case_insensitive_collision",
            Self::ConflictoArchivoDirectorio => "stage_file_directory_conflict",
            Self::ColisionAuxiliarAdministrado => "stage_managed_auxiliary_collision",
            Self::SnapshotInconsistente => "stage_snapshot_inconsistent",
        }
    }
}

impl fmt::Debug for ErrorPlanStaging {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorPlanStaging")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorPlanStaging {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorPlanStaging {}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Lo consume el flujo apply en la siguiente unidad.
enum CodigoEscrituraStage {
    Plan(ErrorPlanStaging),
    Aleatorio,
    Io,
    EnlaceNoPermitido,
    TipoNoAdmitido,
    ConflictoEntrada,
    ColisionesAgotadas,
}

#[allow(dead_code)] // Lo consume el flujo apply en la siguiente unidad.
impl CodigoEscrituraStage {
    const fn codigo(self) -> &'static str {
        match self {
            Self::Plan(error) => error.codigo(),
            Self::Aleatorio => "stage_random_unavailable",
            Self::Io => "stage_io",
            Self::EnlaceNoPermitido => "stage_link_not_allowed",
            Self::TipoNoAdmitido => "stage_entry_type_not_allowed",
            Self::ConflictoEntrada => "stage_create_conflict",
            Self::ColisionesAgotadas => "stage_name_collision",
        }
    }
}

/// Sibling stage creado exclusivamente por este módulo. Su dueño debe decidir
/// cleanup; no hay Drop/remove_dir_all basado solo en el nombre path-based.
#[allow(dead_code)] // Se consume desde apply/recovery en unidades posteriores.
pub(crate) struct StagePropio {
    ruta: PathBuf,
}

#[allow(dead_code)] // Se consume desde apply/recovery en unidades posteriores.
impl StagePropio {
    pub(crate) fn ruta(&self) -> &Path {
        &self.ruta
    }
}

impl fmt::Debug for StagePropio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StagePropio { ruta: [redacted] }")
    }
}

/// Error estable/redactado. Si `stage_parcial` existe, el llamador posee el
/// sibling creado y puede conservarlo para cleanup posterior verificable.
#[allow(dead_code)] // Se consume desde apply/recovery en unidades posteriores.
pub(crate) struct ErrorEscrituraStage {
    codigo: CodigoEscrituraStage,
    stage_parcial: Option<StagePropio>,
}

#[allow(dead_code)] // Se consume desde apply/recovery en unidades posteriores.
impl ErrorEscrituraStage {
    pub(crate) const fn codigo(&self) -> &'static str {
        self.codigo.codigo()
    }

    pub(crate) fn stage_parcial(&self) -> Option<&StagePropio> {
        self.stage_parcial.as_ref()
    }

    fn nuevo(codigo: CodigoEscrituraStage) -> Self {
        Self {
            codigo,
            stage_parcial: None,
        }
    }
}

impl fmt::Debug for ErrorEscrituraStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorEscrituraStage")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorEscrituraStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorEscrituraStage {}

#[allow(dead_code)] // La integración transaccional consume el límite de retry.
const INTENTOS_STAGE: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FuenteEntradaStage {
    Directorio,
    Auxiliar(usize),
    DocumentoNuevo(usize),
}

#[allow(dead_code)] // El consumidor de staging se implementa en la siguiente unidad.
pub(crate) struct EntradaStage {
    pub(crate) ruta: String,
    pub(crate) fuente: FuenteEntradaStage,
}

impl fmt::Debug for EntradaStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EntradaStage { .. }")
    }
}

#[allow(dead_code)] // El consumidor de staging se implementa en la siguiente unidad.
pub(crate) struct PlanStaging {
    pub(crate) directorios: Vec<String>,
    pub(crate) auxiliares: Vec<EntradaSnapshot>,
    pub(crate) managed_old: Vec<String>,
    pub(crate) managed_new: Vec<DocumentoCandidato>,
    pub(crate) removed: Vec<String>,
    pub(crate) entradas_objetivo: Vec<EntradaStage>,
}

impl fmt::Debug for PlanStaging {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlanStaging { .. }")
    }
}

#[allow(dead_code)] // El builder de staging lo consume en la siguiente unidad.
pub(crate) fn planificar_staging(
    snapshot: &Snapshot,
    documentos: &[DocumentoCandidato],
) -> Result<PlanStaging, ErrorPlanStaging> {
    let managed_old = snapshot
        .estructura()
        .nodos()
        .map(|(_, nodo)| nodo.path.clone())
        .collect::<Vec<_>>();
    planificar_staging_desde_inventario(snapshot.entradas(), &managed_old, documentos)
}

/// Materializa un plan desde los buffers raw en un sibling nuevo. El parent
/// validado se asume estable durante la operación: esta API path-based no debe
/// alimentar apply hasta que parent y stage queden anclados a handles.
#[allow(dead_code)] // Se integra a apply en la siguiente unidad.
pub(crate) fn materializar_staging(
    ubicacion: &UbicacionRaiz,
    plan: &PlanStaging,
) -> Result<StagePropio, ErrorEscrituraStage> {
    materializar_staging_con(ubicacion, plan, generar_id_stage, escribir_archivo_nuevo)
}

#[allow(dead_code)] // Se mantiene como seam de fault injection para pruebas.
fn materializar_staging_con<G, W>(
    ubicacion: &UbicacionRaiz,
    plan: &PlanStaging,
    mut siguiente_id: G,
    mut escribir_archivo: W,
) -> Result<StagePropio, ErrorEscrituraStage>
where
    G: FnMut() -> Result<String, CodigoEscrituraStage>,
    W: FnMut(&Path, &[u8]) -> Result<(), CodigoEscrituraStage>,
{
    validar_plan_para_escritura(plan).map_err(ErrorEscrituraStage::nuevo)?;
    let stage = crear_stage_unico(ubicacion, &mut siguiente_id)?;
    if let Err(codigo) = escribir_plan_en_stage(&stage, plan, &mut escribir_archivo) {
        return Err(ErrorEscrituraStage {
            codigo,
            stage_parcial: Some(stage),
        });
    }
    Ok(stage)
}

#[allow(dead_code)] // Se consume desde materializar_staging.
fn crear_stage_unico<G>(
    ubicacion: &UbicacionRaiz,
    siguiente_id: &mut G,
) -> Result<StagePropio, ErrorEscrituraStage>
where
    G: FnMut() -> Result<String, CodigoEscrituraStage>,
{
    for _ in 0..INTENTOS_STAGE {
        let id = siguiente_id().map_err(ErrorEscrituraStage::nuevo)?;
        let ruta = ubicacion.stage(&id).map_err(|_| {
            ErrorEscrituraStage::nuevo(CodigoEscrituraStage::Plan(ErrorPlanStaging::RutaInvalida))
        })?;
        match fs::create_dir(ruta.as_path()) {
            Ok(()) => {
                let stage = StagePropio {
                    ruta: ruta.as_path().to_path_buf(),
                };
                if let Err(codigo) = validar_directorio_stage(stage.ruta()) {
                    return Err(ErrorEscrituraStage {
                        codigo,
                        stage_parcial: Some(stage),
                    });
                }
                return Ok(stage);
            }
            // Never inspect, reuse, or remove a preexisting object. It may be
            // a file, link, reparse point, or another transaction's stage.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(ErrorEscrituraStage::nuevo(CodigoEscrituraStage::Io)),
        }
    }
    Err(ErrorEscrituraStage::nuevo(
        CodigoEscrituraStage::ColisionesAgotadas,
    ))
}

#[allow(dead_code)] // Se consume desde materializar_staging.
fn generar_id_stage() -> Result<String, CodigoEscrituraStage> {
    let mut aleatorio = [0_u8; 16];
    getrandom::fill(&mut aleatorio).map_err(|_| CodigoEscrituraStage::Aleatorio)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut id = String::with_capacity(aleatorio.len() * 2);
    for byte in aleatorio {
        id.push(HEX[(byte >> 4) as usize] as char);
        id.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(id)
}

#[allow(dead_code)] // Se consume desde materializar_staging.
fn validar_plan_para_escritura(plan: &PlanStaging) -> Result<(), CodigoEscrituraStage> {
    let mut directorios = BTreeSet::new();
    for ruta in &plan.directorios {
        validar_ruta_relativa(ruta).map_err(CodigoEscrituraStage::Plan)?;
        if !directorios.insert(ruta.as_str()) {
            return Err(CodigoEscrituraStage::Plan(
                ErrorPlanStaging::EntradaDuplicada,
            ));
        }
    }

    let mut entradas = BTreeSet::new();
    let mut directorios_vistos = BTreeSet::new();
    for entrada in &plan.entradas_objetivo {
        validar_ruta_relativa(&entrada.ruta).map_err(CodigoEscrituraStage::Plan)?;
        if !entradas.insert(entrada.ruta.as_str()) {
            return Err(CodigoEscrituraStage::Plan(
                ErrorPlanStaging::EntradaDuplicada,
            ));
        }
        match entrada.fuente {
            FuenteEntradaStage::Directorio => {
                if !directorios.contains(entrada.ruta.as_str()) {
                    return Err(CodigoEscrituraStage::Plan(
                        ErrorPlanStaging::SnapshotInconsistente,
                    ));
                }
                directorios_vistos.insert(entrada.ruta.as_str());
            }
            FuenteEntradaStage::Auxiliar(indice) => {
                let auxiliar = plan
                    .auxiliares
                    .get(indice)
                    .ok_or(CodigoEscrituraStage::Plan(
                        ErrorPlanStaging::SnapshotInconsistente,
                    ))?;
                if auxiliar.tipo() != TipoEntradaSnapshot::Archivo
                    || auxiliar.bytes().is_none()
                    || auxiliar.ruta() != entrada.ruta
                {
                    return Err(CodigoEscrituraStage::Plan(
                        ErrorPlanStaging::SnapshotInconsistente,
                    ));
                }
            }
            FuenteEntradaStage::DocumentoNuevo(indice) => {
                let documento = plan
                    .managed_new
                    .get(indice)
                    .ok_or(CodigoEscrituraStage::Plan(
                        ErrorPlanStaging::SnapshotInconsistente,
                    ))?;
                if ruta_documento(documento.path()) != entrada.ruta {
                    return Err(CodigoEscrituraStage::Plan(
                        ErrorPlanStaging::SnapshotInconsistente,
                    ));
                }
            }
        }
        for padre in directorios_padre(&entrada.ruta) {
            if !directorios.contains(padre) {
                return Err(CodigoEscrituraStage::Plan(
                    ErrorPlanStaging::SnapshotInconsistente,
                ));
            }
        }
    }
    if directorios.len() != directorios_vistos.len()
        || directorios
            .iter()
            .any(|ruta| !directorios_vistos.contains(ruta))
    {
        return Err(CodigoEscrituraStage::Plan(
            ErrorPlanStaging::SnapshotInconsistente,
        ));
    }
    #[cfg(windows)]
    validar_colisiones_case_insensitive(plan.entradas_objetivo.iter().map(|e| e.ruta.as_str()))
        .map_err(CodigoEscrituraStage::Plan)?;
    Ok(())
}

#[allow(dead_code)] // Se consume desde materializar_staging.
fn escribir_plan_en_stage<W>(
    stage: &StagePropio,
    plan: &PlanStaging,
    mut escribir_archivo: W,
) -> Result<(), CodigoEscrituraStage>
where
    W: FnMut(&Path, &[u8]) -> Result<(), CodigoEscrituraStage>,
{
    validar_directorio_stage(stage.ruta())?;
    let mut directorios = plan.directorios.iter().collect::<Vec<_>>();
    directorios.sort_by(|a, b| {
        a.matches('/')
            .count()
            .cmp(&b.matches('/').count())
            .then_with(|| a.as_bytes().cmp(b.as_bytes()))
    });
    for ruta in directorios {
        let destino = unir_ruta(stage.ruta(), ruta);
        validar_padres_stage(stage.ruta(), ruta)?;
        fs::create_dir(&destino).map_err(mapear_error_creacion)?;
        validar_directorio_stage(&destino)?;
    }

    for entrada in &plan.entradas_objetivo {
        let bytes = match entrada.fuente {
            FuenteEntradaStage::Directorio => continue,
            FuenteEntradaStage::Auxiliar(indice) => {
                plan.auxiliares[indice]
                    .bytes()
                    .ok_or(CodigoEscrituraStage::Plan(
                        ErrorPlanStaging::SnapshotInconsistente,
                    ))?
            }
            FuenteEntradaStage::DocumentoNuevo(indice) => {
                plan.managed_new[indice].contenido().as_bytes()
            }
        };
        validar_padres_stage(stage.ruta(), &entrada.ruta)?;
        escribir_archivo(&unir_ruta(stage.ruta(), &entrada.ruta), bytes)?;
    }
    Ok(())
}

#[allow(dead_code)] // Se consume desde materializar_staging.
fn escribir_archivo_nuevo(path: &Path, bytes: &[u8]) -> Result<(), CodigoEscrituraStage> {
    let mut archivo = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(mapear_error_creacion)?;
    let metadata = archivo.metadata().map_err(|_| CodigoEscrituraStage::Io)?;
    if metadata_es_enlace_o_reparse(&metadata) {
        return Err(CodigoEscrituraStage::EnlaceNoPermitido);
    }
    if !metadata.is_file() {
        return Err(CodigoEscrituraStage::TipoNoAdmitido);
    }
    archivo
        .write_all(bytes)
        .map_err(|_| CodigoEscrituraStage::Io)
}

#[allow(dead_code)] // Se consume desde el escritor del stage.
fn mapear_error_creacion(error: std::io::Error) -> CodigoEscrituraStage {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        CodigoEscrituraStage::ConflictoEntrada
    } else {
        CodigoEscrituraStage::Io
    }
}

#[allow(dead_code)] // Se consume desde el escritor del stage.
fn validar_directorio_stage(path: &Path) -> Result<(), CodigoEscrituraStage> {
    let metadata = fs::symlink_metadata(path).map_err(|_| CodigoEscrituraStage::Io)?;
    if metadata_es_enlace_o_reparse(&metadata) {
        return Err(CodigoEscrituraStage::EnlaceNoPermitido);
    }
    if !metadata.is_dir() {
        return Err(CodigoEscrituraStage::TipoNoAdmitido);
    }
    Ok(())
}

#[allow(dead_code)] // Se consume desde el escritor del stage.
fn validar_padres_stage(stage: &Path, ruta: &str) -> Result<(), CodigoEscrituraStage> {
    validar_directorio_stage(stage)?;
    let Some((padres, _)) = ruta.rsplit_once('/') else {
        return Ok(());
    };
    let mut actual = stage.to_path_buf();
    for componente in padres.split('/') {
        actual.push(componente);
        validar_directorio_stage(&actual)?;
    }
    Ok(())
}

#[allow(dead_code)] // Se consume desde el escritor del stage.
fn unir_ruta(base: &Path, ruta: &str) -> PathBuf {
    let mut resultado = base.to_path_buf();
    for componente in ruta.split('/') {
        resultado.push(componente);
    }
    resultado
}

fn planificar_staging_desde_inventario(
    entradas: &[EntradaSnapshot],
    managed_old: &[BundlePath],
    documentos: &[DocumentoCandidato],
) -> Result<PlanStaging, ErrorPlanStaging> {
    let mut tipos_raw = BTreeMap::new();
    for entrada in entradas {
        let ruta = entrada.ruta();
        validar_ruta_relativa(ruta)?;
        let tipo = entrada.tipo();
        let forma_valida = match tipo {
            TipoEntradaSnapshot::Archivo => entrada.bytes().is_some(),
            TipoEntradaSnapshot::Directorio => entrada.bytes().is_none(),
        };
        if !forma_valida {
            return Err(ErrorPlanStaging::SnapshotInconsistente);
        }
        if let Some(anterior) = tipos_raw.insert(ruta.to_owned(), tipo) {
            return Err(if anterior != tipo {
                ErrorPlanStaging::ConflictoArchivoDirectorio
            } else {
                ErrorPlanStaging::EntradaDuplicada
            });
        }
    }
    validar_ancestros_raw(&tipos_raw)?;

    let mut old = Vec::with_capacity(managed_old.len());
    let mut old_set = BTreeSet::new();
    for path in managed_old {
        let ruta = ruta_documento(path);
        validar_ruta_relativa(&ruta)?;
        if !old_set.insert(ruta.clone()) {
            return Err(ErrorPlanStaging::EntradaDuplicada);
        }
        match tipos_raw.get(&ruta) {
            Some(TipoEntradaSnapshot::Archivo) => old.push(ruta),
            Some(TipoEntradaSnapshot::Directorio) => {
                return Err(ErrorPlanStaging::ConflictoArchivoDirectorio);
            }
            None => return Err(ErrorPlanStaging::SnapshotInconsistente),
        }
    }
    old.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

    let mut managed_new = documentos.to_vec();
    managed_new.sort_by(|a, b| {
        a.path()
            .como_str()
            .as_bytes()
            .cmp(b.path().como_str().as_bytes())
    });
    let mut new_set = BTreeSet::new();
    let mut new_paths = Vec::with_capacity(managed_new.len());
    for documento in &managed_new {
        let ruta = ruta_documento(documento.path());
        validar_ruta_relativa(&ruta)?;
        if !new_set.insert(ruta.clone()) {
            return Err(ErrorPlanStaging::EntradaDuplicada);
        }
        new_paths.push(ruta);
    }

    let removed = old
        .iter()
        .filter(|ruta| !new_set.contains(*ruta))
        .cloned()
        .collect::<Vec<_>>();
    let mut auxiliares = entradas
        .iter()
        .filter(|entrada| {
            entrada.tipo() == TipoEntradaSnapshot::Archivo && !old_set.contains(entrada.ruta())
        })
        .cloned()
        .collect::<Vec<_>>();
    auxiliares.sort_by(|a, b| a.ruta().as_bytes().cmp(b.ruta().as_bytes()));

    let mut destino = BTreeMap::new();
    for (ruta, tipo) in &tipos_raw {
        if *tipo == TipoEntradaSnapshot::Directorio {
            destino.insert(ruta.clone(), FuenteEntradaStage::Directorio);
        }
    }
    for (indice, auxiliar) in auxiliares.iter().enumerate() {
        insertar_archivo(
            &mut destino,
            auxiliar.ruta().to_owned(),
            FuenteEntradaStage::Auxiliar(indice),
        )?;
    }
    for (indice, ruta) in new_paths.iter().enumerate() {
        insertar_archivo(
            &mut destino,
            ruta.clone(),
            FuenteEntradaStage::DocumentoNuevo(indice),
        )?;
    }
    for ruta in &new_paths {
        for directorio in directorios_padre(ruta) {
            insertar_directorio(&mut destino, directorio.to_owned())?;
        }
    }
    validar_ancestros_destino(&destino)?;
    #[cfg(windows)]
    validar_colisiones_case_insensitive(
        destino
            .keys()
            .map(String::as_str)
            .chain(old.iter().map(String::as_str)),
    )?;

    let mut directorios = destino
        .iter()
        .filter_map(|(ruta, fuente)| {
            (*fuente == FuenteEntradaStage::Directorio).then_some(ruta.clone())
        })
        .collect::<Vec<_>>();
    directorios.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let mut entradas_objetivo = destino
        .into_iter()
        .map(|(ruta, fuente)| EntradaStage { ruta, fuente })
        .collect::<Vec<_>>();
    entradas_objetivo.sort_by(|a, b| a.ruta.as_bytes().cmp(b.ruta.as_bytes()));

    Ok(PlanStaging {
        directorios,
        auxiliares,
        managed_old: old,
        managed_new,
        removed,
        entradas_objetivo,
    })
}

fn ruta_documento(path: &BundlePath) -> String {
    format!("{}.md", path.como_str())
}

fn validar_ruta_relativa(ruta: &str) -> Result<(), ErrorPlanStaging> {
    if ruta.is_empty()
        || ruta.starts_with('/')
        || ruta.ends_with('/')
        || ruta.contains('\\')
        || ruta.contains('\0')
        || ruta
            .split('/')
            .any(|segmento| matches!(segmento, "" | "." | ".."))
    {
        return Err(ErrorPlanStaging::RutaInvalida);
    }
    #[cfg(windows)]
    if matches!(
        std::path::Path::new(ruta).components().next(),
        Some(std::path::Component::Prefix(_))
    ) || ruta.split('/').any(|componente| {
        super::raiz::validar_basename_prospectivo(std::ffi::OsStr::new(componente)).is_err()
    }) {
        return Err(ErrorPlanStaging::RutaInvalida);
    }
    Ok(())
}

#[cfg(windows)]
fn validar_colisiones_case_insensitive<'a>(
    rutas: impl IntoIterator<Item = &'a str>,
) -> Result<(), ErrorPlanStaging> {
    let mut vistas = rutas
        .into_iter()
        .map(|ruta| (ruta, ruta.encode_utf16().collect::<Vec<_>>()))
        .collect::<Vec<_>>();
    if vistas
        .iter()
        .any(|(_, wide)| i32::try_from(wide.len()).is_err())
    {
        return Err(ErrorPlanStaging::RutaInvalida);
    }
    let mut fallo_comparacion = false;
    vistas.sort_by(|(ruta_a, wide_a), (ruta_b, wide_b)| {
        match ordinal_comparar_ignore_case(wide_a, wide_b) {
            Some(Ordering::Equal) => ruta_a.cmp(ruta_b),
            Some(orden) => orden,
            None => {
                fallo_comparacion = true;
                ruta_a.cmp(ruta_b)
            }
        }
    });
    if fallo_comparacion {
        return Err(ErrorPlanStaging::RutaInvalida);
    }
    for par in vistas.windows(2) {
        if par[0].0 != par[1].0
            && ordinal_comparar_ignore_case(&par[0].1, &par[1].1) == Some(Ordering::Equal)
        {
            return Err(ErrorPlanStaging::ColisionCaseInsensitive);
        }
    }
    Ok(())
}

/// Windows collision policy: compare full paths with the OS ordinal-ignore-case
/// API. Keep original path spellings; never fold or normalize Unicode locally.
#[cfg(windows)]
fn ordinal_comparar_ignore_case(a: &[u16], b: &[u16]) -> Option<Ordering> {
    let Ok(a_len) = i32::try_from(a.len()) else {
        return None;
    };
    let Ok(b_len) = i32::try_from(b.len()) else {
        return None;
    };
    // The explicit lengths avoid relying on NUL termination; NUL itself is
    // rejected by `validar_ruta_relativa`.
    let resultado = unsafe { CompareStringOrdinal(a.as_ptr(), a_len, b.as_ptr(), b_len, 1) };
    match resultado {
        CSTR_LESS_THAN => Some(Ordering::Less),
        CSTR_EQUAL => Some(Ordering::Equal),
        CSTR_GREATER_THAN => Some(Ordering::Greater),
        _ => None,
    }
}

fn validar_ancestros_raw(
    tipos_raw: &BTreeMap<String, TipoEntradaSnapshot>,
) -> Result<(), ErrorPlanStaging> {
    for ruta in tipos_raw.keys() {
        for padre in directorios_padre(ruta) {
            match tipos_raw.get(padre) {
                Some(TipoEntradaSnapshot::Directorio) => {}
                Some(TipoEntradaSnapshot::Archivo) => {
                    return Err(ErrorPlanStaging::ConflictoArchivoDirectorio);
                }
                None => return Err(ErrorPlanStaging::SnapshotInconsistente),
            }
        }
    }
    Ok(())
}

fn validar_ancestros_destino(
    destino: &BTreeMap<String, FuenteEntradaStage>,
) -> Result<(), ErrorPlanStaging> {
    for (ruta, fuente) in destino {
        if *fuente != FuenteEntradaStage::Directorio {
            for padre in directorios_padre(ruta) {
                match destino.get(padre) {
                    Some(FuenteEntradaStage::Directorio) => {}
                    Some(_) => return Err(ErrorPlanStaging::ConflictoArchivoDirectorio),
                    None => return Err(ErrorPlanStaging::SnapshotInconsistente),
                }
            }
        }
    }
    Ok(())
}

fn insertar_archivo(
    destino: &mut BTreeMap<String, FuenteEntradaStage>,
    ruta: String,
    fuente: FuenteEntradaStage,
) -> Result<(), ErrorPlanStaging> {
    if let Some(anterior) = destino.get(&ruta) {
        return Err(match (*anterior, fuente) {
            (FuenteEntradaStage::Directorio, _) | (_, FuenteEntradaStage::Directorio) => {
                ErrorPlanStaging::ConflictoArchivoDirectorio
            }
            (FuenteEntradaStage::Auxiliar(_), FuenteEntradaStage::DocumentoNuevo(_))
            | (FuenteEntradaStage::DocumentoNuevo(_), FuenteEntradaStage::Auxiliar(_)) => {
                ErrorPlanStaging::ColisionAuxiliarAdministrado
            }
            _ => ErrorPlanStaging::EntradaDuplicada,
        });
    }
    destino.insert(ruta, fuente);
    Ok(())
}

fn insertar_directorio(
    destino: &mut BTreeMap<String, FuenteEntradaStage>,
    ruta: String,
) -> Result<(), ErrorPlanStaging> {
    match destino.get(&ruta) {
        Some(FuenteEntradaStage::Directorio) => Ok(()),
        Some(_) => Err(ErrorPlanStaging::ConflictoArchivoDirectorio),
        None => {
            destino.insert(ruta, FuenteEntradaStage::Directorio);
            Ok(())
        }
    }
}

fn directorios_padre(mut ruta: &str) -> Vec<&str> {
    let mut padres = Vec::new();
    while let Some((padre, _)) = ruta.rsplit_once('/') {
        padres.push(padre);
        ruta = padre;
    }
    padres
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::captura::capturar_snapshot;
    use std::fs;
    use tempfile::{TempDir, tempdir};

    fn directorio(ruta: &str) -> EntradaSnapshot {
        EntradaSnapshot::directorio(ruta)
    }

    fn archivo(ruta: &str, bytes: &[u8]) -> EntradaSnapshot {
        EntradaSnapshot::archivo(ruta, bytes.to_vec())
    }

    fn documento(ruta: &str, contenido: &str) -> DocumentoCandidato {
        DocumentoCandidato::nuevo(BundlePath::nuevo(ruta).unwrap(), contenido.to_owned())
    }

    fn plan(
        entradas: &[EntradaSnapshot],
        old: &[&str],
        nuevos: &[DocumentoCandidato],
    ) -> Result<PlanStaging, ErrorPlanStaging> {
        let managed_old = old
            .iter()
            .map(|ruta| BundlePath::nuevo(ruta).unwrap())
            .collect::<Vec<_>>();
        planificar_staging_desde_inventario(entradas, &managed_old, nuevos)
    }

    fn rutas(plan: &PlanStaging) -> Vec<String> {
        plan.entradas_objetivo
            .iter()
            .map(|entrada| entrada.ruta.clone())
            .collect()
    }

    fn fuentes(plan: &PlanStaging) -> Vec<(String, FuenteEntradaStage)> {
        plan.entradas_objetivo
            .iter()
            .map(|entrada| (entrada.ruta.clone(), entrada.fuente))
            .collect()
    }

    fn snapshot_minimo() -> (TempDir, Snapshot) {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        fs::create_dir_all(raiz.join("knowledge")).unwrap();
        fs::create_dir(raiz.join("empty")).unwrap();
        fs::write(raiz.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();
        fs::write(raiz.join("documento.md.tmp"), b"sentinel").unwrap();
        fs::write(raiz.join("payload.bin"), [0, 255, 17]).unwrap();

        let manifiesto = temp.path().join("manifest.toml");
        let invariantes = temp.path().join("invariants.smt2");
        fs::write(
            &manifiesto,
            r#"version = "snapshot-test/1"

[nodos]
sortes = ["a", "Retirado", "Patrón"]

[nodos.disco]
"Patrón" = "pattern"

[layout]
directorios = ["knowledge"]
sin_iota = []
ignorados = ["ignored.md"]
"#,
        )
        .unwrap();
        fs::write(&invariantes, "\n").unwrap();
        let snapshot =
            capturar_snapshot(&raiz, &manifiesto, &invariantes, "engine/1", "policy/1").unwrap();
        (temp, snapshot)
    }

    #[test]
    fn crea_sibling_unico_y_reintenta_colision_sin_tocar_el_objeto_anterior() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();
        let colision = ubicacion.stage("occupied").unwrap();
        fs::write(colision.as_path(), b"preexisting").unwrap();
        let mut ids = ["occupied", "fresh"].into_iter();

        let stage =
            crear_stage_unico(&ubicacion, &mut || Ok(ids.next().unwrap().to_owned())).unwrap();

        let parent = fs::canonicalize(temp.path()).unwrap();
        assert_eq!(stage.ruta().parent(), Some(parent.as_path()));
        assert_eq!(fs::read(colision.as_path()).unwrap(), b"preexisting");
        assert!(stage.ruta().is_dir());
        assert_ne!(stage.ruta(), colision.as_path());
        assert!(!raiz.exists());
    }

    #[test]
    fn reintenta_colision_con_directorio_no_vacio_sin_alterarlo() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();
        let colision = ubicacion.stage("occupied-dir").unwrap();
        fs::create_dir(colision.as_path()).unwrap();
        fs::write(colision.as_path().join("canary.bin"), b"do not touch").unwrap();
        let mut ids = ["occupied-dir", "fresh-dir"].into_iter();
        let mut intentos = 0;

        let stage = crear_stage_unico(&ubicacion, &mut || {
            intentos += 1;
            Ok(ids.next().unwrap().to_owned())
        })
        .unwrap();

        assert_eq!(intentos, 2);
        assert_eq!(
            fs::read(colision.as_path().join("canary.bin")).unwrap(),
            b"do not touch"
        );
        assert!(stage.ruta().is_dir());
        assert_ne!(stage.ruta(), colision.as_path());
    }

    #[cfg(unix)]
    #[test]
    fn reintenta_colision_symlink_sin_seguirla_ni_alterar_destino() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();
        let destino = temp.path().join("target");
        fs::create_dir(&destino).unwrap();
        fs::write(destino.join("canary.bin"), b"target untouched").unwrap();
        let colision = ubicacion.stage("linked").unwrap();
        symlink(&destino, colision.as_path()).unwrap();
        let mut ids = ["linked", "fresh-link"].into_iter();

        let stage =
            crear_stage_unico(&ubicacion, &mut || Ok(ids.next().unwrap().to_owned())).unwrap();

        assert!(
            fs::symlink_metadata(colision.as_path())
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read(destino.join("canary.bin")).unwrap(),
            b"target untouched"
        );
        assert!(stage.ruta().is_dir());
        assert_ne!(stage.ruta(), colision.as_path());
    }

    #[cfg(windows)]
    #[test]
    fn reintenta_colision_junction_sin_seguirla_ni_alterar_destino() {
        use std::process::Command;

        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();
        let destino = temp.path().join("target");
        fs::create_dir(&destino).unwrap();
        fs::write(destino.join("canary.bin"), b"target untouched").unwrap();
        let colision = ubicacion.stage("linked").unwrap();
        let resultado = Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(colision.as_path())
            .arg(&destino)
            .output()
            .unwrap();
        assert!(
            resultado.status.success(),
            "mklink /J must create a junction"
        );
        let mut ids = ["linked", "fresh-link"].into_iter();

        let stage =
            crear_stage_unico(&ubicacion, &mut || Ok(ids.next().unwrap().to_owned())).unwrap();

        let metadata = fs::symlink_metadata(colision.as_path()).unwrap();
        assert!(metadata_es_enlace_o_reparse(&metadata));
        assert_eq!(
            fs::read(destino.join("canary.bin")).unwrap(),
            b"target untouched"
        );
        assert!(stage.ruta().is_dir());
        assert_ne!(stage.ruta(), colision.as_path());
    }

    #[test]
    fn materializa_raw_auxiliares_directorios_vacios_y_documento_junto_a_md_tmp() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        fs::create_dir(&raiz).unwrap();
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();
        let entradas = [
            directorio("vacio"),
            directorio("assets"),
            archivo("assets/data.bin", &[0, 255, 17]),
            archivo("assets/empty.bin", &[]),
            archivo("doc.md.tmp", b"temporary sentinel"),
        ];
        let plan = plan(&entradas, &[], &[documento("doc", "candidate bytes")]).unwrap();
        let mut ids = ["stage-write"].into_iter();

        let stage = materializar_staging_con(
            &ubicacion,
            &plan,
            || Ok(ids.next().unwrap().to_owned()),
            escribir_archivo_nuevo,
        )
        .unwrap();

        let parent = fs::canonicalize(temp.path()).unwrap();
        assert_eq!(stage.ruta().parent(), Some(parent.as_path()));
        assert_eq!(
            fs::read(stage.ruta().join("doc.md.tmp")).unwrap(),
            b"temporary sentinel"
        );
        assert_eq!(
            fs::read(stage.ruta().join("doc.md")).unwrap(),
            b"candidate bytes"
        );
        assert_eq!(
            fs::read(stage.ruta().join("assets/data.bin")).unwrap(),
            [0, 255, 17]
        );
        assert_eq!(
            fs::read(stage.ruta().join("assets/empty.bin")).unwrap(),
            b""
        );
        assert!(stage.ruta().join("vacio").is_dir());
        assert!(!raiz.join("doc.md").exists());
    }

    #[test]
    fn fallo_despues_de_escritura_deja_stage_parcial_y_no_cambia_el_hash_del_root() {
        let (temp, snapshot) = snapshot_minimo();
        let raiz = temp.path().join("bundle");
        let manifiesto = temp.path().join("manifest.toml");
        let invariantes = temp.path().join("invariants.smt2");
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();
        let documentos = [documento("knowledge/new", "candidate")];
        let plan = planificar_staging(&snapshot, &documentos).unwrap();
        let mut ids = ["partial"].into_iter();
        let mut escrituras = 0;
        let canario = b"injected create_new conflict";

        let error = materializar_staging_con(
            &ubicacion,
            &plan,
            || Ok(ids.next().unwrap().to_owned()),
            |path, bytes| {
                escrituras += 1;
                if escrituras == 2 {
                    fs::write(path, canario).map_err(|_| CodigoEscrituraStage::Io)?;
                }
                escribir_archivo_nuevo(path, bytes)
            },
        )
        .unwrap_err();

        assert_eq!(escrituras, 2);
        assert_eq!(error.codigo(), "stage_create_conflict");
        let debug = format!("{error:?}");
        let display = error.to_string();
        for detalle in [&debug, &display] {
            assert!(!detalle.contains("partial"));
            assert!(!detalle.contains(temp.path().to_string_lossy().as_ref()));
            assert!(!detalle.contains("injected create_new conflict"));
            assert!(!detalle.contains("StagePropio"));
        }
        let stage = error.stage_parcial().unwrap();
        assert_eq!(
            fs::read(stage.ruta().join("documento.md.tmp")).unwrap(),
            b"sentinel"
        );
        assert_eq!(
            fs::read(stage.ruta().join("knowledge/new.md")).unwrap(),
            canario
        );
        let stage_debug = format!("{stage:?}");
        assert!(!stage_debug.contains("partial"));
        assert!(!stage_debug.contains(temp.path().to_string_lossy().as_ref()));
        assert!(!stage_debug.contains("injected create_new conflict"));
        assert_eq!(
            snapshot.token(),
            capturar_snapshot(&raiz, &manifiesto, &invariantes, "engine/1", "policy/1",)
                .unwrap()
                .token()
        );
    }

    #[test]
    fn dieciseis_colisiones_agotadas_no_tocan_objetos_y_error_es_redactado() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        fs::create_dir(&raiz).unwrap();
        fs::write(raiz.join("root.bin"), b"root stays intact").unwrap();
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();
        let mut ids = Vec::new();
        for indice in 0..INTENTOS_STAGE {
            let id = format!("collision-{indice}");
            let ruta = ubicacion.stage(&id).unwrap();
            fs::create_dir(ruta.as_path()).unwrap();
            fs::write(ruta.as_path().join("canary.bin"), b"preexisting").unwrap();
            ids.push(id);
        }
        let mut restantes = ids.iter();
        let plan = plan(&[], &[], &[]).unwrap();
        let error = materializar_staging_con(
            &ubicacion,
            &plan,
            || Ok(restantes.next().unwrap().clone()),
            escribir_archivo_nuevo,
        )
        .unwrap_err();

        assert_eq!(error.codigo(), "stage_name_collision");
        assert!(error.stage_parcial().is_none());
        let debug = format!("{error:?}");
        let display = error.to_string();
        for detalle in [&debug, &display] {
            assert!(!detalle.contains("collision-"));
            assert!(!detalle.contains(temp.path().to_string_lossy().as_ref()));
            assert!(!detalle.contains("preexisting"));
            assert!(!detalle.contains("StagePropio"));
        }
        for id in ids {
            let ruta = ubicacion.stage(&id).unwrap();
            assert_eq!(
                fs::read(ruta.as_path().join("canary.bin")).unwrap(),
                b"preexisting"
            );
        }
        assert_eq!(
            fs::read(raiz.join("root.bin")).unwrap(),
            b"root stays intact"
        );
    }

    #[test]
    fn fallo_del_seam_aleatorio_es_redactado_y_no_crea_stage() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("root-with-private-name");
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();
        let plan = plan(&[], &[], &[]).unwrap();
        let error = materializar_staging_con(
            &ubicacion,
            &plan,
            || Err(CodigoEscrituraStage::Aleatorio),
            escribir_archivo_nuevo,
        )
        .unwrap_err();

        assert_eq!(error.codigo(), "stage_random_unavailable");
        assert!(error.stage_parcial().is_none());
        let debug = format!("{error:?}");
        let display = error.to_string();
        for detalle in [&debug, &display] {
            assert!(!detalle.contains(temp.path().to_string_lossy().as_ref()));
            assert!(!detalle.contains("root-with-private-name"));
            assert!(!detalle.contains("doc.md.tmp"));
            assert!(!detalle.contains("StagePropio"));
        }
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn planifica_deterministicamente_y_preserva_directorios_y_auxiliares() {
        let entradas = vec![
            // Canario: `documento.md.tmp` sigue siendo auxiliar. El builder
            // futuro debe usar un namespace/temporal único por transacción y
            // jamás llamar `serializar_bundle` sobre este árbol destino.
            archivo("documento.md.tmp", b"sentinel"),
            directorio("empty"),
            directorio("docs"),
            archivo("assets/zero.dat", b""),
            directorio("assets"),
            archivo("assets/bin.dat", &[0, 255, 17]),
        ];
        let primero = planificar_staging_desde_inventario(&entradas, &[], &[]).unwrap();
        let mut entradas_reversas = entradas.clone();
        entradas_reversas.reverse();
        let segundo = planificar_staging_desde_inventario(&entradas_reversas, &[], &[]).unwrap();

        assert_eq!(rutas(&primero), rutas(&segundo));
        assert_eq!(primero.directorios, ["assets", "docs", "empty"]);
        assert_eq!(
            rutas(&primero),
            [
                "assets".to_owned(),
                "assets/bin.dat".to_owned(),
                "assets/zero.dat".to_owned(),
                "docs".to_owned(),
                "documento.md.tmp".to_owned(),
                "empty".to_owned(),
            ]
        );
        assert_eq!(primero.auxiliares[0].bytes(), Some([0, 255, 17].as_slice()));
        assert_eq!(primero.auxiliares[1].bytes(), Some([].as_slice()));
        assert_eq!(primero.auxiliares[2].ruta(), "documento.md.tmp");
        assert_eq!(primero.auxiliares[2].bytes(), Some(b"sentinel".as_slice()));
    }

    #[test]
    fn clasifica_managed_deterministicamente_bajo_permutaciones() {
        let entradas = vec![
            directorio("docs"),
            archivo("docs/old-z.md", b"viejo z"),
            archivo("docs/old-a.md", b"viejo a"),
        ];
        let managed_old = ["docs/old-z", "docs/old-a"];
        let nuevos = [
            documento("docs/z", "último"),
            documento("docs/new", "nuevo"),
        ];
        let primero = plan(&entradas, &managed_old, &nuevos).unwrap();

        let mut entradas_reversas = entradas.clone();
        entradas_reversas.reverse();
        let mut managed_old_reversos = managed_old;
        managed_old_reversos.reverse();
        let mut nuevos_reversos = nuevos.clone();
        nuevos_reversos.reverse();
        let segundo = plan(&entradas_reversas, &managed_old_reversos, &nuevos_reversos).unwrap();

        assert_eq!(primero.managed_old, ["docs/old-a.md", "docs/old-z.md"]);
        assert_eq!(primero.removed, primero.managed_old);
        assert_eq!(primero.managed_new[0].path().como_str(), "docs/new");
        assert_eq!(primero.managed_new[1].path().como_str(), "docs/z");
        assert_eq!(primero.managed_old, segundo.managed_old);
        assert_eq!(primero.removed, segundo.removed);
        assert_eq!(
            primero
                .managed_new
                .iter()
                .map(|documento| documento.path().como_str())
                .collect::<Vec<_>>(),
            segundo
                .managed_new
                .iter()
                .map(|documento| documento.path().como_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(rutas(&primero), rutas(&segundo));
        assert_eq!(fuentes(&primero), fuentes(&segundo));
        assert_eq!(
            rutas(&primero),
            [
                "docs".to_owned(),
                "docs/new.md".to_owned(),
                "docs/z.md".to_owned(),
            ]
        );
    }

    #[test]
    fn wrapper_planifica_desde_snapshot_capturado_real() {
        let (_temp, snapshot) = snapshot_minimo();
        let nuevos = [documento("knowledge/new", "contenido")];
        let resultado = planificar_staging(&snapshot, &nuevos).unwrap();

        assert_eq!(resultado.managed_old, ["knowledge/a.md"]);
        assert_eq!(resultado.removed, ["knowledge/a.md"]);
        assert_eq!(resultado.managed_new[0].path().como_str(), "knowledge/new");
        assert_eq!(resultado.directorios, ["empty", "knowledge"]);
        assert_eq!(
            rutas(&resultado),
            [
                "documento.md.tmp".to_owned(),
                "empty".to_owned(),
                "knowledge".to_owned(),
                "knowledge/new.md".to_owned(),
                "payload.bin".to_owned(),
            ]
        );
        assert_eq!(resultado.auxiliares[0].ruta(), "documento.md.tmp");
        assert_eq!(
            resultado.auxiliares[0].bytes(),
            Some(b"sentinel".as_slice())
        );
        assert_eq!(resultado.auxiliares[1].ruta(), "payload.bin");
        assert_eq!(
            resultado.auxiliares[1].bytes(),
            Some([0, 255, 17].as_slice())
        );
    }

    #[test]
    fn errores_de_colision_y_path_son_estables_y_redactados() {
        let auxiliares = [directorio("docs"), archivo("docs/doc.md", b"secreto")];
        let error = plan(&auxiliares, &[], &[documento("docs/doc", "privado")]).unwrap_err();
        assert_eq!(error.codigo(), "stage_managed_auxiliary_collision");
        assert!(!format!("{error:?} {error}").contains("privado"));
        assert!(!format!("{error:?} {error}").contains("docs/doc"));

        assert_eq!(
            plan(&[archivo("../secreto", b"x")], &[], &[])
                .unwrap_err()
                .codigo(),
            "stage_path_invalid"
        );
        assert_eq!(
            plan(&[archivo("/absoluto", b"x")], &[], &[])
                .unwrap_err()
                .codigo(),
            "stage_path_invalid"
        );
        assert_eq!(
            plan(&[archivo("con\0nul", b"x")], &[], &[])
                .unwrap_err()
                .codigo(),
            "stage_path_invalid"
        );
        let duplicadas = [archivo("data.bin", b"a"), archivo("data.bin", b"b")];
        assert_eq!(
            plan(&duplicadas, &[], &[]).unwrap_err().codigo(),
            "stage_duplicate_entry"
        );
        let file_dir = [archivo("node", b"archivo"), archivo("node/child.bin", b"x")];
        assert_eq!(
            plan(&file_dir, &[], &[]).unwrap_err().codigo(),
            "stage_file_directory_conflict"
        );
    }

    #[test]
    fn rechaza_documentocandidato_duplicado() {
        let duplicados = [documento("docs/doc", "uno"), documento("docs/doc", "dos")];
        let error = plan(&[], &[], &duplicados).unwrap_err();
        assert_eq!(error.codigo(), "stage_duplicate_entry");
        assert!(!format!("{error:?} {error}").contains("docs/doc"));
        assert!(!format!("{error:?} {error}").contains("uno"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_rechaza_componentes_auxiliares_que_no_preservan_identidad_raw() {
        for ruta in [
            "folder/file:stream",
            "folder/trailing.",
            "folder/trailing ",
            "folder/CON.txt",
            "folder/ALIAS~1.txt",
        ] {
            assert_eq!(
                plan(&[archivo(ruta, b"bytes")], &[], &[])
                    .unwrap_err()
                    .codigo(),
                "stage_path_invalid",
                "ruta: {ruta}"
            );
        }
    }

    #[test]
    fn rechaza_conflicto_file_directory_creado_por_documentos_candidatos() {
        // `node` se materializa como `node.md`; el segundo documento necesita
        // que esa misma ruta física sea un directorio.
        let candidatos = [
            documento("node", "archivo"),
            documento("node.md/child", "hijo"),
        ];
        let error = plan(&[], &[], &candidatos).unwrap_err();
        assert_eq!(error.codigo(), "stage_file_directory_conflict");
    }

    #[cfg(unix)]
    #[test]
    fn unix_preserva_unicode_y_case_exactos() {
        let entradas = [archivo("ñ.txt", b"a"), archivo("Ñ.txt", b"b")];
        let resultado = plan(&entradas, &[], &[]).unwrap();
        assert_eq!(rutas(&resultado), ["Ñ.txt", "ñ.txt"]);
    }

    #[cfg(windows)]
    #[test]
    fn windows_detecta_colision_unicode_con_ordinal_ignore_case() {
        let entradas = [archivo("café.md", b"a"), archivo("CAFÉ.md", b"b")];
        let error = plan(&entradas, &[], &[]).unwrap_err();
        assert_eq!(error.codigo(), "stage_case_insensitive_collision");
    }

    #[cfg(windows)]
    #[test]
    fn windows_detecta_managed_auxiliar_y_rename_solo_de_case() {
        let auxiliar = [archivo("CAFÉ.md", b"auxiliar")];
        let error = plan(&auxiliar, &[], &[documento("café", "privado")]).unwrap_err();
        assert_eq!(error.codigo(), "stage_case_insensitive_collision");

        let entradas = [directorio("docs"), archivo("docs/old.md", b"viejo")];
        let error = plan(&entradas, &["docs/old"], &[documento("docs/OLD", "nuevo")]).unwrap_err();
        assert_eq!(error.codigo(), "stage_case_insensitive_collision");
    }

    #[cfg(windows)]
    #[test]
    fn windows_rechaza_prefijo_de_unidad_absoluto() {
        let error = plan(&[archivo("C:/secreto.txt", b"x")], &[], &[]).unwrap_err();
        assert_eq!(error.codigo(), "stage_path_invalid");
    }
}
