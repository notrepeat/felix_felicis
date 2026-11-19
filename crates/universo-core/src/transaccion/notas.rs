//! Notas (nodos) nuevas de un delta: validación e inserción en una base.
//!
//! Las notas se insertan en una copia del snapshot antes de resolver el resto
//! del delta, para que sus aristas, etiquetas y atributos puedan referirse a
//! ellas. La nota debe quedar idéntica a como el cargador OKF la leería del
//! disco: sin `passthrough`, con el cuerpo como anexo y con el padre legado
//! `dir/index` si existe y el directorio no está en `sin_iota`.
//!
//! Alcance (propuesta del paso 5): no se crean `index` (reasignarían el padre
//! de notas existentes) ni con sortes reservadas (`Retirado`, `Patrón`: retirar
//! es retipar). Como con cualquier nota, el mismo delta puede retipar después
//! la nota creada; eso equivale a crearla y retirarla. Borrar notas no está
//! soportado.
use crate::{Anexo, Delta, ErrorEstructura, Estructura, NotaNueva};

use super::{ClaseOperacion, ErrorCandidato};

const SORTES_RESERVADAS: [&str; 2] = ["Retirado", "Patrón"];

fn error(codigo: &'static str) -> ErrorCandidato {
    ErrorCandidato::InvalidOperation {
        clase: ClaseOperacion::Nota,
        codigo,
    }
}

/// `ruta` es una de `directorios` o está bajo alguna (por componentes).
fn bajo_alguno(ruta: &str, directorios: &[String]) -> bool {
    directorios.iter().any(|d| {
        ruta == d
            || ruta
                .strip_prefix(d.as_str())
                .is_some_and(|resto| resto.starts_with('/'))
    })
}

fn validar(snapshot: &Estructura, nota: &NotaNueva) -> Result<(), ErrorCandidato> {
    if nota.path.stem() == "index" {
        return Err(error("note_index_not_supported"));
    }
    if SORTES_RESERVADAS.contains(&nota.tipo.como_str()) {
        return Err(error("note_reserved_sort"));
    }
    let layout = &snapshot.manifiesto().layout;
    let en_layout = nota
        .path
        .directorio()
        .is_some_and(|dir| bajo_alguno(dir, &layout.directorios));
    // Mismo criterio que el cargador: se ignora el archivo cuyo nombre final
    // (con extensión) está en `ignorados`; la nota desaparecería al recargar.
    let nombre_archivo = format!("{}.md", nota.path.stem());
    if !en_layout || layout.ignorados.contains(&nombre_archivo) {
        return Err(error("note_outside_layout"));
    }
    if nota.cuerpo.contains('\r') {
        return Err(error("note_invalid_body"));
    }
    Ok(())
}

/// Reemplaza el cuerpo (anexo) de notas existentes, conservando su
/// frontmatter no mapeado. No actúa sobre notas creadas en el mismo delta.
pub(crate) fn aplicar_cuerpos(
    candidato: &mut Estructura,
    snapshot: &Estructura,
    delta: &Delta,
) -> Result<(), ErrorCandidato> {
    for cuerpo in delta.cuerpos() {
        if delta
            .crear()
            .iter()
            .any(|n| n.path.clave_case() == cuerpo.path.clave_case())
        {
            return Err(error("note_update_of_created"));
        }
        if snapshot.por_path(&cuerpo.path).is_none() {
            return Err(error("note_not_found"));
        }
        if cuerpo.cuerpo.contains('\r') {
            return Err(error("note_invalid_body"));
        }
        let id = candidato
            .por_path(&cuerpo.path)
            .ok_or_else(|| error("note_not_found"))?;
        let mut anexo = candidato.anexo(id).cloned().unwrap_or_default();
        anexo.cuerpo = cuerpo.cuerpo.clone();
        candidato
            .anexar(id, anexo)
            .map_err(|_| ErrorCandidato::SnapshotInconsistente)?;
    }
    Ok(())
}

/// Las notas nuevas se escriben sin clave de padre: en el candidato final su
/// padre debe ser el legado, o la recarga desde disco no lo reproduciría.
pub(crate) fn verificar_padres_legados(
    candidato: &Estructura,
    delta: &Delta,
) -> Result<(), ErrorCandidato> {
    for nota in delta.crear() {
        let id = candidato
            .por_path(&nota.path)
            .ok_or(ErrorCandidato::SnapshotInconsistente)?;
        if candidato.padre(id) != padre_legado(candidato, &nota.path) {
            return Err(error("note_parent_not_legacy"));
        }
    }
    Ok(())
}

