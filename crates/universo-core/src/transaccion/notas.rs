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
    // Padre legado, como el cargador: `dir/index` si existe y `dir` no está en
    // `sin_iota`. Las notas nuevas nunca son `index`, así que sólo pueden
    // colgar de un índice existente.
    let sin_iota = base.manifiesto().layout.sin_iota.clone();
    for (id, nota) in creadas {
        let Some(dir) = nota.path.directorio() else {
            continue;
        };
        if bajo_alguno(dir, &sin_iota) {
            continue;
        }
        let Ok(candidato) = crate::BundlePath::nuevo(&format!("{dir}/index")) else {
            continue;
        };
        if let Some(padre) = snapshot.por_path(&candidato) {
            base.asignar_padre(id, padre)
                .map_err(|_| ErrorCandidato::SnapshotInconsistente)?;
        }
    }
    Ok(Base::Propia(Box::new(base)))
}
