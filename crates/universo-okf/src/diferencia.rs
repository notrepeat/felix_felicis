//! Delta desde un borrador: lo que lleva el root admitido al estado editado.
//!
//! Flujo de U2: se edita una copia del bundle (borrador) y Felix calcula el
//! delta canónico contra el root, que sigue siendo el estado admitido. El
//! delta se revisa y se aplica con `propose --apply`. Lo que ningún delta puede
//! expresar se rechaza en vez de descartarse: borrar notas, cambiar frontmatter
//! no mapeado (`passthrough`), añadir o quitar la clave explícita de padre, y
//! cualquier diferencia en archivos que no son notas cargadas (auxiliares,
//! ignorados o fuera del layout). El delta se verifica antes de devolverlo:
//! construido sobre el root reproduce los átomos y anexos del borrador. El
//! formato del frontmatter de una nota (orden, comillas) no se conserva: el
//! apply escribe la forma canónica.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs, io,
    path::Path,
};

use universo_core::{
    Atomo, CuerpoNuevo, Delta, Estructura, Manifiesto, NotaNueva, construir_candidato,
};

use crate::cargar_bundle;

/// Error de [`delta_desde_borrador`]: código estable y redactado.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ErrorDiferencia(&'static str);
impl ErrorDiferencia {
    pub fn codigo(&self) -> &'static str {
        self.0
    }
}
impl fmt::Debug for ErrorDiferencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl fmt::Display for ErrorDiferencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ErrorDiferencia {}

fn atomos(estructura: &Estructura) -> BTreeSet<Atomo> {
    estructura.atomos().into_iter().collect()
}

/// Archivos regulares bajo `raiz` (rutas relativas con `/`) y sus bytes. Un
/// enlace simbólico se registra como tal, sin seguirlo.
fn archivos(raiz: &Path) -> io::Result<BTreeMap<String, Option<Vec<u8>>>> {
    fn recorrer(
        raiz: &Path,
        dir: &Path,
        salida: &mut BTreeMap<String, Option<Vec<u8>>>,
    ) -> io::Result<()> {
        for entrada in fs::read_dir(dir)? {
            let entrada = entrada?;
            let ruta = entrada.path();
            let tipo = fs::symlink_metadata(&ruta)?.file_type();
            let relativa = ruta
                .strip_prefix(raiz)
                .map_err(io::Error::other)?
                .to_string_lossy()
                .replace('\\', "/");
            if tipo.is_dir() {
                recorrer(raiz, &ruta, salida)?;
            } else if tipo.is_file() {
                salida.insert(relativa, Some(fs::read(&ruta)?));
            } else {
                salida.insert(relativa, None);
            }
        }
        Ok(())
    }
    let mut salida = BTreeMap::new();
    recorrer(raiz, raiz, &mut salida)?;
    Ok(salida)
}