/// Padre que el cargador OKF infiere para una nota sin clave de padre
/// explícita: `dir/index` si existe en `estructura` y `dir` no está en
/// `sin_iota`. Nunca se aplica a un `index` (su padre sería el del directorio
/// superior; las notas nuevas no pueden ser `index`).
pub fn padre_legado(estructura: &Estructura, path: &crate::BundlePath) -> Option<crate::NodoId> {
    if path.stem() == "index" {
        return None;
    }
    let dir = path.directorio()?;
    if bajo_alguno(dir, &estructura.manifiesto().layout.sin_iota) {
        return None;
    }
    let candidato = crate::BundlePath::nuevo(&format!("{dir}/index")).ok()?;
    estructura.por_path(&candidato)
}

/// Base sobre la que se resuelve el delta: el snapshot prestado si no crea
/// notas, o una copia propia con las notas nuevas insertadas.
pub(crate) enum Base<'a> {
    Prestada(&'a Estructura),
    Propia(Box<Estructura>),
}

impl std::ops::Deref for Base<'_> {
    type Target = Estructura;
    fn deref(&self) -> &Estructura {
        match self {
            Base::Prestada(estructura) => estructura,
            Base::Propia(estructura) => estructura,
        }
    }
}

/// Una arista se escribe en el bundle como `[[stem]]` del destino: si otra
/// nota comparte ese nombre, el cargador OKF la vería ambigua y el apply
/// fallaría al recargar. Se rechaza aquí, con el mismo criterio del cargador
/// (stem exacto), para que la simulación y el apply coincidan. Dos notas con
/// el mismo nombre sin aristas hacia ellas siguen siendo válidas.
pub(crate) fn verificar_enlaces(e: &Estructura) -> Result<(), ErrorCandidato> {
    let mut cuenta: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (_, nodo) in e.nodos() {
        *cuenta.entry(nodo.path.stem()).or_default() += 1;
    }
    if cuenta.values().all(|n| *n < 2) {
        return Ok(());
    }
    for (_, arista) in e.aristas() {
        let destino = e
            .nodo(arista.destino)
            .ok_or(ErrorCandidato::SnapshotInconsistente)?;
        if cuenta.get(destino.path.stem()).is_some_and(|n| *n > 1) {
            return Err(ErrorCandidato::InvalidOperation {
                clase: ClaseOperacion::Edge,
                codigo: "edge_target_ambiguous",
            });
        }
    }
    Ok(())
}

pub(crate) fn base_con_notas<'a>(
    snapshot: &'a Estructura,
    delta: &Delta,
) -> Result<Base<'a>, ErrorCandidato> {
    if delta.crear().is_empty() {
        return Ok(Base::Prestada(snapshot));
    }
    let mut base = snapshot.clonar_para_candidato();
    let mut creadas = Vec::new();
    for nota in delta.crear() {
        validar(snapshot, nota)?;
        let id = base
            .insertar_nodo(nota.path.clone(), nota.tipo.clone(), nota.etiqueta.clone())
            .map_err(|e| match e {
                ErrorEstructura::SorteNodoDesconocido(_) => error("note_unknown_sort"),
                ErrorEstructura::EtiquetaInvalida(_) => error("note_invalid_label"),
                ErrorEstructura::PathDuplicado(_) | ErrorEstructura::ColisionDeCase(..) => {
                    error("note_exists")
                }
                _ => error("note_invalid"),
            })?;
        base.anexar(
            id,
            Anexo {
                cuerpo: nota.cuerpo.clone(),
                passthrough: Vec::new(),
            },
        )
        .map_err(|_| ErrorCandidato::SnapshotInconsistente)?;
        creadas.push((id, nota));
    }
    // Las notas nuevas nunca son `index`, así que sólo pueden colgar de un
    // índice existente en el snapshot.
    for (id, nota) in creadas {
        if let Some(padre) = padre_legado(snapshot, &nota.path) {
            base.asignar_padre(id, padre)
                .map_err(|_| ErrorCandidato::SnapshotInconsistente)?;
        }
    }
    Ok(Base::Propia(Box::new(base)))
}
