//! Plan puro de staging desde inventario raw, sin raíz ni operaciones de filesystem.
//! En Windows las colisiones usan `CompareStringOrdinal`; en Unix, los paths
//! conservan identidad exacta y sensible a case, incluidos sus bytes Unicode.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use universo_core::{Anexo, BundlePath, Candidato, Estructura};

use crate::serializar::DocumentoCandidato;

use super::captura::{Snapshot, materializar_desde_entradas};
use super::inventario::{inventariar_root_con_lector, leer_archivo_regular};
use super::raiz::{UbicacionRaiz, metadata_es_enlace_o_reparse};
use super::snapshot::{EntradaSnapshot, ErrorSnapshot, SnapshotToken, TipoEntradaSnapshot};

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
    ConfiguracionEnConflicto,
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
            Self::ConfiguracionEnConflicto => "stage_configuration_collision",
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
    Snapshot(ErrorSnapshot),
    InventarioInconsistente,
    CandidatoInconsistente,
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
            Self::Snapshot(error) => error.codigo(),
            Self::InventarioInconsistente => "stage_inventory_mismatch",
            Self::CandidatoInconsistente => "stage_candidate_mismatch",
        }
    }
}

/// Sibling stage creado exclusivamente por este módulo. Su dueño debe decidir
/// cleanup; no hay Drop/remove_dir_all basado solo en el nombre path-based.
#[allow(dead_code)] // Se consume desde apply/recovery en unidades posteriores.
pub(crate) struct StagePropio {
    ruta: PathBuf,
    stage_id: String,
    recibo: Option<ReciboStage>,
}

/// Identidad interna del stage recargado; no implica identidad del objeto
/// filesystem. U7 aún debe anclar/revalidar handles antes de publicar.
#[allow(dead_code)] // El journal/commit consume identidad y token en U7.
pub(crate) struct ReciboStage {
    stage_id: String,
    snapshot: SnapshotToken,
}

#[allow(dead_code)] // Las capas de journal/recovery consumirán estos campos en U7.
impl ReciboStage {
    pub(crate) fn stage_id(&self) -> &str {
        &self.stage_id
    }

    pub(crate) fn snapshot(&self) -> SnapshotToken {
        self.snapshot
    }
}

impl fmt::Debug for ReciboStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ReciboStage { .. }")
    }
}

impl fmt::Display for ReciboStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[stage receipt redacted]")
    }
}

#[allow(dead_code)] // Se consume desde apply/recovery en unidades posteriores.
impl StagePropio {
    pub(crate) fn ruta(&self) -> &Path {
        &self.ruta
    }

    pub(crate) fn stage_id(&self) -> &str {
        &self.stage_id
    }

