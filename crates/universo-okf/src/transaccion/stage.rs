//! Plan puro de staging desde inventario raw, sin raíz ni operaciones de filesystem.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use universo_core::BundlePath;

use crate::serializar::DocumentoCandidato;

use super::captura::Snapshot;
use super::snapshot::{EntradaSnapshot, TipoEntradaSnapshot};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorPlanStaging {
    RutaInvalida,
    EntradaDuplicada,
    ConflictoArchivoDirectorio,
    ColisionAuxiliarAdministrado,
    SnapshotInconsistente,
}

impl ErrorPlanStaging {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::RutaInvalida => "stage_path_invalid",
            Self::EntradaDuplicada => "stage_duplicate_entry",
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
        || ruta
            .split('/')
            .any(|segmento| matches!(segmento, "" | "." | ".."))
    {
        return Err(ErrorPlanStaging::RutaInvalida);
    }
    Ok(())
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

    fn directorio(ruta: &str) -> EntradaSnapshot {
        EntradaSnapshot::directorio(ruta)
    }

    fn archivo(ruta: &str, bytes: &[u8]) -> EntradaSnapshot {
        EntradaSnapshot::archivo(ruta, bytes.to_vec())
    }

    fn rutas(plan: &PlanStaging) -> Vec<String> {
        plan.entradas_objetivo
            .iter()
            .map(|entrada| entrada.ruta.clone())
            .collect()
    }

    #[test]
    fn planifica_deterministicamente_y_preserva_directorios_y_auxiliares() {
        let entradas = vec![
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
}
