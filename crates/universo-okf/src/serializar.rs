//! Serializador canónico: `Estructura` → texto de nodo `.md` / bundle en
//! disco (spec §4.3, §4.4).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use universo_core::{BundlePath, Candidato, Estructura, NodoId, Sorte};

use crate::errores::ErrorEscritura;

/// Caracteres con los que un escalar literal no puede empezar (spec §4.3).
const PRIMEROS_PROHIBIDOS: [char; 12] =
    ['"', '\'', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>'];

/// Valor de persistencia del `Parent` final de un hijo que cambió.
/// `padre: None` solicita persistir una raíz explícita.
pub struct ParentOverride {
    hijo: BundlePath,
    padre: Option<BundlePath>,
}

impl ParentOverride {
    /// Crea un override lógico para un hijo.
    pub fn new(hijo: BundlePath, padre: Option<BundlePath>) -> Self {
        Self { hijo, padre }
    }

    /// Hijo al que pertenece el override.
    pub fn hijo(&self) -> &BundlePath {
        &self.hijo
    }

    /// Padre lógico final, o `None` para raíz.
    pub fn padre(&self) -> Option<&BundlePath> {
        self.padre.as_ref()
    }
}

impl std::fmt::Debug for ParentOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ParentOverride { .. }")
    }
}

/// Archivo `.md` candidato. Su formato de depuración omite path y contenido.
#[derive(Clone, PartialEq, Eq)]
pub struct DocumentoCandidato {
    path: BundlePath,
    contenido: String,
}

impl DocumentoCandidato {
    #[cfg(test)]
    pub(crate) fn nuevo(path: BundlePath, contenido: String) -> Self {
        Self { path, contenido }
    }

    /// Path lógico del nodo serializado.
    pub fn path(&self) -> &BundlePath {
        &self.path
    }

    /// Contenido UTF-8 canónico del documento.
    pub fn contenido(&self) -> &str {
        &self.contenido
    }
}

impl std::fmt::Debug for DocumentoCandidato {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DocumentoCandidato { .. }")
    }
}

/// Error redactado al serializar un candidato de Parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorSerializarCandidato {
    ParentNoPersistible,
    ParentOverrideRequerido,
    ParentOverrideSobrante,
    ParentOverrideNoCoincide,
    SnapshotIncompatible,
}

impl ErrorSerializarCandidato {
    /// Código estable y sin rutas ni contenido del bundle.
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::ParentNoPersistible => "parent_not_persistable",
            Self::ParentOverrideRequerido => "parent_override_required",
            Self::ParentOverrideSobrante => "parent_override_unexpected",
            Self::ParentOverrideNoCoincide => "parent_override_mismatch",
            Self::SnapshotIncompatible => "snapshot_incompatible",
        }
    }
}