    pub(crate) fn recibo(&self) -> Option<&ReciboStage> {
        self.recibo.as_ref()
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
    let plan = planificar_staging_desde_inventario(snapshot.entradas(), &managed_old, documentos)?;
    validar_configuracion_en_plan(snapshot, &plan)?;
    Ok(plan)
}

/// Materializa un plan desde los buffers raw en un sibling nuevo. El parent
/// validado se asume estable durante la operación: esta API path-based no debe
/// alimentar apply hasta que parent y stage queden anclados a handles.
#[allow(dead_code)] // Se integra a apply en la siguiente unidad.
pub(crate) fn materializar_staging(
    ubicacion: &UbicacionRaiz,
    snapshot: &Snapshot,
    plan: &PlanStaging,
    candidato: &Candidato,
) -> Result<StagePropio, ErrorEscrituraStage> {
    let (mut stage, recapturado) = materializar_staging_y_recargar_con(
        ubicacion,
        snapshot,
        plan,
        generar_id_stage,
        escribir_archivo_nuevo,
        leer_archivo_regular,
    )?;
    if validar_candidato_recargado(snapshot, candidato, &recapturado).is_err() {
        return Err(ErrorEscrituraStage {
            codigo: CodigoEscrituraStage::CandidatoInconsistente,
            stage_parcial: Some(stage),
        });
    }
    stage.recibo = Some(ReciboStage {
        stage_id: stage.stage_id.clone(),
        snapshot: recapturado.token(),
    });
    Ok(stage)
}

fn validar_candidato_recargado(
    original: &Snapshot,
    candidato: &Candidato,
    recapturado: &Snapshot,
) -> Result<(), CodigoEscrituraStage> {
    let esperado = candidato.estructura();
    let actual = recapturado.estructura();
    if original.estructura().manifiesto() != esperado.manifiesto()
        || esperado.manifiesto() != actual.manifiesto()
        || atomos_canonicos(esperado) != atomos_canonicos(actual)
        || anexos_por_path(esperado) != anexos_por_path(actual)
        || parent_explicito_esperado(original.estructura(), esperado)
            != parent_explicito_por_path(actual)
    {
        return Err(CodigoEscrituraStage::CandidatoInconsistente);
    }
    Ok(())
}

fn atomos_canonicos(estructura: &Estructura) -> Vec<universo_core::Atomo> {
    let mut atomos = estructura.atomos();
    atomos.sort_unstable();
    atomos
}

fn anexos_por_path(estructura: &Estructura) -> BTreeMap<BundlePath, Anexo> {
    estructura
        .nodos()
        .map(|(id, nodo)| {
            (
                nodo.path.clone(),
                estructura.anexo(id).cloned().unwrap_or_default(),
            )
        })
        .collect()
}

fn parent_explicito_por_path(estructura: &Estructura) -> BTreeMap<BundlePath, bool> {
    estructura
        .nodos()
        .map(|(id, nodo)| {
            (
                nodo.path.clone(),
                estructura.parent_frontmatter_explicito(id),
            )
        })
        .collect()
}

fn parent_explicito_esperado(
    original: &Estructura,
    candidato: &Estructura,
) -> BTreeMap<BundlePath, bool> {
    let padres_originales = padres_por_path(original);
    let padres_candidatos = padres_por_path(candidato);
    candidato
        .nodos()
        .map(|(id, nodo)| {
            let cambio_parent =
                padres_originales.get(&nodo.path) != padres_candidatos.get(&nodo.path);
            (
                nodo.path.clone(),
                candidato.parent_frontmatter_explicito(id) || cambio_parent,
            )
        })
        .collect()
}

fn padres_por_path(estructura: &Estructura) -> BTreeMap<BundlePath, Option<BundlePath>> {
    estructura
        .nodos()
        .map(|(id, nodo)| {
            let padre = estructura
                .padre(id)
                .and_then(|padre| estructura.nodo(padre))
                .map(|padre| padre.path.clone());
            (nodo.path.clone(), padre)
        })
        .collect()
}

/// Escribe el sibling, recaptura su inventario real una sola vez y materializa
/// desde esos mismos buffers junto con la configuración raw/versionada del
/// snapshot original. Esta ruta sigue siendo path-based y no cierra TOCTOU:
/// U7 deberá aportar operaciones relativas a handles antes de publicar.
fn materializar_staging_y_recargar_con<G, W, R>(
    ubicacion: &UbicacionRaiz,
    snapshot: &Snapshot,
    plan: &PlanStaging,
    siguiente_id: G,
    escribir_archivo: W,
    leer_archivo: R,
) -> Result<(StagePropio, Snapshot), ErrorEscrituraStage>
where
    G: FnMut() -> Result<String, CodigoEscrituraStage>,
    W: FnMut(&Path, &[u8]) -> Result<(), CodigoEscrituraStage>,
    R: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let stage = materializar_staging_con(ubicacion, plan, siguiente_id, escribir_archivo)?;
    match recargar_staging_con_lector(snapshot, plan, &stage, leer_archivo) {
        Ok(snapshot_recapturado) => Ok((stage, snapshot_recapturado)),
        Err(codigo) => Err(ErrorEscrituraStage {
            codigo,
            stage_parcial: Some(stage),
        }),
    }
}