/// Calcula el delta que lleva el bundle admitido en `raiz` al `borrador`.
/// Los errores son códigos estables, sin rutas ni contenido.
pub fn delta_desde_borrador(
    raiz: &Path,
    borrador: &Path,
    manifiesto: Manifiesto,
) -> Result<Delta, ErrorDiferencia> {
    let base =
        cargar_bundle(raiz, manifiesto.clone()).map_err(|_| ErrorDiferencia("base_invalid"))?;
    let editado =
        cargar_bundle(borrador, manifiesto).map_err(|_| ErrorDiferencia("draft_invalid"))?;
    // Archivos que no son notas cargadas (auxiliares, ignorados, fuera del
    // layout): ningún delta los cambia, así que deben ser idénticos.
    let rutas_notas = |e: &Estructura| -> BTreeSet<String> {
        e.nodos()
            .map(|(_, nodo)| format!("{}.md", nodo.path.como_str()))
            .collect()
    };
    let otros = |todos: BTreeMap<String, Option<Vec<u8>>>, notas: BTreeSet<String>| {
        todos
            .into_iter()
            .filter(|(ruta, _)| !notas.contains(ruta))
            .collect::<BTreeMap<_, _>>()
    };
    let otros_base = otros(
        archivos(raiz).map_err(|_| ErrorDiferencia("base_invalid"))?,
        rutas_notas(&base),
    );
    let otros_editados = otros(
        archivos(borrador).map_err(|_| ErrorDiferencia("draft_invalid"))?,
        rutas_notas(&editado),
    );
    if otros_base != otros_editados {
        return Err(ErrorDiferencia("draft_unmanaged_change"));
    }
    if base
        .nodos()
        .any(|(_, nodo)| editado.por_path(&nodo.path).is_none())
    {
        return Err(ErrorDiferencia("note_deletion_unsupported"));
    }

    let mut notas = Vec::new();
    let mut cuerpos = Vec::new();
    for (id, nodo) in editado.nodos() {
        let anexo = editado.anexo(id).cloned().unwrap_or_default();
        match base.por_path(&nodo.path) {
            None => {
                if !anexo.passthrough.is_empty() {
                    return Err(ErrorDiferencia("passthrough_change_unsupported"));
                }
                // Una nota nueva se escribe sin clave de padre (legado).
                if editado.parent_frontmatter_explicito(id) {
                    return Err(ErrorDiferencia("parent_key_change_unsupported"));
                }
                notas.push(NotaNueva {
                    path: nodo.path.clone(),
                    tipo: nodo.tipo.clone(),
                    etiqueta: nodo.etiqueta.clone(),
                    cuerpo: anexo.cuerpo,
                });
            }
            Some(original) => {
                // La clave explícita de padre no se expresa en un delta: el
                // apply la conserva salvo que el padre cambie.
                let padre =
                    |e: &Estructura, n| e.padre(n).and_then(|p| e.nodo(p)).map(|p| p.path.clone());
                let explicita_esperada = base.parent_frontmatter_explicito(original)
                    || padre(&base, original) != padre(&editado, id);
                if editado.parent_frontmatter_explicito(id) != explicita_esperada {
                    return Err(ErrorDiferencia("parent_key_change_unsupported"));
                }
                let previo = base.anexo(original).cloned().unwrap_or_default();
                if previo.passthrough != anexo.passthrough {
                    return Err(ErrorDiferencia("passthrough_change_unsupported"));
                }
                if previo.cuerpo != anexo.cuerpo {
                    cuerpos.push(CuerpoNuevo {
                        path: nodo.path.clone(),
                        cuerpo: anexo.cuerpo,
                    });
                }
            }
        }
    }

    let no_representable = |_| ErrorDiferencia("draft_not_representable");
    let solo_notas = Delta::default()
        .con_notas(notas.clone())
        .map_err(no_representable)?;
    let con_notas =
        construir_candidato(&base, &solo_notas).map_err(|e| ErrorDiferencia(e.codigo()))?;
    let objetivo = atomos(&editado);
    let actual = atomos(con_notas.estructura());
    let agregar = objetivo.difference(&actual).cloned();
    let retirar = actual.difference(&objetivo).cloned();
    let delta = Delta::nuevo(agregar, retirar)
        .and_then(|delta| delta.con_notas(notas))
        .and_then(|delta| delta.con_cuerpos(cuerpos))
        .map_err(no_representable)?;

    // El delta debe reproducir exactamente el borrador (átomos y anexos).
    let candidato = construir_candidato(&base, &delta).map_err(|e| ErrorDiferencia(e.codigo()))?;
    let estructura = candidato.estructura();
    let anexos_iguales = editado.nodos().all(|(id, nodo)| {
        estructura
            .por_path(&nodo.path)
            .is_some_and(|otro| estructura.anexo(otro) == editado.anexo(id))
    });
    if atomos(estructura) != objetivo || !anexos_iguales {
        return Err(ErrorDiferencia("draft_not_representable"));
    }
    Ok(delta)
}