impl std::fmt::Display for ErrorSerializarCandidato {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorSerializarCandidato {}

/// `true` si `valor` puede escribirse literal (sin comillas): no vacío, no
/// empieza ni termina en blanco, no empieza con uno de los caracteres de
/// `PRIMEROS_PROHIBIDOS`, `%`, `` ` `` o `-`, y no contiene `: `, ` #`,
/// `"`, `\`, `, ` (coma-espacio) ni `\n` (spec §4.3, que lista `, `
/// explícitamente entre los caracteres que fuerzan comillas): un `scope`
/// con comas (`tension`/`scope`, spec §4.1) nunca debe salir sin comillas,
/// porque una coma-espacio sin comillas sería indistinguible de un
/// separador de lista.
fn es_literal_seguro(valor: &str) -> bool {
    if valor.is_empty() {
        return false;
    }
    let primero = valor.chars().next().expect("no vacío");
    if primero.is_whitespace() {
        return false;
    }
    if PRIMEROS_PROHIBIDOS.contains(&primero) || primero == '%' || primero == '`' || primero == '-'
    {
        return false;
    }
    let ultimo = valor.chars().next_back().expect("no vacío");
    if ultimo.is_whitespace() {
        return false;
    }
    if valor.contains(": ")
        || valor.contains(" #")
        || valor.contains(", ")
        || valor.contains('"')
        || valor.contains('\\')
    {
        return false;
    }
    if valor.contains('\n') {
        return false;
    }
    true
}

/// Escalar de salida: literal cuando es seguro (`es_literal_seguro`); si
/// no, entre comillas dobles con `\"` y `\\` escapados (spec §4.3). Debe
/// hacer round-trip exacto con `EntradaCruda::valor_escalar`.
pub fn escalar_salida(valor: &str) -> String {
    if es_literal_seguro(valor) {
        return valor.to_string();
    }
    let mut salida = String::with_capacity(valor.len() + 2);
    salida.push('"');
    for c in valor.chars() {
        match c {
            '\\' => salida.push_str("\\\\"),
            '"' => salida.push_str("\\\""),
            otro => salida.push(otro),
        }
    }
    salida.push('"');
    salida
}

/// Texto completo del archivo `.md` de un nodo, en forma canónica (spec
/// §4.3). Devuelve la cadena vacía si `id` está stale.
pub fn serializar_nodo(u: &Estructura, id: NodoId) -> String {
    serializar_nodo_con_parent(u, id, None)
}

/// El `Some` externo fuerza la declaración administrada de Parent, incluido
/// `Some(None)` para persistir una raíz explícita. Sin override, conserva la
/// distinción entre frontmatter Parent explícito e inferencia legacy.
fn serializar_nodo_con_parent(
    u: &Estructura,
    id: NodoId,
    parent_override: Option<Option<&universo_core::BundlePath>>,
) -> String {
    let Some(nodo) = u.nodo(id) else {
        return String::new();
    };
    let manifiesto = u.manifiesto();
    let vacio_anexo = universo_core::Anexo::default();
    let anexo = u.anexo(id).unwrap_or(&vacio_anexo);

    let mut fm = String::new();
    fm.push_str("---\n");

    // 1. `type:`.
    if nodo.tipo == Sorte::nuevo("Retirado") {
        if let Some(entrada) = anexo.passthrough.iter().find(|e| e.clave == "type") {
            fm.push_str(&entrada.crudo);
        } else {
            fm.push_str("type: Retirado\n");
        }
        if let Some((clave, valor)) = &manifiesto.retirado_en_disco {
            fm.push_str(&format!("{clave}: {}\n", escalar_salida(valor)));
        }
    } else {
        fm.push_str(&format!(
            "type: {}\n",
            escalar_salida(manifiesto.sorte_a_disco(&nodo.tipo))
        ));
    }

    // 2. `title:`.
    fm.push_str(&format!(
        "title: {}\n",
        escalar_salida(nodo.etiqueta.como_str())
    ));

    if let Some(clave) = manifiesto.layout.parent_key.as_deref() {
        let padre_explicito = parent_override.or_else(|| {
            u.parent_frontmatter_explicito(id).then(|| {
                u.padre(id)
                    .and_then(|padre| u.nodo(padre))
                    .map(|padre| &padre.path)
            })
        });
        if let Some(padre) = padre_explicito {
            let valor = padre.map_or("", universo_core::BundlePath::como_str);
            fm.push_str(&format!("{clave}: {}\n", escalar_salida(valor)));
        }
    }

    // 3. Predicados de extensión en orden de declaración, solo los
    //    presentes.
    let atributos = u.atributos(id);
    for decl in &manifiesto.predicados {
        // `atributos` se indexa por el nombre del predicado (spec §3.4),
        // no por su clave de frontmatter.
        if let Some(constante) = atributos.get(&decl.nombre) {
            fm.push_str(&format!("{}: {}\n", decl.clave, escalar_salida(constante)));
        }
    }

    // 4. Passthrough en orden original, crudo verbatim (salvo el `type`
    //    ya emitido en el paso 1 cuando el nodo es Retirado).
    for entrada in &anexo.passthrough {
        if nodo.tipo == Sorte::nuevo("Retirado") && entrada.clave == "type" {
            continue;
        }
        if manifiesto.layout.parent_key.as_deref() == Some(entrada.clave.as_str()) {
            continue;
        }
        fm.push_str(&entrada.crudo);
    }

    // 5. Aristas por tipo en orden de declaración del manifiesto, solo
    //    los tipos con al menos una arista saliente de ese tipo.
    let salientes = u.salientes(id);
    for decl in &manifiesto.aristas {
        let del_tipo: Vec<_> = salientes
            .iter()
            .filter_map(|&aid| u.arista(aid))
            .filter(|a| a.tipo.como_str() == decl.clave)
            .collect();
        if del_tipo.is_empty() {
            continue;
        }
        match (&decl.destino, &decl.etiqueta) {
            (Some(destino_campo), Some(etiqueta_campo)) => {
                fm.push_str(&format!("{}:\n", decl.clave));
                for arista in &del_tipo {
                    let stem = u.nodo(arista.destino).map(|n| n.path.stem()).unwrap_or("");
                    fm.push_str(&format!(
                        "  - {destino_campo}: \"[[{stem}]]\"\n    {etiqueta_campo}: {}\n",
                        escalar_salida(arista.etiqueta.como_str())
                    ));
                }
            }
            _ => {
                let stems: Vec<String> = del_tipo
                    .iter()
                    .map(|arista| {
                        let stem = u.nodo(arista.destino).map(|n| n.path.stem()).unwrap_or("");
                        format!("\"[[{stem}]]\"")
                    })
                    .collect();
                fm.push_str(&format!("{}: [{}]\n", decl.clave, stems.join(", ")));
            }
        }
    }

    fm.push_str("---\n");
    fm.push_str(&anexo.cuerpo);
    fm
}

/// Serializa los documentos de un candidato sin acceder al filesystem.
///
/// El mapa de overrides debe nombrar exactamente los hijos cuyo `Parent`
/// lógico cambió desde `snapshot`; cada valor debe ser el padre final del
/// candidato, o `None` para persistir una raíz explícita. Los nodos sin
/// cambio conservan si usaban una clave Parent explícita o inferencia legacy.
pub fn serializar_candidato(
    snapshot: &Estructura,
    candidato: &Candidato,
    overrides: &[ParentOverride],
) -> Result<Vec<DocumentoCandidato>, ErrorSerializarCandidato> {
    let estructura = candidato.estructura();
    if snapshot.manifiesto() != estructura.manifiesto() {
        return Err(ErrorSerializarCandidato::SnapshotIncompatible);
    }

    let padres_snapshot = padres_por_path(snapshot);
    let padres_candidato = padres_por_path(estructura);
    if padres_snapshot.keys().ne(padres_candidato.keys()) {
        return Err(ErrorSerializarCandidato::SnapshotIncompatible);
    }

    let cambiados: BTreeSet<_> = padres_snapshot
        .iter()
        .filter_map(|(hijo, padre)| {
            (padres_candidato.get(hijo) != Some(padre)).then_some(hijo.clone())
        })
        .collect();
    if !cambiados.is_empty() && estructura.manifiesto().layout.parent_key.is_none() {
        return Err(ErrorSerializarCandidato::ParentNoPersistible);
    }

    let mut por_hijo = BTreeMap::new();
    for override_parent in overrides {
        if por_hijo
            .insert(override_parent.hijo.clone(), override_parent.padre.clone())
            .is_some()
        {
            return Err(ErrorSerializarCandidato::ParentOverrideSobrante);
        }
    }
    if cambiados.iter().any(|hijo| !por_hijo.contains_key(hijo)) {
        return Err(ErrorSerializarCandidato::ParentOverrideRequerido);
    }
    if por_hijo.keys().any(|hijo| !cambiados.contains(hijo)) {
        return Err(ErrorSerializarCandidato::ParentOverrideSobrante);
    }
    if por_hijo
        .iter()
        .any(|(hijo, padre)| padres_candidato.get(hijo) != Some(padre))
    {
        return Err(ErrorSerializarCandidato::ParentOverrideNoCoincide);
    }

    let mut nodos: Vec<_> = estructura.nodos().collect();
    nodos.sort_by(|(_, a), (_, b)| a.path.cmp(&b.path));
    Ok(nodos
        .into_iter()
        .map(|(id, nodo)| DocumentoCandidato {
            path: nodo.path.clone(),
            contenido: serializar_nodo_con_parent(
                estructura,
                id,
                por_hijo.get(&nodo.path).map(|padre| padre.as_ref()),
            ),
        })
        .collect())
}

fn padres_por_path(estructura: &Estructura) -> BTreeMap<BundlePath, Option<BundlePath>> {
    estructura
        .nodos()
        .map(|(id, nodo)| {
            let padre = estructura
                .padre(id)
                .and_then(|id_padre| estructura.nodo(id_padre))
                .map(|nodo_padre| nodo_padre.path.clone());
            (nodo.path.clone(), padre)
        })
        .collect()
}

/// Escribe `raiz/{path}.md` para cada nodo, creando directorios. Escritura
/// por archivo a temporal (`<destino>.tmp`) + rename: un archivo nunca
/// queda a medias. La atomicidad de todo el bundle (todos los archivos a
/// la vez o ninguno) es del plan 06; aquí cada archivo individual queda,
/// en todo momento, o con su contenido anterior o con el nuevo, nunca a
/// medio escribir.
pub fn serializar_bundle(u: &Estructura, raiz: &Path) -> Result<(), ErrorEscritura> {
    for (id, nodo) in u.nodos() {
        let destino = raiz.join(format!("{}.md", nodo.path.como_str()));
        let temporal = raiz.join(format!("{}.md.tmp", nodo.path.como_str()));
        let texto = serializar_nodo(u, id);

        let reportar = |detalle: std::io::Error| ErrorEscritura {
            path: nodo.path.como_str().to_string(),
            detalle: detalle.to_string(),
        };

        if let Some(padre) = destino.parent() {
            std::fs::create_dir_all(padre).map_err(reportar)?;
        }
        std::fs::write(&temporal, texto.as_bytes()).map_err(reportar)?;
        // La escritura va a `<destino>.md.tmp` y `rename` reemplaza el
        // destino en un solo paso (en Windows, `MoveFileExW` con
        // `MOVEFILE_REPLACE_EXISTING`); el archivo está siempre completo,
        // con el contenido anterior o el nuevo. La atomicidad del bundle
        // entero es del plan 06.
        std::fs::rename(&temporal, &destino).map_err(reportar)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsear_bundle;
    use universo_core::BundlePath;
    use universo_testkit::manifiesto_minimo;

    #[test]
    fn orden_canonico_del_frontmatter() {
        let archivos = vec![
            ("n/x.md".to_string(), "---\nextra: 1\nrel: [\"[[y]]\"]\nnivel: alto\ntitle: X\ntype: a\nten:\n  - with:  \"[[y]]\"\n    scope: \"a, b\"\n---\ncuerpo".to_string()),
            ("n/y.md".to_string(), "---\ntype: b\ntitle: Y\nrel: [\"[[x]]\"]\nten:\n  - with: \"[[x]]\"\n    scope: c\n---\n".to_string()),
        ];
        let u = parsear_bundle(&archivos, manifiesto_minimo()).unwrap();
        let x = u.por_path(&BundlePath::nuevo("n/x").unwrap()).unwrap();
        assert_eq!(
            serializar_nodo(&u, x),
            "---\ntype: a\ntitle: X\nnivel: alto\nextra: 1\nrel: [\"[[y]]\"]\nten:\n  - with: \"[[y]]\"\n    scope: \"a, b\"\n---\ncuerpo"
        );
        let y = u.por_path(&BundlePath::nuevo("n/y").unwrap()).unwrap();
        assert_eq!(
            serializar_nodo(&u, y),
            "---\ntype: b\ntitle: Y\nrel: [\"[[x]]\"]\nten:\n  - with: \"[[x]]\"\n    scope: c\n---\n"
        );
    }
    #[test]
    fn retirado_emite_type_original_y_marca() {
        let archivos = vec![(
            "n/r.md".to_string(),
            "---\ntype: a\ntitle: R\nstatus: deprecated\n---\n".to_string(),
        )];
        let u = parsear_bundle(&archivos, manifiesto_minimo()).unwrap();
        let r = u.por_path(&BundlePath::nuevo("n/r").unwrap()).unwrap();
        assert_eq!(
            serializar_nodo(&u, r),
            "---\ntype: a\nstatus: deprecated\ntitle: R\n---\n"
        );
    }
    #[test]
    fn patron_emite_su_valor_de_disco() {
        let archivos = vec![(
            "n/p.md".to_string(),
            "---\ntype: pattern\ntitle: P\n---\n".to_string(),
        )];
        let u = parsear_bundle(&archivos, manifiesto_minimo()).unwrap();
        let p = u.por_path(&BundlePath::nuevo("n/p").unwrap()).unwrap();
        assert_eq!(
            serializar_nodo(&u, p),
            "---\ntype: pattern\ntitle: P\n---\n"
        );
    }
    #[test]
    fn nodo_sin_anexo() {
        let mut u = universo_core::Estructura::nueva(manifiesto_minimo()).unwrap();
        let x = u
            .insertar_nodo(
                BundlePath::nuevo("n/x").unwrap(),
                universo_core::Sorte::nuevo("a"),
                universo_core::Etiqueta::nuevo("X: y"),
            )
            .unwrap();
        assert_eq!(
            serializar_nodo(&u, x),
            "---\ntype: a\ntitle: \"X: y\"\n---\n"
        );
    }
    #[test]
    fn escalares_se_citan_solo_cuando_hace_falta() {
        assert_eq!(escalar_salida("hola"), "hola");
        assert_eq!(escalar_salida("a: b"), "\"a: b\"");
        assert_eq!(escalar_salida("a:b"), "a:b");
        assert_eq!(escalar_salida("[x]"), "\"[x]\"");
        assert_eq!(escalar_salida(" x"), "\" x\"");
        assert_eq!(escalar_salida("x "), "\"x \"");
        assert_eq!(escalar_salida(""), "\"\"");
        assert_eq!(escalar_salida("di \"hola\" \\"), "\"di \\\"hola\\\" \\\\\"");
        assert_eq!(escalar_salida("- x"), "\"- x\"");
        assert_eq!(escalar_salida("x #y"), "\"x #y\"");
        assert_eq!(
            escalar_salida("Título — con guion largo"),
            "Título — con guion largo"
        );
        for v in ["hola", "a: b", "", "di \"hola\" \\", "- x", " x"] {
            let doc =
                crate::parsear_documento(&format!("---\nk: {}\n---\n", escalar_salida(v))).unwrap();
            assert_eq!(doc.entradas[0].valor_escalar(), v, "round-trip de {v:?}");
        }
    }

    #[test]
    fn serializar_bundle_dos_veces_sobre_el_mismo_directorio_reemplaza_el_archivo() {
        let archivos = vec![(
            "n/x.md".to_string(),
            "---\ntype: a\ntitle: X\n---\n".to_string(),
        )];
        let u = parsear_bundle(&archivos, manifiesto_minimo()).unwrap();
        let dir = tempfile::tempdir().unwrap();

        serializar_bundle(&u, dir.path()).unwrap();
        serializar_bundle(&u, dir.path()).unwrap();

        let x = u.por_path(&BundlePath::nuevo("n/x").unwrap()).unwrap();
        let contenido = std::fs::read_to_string(dir.path().join("n/x.md")).unwrap();
        assert_eq!(contenido, serializar_nodo(&u, x));
    }

    #[test]
    fn serializar_bundle_admite_el_stem_248_que_deja_temporal_en_255_unidades() {
        let stem = "a".repeat(248);
        let archivo = format!("n/{stem}.md");
        let archivos = vec![(archivo, "---\ntype: a\ntitle: X\n---\n".to_string())];
        let u = parsear_bundle(&archivos, manifiesto_minimo()).unwrap();
        let dir = tempfile::tempdir().unwrap();

        serializar_bundle(&u, dir.path()).unwrap();

        let destino = dir.path().join("n").join(format!("{stem}.md"));
        let temporal = dir.path().join("n").join(format!("{stem}.md.tmp"));
        assert!(destino.is_file());
        assert!(!temporal.exists());
    }
}