fn recargar_staging_con_lector<R>(
    snapshot: &Snapshot,
    plan: &PlanStaging,
    stage: &StagePropio,
    leer_archivo: R,
) -> Result<Snapshot, CodigoEscrituraStage>
where
    R: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let recapturadas =
        inventariar_root_con_lector(stage.ruta(), leer_archivo).map_err(mapear_error_snapshot)?;
    let objetivo = inventario_objetivo(plan)?;
    if !inventarios_iguales(&recapturadas, &objetivo) {
        return Err(CodigoEscrituraStage::InventarioInconsistente);
    }
    materializar_desde_entradas(snapshot, recapturadas).map_err(mapear_error_snapshot)
}

fn mapear_error_snapshot(error: ErrorSnapshot) -> CodigoEscrituraStage {
    match error {
        ErrorSnapshot::EnlaceNoPermitido => CodigoEscrituraStage::EnlaceNoPermitido,
        ErrorSnapshot::TipoNoAdmitido => CodigoEscrituraStage::TipoNoAdmitido,
        otro => CodigoEscrituraStage::Snapshot(otro),
    }
}

fn inventario_objetivo(plan: &PlanStaging) -> Result<Vec<EntradaSnapshot>, CodigoEscrituraStage> {
    let mut entradas = Vec::with_capacity(plan.entradas_objetivo.len());
    for entrada in &plan.entradas_objetivo {
        let capturada = match entrada.fuente {
            FuenteEntradaStage::Directorio => EntradaSnapshot::directorio(&entrada.ruta),
            FuenteEntradaStage::Auxiliar(indice) => plan
                .auxiliares
                .get(indice)
                .filter(|auxiliar| {
                    auxiliar.tipo() == TipoEntradaSnapshot::Archivo
                        && auxiliar.bytes().is_some()
                        && auxiliar.ruta() == entrada.ruta
                })
                .cloned()
                .ok_or(CodigoEscrituraStage::Plan(
                    ErrorPlanStaging::SnapshotInconsistente,
                ))?,
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
                EntradaSnapshot::archivo(&entrada.ruta, documento.contenido().as_bytes().to_vec())
            }
        };
        entradas.push(capturada);
    }
    entradas.sort_by(|a, b| a.ruta().as_bytes().cmp(b.ruta().as_bytes()));
    Ok(entradas)
}

fn inventarios_iguales(actual: &[EntradaSnapshot], esperado: &[EntradaSnapshot]) -> bool {
    actual.len() == esperado.len()
        && actual.iter().zip(esperado).all(|(actual, esperado)| {
            actual.ruta() == esperado.ruta()
                && actual.tipo() == esperado.tipo()
                && actual.bytes() == esperado.bytes()
        })
}

fn validar_configuracion_en_plan(
    snapshot: &Snapshot,
    plan: &PlanStaging,
) -> Result<(), ErrorPlanStaging> {
    for (ruta, bytes) in [
        (snapshot.ruta_manifiesto_root(), snapshot.manifiesto_raw()),
        (snapshot.ruta_invariantes_root(), snapshot.invariantes_raw()),
    ] {
        let Some(ruta) = ruta else {
            continue;
        };
        let Some(entrada) = plan
            .entradas_objetivo
            .iter()
            .find(|entrada| entrada.ruta == ruta)
        else {
            return Err(ErrorPlanStaging::ConfiguracionEnConflicto);
        };
        let conservada = match entrada.fuente {
            FuenteEntradaStage::Auxiliar(indice) => {
                plan.auxiliares.get(indice).and_then(EntradaSnapshot::bytes)
            }
            FuenteEntradaStage::DocumentoNuevo(indice) => plan
                .managed_new
                .get(indice)
                .map(|documento| documento.contenido().as_bytes()),
            FuenteEntradaStage::Directorio => None,
        };
        if conservada != Some(bytes) {
            return Err(ErrorPlanStaging::ConfiguracionEnConflicto);
        }
    }
    Ok(())
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
                    stage_id: id,
                    recibo: None,
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
    use crate::serializar::{ParentOverride, serializar_candidato};
    use crate::transaccion::captura::{capturar_snapshot, materializar_desde_entradas};
    use std::fs;
    use tempfile::{TempDir, tempdir};
    use universo_core::{Atomo, Delta, Etiqueta, Sujeto, construir_candidato};

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

    fn snapshot_con_parent() -> (TempDir, Snapshot, PathBuf, PathBuf) {
        snapshot_con_parent_version("stage-parent/1")
    }

    fn snapshot_con_parent_version(version: &str) -> (TempDir, Snapshot, PathBuf, PathBuf) {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        fs::create_dir_all(raiz.join("knowledge/sub")).unwrap();
        fs::write(
            raiz.join("knowledge/index.md"),
            "---\ntype: a\ntitle: Root\nparent: \"\"\ncustom: keep\n---\nroot body\n",
        )
        .unwrap();
        fs::write(
            raiz.join("knowledge/sub/index.md"),
            "---\ntype: a\ntitle: Sub\n---\nsub body\n",
        )
        .unwrap();
        fs::write(
            raiz.join("knowledge/sub/leaf.md"),
            "---\ntype: a\ntitle: Leaf\nparent: knowledge/index\ncustom: ordered\n---\nleaf body\n",
        )
        .unwrap();
        let manifiesto = temp.path().join("manifest.toml");
        let invariantes = temp.path().join("invariants.smt2");
        fs::write(
            &manifiesto,
            format!("version = \"{version}\"\n\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patrón\"]\n\n[layout]\ndirectorios = [\"knowledge\"]\nsin_iota = []\nignorados = []\nparent_key = \"parent\"\n"),
        )
        .unwrap();
        fs::write(&invariantes, "\n").unwrap();
        let snapshot =
            capturar_snapshot(&raiz, &manifiesto, &invariantes, "engine/1", "policy/1").unwrap();
        (temp, snapshot, manifiesto, invariantes)
    }

    fn candidato_con_titulo(snapshot: &Snapshot, titulo: &str) -> Candidato {
        let path = BundlePath::nuevo("knowledge/index").unwrap();
        let anterior = Atomo::HasLab {
            sujeto: Sujeto::Nodo(path.clone()),
            etiqueta: Etiqueta::nuevo("Root"),
        };
        let nuevo = Atomo::HasLab {
            sujeto: Sujeto::Nodo(path),
            etiqueta: Etiqueta::nuevo(titulo),
        };
        let delta = Delta::nuevo([nuevo], [anterior]).unwrap();
        construir_candidato(snapshot.estructura(), &delta).unwrap()
    }

    fn candidato_con_padre(snapshot: &Snapshot, nuevo_padre: Option<&str>) -> Candidato {
        let hijo = BundlePath::nuevo("knowledge/sub/index").unwrap();
        let anterior = Atomo::Parent {
            hijo: hijo.clone(),
            padre: BundlePath::nuevo("knowledge/index").unwrap(),
        };
        let agregar = nuevo_padre.map(|padre| Atomo::Parent {
            hijo,
            padre: BundlePath::nuevo(padre).unwrap(),
        });
        let delta = Delta::nuevo(agregar, [anterior]).unwrap();
        construir_candidato(snapshot.estructura(), &delta).unwrap()
    }

    fn stage_para_recarga(
        snapshot: &Snapshot,
        root: &Path,
        id: &str,
    ) -> (PlanStaging, StagePropio) {
        let plan = planificar_staging(
            snapshot,
            &[documento("knowledge/a", "---\ntype: a\ntitle: A\n---\n")],
        )
        .unwrap();
        let ubicacion = UbicacionRaiz::nueva(root).unwrap();
        let mut ids = [id].into_iter();
        let stage = materializar_staging_con(
            &ubicacion,
            &plan,
            || Ok(ids.next().unwrap().to_owned()),
            escribir_archivo_nuevo,
        )
        .unwrap();
        (plan, stage)
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
    fn inventario_real_del_stage_rechaza_corrupcion_omisiones_extras_y_tipos() {
        for escenario in [
            "success",
            "corrupcion",
            "missing",
            "missing-dir",
            "extra-file",
            "extra-dir",
            "file-to-dir",
            "type",
        ] {
            let (temp, snapshot) = snapshot_minimo();
            let root = temp.path().join("bundle");
            let original = fs::read(root.join("knowledge/a.md")).unwrap();
            let (plan, stage) = stage_para_recarga(&snapshot, &root, "readback-test");

            match escenario {
                "success" => {}
                "corrupcion" => fs::write(stage.ruta().join("knowledge/a.md"), b"corrupt").unwrap(),
                "missing" => fs::remove_file(stage.ruta().join("knowledge/a.md")).unwrap(),
                "missing-dir" => fs::remove_dir(stage.ruta().join("empty")).unwrap(),
                "extra-file" => fs::write(stage.ruta().join("extra.bin"), b"extra").unwrap(),
                "extra-dir" => {
                    fs::create_dir(stage.ruta().join("extra-nonempty")).unwrap();
                    fs::write(stage.ruta().join("extra-nonempty/canary.bin"), b"extra").unwrap();
                }
                "file-to-dir" => {
                    fs::remove_file(stage.ruta().join("payload.bin")).unwrap();
                    fs::create_dir(stage.ruta().join("payload.bin")).unwrap();
                    fs::write(stage.ruta().join("payload.bin/canary.bin"), b"wrong type").unwrap();
                }
                "type" => {
                    fs::remove_dir(stage.ruta().join("empty")).unwrap();
                    fs::write(stage.ruta().join("empty"), b"wrong type").unwrap();
                }
                _ => unreachable!(),
            }

            let resultado =
                recargar_staging_con_lector(&snapshot, &plan, &stage, leer_archivo_regular);
            if escenario == "success" {
                let recargado = match resultado {
                    Ok(recargado) => recargado,
                    Err(error) => panic!("recarga inesperadamente fallida: {}", error.codigo()),
                };
                assert_eq!(recargado.estructura().nodos().count(), 1);
                assert_eq!(recargado.token(), snapshot.token());
            } else {
                let error = resultado.unwrap_err();
                assert_eq!(error.codigo(), "stage_inventory_mismatch", "{escenario}");
                assert!(!error.codigo().contains("corrupt"));
                assert!(!error.codigo().contains("extra"));
            }
            assert_eq!(fs::read(root.join("knowledge/a.md")).unwrap(), original);
        }
    }

    #[test]
    fn recarga_con_inventario_exacto_rechaza_documento_invalido_sin_revelar_canarios() {
        let (temp, snapshot) = snapshot_minimo();
        let root = temp.path().join("bundle");
        let original = fs::read(root.join("knowledge/a.md")).unwrap();
        let canario = "PRIVATE_RELOAD_PARSE_CANARY";
        let plan = planificar_staging(&snapshot, &[documento("knowledge/a", canario)]).unwrap();
        let error = materializar_staging_y_recargar_con(
            &UbicacionRaiz::nueva(&root).unwrap(),
            &snapshot,
            &plan,
            || Ok("invalid-doc-stage".to_owned()),
            escribir_archivo_nuevo,
            leer_archivo_regular,
        )
        .unwrap_err();

        assert_eq!(error.codigo(), "snapshot_bundle_invalid");
        let visible = format!("{error:?} {error}");
        assert!(!visible.contains(canario));
        assert!(!visible.contains("knowledge/a.md"));
        assert!(!visible.contains("invalid-doc-stage"));
        assert!(!visible.contains(temp.path().to_string_lossy().as_ref()));
        assert_eq!(fs::read(root.join("knowledge/a.md")).unwrap(), original);
    }

    #[cfg(unix)]
    #[test]
    fn recarga_stage_rechaza_symlink_y_preserva_root() {
        use std::os::unix::fs::symlink;

        let (temp, snapshot) = snapshot_minimo();
        let root = temp.path().join("bundle");
        let original = fs::read(root.join("knowledge/a.md")).unwrap();
        let (plan, stage) = stage_para_recarga(&snapshot, &root, "symlink-test");
        let externo = temp.path().join("outside.bin");
        fs::write(&externo, b"PRIVATE-SYMLINK-CANARY").unwrap();
        fs::remove_file(stage.ruta().join("payload.bin")).unwrap();
        symlink(&externo, stage.ruta().join("payload.bin")).unwrap();

        let error = recargar_staging_con_lector(&snapshot, &plan, &stage, leer_archivo_regular)
            .unwrap_err();

        assert_eq!(error.codigo(), "stage_link_not_allowed");
        assert!(!format!("{error:?} {error}").contains("PRIVATE-SYMLINK-CANARY"));
        assert_eq!(fs::read(root.join("knowledge/a.md")).unwrap(), original);
    }

    #[cfg(windows)]
    #[test]
    fn recarga_stage_rechaza_junction_y_preserva_root() {
        use std::process::Command;

        let (temp, snapshot) = snapshot_minimo();
        let root = temp.path().join("bundle");
        let original = fs::read(root.join("knowledge/a.md")).unwrap();
        let (plan, stage) = stage_para_recarga(&snapshot, &root, "junction-test");
        let externo = temp.path().join("outside-dir");
        fs::create_dir(&externo).unwrap();
        fs::write(externo.join("canary.bin"), b"junction target").unwrap();
        fs::remove_dir(stage.ruta().join("empty")).unwrap();
        let salida = Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(stage.ruta().join("empty"))
            .arg(&externo)
            .output()
            .unwrap();
        assert!(salida.status.success(), "mklink /J must create a junction");

        let error = recargar_staging_con_lector(&snapshot, &plan, &stage, leer_archivo_regular)
            .unwrap_err();

        assert_eq!(error.codigo(), "stage_link_not_allowed");
        assert_eq!(
            fs::read(externo.join("canary.bin")).unwrap(),
            b"junction target"
        );
        assert_eq!(fs::read(root.join("knowledge/a.md")).unwrap(), original);
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

    #[test]
    fn stages_equivalentes_comparten_token_con_ids_distintos_y_redactados() {
        let (temp, snapshot, manifiesto, invariantes) = snapshot_con_parent();
        let raiz = temp.path().join("bundle");
        let antes = fs::read(raiz.join("knowledge/index.md")).unwrap();
        let candidato = candidato_con_titulo(&snapshot, "Root edited");
        let documentos = serializar_candidato(snapshot.estructura(), &candidato, &[]).unwrap();
        let plan = planificar_staging(&snapshot, &documentos).unwrap();
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();

        let stage = materializar_staging(&ubicacion, &snapshot, &plan, &candidato).unwrap();
        let segundo = materializar_staging(&ubicacion, &snapshot, &plan, &candidato).unwrap();
        let recibo = stage.recibo().unwrap();
        let recibo_segundo = segundo.recibo().unwrap();
        assert_eq!(recibo.stage_id(), stage.stage_id());
        assert_eq!(recibo_segundo.stage_id(), segundo.stage_id());
        assert_ne!(recibo.stage_id(), recibo_segundo.stage_id());
        assert_eq!(recibo.snapshot(), recibo_segundo.snapshot());
        assert_ne!(recibo.snapshot(), snapshot.token());
        for (stage, recibo) in [(&stage, recibo), (&segundo, recibo_segundo)] {
            let visible = format!("{recibo:?} {recibo}");
            assert!(!visible.contains(stage.stage_id()));
            assert!(!visible.contains(&recibo.snapshot().to_hex()));
            assert!(!format!("{stage:?}").contains(stage.stage_id()));
        }
        assert_eq!(fs::read(raiz.join("knowledge/index.md")).unwrap(), antes);
        assert_eq!(
            capturar_snapshot(&raiz, &manifiesto, &invariantes, "engine/1", "policy/1")
                .unwrap()
                .token(),
            snapshot.token()
        );
    }

    #[test]
    fn comparador_semantico_ignora_ids_y_orden_pero_exige_anexos_y_parent_explicito() {
        let (_temp, snapshot, _, _) = snapshot_con_parent();
        let candidato = construir_candidato(snapshot.estructura(), &Delta::default()).unwrap();
        let mut entradas = snapshot.entradas().to_vec();
        entradas.reverse();
        let recapturado = materializar_desde_entradas(&snapshot, entradas).unwrap();
        assert!(validar_candidato_recargado(&snapshot, &candidato, &recapturado).is_ok());

        let path = "knowledge/index.md";
        for (buscar, reemplazo) in [
            ("custom: keep", "custom: changed"),
            ("parent: \"\"\n", ""),
            ("root body\n", "tampered body\n"),
        ] {
            let mut entradas = snapshot.entradas().to_vec();
            let indice = entradas.iter().position(|e| e.ruta() == path).unwrap();
            let original = std::str::from_utf8(entradas[indice].bytes().unwrap()).unwrap();
            assert!(original.contains(buscar));
            entradas[indice] =
                EntradaSnapshot::archivo(path, original.replace(buscar, reemplazo).into_bytes());
            let recapturado = materializar_desde_entradas(&snapshot, entradas).unwrap();
            assert_eq!(
                atomos_canonicos(candidato.estructura()),
                atomos_canonicos(recapturado.estructura())
            );
            assert_eq!(
                validar_candidato_recargado(&snapshot, &candidato, &recapturado)
                    .unwrap_err()
                    .codigo(),
                "stage_candidate_mismatch"
            );
        }
    }

    #[test]
    fn recarga_usa_configuracion_externa_capturada_sin_releerla() {
        let (temp, snapshot) = snapshot_minimo();
        let raiz = temp.path().join("bundle");
        let ruta_documento = raiz.join("knowledge/a.md");
        let antes = fs::read(&ruta_documento).unwrap();
        let candidato = construir_candidato(snapshot.estructura(), &Delta::default()).unwrap();
        let documentos = serializar_candidato(snapshot.estructura(), &candidato, &[]).unwrap();
        let plan = planificar_staging(&snapshot, &documentos).unwrap();
        fs::remove_file(temp.path().join("manifest.toml")).unwrap();
        fs::remove_file(temp.path().join("invariants.smt2")).unwrap();

        let stage = materializar_staging(
            &UbicacionRaiz::nueva(&raiz).unwrap(),
            &snapshot,
            &plan,
            &candidato,
        )
        .unwrap();
        assert!(stage.recibo().is_some());
        assert_eq!(fs::read(&ruta_documento).unwrap(), antes);
    }

    #[test]
    fn comparador_rechaza_manifiesto_de_candidato_distinto() {
        let (temp, original, _, _) = snapshot_con_parent();
        let raiz = temp.path().join("bundle");
        let (_otro_temp, otro_manifiesto, _, _) = snapshot_con_parent_version("stage-parent/2");
        let candidato =
            construir_candidato(otro_manifiesto.estructura(), &Delta::default()).unwrap();
        let esperado = construir_candidato(original.estructura(), &Delta::default()).unwrap();
        let documentos = serializar_candidato(original.estructura(), &esperado, &[]).unwrap();
        let plan = planificar_staging(&original, &documentos).unwrap();
        let error = materializar_staging(
            &UbicacionRaiz::nueva(&raiz).unwrap(),
            &original,
            &plan,
            &candidato,
        )
        .unwrap_err();

        assert_eq!(error.codigo(), "stage_candidate_mismatch");
        assert!(!format!("{error:?} {error}").contains("stage-parent/2"));
    }

    #[test]
    fn parent_inferido_que_cambia_se_persiste_explicito_al_reasignar_o_quitar() {
        for nuevo_padre in [Some("knowledge/sub/leaf"), None] {
            let (temp, snapshot, _, _) = snapshot_con_parent();
            let raiz = temp.path().join("bundle");
            let hijo = BundlePath::nuevo("knowledge/sub/index").unwrap();
            let id = snapshot.estructura().por_path(&hijo).unwrap();
            assert!(!snapshot.estructura().parent_frontmatter_explicito(id));
            let candidato = candidato_con_padre(&snapshot, nuevo_padre);
            let override_parent = ParentOverride::new(
                hijo.clone(),
                nuevo_padre.map(|padre| BundlePath::nuevo(padre).unwrap()),
            );
            let documentos =
                serializar_candidato(snapshot.estructura(), &candidato, &[override_parent])
                    .unwrap();
            let hijo_doc = documentos.iter().find(|doc| doc.path() == &hijo).unwrap();
            assert!(hijo_doc.contenido().contains("parent:"));
            let plan = planificar_staging(&snapshot, &documentos).unwrap();

            let stage = materializar_staging(
                &UbicacionRaiz::nueva(&raiz).unwrap(),
                &snapshot,
                &plan,
                &candidato,
            )
            .unwrap();
            let esperado = parent_explicito_esperado(snapshot.estructura(), candidato.estructura());
            assert_eq!(esperado.get(&hijo), Some(&true));
            assert!(stage.recibo().is_some());
        }
    }

    #[test]
    fn rechaza_candidato_distinto_y_conserva_stage_parcial_sin_recibo() {
        let (temp, snapshot, _, _) = snapshot_con_parent();
        let raiz = temp.path().join("bundle");
        let antes = fs::read(raiz.join("knowledge/index.md")).unwrap();
        let candidato = candidato_con_titulo(&snapshot, "Root edited");
        let mut documentos = serializar_candidato(snapshot.estructura(), &candidato, &[]).unwrap();
        let index = documentos
            .iter()
            .position(|documento| documento.path().como_str() == "knowledge/index")
            .unwrap();
        documentos[index] = documento(
            "knowledge/index",
            &documentos[index]
                .contenido()
                .replace("Root edited", "Wrong target"),
        );
        let plan = planificar_staging(&snapshot, &documentos).unwrap();
        let error = materializar_staging(
            &UbicacionRaiz::nueva(&raiz).unwrap(),
            &snapshot,
            &plan,
            &candidato,
        )
        .unwrap_err();

        assert_eq!(error.codigo(), "stage_candidate_mismatch");
        assert!(error.stage_parcial().unwrap().recibo().is_none());
        assert!(!format!("{error:?} {error}").contains("Wrong target"));
        assert_eq!(fs::read(raiz.join("knowledge/index.md")).unwrap(), antes);
    }

    #[test]
    fn recarga_copia_configuracion_local_exacta_al_stage() {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        fs::create_dir_all(raiz.join("knowledge")).unwrap();
        fs::write(raiz.join("knowledge/a.md"), "---\ntype: a\ntitle: A\n---\n").unwrap();
        let manifiesto = "version = \"local/1\"\n\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patrón\"]\n\n[layout]\ndirectorios = [\"knowledge\"]\nsin_iota = []\nignorados = []\n";
        let invariantes = "\n; raw local config\n";
        fs::write(raiz.join("manifest.toml"), manifiesto).unwrap();
        fs::write(raiz.join("invariants.smt2"), invariantes).unwrap();
        let snapshot = capturar_snapshot(
            &raiz,
            &raiz.join("manifest.toml"),
            &raiz.join("invariants.smt2"),
            "engine/1",
            "policy/1",
        )
        .unwrap();
        let candidato = construir_candidato(snapshot.estructura(), &Delta::default()).unwrap();
        let documentos = serializar_candidato(snapshot.estructura(), &candidato, &[]).unwrap();
        let plan = planificar_staging(&snapshot, &documentos).unwrap();
        fs::remove_file(raiz.join("manifest.toml")).unwrap();
        fs::remove_file(raiz.join("invariants.smt2")).unwrap();
        let stage = materializar_staging(
            &UbicacionRaiz::nueva(&raiz).unwrap(),
            &snapshot,
            &plan,
            &candidato,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(stage.ruta().join("manifest.toml")).unwrap(),
            manifiesto
        );
        assert_eq!(
            fs::read_to_string(stage.ruta().join("invariants.smt2")).unwrap(),
            invariantes
        );
        assert!(stage.recibo().is_some());
    }
}
