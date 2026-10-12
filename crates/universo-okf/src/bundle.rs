//! Carga de un bundle OKF a una `Estructura` (spec §3.3, §3.4, §4.2).

use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use universo_core::{
    Anexo, BundlePath, Entrada, ErrorEstructura, ErrorManifiesto, Estructura, Etiqueta, Manifiesto,
    NodoId, Sorte,
};

use crate::documento::{EntradaCruda, parsear_documento};
use crate::errores::{ErrorBundle, ErrorDocumento, ErrorEntrada, Regla};
use crate::transaccion::anclado::{
    DirectorioLecturaAnclado, ErrorDirectorioAnclado, IdentidadDirectorio,
    identidad_directorio_para_recorrido,
};
use crate::transaccion::raiz::UbicacionRaiz;

/// Un archivo ya localizado como `BundlePath`, con su contenido crudo.
struct ArchivoBundle {
    path: BundlePath,
    contenido: String,
}

/// Valor pendiente de la clave optativa que declara el padre de un nodo.
enum PadreFrontmatter {
    /// La clave no estaba presente; conserva la inferencia por directorio.
    InferenciaLegacy,
    /// La clave estaba presente y vacía; no se infiere un padre.
    RaizExplicita,
    /// La clave declara el `BundlePath` lógico exacto del padre.
    Referencia(BundlePath),
}

/// Último segmento del path (nombre de archivo con extensión), aceptando
/// `/` o `\` como separador: mismo criterio que usa `cargar_bundle` para
/// comparar contra `layout.ignorados`.
fn nombre_archivo(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn nombres_ignorados_ordenados(ignorados: &[String]) -> Vec<&str> {
    let mut nombres: Vec<_> = ignorados.iter().map(String::as_str).collect();
    nombres.sort_unstable();
    nombres.dedup();
    nombres
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

fn contiene_directorio_ancestro(ruta: &str, directorios: &[String]) -> bool {
    let contiene = |directorio: &str| {
        directorios
            .binary_search_by(|candidato| candidato.as_str().cmp(directorio))
            .is_ok()
    };
    contiene(ruta)
        || ruta
            .match_indices('/')
            .any(|(indice, _)| contiene(&ruta[..indice]))
}

/// Orden determinista por (path, línea); todo `Err` del cargador sale
/// ordenado así.
fn ordenar_errores(errores: &mut [ErrorBundle]) {
    errores.sort_by(|a, b| (a.path.clone(), a.linea).cmp(&(b.path.clone(), b.linea)));
}

fn errores_validacion_manifiesto(errores_manifiesto: Vec<ErrorManifiesto>) -> Vec<ErrorBundle> {
    let mut errores: Vec<ErrorBundle> = errores_manifiesto
        .into_iter()
        .map(|detalle| error(None, None, Regla::Estructura, detalle.to_string()))
        .collect();
    ordenar_errores(&mut errores);
    errores
}

fn errores_colisiones_case(resueltos: &[ArchivoBundle]) -> Vec<ErrorBundle> {
    let mut por_case: HashMap<String, Vec<usize>> = HashMap::new();
    for (indice, archivo) in resueltos.iter().enumerate() {
        por_case
            .entry(archivo.path.clave_case())
            .or_default()
            .push(indice);
    }

    let mut errores = Vec::new();
    for indices in por_case.values_mut() {
        if indices.len() < 2 {
            continue;
        }
        // Un testigo basta para identificar cada colisión. El más corto
        // mantiene el total de bytes diagnósticos acotado por el tamaño de
        // las rutas de entrada, incluso si un grupo contiene miles de rutas.
        indices.sort_unstable_by_key(|&indice| {
            let ruta = resueltos[indice].path.como_str();
            (ruta.len(), ruta)
        });
        let primero = indices[0];
        let segundo = indices[1];
        for (posicion, &indice) in indices.iter().enumerate() {
            let testigo = if posicion == 0 { segundo } else { primero };
            errores.push(error(
                Some(resueltos[indice].path.como_str()),
                None,
                Regla::ColisionDeCase,
                format!("colisiona con: {}", resueltos[testigo].path.como_str()),
            ));
        }
    }
    errores
}

fn detalle_stem_ambiguo(slug: &str, clave: &str, cantidad: usize) -> String {
    format!("[[{slug}]] desde clave {clave} tiene {cantidad} candidatos")
}

fn error(
    path: Option<&str>,
    linea: Option<usize>,
    regla: Regla,
    detalle: impl Into<String>,
) -> ErrorBundle {
    ErrorBundle {
        path: path.map(str::to_string),
        linea,
        regla,
        detalle: detalle.into(),
    }
}

/// Índices locales para las consultas repetidas del parser. Almacena claves
/// propias e índices de declaración para no retener préstamos de la estructura
/// mientras `parsear_bundle` la modifica.
struct IndicesManifiesto {
    sorte_por_nombre: HashMap<String, usize>,
    sorte_por_alias: HashMap<String, usize>,
    predicado_por_clave: HashMap<String, usize>,
    arista_por_clave: HashMap<String, usize>,
}

impl IndicesManifiesto {
    fn nuevo(manifiesto: &Manifiesto) -> Self {
        let sorte_por_nombre: HashMap<String, usize> = manifiesto
            .sortes_nodo
            .iter()
            .enumerate()
            .map(|(indice, sorte)| (sorte.como_str().to_string(), indice))
            .collect();
        let mut sorte_por_alias = HashMap::with_capacity(manifiesto.sortes_nodo_disco.len());
        // La BTreeMap conserva el orden de la búsqueda anterior. Si dos
        // sortes comparten alias, se mantiene la primera por su clave de sorte.
        for (sorte, alias) in &manifiesto.sortes_nodo_disco {
            if let Some(&indice) = sorte_por_nombre.get(sorte.como_str()) {
                sorte_por_alias.entry(alias.clone()).or_insert(indice);
            }
        }

        Self {
            sorte_por_nombre,
            sorte_por_alias,
            predicado_por_clave: manifiesto
                .predicados
                .iter()
                .enumerate()
                .map(|(indice, predicado)| (predicado.clave.clone(), indice))
                .collect(),
            arista_por_clave: manifiesto
                .aristas
                .iter()
                .enumerate()
                .map(|(indice, arista)| (arista.clave.clone(), indice))
                .collect(),
        }
    }

    fn sorte_desde_disco(&self, manifiesto: &Manifiesto, valor: &str) -> Option<Sorte> {
        let indice = self
            .sorte_por_alias
            .get(valor)
            .or_else(|| self.sorte_por_nombre.get(valor))?;
        manifiesto.sortes_nodo.get(*indice).cloned()
    }

    fn predicado_por_clave<'a>(
        &self,
        manifiesto: &'a Manifiesto,
        clave: &str,
    ) -> Option<&'a universo_core::DeclPredicado> {
        manifiesto
            .predicados
            .get(*self.predicado_por_clave.get(clave)?)
    }

    fn arista_por_clave<'a>(
        &self,
        manifiesto: &'a Manifiesto,
        clave: &str,
    ) -> Option<&'a universo_core::DeclArista> {
        manifiesto.aristas.get(*self.arista_por_clave.get(clave)?)
    }

    fn contiene_arista(&self, clave: &str) -> bool {
        self.arista_por_clave.contains_key(clave)
    }
}

/// Carga pura desde memoria: `archivos` = (path relativo del archivo CON
/// extensión `.md` y separador `/`, contenido). Acumula todos los errores
/// encontrados; si hay al menos uno, nunca devuelve una `Estructura` a
/// medias.
pub fn parsear_bundle(
    archivos: &[(String, String)],
    manifiesto: Manifiesto,
) -> Result<Estructura, Vec<ErrorBundle>> {
    let estructura = match Estructura::nueva(manifiesto) {
        Ok(e) => e,
        Err(err) => {
            let detalles = match err {
                universo_core::ErrorEstructura::Manifiesto(errs) => errs,
                _ => unreachable!("Estructura::nueva solo falla por manifiesto inválido"),
            };
            return Err(errores_validacion_manifiesto(detalles));
        }
    };

    parsear_bundle_desde_estructura(archivos, estructura)
}

pub(crate) fn parsear_bundle_desde_estructura(
    archivos: &[(String, String)],
    mut estructura: Estructura,
) -> Result<Estructura, Vec<ErrorBundle>> {
    let mut errores: Vec<ErrorBundle> = Vec::new();
    let indices_manifiesto = IndicesManifiesto::nuevo(estructura.manifiesto());

    // Paso 0: saltar los archivos cuyo nombre (último segmento del path,
    // con extensión) está en `layout.ignorados` — mismo criterio que
    // aplica `cargar_bundle` al recorrer disco.
    let ignorados = nombres_ignorados_ordenados(&estructura.manifiesto().layout.ignorados);
    let archivos: Vec<&(String, String)> = archivos
        .iter()
        .filter(|(path_original, _)| {
            !contiene_nombre_ordenado(&ignorados, nombre_archivo(path_original), || {})
        })
        .collect();
    drop(ignorados);

    // Paso 1: resolver los paths, detectar colisiones de case y duplicados
    // exactos.
    let mut resueltos: Vec<ArchivoBundle> = Vec::new();
    for (path_original, contenido) in archivos {
        match BundlePath::desde_archivo(path_original) {
            Ok(path) => resueltos.push(ArchivoBundle {
                path,
                contenido: contenido.clone(),
            }),
            Err(err) => {
                // Se reporta con la misma normalización que aplica
                // `desde_archivo` antes de validar (separador `/`, sin
                // `.md`), no el path original tal cual vino.
                let normalizada = path_original.replace('\\', "/");
                let sin_extension = normalizada.strip_suffix(".md").unwrap_or(&normalizada);
                errores.push(error(
                    Some(sin_extension),
                    None,
                    Regla::PathInvalido,
                    err.to_string(),
                ));
            }
        }
    }

    // Detectar colisiones de case (incluye duplicados exactos) sin enumerar
    // todos los pares del grupo en cada diagnóstico.
    errores.extend(errores_colisiones_case(&resueltos));

    struct NodoCargado {
        id: NodoId,
        path: BundlePath,
        padre_frontmatter: PadreFrontmatter,
        /// Entradas de arista pendientes de resolver (clave, entrada).
        aristas_pendientes: Vec<(String, EntradaCruda)>,
    }

    let mut cargados: Vec<NodoCargado> = Vec::new();
    let parent_key = estructura.manifiesto().layout.parent_key.clone();

    for archivo in &resueltos {
        let path_str = archivo.path.como_str().to_string();

        let documento = match parsear_documento(&archivo.contenido) {
            Ok(d) => d,
            Err(err) => {
                let (regla, linea) = match err {
                    ErrorDocumento::FrontmatterAusente => (Regla::FrontmatterAusente, None),
                    ErrorDocumento::FrontmatterSinCierre => (Regla::FrontmatterSinCierre, None),
                    ErrorDocumento::FrontmatterMalformado { linea } => {
                        (Regla::FrontmatterMalformado, Some(linea))
                    }
                    ErrorDocumento::ClaveEstructuralDuplicada { linea, .. } => {
                        (Regla::ClaveEstructuralDuplicada, Some(linea))
                    }
                };
                errores.push(error(Some(&path_str), linea, regla, err.to_string()));
                continue;
            }
        };

        // Buscar `type` y, si aplica, la marca de retirado en disco.
        let entrada_type = documento.entradas.iter().find(|e| e.clave == "type");
        let Some(entrada_type) = entrada_type else {
            errores.push(error(
                Some(&path_str),
                None,
                Regla::TypeAusente,
                "falta la entrada type",
            ));
            continue;
        };

        let clave_consumida_retirado =
            estructura
                .manifiesto()
                .retirado_en_disco
                .clone()
                .and_then(|(clave, valor)| {
                    documento
                        .entradas
                        .iter()
                        .find(|e| e.clave == clave && e.valor_escalar() == valor)
                        .map(|_| clave)
                });

        let tipo = if clave_consumida_retirado.is_some() {
            Sorte::nuevo("Retirado")
        } else {
            match indices_manifiesto
                .sorte_desde_disco(estructura.manifiesto(), &entrada_type.valor_escalar())
            {
                Some(t) if t == Sorte::nuevo("Retirado") => {
                    if let Some((k, v)) = &estructura.manifiesto().retirado_en_disco {
                        errores.push(error(
                            Some(&path_str),
                            Some(entrada_type.linea),
                            Regla::TypeDesconocido,
                            format!(
                                "el sorte reservado Retirado se marca en disco con `{k}: {v}`, no con type"
                            ),
                        ));
                        continue;
                    }
                    t
                }
                Some(t) => t,
                None => {
                    errores.push(error(
                        Some(&path_str),
                        Some(entrada_type.linea),
                        Regla::TypeDesconocido,
                        format!(
                            "type {:?} no resuelve a ninguna sorte",
                            entrada_type.valor_escalar()
                        ),
                    ));
                    continue;
                }
            }
        };

        let entrada_title = documento.entradas.iter().find(|e| e.clave == "title");
        let Some(entrada_title) = entrada_title else {
            errores.push(error(
                Some(&path_str),
                None,
                Regla::TitleAusente,
                "falta la entrada title",
            ));
            continue;
        };
        let etiqueta = Etiqueta::nuevo(&entrada_title.valor_escalar());

        let padre_frontmatter = if let Some(clave) = parent_key.as_deref() {
            let mut entradas_padre = documento.entradas.iter().filter(|e| e.clave == clave);
            let entrada_padre = entradas_padre.next();
            if entradas_padre.next().is_some() {
                errores.push(error(
                    None,
                    None,
                    Regla::ParentDuplicado,
                    "la clave configurada para Parent aparece más de una vez",
                ));
                continue;
            }
            match entrada_padre {
                None => PadreFrontmatter::InferenciaLegacy,
                Some(entrada) => {
                    let valor = entrada.valor_escalar();
                    if valor.is_empty() {
                        PadreFrontmatter::RaizExplicita
                    } else if let Ok(path) = BundlePath::nuevo(&valor) {
                        PadreFrontmatter::Referencia(path)
                    } else {
                        errores.push(error(
                            None,
                            None,
                            Regla::ParentInvalido,
                            "la referencia Parent no es un BundlePath lógico válido",
                        ));
                        continue;
                    }
                }
            }
        } else {
            PadreFrontmatter::InferenciaLegacy
        };

        let id = match estructura.insertar_nodo(archivo.path.clone(), tipo, etiqueta) {
            Ok(id) => id,
            Err(err) => {
                errores.push(error(
                    Some(&path_str),
                    None,
                    Regla::Estructura,
                    err.to_string(),
                ));
                continue;
            }
        };

        // Recorrer entradas en orden: predicados → atributo; aristas →
        // pendiente; resto → passthrough.
        let mut passthrough: Vec<Entrada> = Vec::new();
        let mut aristas_pendientes: Vec<(String, EntradaCruda)> = Vec::new();
        for entrada in &documento.entradas {
            if entrada.clave == "title" {
                continue;
            }
            if parent_key.as_deref() == Some(entrada.clave.as_str()) {
                // La clave configurada está administrada por el cargador:
                // se consume y nunca se conserva como passthrough.
                continue;
            }
            if clave_consumida_retirado.as_deref() == Some(entrada.clave.as_str()) {
                // La clave que activó el retiro (p. ej. `status`) queda
                // consumida: ni passthrough ni clasificación.
                continue;
            }
            if entrada.clave == "type" {
                if clave_consumida_retirado.is_some() {
                    // Con retiro activo, `type` se conserva verbatim como
                    // passthrough (spec §3.3/§4.2).
                    passthrough.push(Entrada {
                        clave: entrada.clave.clone(),
                        crudo: entrada.crudo.clone(),
                    });
                }
                continue;
            }
            if let Some(decl) =
                indices_manifiesto.predicado_por_clave(estructura.manifiesto(), &entrada.clave)
            {
                let nombre = decl.nombre.clone();
                let constante = entrada.valor_escalar();
                if let Err(err) = estructura.asignar_atributo(id, &nombre, &constante) {
                    let regla = match &err {
                        ErrorEstructura::ConstanteFueraDeDominio(..) => {
                            Regla::ConstanteFueraDeDominio
                        }
                        _ => Regla::Estructura,
                    };
                    errores.push(error(
                        Some(&path_str),
                        Some(entrada.linea),
                        regla,
                        err.to_string(),
                    ));
                }
                continue;
            }
            if indices_manifiesto.contiene_arista(&entrada.clave) {
                aristas_pendientes.push((entrada.clave.clone(), entrada.clone()));
                continue;
            }
            passthrough.push(Entrada {
                clave: entrada.clave.clone(),
                crudo: entrada.crudo.clone(),
            });
        }

        if let Err(err) = estructura.anexar(
            id,
            Anexo {
                cuerpo: documento.cuerpo.clone(),
                passthrough,
            },
        ) {
            errores.push(error(
                Some(&path_str),
                None,
                Regla::Estructura,
                err.to_string(),
            ));
        }

        if !matches!(&padre_frontmatter, PadreFrontmatter::InferenciaLegacy)
            && let Err(err) = estructura.marcar_parent_frontmatter_explicito(id)
        {
            errores.push(error(None, None, Regla::Estructura, err.to_string()));
        }

        cargados.push(NodoCargado {
            id,
            path: archivo.path.clone(),
            padre_frontmatter,
            aristas_pendientes,
        });
    }

    // Paso 5: índice de stems.
    let mut stems: HashMap<String, Vec<BundlePath>> = HashMap::new();
    for nodo in &cargados {
        stems
            .entry(nodo.path.stem().to_string())
            .or_default()
            .push(nodo.path.clone());
    }
    let indice_por_path: HashMap<BundlePath, NodoId> =
        cargados.iter().map(|n| (n.path.clone(), n.id)).collect();

    // Paso 6: resolver aristas.
    for nodo in &cargados {
        let path_str = nodo.path.como_str().to_string();
        for (clave, entrada) in &nodo.aristas_pendientes {
            let decl = indices_manifiesto
                .arista_por_clave(estructura.manifiesto(), clave)
                .expect("es_sorte_arista lo garantiza")
                .clone();

            let objetivos: Result<Vec<(String, Etiqueta)>, ErrorEntrada> =
                if let Some(destino_campo) = &decl.destino {
                    let etiqueta_campo = decl.etiqueta.clone().unwrap_or_default();
                    entrada
                        .lista_mappings(destino_campo, &etiqueta_campo)
                        .map(|v| {
                            v.into_iter()
                                .map(|(slug, texto)| (slug, Etiqueta::nuevo(&texto)))
                                .collect()
                        })
                } else {
                    entrada.lista_wikilinks().map(|v| {
                        v.into_iter()
                            .map(|slug| (slug, Etiqueta::nuevo("")))
                            .collect()
                    })
                };

            let objetivos = match objetivos {
                Ok(v) => v,
                Err(err) => {
                    let regla = match err {
                        ErrorEntrada::LinkMalformado(_) => Regla::LinkMalformado,
                        ErrorEntrada::FlowNoAdmitido => Regla::FlowNoAdmitido,
                        ErrorEntrada::ValorEnLineaDeClave => Regla::ValorEnLineaDeClave,
                        ErrorEntrada::EtiquetaAusente => Regla::EtiquetaAusente,
                        ErrorEntrada::DestinoAusente => Regla::DestinoAusente,
                        ErrorEntrada::CampoDuplicado(_) => Regla::ClaveEstructuralDuplicada,
                    };
                    errores.push(error(
                        Some(&path_str),
                        Some(entrada.linea),
                        regla,
                        err.to_string(),
                    ));
                    continue;
                }
            };

            for (slug, etiqueta) in objetivos {
                let Some(candidatos) = stems.get(&slug) else {
                    errores.push(error(
                        Some(&path_str),
                        Some(entrada.linea),
                        Regla::TargetColgante,
                        format!("[[{slug}]] desde clave {clave}"),
                    ));
                    continue;
                };
                if candidatos.len() > 1 {
                    errores.push(error(
                        Some(&path_str),
                        Some(entrada.linea),
                        Regla::StemAmbiguo,
                        detalle_stem_ambiguo(&slug, clave, candidatos.len()),
                    ));
                    continue;
                }
                let destino_id = indice_por_path[&candidatos[0]];
                if let Err(err) =
                    estructura.insertar_arista(nodo.id, destino_id, Sorte::nuevo(clave), etiqueta)
                {
                    errores.push(error(
                        Some(&path_str),
                        Some(entrada.linea),
                        Regla::Estructura,
                        err.to_string(),
                    ));
                }
            }
        }
    }

    // Paso 7: resolver Parent explícito y, solo cuando la clave configurada
    // está ausente, conservar la inferencia legacy por directorio.
    let mut sin_iota = estructura.manifiesto().layout.sin_iota.clone();
    sin_iota.sort_unstable();
    sin_iota.dedup();
    let mut errores_parent: Vec<Option<ErrorBundle>> = (0..cargados.len()).map(|_| None).collect();
    let mut asignaciones_parent = Vec::new();
    for (indice_nodo, nodo) in cargados.iter().enumerate() {
        match &nodo.padre_frontmatter {
            PadreFrontmatter::RaizExplicita => continue,
            PadreFrontmatter::Referencia(parent_path) => {
                let Some(&parent_id) = indice_por_path.get(parent_path) else {
                    errores_parent[indice_nodo] = Some(error(
                        None,
                        None,
                        Regla::ParentNoEncontrado,
                        "la referencia Parent no resuelve a un nodo cargado",
                    ));
                    continue;
                };
                if parent_path == &nodo.path {
                    errores_parent[indice_nodo] = Some(error(
                        None,
                        None,
                        Regla::ParentDeSiMismo,
                        "un nodo no puede ser su propio padre",
                    ));
                    continue;
                }
                asignaciones_parent.push((
                    nodo.id,
                    parent_id,
                    true,
                    nodo.path.clone(),
                    indice_nodo,
                ));
                continue;
            }
            PadreFrontmatter::InferenciaLegacy => {}
        }

        let Some(dir) = nodo.path.directorio() else {
            continue;
        };
        let es_index = nodo.path.stem() == "index";
        // El directorio relevante para "sin ι" es siempre el propio
        // directorio del nodo (para `d/sub/index` es `d/sub`).
        if contiene_directorio_ancestro(dir, &sin_iota) {
            continue;
        }
        let candidato = if es_index {
            match dir.rfind('/') {
                Some(i) => format!("{}/index", &dir[..i]),
                None => continue, // `d/index` de nivel superior: sin candidato
            }
        } else {
            format!("{dir}/index")
        };
        let Ok(candidato_path) = BundlePath::nuevo(&candidato) else {
            continue;
        };
        if candidato_path == nodo.path {
            continue;
        }
        if let Some(&padre_id) = indice_por_path.get(&candidato_path) {
            asignaciones_parent.push((nodo.id, padre_id, false, nodo.path.clone(), indice_nodo));
        }
    }

    let relaciones_parent: Vec<_> = asignaciones_parent
        .iter()
        .map(|&(hijo, padre, _, _, _)| (hijo, padre))
        .collect();
    if let Err(fallos) = estructura.asignar_padres_en_lote(&relaciones_parent) {
        for (indice, err) in fallos {
            let (_, _, explicito, path, indice_nodo) = &asignaciones_parent[indice];
            if *explicito {
                let (regla, detalle) = match err {
                    ErrorEstructura::CicloEnIota => {
                        (Regla::ParentCiclo, "la relación Parent formaría un ciclo")
                    }
                    ErrorEstructura::PadreDeSiMismo => (
                        Regla::ParentDeSiMismo,
                        "un nodo no puede ser su propio padre",
                    ),
                    _ => (
                        Regla::ParentInvalido,
                        "no se pudo resolver la relación Parent",
                    ),
                };
                errores_parent[*indice_nodo] = Some(error(None, None, regla, detalle));
            } else if parent_key.is_some() {
                let regla = if err == ErrorEstructura::CicloEnIota {
                    Regla::ParentCiclo
                } else {
                    Regla::ParentInvalido
                };
                errores_parent[*indice_nodo] = Some(error(
                    None,
                    None,
                    regla,
                    "no se pudo resolver la relación Parent",
                ));
            } else {
                errores_parent[*indice_nodo] = Some(error(
                    Some(path.como_str()),
                    None,
                    Regla::Estructura,
                    err.to_string(),
                ));
            }
        }
    }
    errores.extend(errores_parent.into_iter().flatten());

    if errores.is_empty() {
        Ok(estructura)
    } else {
        ordenar_errores(&mut errores);
        Err(errores)
    }
}

/// Recorre `raiz/{layout.directorios}` recursivamente, solo `*.md`, salta
/// nombres de archivo en `layout.ignorados`, lee UTF-8, delega en
/// `parsear_bundle`.
pub fn cargar_bundle(raiz: &Path, manifiesto: Manifiesto) -> Result<Estructura, Vec<ErrorBundle>> {
    cargar_bundle_observando(raiz, manifiesto, identidad_directorio_para_recorrido, || {})
}

fn cargar_bundle_observando(
    raiz: &Path,
    manifiesto: Manifiesto,
    mut obtener_identidad: impl FnMut(
        &Path,
    )
        -> Result<Option<IdentidadDirectorio>, ErrorDirectorioAnclado>,
    mut observar_recorrido: impl FnMut(),
) -> Result<Estructura, Vec<ErrorBundle>> {
    cargar_bundle_con_observadores(
        raiz,
        manifiesto,
        &mut obtener_identidad,
        &mut observar_recorrido,
        |_| {},
    )
}

fn cargar_bundle_con_observadores(
    raiz: &Path,
    manifiesto: Manifiesto,
    mut obtener_identidad: impl FnMut(
        &Path,
    )
        -> Result<Option<IdentidadDirectorio>, ErrorDirectorioAnclado>,
    mut observar_recorrido: impl FnMut(),
    mut antes_de_abrir_archivo: impl FnMut(&Path),
) -> Result<Estructura, Vec<ErrorBundle>> {
    let mut errores: Vec<ErrorBundle> = Vec::new();
    let mut archivos: Vec<(String, String)> = Vec::new();

    if let Err(detalles) = manifiesto.validar() {
        return Err(errores_validacion_manifiesto(detalles));
    }
    let ignorados = nombres_ignorados_ordenados(&manifiesto.layout.ignorados);

    let metadatos_raiz = match std::fs::metadata(raiz) {
        Ok(metadatos) => metadatos,
        Err(err) => {
            return Err(vec![error(
                None,
                None,
                Regla::Io,
                format!("no se puede acceder a la raíz del bundle: {err}"),
            )]);
        }
    };
    if !metadatos_raiz.is_dir() {
        return Err(vec![error(
            None,
            None,
            Regla::Io,
            "la raíz del bundle no es un directorio",
        )]);
    }
    let ubicacion_raiz = match UbicacionRaiz::nueva(raiz) {
        Ok(ubicacion) => ubicacion,
        Err(err) => {
            return Err(vec![error(
                None,
                None,
                Regla::Io,
                format!("no se puede acceder a la raíz del bundle: {err:?}"),
            )]);
        }
    };
    let raiz_anclada = match DirectorioLecturaAnclado::desde_ubicacion(&ubicacion_raiz) {
        Ok(raiz) => raiz,
        Err(err) => {
            return Err(vec![error(
                None,
                None,
                Regla::Io,
                format!("no se puede acceder a la raíz del bundle: {err:?}"),
            )]);
        }
    };
    let raiz_canonica = ubicacion_raiz.ruta().to_path_buf();

    let mut bases_procesadas: HashSet<IdentidadBaseLayout> = HashSet::new();
    for directorio in &manifiesto.layout.directorios {
        if !directorio_layout_seguro(directorio) {
            errores.push(error(
                Some(directorio),
                None,
                Regla::Io,
                "el directorio declarado debe ser una ruta relativa segura dentro de la raíz",
            ));
            continue;
        }
        let base = raiz_canonica.join(directorio);
        let metadatos = match std::fs::metadata(&base) {
            Ok(metadatos) => metadatos,
            Err(err) => {
                errores.push(error(
                    Some(directorio),
                    None,
                    Regla::Io,
                    format!("no se puede acceder al directorio declarado: {err}"),
                ));
                continue;
            }
        };
        if !metadatos.is_dir() {
            errores.push(error(
                Some(directorio),
                None,
                Regla::Io,
                "la ruta declarada no es un directorio",
            ));
            continue;
        }
        let base_canonica = match std::fs::canonicalize(&base) {
            Ok(c) => c,
            Err(err) => {
                errores.push(error(Some(directorio), None, Regla::Io, err.to_string()));
                continue;
            }
        };
        if !base_canonica.starts_with(&raiz_canonica) {
            errores.push(error(
                Some(directorio),
                None,
                Regla::Io,
                "el directorio declarado resuelve fuera de la raíz del bundle",
            ));
            continue;
        }
        let identidad_base = match obtener_identidad(&base_canonica) {
            Ok(Some(identidad)) => IdentidadBaseLayout::Fisica(identidad),
            Ok(None) => IdentidadBaseLayout::Canonica(base_canonica.clone()),
            Err(_) => {
                errores.push(error(
                    Some(directorio),
                    None,
                    Regla::Io,
                    "no se pudo identificar el directorio declarado",
                ));
                continue;
            }
        };
        if !registrar_directorio_procesado(&mut bases_procesadas, identidad_base) {
            errores.push(error(
                None,
                None,
                Regla::Estructura,
                "layout.directorios contiene rutas que resuelven al mismo directorio",
            ));
            continue;
        }
        let mut encontrados: Vec<std::path::PathBuf> = Vec::new();
        let mut camino: Vec<PathBuf> = Vec::new();
        let mut procesados: HashSet<PathBuf> = HashSet::new();
        let recorrido = Recorrido {
            raiz: &raiz_canonica,
            base_declarada: directorio,
            base_canonica: &base_canonica,
            raiz_canonica: &raiz_canonica,
        };
        observar_recorrido();
        recorrer(
            &recorrido,
            &base,
            &mut encontrados,
            &mut errores,
            &mut camino,
            &mut procesados,
        );
        encontrados.sort();
        for ruta in encontrados {
            let relativa = relativa_a_raiz(&raiz_canonica, &ruta);
            if contiene_nombre_ordenado(&ignorados, nombre_archivo(&relativa), || {}) {
                continue;
            }
            let ruta_canonica = match std::fs::canonicalize(&ruta) {
                Ok(ruta_canonica) => ruta_canonica,
                Err(err) => {
                    errores.push(error(
                        Some(&relativa),
                        None,
                        Regla::Io,
                        format!("no se puede resolver el archivo dentro del bundle: {err}"),
                    ));
                    continue;
                }
            };
            if !ruta_canonica.starts_with(&raiz_canonica)
                || !ruta_canonica.starts_with(&base_canonica)
            {
                errores.push(error(
                    Some(&relativa),
                    None,
                    Regla::Io,
                    format!(
                        "enlace fuera del directorio declarado: {relativa} resuelve fuera de {directorio}"
                    ),
                ));
                continue;
            }
            antes_de_abrir_archivo(&ruta_canonica);
            match leer_archivo_regular_bundle(&raiz_anclada, &raiz_canonica, &ruta_canonica) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(texto) => archivos.push((relativa, texto)),
                    Err(_) => {
                        let path_reportado = BundlePath::desde_archivo(&relativa)
                            .map(|p| p.como_str().to_string())
                            .unwrap_or_else(|_| relativa.clone());
                        errores.push(error(
                            Some(&path_reportado),
                            None,
                            Regla::NoUtf8,
                            "el archivo no es UTF-8 válido",
                        ));
                    }
                },
                Err(err) => errores.push(error(Some(&relativa), None, Regla::Io, err.to_string())),
            }
        }
    }

    // Los archivos ilegibles no impiden parsear el resto: siempre se
    // reportan todos los errores del bundle a la vez, sin ocultar unos
    // detrás de otros.
    drop(ignorados);
    match parsear_bundle(&archivos, manifiesto) {
        Ok(estructura) if errores.is_empty() => Ok(estructura),
        Ok(_) => {
            ordenar_errores(&mut errores);
            Err(errores)
        }
        Err(mas) => {
            errores.extend(mas);
            ordenar_errores(&mut errores);
            Err(errores)
        }
    }
}

/// Lee solo archivos regulares con cada ancestro abierto desde la raíz.
/// Unix añade O_NONBLOCK para que una sustitución concurrente por FIFO no
/// bloquee; Windows abre cada componente relativo sin seguir reparse points.
fn leer_archivo_regular_bundle(
    raiz: &DirectorioLecturaAnclado,
    raiz_canonica: &Path,
    ruta_canonica: &Path,
) -> io::Result<Vec<u8>> {
    let relativa = ruta_canonica.strip_prefix(raiz_canonica).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "ruta fuera de la raíz anclada")
    })?;
    let mut archivo = raiz
        .abrir_archivo_regular_relativo(relativa)
        .map_err(error_lectura_anclada)?;
    let mut bytes = Vec::new();
    archivo.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn error_lectura_anclada(error: ErrorDirectorioAnclado) -> io::Error {
    let kind = match error {
        ErrorDirectorioAnclado::EntradaAusente => io::ErrorKind::NotFound,
        ErrorDirectorioAnclado::EnlaceNoPermitido | ErrorDirectorioAnclado::TipoNoAdmitido => {
            io::ErrorKind::InvalidData
        }
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, "no se pudo abrir el archivo bajo la raíz anclada")
}

#[derive(PartialEq, Eq, Hash)]
enum IdentidadBaseLayout {
    Fisica(IdentidadDirectorio),
    Canonica(PathBuf),
}

fn registrar_directorio_procesado<I: std::hash::Hash + Eq>(
    procesados_globales: &mut HashSet<I>,
    identidad: I,
) -> bool {
    procesados_globales.insert(identidad)
}

/// Path relativo a `raiz`, con separador `/`, tal como se reporta en los
/// `ErrorBundle` de esta función.
fn relativa_a_raiz(raiz: &Path, dir: &Path) -> String {
    dir.strip_prefix(raiz)
        .unwrap_or(dir)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Acepta solo segmentos normales portables y separados con `/`. Se evita
/// interpretar rutas absolutas, prefijos de unidad, `.` y `..` de manera
/// distinta entre plataformas.
pub(crate) fn directorio_layout_seguro(directorio: &str) -> bool {
    if directorio.is_empty() || directorio.contains('\\') || directorio.contains(':') {
        return false;
    }
    if directorio
        .split('/')
        .any(|segmento| segmento.is_empty() || matches!(segmento, "." | ".."))
    {
        return false;
    }

    let path = Path::new(directorio);
    !path.is_absolute()
        && path
            .components()
            .all(|componente| matches!(componente, Component::Normal(_)))
}

/// Contexto inmutable del recorrido de un directorio declarado del layout.
struct Recorrido<'a> {
    /// Raíz del bundle (para reportar paths relativos).
    raiz: &'a Path,
    /// Nombre del directorio tal como lo declara el manifiesto.
    base_declarada: &'a str,
    /// Forma canónica de ese directorio: todo lo recorrido debe quedar
    /// dentro.
    base_canonica: &'a Path,
    /// Raíz canónica del bundle: ningún enlace puede cruzar esta frontera.
    raiz_canonica: &'a Path,
}

/// Recorre `dir` recursivamente. Sigue symlinks (vía `Path::is_dir`), así
/// que distingue por el path canonicalizado dos situaciones que un `HashSet`
/// único de "visitados" confundiría:
///
/// - Ciclo real: el directorio a punto de entrar ya está en `camino`, la
///   pila de directorios ancestros de la recursión actual (se empuja antes
///   de recursar y se saca al volver). Un symlink que apunta a sí mismo o a
///   un ancestro repite un canonicalizado que ya está en esa pila; se
///   reporta `Regla::Io` y se corta ahí en vez de recursar sin límite (lo
///   que desbordaría la pila).
/// - Diamante: dos rutas distintas, ninguna ancestro de la otra, llevan al
///   mismo directorio físico (p. ej. dos symlinks al mismo destino). No es
///   un ciclo ni un error. Para no cargar los mismos archivos dos veces
///   bajo paths distintos —lo que produciría nodos duplicados o colisiones
///   de case—, `procesados` guarda el canonicalizado de cada directorio ya
///   procesado por completo; la segunda vía que llega a él se salta en
///   silencio, sin recursar ni reportar nada.
/// - Fuga: un enlace que sale del directorio declarado (`base_canonica`),
///   p. ej. hacia la raíz del bundle o hacia un ancestro. El recorrido solo
///   puede ingerir archivos bajo los directorios del layout; un enlace así
///   se reporta `Regla::Io` y no se recorre. Sin esta comprobación, un
///   enlace a la raíz haría entrar archivos de fuera del layout antes de
///   que la pila de ancestros detectara el ciclo.
fn recorrer(
    ctx: &Recorrido<'_>,
    dir: &Path,
    encontrados: &mut Vec<std::path::PathBuf>,
    errores: &mut Vec<ErrorBundle>,
    camino: &mut Vec<PathBuf>,
    procesados: &mut HashSet<PathBuf>,
) {
    let canonico = match std::fs::canonicalize(dir) {
        Ok(c) => c,
        Err(err) => {
            errores.push(error(
                Some(&relativa_a_raiz(ctx.raiz, dir)),
                None,
                Regla::Io,
                err.to_string(),
            ));
            return;
        }
    };

    if !canonico.starts_with(ctx.raiz_canonica) || !canonico.starts_with(ctx.base_canonica) {
        errores.push(error(
            Some(&relativa_a_raiz(ctx.raiz, dir)),
            None,
            Regla::Io,
            // `base_declarada` es el nombre del layout tal como lo declara
            // el manifiesto: la forma canónica llevaría prefijos de
            // plataforma (`\\?\` en Windows) que no significan nada para
            // quien lee el error.
            format!(
                "enlace fuera del directorio declarado: {} resuelve fuera de {}",
                relativa_a_raiz(ctx.raiz, dir),
                ctx.base_declarada
            ),
        ));
        return;
    }

    if camino.contains(&canonico) {
        errores.push(error(
            Some(&relativa_a_raiz(ctx.raiz, dir)),
            None,
            Regla::Io,
            format!(
                "ciclo de directorios (enlace simbolico) en {}",
                relativa_a_raiz(ctx.raiz, dir)
            ),
        ));
        return;
    }
    if !procesados.insert(canonico.clone()) {
        // Diamante: mismo directorio físico ya procesado por otra ruta.
        return;
    }

    let entradas = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            errores.push(error(
                Some(&relativa_a_raiz(ctx.raiz, dir)),
                None,
                Regla::Io,
                err.to_string(),
            ));
            return;
        }
    };
    let mut hijos: Vec<std::path::PathBuf> = Vec::new();
    for entrada in entradas {
        match entrada {
            Ok(e) => hijos.push(e.path()),
            Err(err) => {
                errores.push(error(None, None, Regla::Io, err.to_string()));
                continue;
            }
        }
    }
    hijos.sort();

    camino.push(canonico);
    for ruta in hijos {
        if ruta.is_dir() {
            recorrer(ctx, &ruta, encontrados, errores, camino, procesados);
        } else if ruta.extension().and_then(|e| e.to_str()) == Some("md") {
            encontrados.push(ruta);
        }
    }
    camino.pop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use universo_core::{BundlePath, DeclArista, DeclPredicado, Sorte};
    use universo_testkit::manifiesto_minimo;

    fn doc(tipo: &str, titulo: &str, extra: &str) -> String {
        format!("---\ntype: {tipo}\ntitle: {titulo}\n{extra}---\ncuerpo\n")
    }
    fn atomos_txt(u: &Estructura) -> Vec<String> {
        u.atomos().iter().map(ToString::to_string).collect()
    }

    #[test]
    fn carga_nodos_aristas_padres_y_atributos() {
        let archivos = vec![
            ("n/index.md".to_string(), doc("a", "Raíz", "")),
            (
                "n/x.md".to_string(),
                doc(
                    "a",
                    "X",
                    "nivel: alto\nrel: [\"[[y]]\", \"[[y]]\"]\nten:\n  - with: \"[[y]]\"\n    scope: s\n",
                ),
            ),
            (
                "n/y.md".to_string(),
                doc(
                    "b",
                    "Y",
                    "rel: [\"[[x]]\", \"[[x]]\"]\nten:\n  - with: \"[[x]]\"\n    scope: s\nextra: 1\n",
                ),
            ),
            ("n/sub/index.md".to_string(), doc("a", "Sub", "")),
            ("n/sub/z.md".to_string(), doc("a", "Z", "")),
            ("n/libre/w.md".to_string(), doc("a", "W", "")),
            ("n/libre/index.md".to_string(), doc("a", "Libre", "")),
        ];
        let u = parsear_bundle(&archivos, manifiesto_minimo()).unwrap();
        let t = atomos_txt(&u);
        assert!(
            t.contains(&"Edge(n/x→n/y#rel#1, n/x, n/y, rel)".into()),
            "{t:?}"
        );
        assert!(t.contains(&"HasLab(n/x→n/y#ten#0, \"s\")".into()));
        assert!(t.contains(&"Nivel(n/x, alto)".into()));
        assert!(t.contains(&"Parent(n/x, n/index)".into()));
        assert!(t.contains(&"Parent(n/sub/z, n/sub/index)".into()));
        assert!(t.contains(&"Parent(n/sub/index, n/index)".into()));
        assert!(
            !t.iter().any(|a| a.starts_with("Parent(n/index,")),
            "n/index es raíz"
        );
        assert!(
            !t.iter().any(|a| a.starts_with("Parent(n/libre/")),
            "sin_iota: raíces"
        );
        let y = u.por_path(&BundlePath::nuevo("n/y").unwrap()).unwrap();
        assert_eq!(u.anexo(y).unwrap().passthrough.len(), 1);
        assert_eq!(u.anexo(y).unwrap().passthrough[0].crudo, "extra: 1\n");
        assert_eq!(u.anexo(y).unwrap().cuerpo, "cuerpo\n");
        assert_eq!(u.len_nodos(), 7);
        assert_eq!(u.len_aristas(), 6);
    }

    #[test]
    fn parser_resuelve_claves_con_indices_en_un_manifiesto_grande() {
        const DECLARACIONES: usize = 256;
        let mut manifiesto = manifiesto_minimo();
        for indice in 0..DECLARACIONES {
            let nombre_sorte = format!("tipo-{indice:03}");
            let sorte = Sorte::nuevo(&nombre_sorte);
            manifiesto.sortes_nodo.push(sorte.clone());
            manifiesto
                .sortes_nodo_disco
                .insert(sorte, format!("disk-{indice:03}"));
            manifiesto.predicados.push(DeclPredicado {
                nombre: format!("atributo-{indice:03}"),
                clave: format!("campo-{indice:03}"),
                dominio: ["si".into()].into_iter().collect(),
            });
            manifiesto.aristas.push(DeclArista {
                clave: format!("edge-{indice:03}"),
                simetrica: true,
                inversa: None,
                destino: None,
                etiqueta: None,
            });
        }

        let ultimo = DECLARACIONES - 1;
        let alias = format!("disk-{ultimo:03}");
        let predicado = format!("atributo-{ultimo:03}");
        let clave_predicado = format!("campo-{ultimo:03}");
        let clave_arista = format!("edge-{ultimo:03}");
        let extra = format!("{clave_predicado}: si\n{clave_arista}: []\n");
        let archivos: Vec<(String, String)> = (0..DECLARACIONES)
            .map(|indice| {
                (
                    format!("n/nodo-{indice:03}.md"),
                    doc(&alias, &format!("Nodo {indice}"), &extra),
                )
            })
            .collect();

        let estructura = parsear_bundle(&archivos, manifiesto).unwrap();
        assert_eq!(estructura.len_nodos(), DECLARACIONES);
        assert_eq!(estructura.len_aristas(), 0);
        let atomos = atomos_txt(&estructura);
        assert_eq!(
            atomos
                .iter()
                .filter(|atomo| atomo.starts_with("Type("))
                .count(),
            DECLARACIONES
        );
        assert_eq!(
            atomos
                .iter()
                .filter(|atomo| atomo.starts_with(&format!("{predicado}(")))
                .count(),
            DECLARACIONES
        );
        assert!(
            atomos
                .iter()
                .filter(|atomo| atomo.starts_with("Type("))
                .all(|atomo| atomo.contains(&format!(", tipo-{ultimo:03})")))
        );
    }

    #[test]
    fn retirado_y_patron_desde_disco() {
        let archivos = vec![
            ("n/p.md".to_string(), doc("pattern", "P", "")),
            ("n/r.md".to_string(), doc("a", "R", "status: deprecated\n")),
            ("n/s.md".to_string(), doc("a", "S", "status: stable\n")),
        ];
        let u = parsear_bundle(&archivos, manifiesto_minimo()).unwrap();
        let t = atomos_txt(&u);
        assert!(t.contains(&"Type(n/p, Patrón)".into()));
        assert!(t.contains(&"Type(n/r, Retirado)".into()));
        assert!(t.contains(&"Type(n/s, a)".into()));
        let r = u.por_path(&BundlePath::nuevo("n/r").unwrap()).unwrap();
        assert_eq!(u.anexo(r).unwrap().passthrough.len(), 1);
        assert_eq!(u.anexo(r).unwrap().passthrough[0].crudo, "type: a\n");
        let s = u.por_path(&BundlePath::nuevo("n/s").unwrap()).unwrap();
        assert_eq!(u.anexo(s).unwrap().passthrough[0].crudo, "status: stable\n");
    }

    #[test]
    fn recolecta_todos_los_errores_sin_estructura_a_medias() {
        let archivos = vec![
            (
                "n/a.md".to_string(),
                doc("a", "A", "rel: [\"[[nadie]]\"]\n"),
            ),
            ("n/b.md".to_string(), "sin frontmatter".to_string()),
            ("n/c.md".to_string(), doc("zzz", "C", "")),
            ("n/d.md".to_string(), "---\ntype: a\n---\n".to_string()),
            ("n/e.md".to_string(), doc("a", "E", "nivel: medio\n")),
            ("n/f.md".to_string(), doc("a", "F", "rel: [\"[[dup]]\"]\n")),
            ("n/dup.md".to_string(), doc("a", "D1", "")),
            ("n/sub/dup.md".to_string(), doc("a", "D2", "")),
            (
                "n/g.md".to_string(),
                doc("a", "G", "ten:\n  - with: \"[[a]]\"\n"),
            ),
            ("n/G.md".to_string(), doc("a", "G2", "")),
            ("n/h.md".to_string(), "---\ntitle: H\n---\n".to_string()),
            ("n/i.md".to_string(), doc("a", "I", "rel: [\"i\"]\n")),
            ("n/j k.md".to_string(), doc("a", "J", "")),
        ];
        let errores = parsear_bundle(&archivos, manifiesto_minimo()).unwrap_err();
        let reglas: Vec<(String, Regla)> = errores
            .iter()
            .map(|e| (e.path.clone().unwrap_or_default(), e.regla.clone()))
            .collect();
        for esperado in [
            ("n/a".to_string(), Regla::TargetColgante),
            ("n/b".into(), Regla::FrontmatterAusente),
            ("n/c".into(), Regla::TypeDesconocido),
            ("n/d".into(), Regla::TitleAusente),
            ("n/e".into(), Regla::ConstanteFueraDeDominio),
            ("n/f".into(), Regla::StemAmbiguo),
            ("n/g".into(), Regla::EtiquetaAusente),
            ("n/G".into(), Regla::ColisionDeCase),
            ("n/g".into(), Regla::ColisionDeCase),
            ("n/h".into(), Regla::TypeAusente),
            ("n/i".into(), Regla::LinkMalformado),
            ("n/j k".into(), Regla::PathInvalido),
        ] {
            assert!(
                reglas.contains(&esperado),
                "falta {esperado:?} en {reglas:?}"
            );
        }
        for e in &errores {
            let con_linea = matches!(
                e.regla,
                Regla::TargetColgante
                    | Regla::TypeDesconocido
                    | Regla::ConstanteFueraDeDominio
                    | Regla::StemAmbiguo
                    | Regla::EtiquetaAusente
                    | Regla::LinkMalformado
            );
            if con_linea {
                assert!(e.linea.is_some(), "{e} debería llevar línea");
            }
            assert!(!e.to_string().is_empty());
        }
        let mut ordenado = errores.clone();
        ordenar_errores(&mut ordenado);
        assert_eq!(
            errores, ordenado,
            "los errores salen ordenados por (path, linea)"
        );
        let ambiguo = errores
            .iter()
            .find(|error| error.regla == Regla::StemAmbiguo)
            .unwrap();
        assert!(ambiguo.detalle.contains("tiene 2 candidatos"));
    }

    #[test]
    fn cargar_bundle_rechaza_layout_repetido_antes_de_acceder_a_la_raiz() {
        let mut manifiesto = manifiesto_minimo();
        manifiesto.layout.directorios = vec!["ruta-privada".into(); 2];
        manifiesto.layout.sin_iota.clear();

        let errores = cargar_bundle(Path::new("raiz-que-no-existe"), manifiesto).unwrap_err();
        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::Estructura);
        assert_eq!(errores[0].path, None);
        assert_eq!(
            errores[0].detalle,
            "layout.directorios contiene rutas duplicadas"
        );
        assert!(!errores[0].to_string().contains("ruta-privada"));
    }

    #[test]
    fn diez_mil_paths_colisionantes_reciben_un_testigo_por_error() {
        let path = BundlePath::nuevo("n/repetido").unwrap();
        let resueltos: Vec<ArchivoBundle> = (0..10_000)
            .map(|_| ArchivoBundle {
                path: path.clone(),
                contenido: String::new(),
            })
            .collect();

        let errores = errores_colisiones_case(&resueltos);
        assert_eq!(errores.len(), resueltos.len());
        assert!(errores.iter().all(|error| {
            error.regla == Regla::ColisionDeCase && error.detalle == "colisiona con: n/repetido"
        }));
        let bytes_diagnosticos: usize = errores.iter().map(|error| error.detalle.len()).sum();
        assert!(bytes_diagnosticos <= resueltos.len() * 32);
    }

    #[test]
    fn detalle_de_stem_ambiguo_reporta_cantidad_sin_enumerar_paths() {
        let detalle = detalle_stem_ambiguo("doc", "rel", 10_000);
        assert_eq!(detalle, "[[doc]] desde clave rel tiene 10000 candidatos");
        assert!(detalle.len() < 64);
    }

    #[test]
    fn type_retirado_literal_se_rechaza_con_marca_declarada() {
        let archivos = vec![("n/r.md".to_string(), doc("Retirado", "R", ""))];
        let errores = parsear_bundle(&archivos, manifiesto_minimo()).unwrap_err();
        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::TypeDesconocido);
        assert_eq!(errores[0].path.as_deref(), Some("n/r"));
        assert!(
            errores[0].detalle.contains("status: deprecated"),
            "{}",
            errores[0].detalle
        );
    }

    #[test]
    fn type_retirado_literal_se_acepta_sin_marca() {
        let mut m = manifiesto_minimo();
        m.retirado_en_disco = None;
        let archivos = vec![("n/r.md".to_string(), doc("Retirado", "R", ""))];
        let u = parsear_bundle(&archivos, m).unwrap();
        let t = atomos_txt(&u);
        assert!(t.contains(&"Type(n/r, Retirado)".into()));
        let r = u.por_path(&BundlePath::nuevo("n/r").unwrap()).unwrap();
        assert!(u.anexo(r).unwrap().passthrough.is_empty());
    }

    #[test]
    fn reglas_de_forma_llevan_path_y_linea() {
        let archivos = vec![
            ("n/a.md".to_string(), "---\ntype: a\n".to_string()),
            (
                "n/b.md".to_string(),
                doc("a", "B", "ten:\n  - { with: \"[[c]]\", scope: x }\n"),
            ),
            ("n/c.md".to_string(), doc("a", "C", "ten:\n  - scope: x\n")),
            ("n/d.md".to_string(), doc("a", "D", "")),
        ];
        let errores = parsear_bundle(&archivos, manifiesto_minimo()).unwrap_err();
        let a = errores
            .iter()
            .find(|e| e.path.as_deref() == Some("n/a"))
            .expect("falta el error de n/a");
        assert_eq!(a.regla, Regla::FrontmatterSinCierre);
        let b = errores
            .iter()
            .find(|e| e.path.as_deref() == Some("n/b"))
            .expect("falta el error de n/b");
        assert_eq!(b.regla, Regla::FlowNoAdmitido);
        assert_eq!(b.linea, Some(4));
        let c = errores
            .iter()
            .find(|e| e.path.as_deref() == Some("n/c"))
            .expect("falta el error de n/c");
        assert_eq!(c.regla, Regla::DestinoAusente);
        assert_eq!(c.linea, Some(4));
    }

    #[test]
    fn ordenar_errores_ordena_por_path_y_linea() {
        let mut errores = vec![
            error(Some("n/b"), Some(3), Regla::Io, "x"),
            error(Some("n/a"), None, Regla::Io, "x"),
            error(Some("n/b"), Some(1), Regla::Io, "x"),
            error(None, None, Regla::Io, "x"),
            error(Some("n/a"), Some(2), Regla::Io, "x"),
        ];
        ordenar_errores(&mut errores);
        let claves: Vec<(Option<String>, Option<usize>)> =
            errores.iter().map(|e| (e.path.clone(), e.linea)).collect();
        assert_eq!(
            claves,
            vec![
                (None, None),
                (Some("n/a".to_string()), None),
                (Some("n/a".to_string()), Some(2)),
                (Some("n/b".to_string()), Some(1)),
                (Some("n/b".to_string()), Some(3)),
            ]
        );
    }

    #[test]
    fn valor_escalar_en_linea_de_clave_de_lista_de_mappings_es_regla_propia() {
        let archivos = vec![("n/a.md".to_string(), doc("a", "A", "ten: algo\n"))];
        let errores = parsear_bundle(&archivos, manifiesto_minimo()).unwrap_err();
        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::ValorEnLineaDeClave);
        assert_eq!(errores[0].path.as_deref(), Some("n/a"));
    }

    #[test]
    fn manifiesto_invalido_es_error_de_bundle() {
        let mut m = manifiesto_minimo();
        m.version.clear();
        let errores = parsear_bundle(&[], m).unwrap_err();
        assert_eq!(errores[0].regla, Regla::Estructura);
        assert_eq!(errores[0].path, None);
    }

    #[test]
    fn parsear_bundle_honra_ignorados() {
        let archivos = vec![
            ("n/x.md".to_string(), doc("a", "X", "")),
            ("n/log.md".to_string(), "no soy nodo".to_string()),
        ];
        let u = parsear_bundle(&archivos, manifiesto_minimo()).unwrap();
        assert_eq!(u.len_nodos(), 1);
    }

    #[test]
    fn buscar_nombre_ignorado_es_logaritmico_y_conserva_el_nombre_exacto() {
        fn techo_log2(valor: usize) -> usize {
            if valor <= 1 {
                0
            } else {
                usize::BITS as usize - (valor - 1).leading_zeros() as usize
            }
        }

        let cantidad = 8192;
        let ignorados: Vec<String> = (0..cantidad)
            .map(|indice| format!("ignorado_{indice:05}.md"))
            .collect();
        let ordenados = nombres_ignorados_ordenados(&ignorados);
        let mut comparaciones = 0;
        assert!(contiene_nombre_ordenado(
            &ordenados,
            "ignorado_08191.md",
            || comparaciones += 1,
        ));
        assert!(
            comparaciones <= techo_log2(cantidad + 1),
            "la búsqueda hizo {comparaciones} comparaciones para {cantidad} nombres"
        );
        assert!(!contiene_nombre_ordenado(
            &ordenados,
            "sub/ignorado_08191.md",
            || {},
        ));
    }

    #[test]
    fn cargar_desde_disco_respeta_layout_e_ignorados() {
        let c = universo_testkit::bundle_sintetico::Constructor::nuevo()
            .archivo("n/x.md", &doc("a", "X", ""))
            .archivo("n/log.md", "no soy nodo")
            .archivo("fuera/y.md", &doc("a", "Y", ""))
            .archivo("n/nota.txt", "tampoco")
            .archivo("n/sub/z.md", &doc("a", "Z", ""));
        let dir = c.escribir();
        let u = cargar_bundle(dir.path(), manifiesto_minimo()).unwrap();
        assert_eq!(u.len_nodos(), 2);
        assert!(u.por_path(&BundlePath::nuevo("n/sub/z").unwrap()).is_some());
        let vacio = tempfile::tempdir().unwrap();
        std::fs::create_dir(vacio.path().join("n")).unwrap();
        let u = cargar_bundle(vacio.path(), manifiesto_minimo()).unwrap();
        assert_eq!(
            u.len_nodos(),
            0,
            "un directorio declarado existente puede estar vacío"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cargar_bundle_rechaza_fifo_md_sin_bloquear() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::sync::mpsc;
        use std::time::Duration;

        let temp = tempfile::tempdir().unwrap();
        let directorio = temp.path().join("n");
        std::fs::create_dir(&directorio).unwrap();
        let fifo = directorio.join("bloqueo.md");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: the path is nul-terminated and mkfifo does not retain it.
        assert_eq!(
            unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) },
            0,
            "mkfifo failed: {}",
            io::Error::last_os_error()
        );

        let raiz = temp.path().to_path_buf();
        let (enviar, recibir) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = enviar.send(cargar_bundle(&raiz, manifiesto_minimo()));
        });
        let errores = recibir
            .recv_timeout(Duration::from_secs(2))
            .expect("la carga de un FIFO sin writer quedó bloqueada")
            .unwrap_err();
        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::Io);
        assert_eq!(errores[0].path.as_deref(), Some("n/bloqueo.md"));
    }

    #[cfg(unix)]
    #[test]
    fn cargar_bundle_no_sigue_ancestro_sustituido_tras_validar_archivo() {
        let temp = tempfile::tempdir().unwrap();
        let ancestro = temp.path().join("ancestro");
        let raiz = ancestro.join("bundle");
        let reubicado = temp.path().join("reubicado");
        let externo = temp.path().join("externo");
        std::fs::create_dir_all(raiz.join("n")).unwrap();
        std::fs::create_dir_all(externo.join("bundle/n")).unwrap();
        std::fs::write(raiz.join("n/x.md"), doc("a", "Interior", "")).unwrap();
        std::fs::write(
            externo.join("bundle/n/x.md"),
            doc("a", "CANARY EXTERNO", ""),
        )
        .unwrap();

        let mut sustituido = false;
        let estructura = cargar_bundle_con_observadores(
            &raiz,
            manifiesto_minimo(),
            identidad_directorio_para_recorrido,
            || {},
            |_| {
                if !sustituido {
                    std::fs::rename(&ancestro, &reubicado).unwrap();
                    std::os::unix::fs::symlink(&externo, &ancestro).unwrap();
                    sustituido = true;
                }
            },
        )
        .unwrap();

        let id = estructura
            .por_path(&BundlePath::nuevo("n/x").unwrap())
            .unwrap();
        assert!(sustituido);
        assert_eq!(
            estructura.nodo(id).unwrap().etiqueta.como_str(),
            "Interior",
            "la ruta sustituida no debe redirigir la lectura al canary externo"
        );
    }

    #[test]
    fn identidad_global_evitar_releer_alias_y_preserva_paths_case_sensitive() {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        struct IdentidadFs(u8);

        // El seam representa dos rutas declaradas distintas que el FS mapea
        // al mismo objeto. El contador modela el recorrido que solo ocurre
        // después de insertar una identidad nueva en el set compartido.
        let raices = [
            ("docs", IdentidadFs(1)),
            ("DOCS", IdentidadFs(1)),
            ("otra", IdentidadFs(2)),
        ];
        let mut procesadas = HashSet::new();
        let mut arboles_leidos = 0;
        for (_declarada, identidad) in raices {
            if registrar_directorio_procesado(&mut procesadas, identidad) {
                arboles_leidos += 1;
            }
        }
        assert_eq!(arboles_leidos, 2);

        let mut paths_case_sensitive = HashSet::new();
        assert!(registrar_directorio_procesado(
            &mut paths_case_sensitive,
            PathBuf::from("docs")
        ));
        assert!(registrar_directorio_procesado(
            &mut paths_case_sensitive,
            PathBuf::from("DOCS")
        ));
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn identidad_no_soportada_deduplica_por_ruta_canonica_antes_del_recorrido() {
        let temp = tempfile::tempdir().unwrap();
        let docs = temp.path().join("docs");
        let alias = temp.path().join("alias");
        std::fs::create_dir(&docs).unwrap();
        std::fs::write(docs.join("doc.md"), doc("a", "Doc", "")).unwrap();
        enlazar_directorio(&alias, &docs);

        let mut manifiesto = manifiesto_minimo();
        manifiesto.layout.directorios = vec!["docs".into(), "alias".into()];
        manifiesto.layout.sin_iota.clear();
        let mut rutas_identificadas = Vec::new();
        let mut recorridos = 0;
        let errores = cargar_bundle_observando(
            temp.path(),
            manifiesto,
            |ruta| {
                rutas_identificadas.push(ruta.to_path_buf());
                Ok(None)
            },
            || recorridos += 1,
        )
        .unwrap_err();

        assert_eq!(rutas_identificadas.len(), 2);
        assert_eq!(rutas_identificadas[0], rutas_identificadas[1]);
        assert_eq!(recorridos, 1);
        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::Estructura);
        assert_eq!(errores[0].path, None);
        assert_eq!(
            errores[0].detalle,
            "layout.directorios contiene rutas que resuelven al mismo directorio"
        );
    }

    #[cfg(windows)]
    #[test]
    fn cargar_bundle_rechaza_alias_case_de_directorio_en_filesystem_insensible() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("docs")).unwrap();
        std::fs::write(temp.path().join("docs").join("doc.md"), doc("a", "Doc", "")).unwrap();
        let alias = temp.path().join("DOCS");
        if !alias.is_dir() {
            eprintln!("SKIP explícito: el filesystem de prueba distingue mayúsculas");
            return;
        }

        let mut manifiesto = manifiesto_minimo();
        manifiesto.layout.directorios = vec!["docs".into(), "DOCS".into()];
        manifiesto.layout.sin_iota.clear();
        let errores = cargar_bundle(temp.path(), manifiesto).unwrap_err();

        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::Estructura);
        assert_eq!(errores[0].path, None);
        assert_eq!(
            errores[0].detalle,
            "layout.directorios contiene rutas que resuelven al mismo directorio"
        );
        assert!(
            !errores[0]
                .detalle
                .contains(&temp.path().to_string_lossy().to_string()),
            "el error no debe revelar rutas físicas"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cargar_bundle_conserva_directorios_distintos_por_case_en_unix() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("docs")).unwrap();
        std::fs::create_dir(temp.path().join("DOCS")).unwrap();
        std::fs::write(temp.path().join("docs").join("uno.md"), doc("a", "Uno", "")).unwrap();
        std::fs::write(temp.path().join("DOCS").join("dos.md"), doc("a", "Dos", "")).unwrap();

        let mut manifiesto = manifiesto_minimo();
        manifiesto.layout.directorios = vec!["docs".into(), "DOCS".into()];
        manifiesto.layout.sin_iota.clear();
        let estructura = cargar_bundle(temp.path(), manifiesto).unwrap();

        assert_eq!(estructura.len_nodos(), 2);
    }

    #[test]
    fn cargar_bundle_rechaza_raiz_inexistente() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path().join("no-existe");

        let errores = cargar_bundle(&raiz, manifiesto_minimo()).unwrap_err();

        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::Io);
        assert_eq!(errores[0].path, None);
        assert!(errores[0].detalle.contains("raíz del bundle"));
    }

    #[test]
    fn cargar_bundle_rechaza_raiz_que_no_es_directorio() {
        let dir = tempfile::tempdir().unwrap();
        let raiz = dir.path().join("archivo");
        std::fs::write(&raiz, "no es un bundle").unwrap();

        let errores = cargar_bundle(&raiz, manifiesto_minimo()).unwrap_err();

        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::Io);
        assert_eq!(errores[0].path, None);
        assert!(errores[0].detalle.contains("no es un directorio"));
    }

    #[test]
    fn cargar_bundle_rechaza_directorio_de_layout_ausente() {
        let dir = tempfile::tempdir().unwrap();

        let errores = cargar_bundle(dir.path(), manifiesto_minimo()).unwrap_err();

        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::Io);
        assert_eq!(errores[0].path.as_deref(), Some("n"));
        assert!(errores[0].detalle.contains("directorio declarado"));
    }

    #[test]
    fn cargar_bundle_rechaza_ruta_de_layout_que_no_es_directorio() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("n"), "no es un directorio").unwrap();

        let errores = cargar_bundle(dir.path(), manifiesto_minimo()).unwrap_err();

        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::Io);
        assert_eq!(errores[0].path.as_deref(), Some("n"));
        assert!(errores[0].detalle.contains("no es un directorio"));
    }

    #[test]
    fn cargar_bundle_rechaza_directorios_de_layout_no_relativos_o_con_parent() {
        let dir = tempfile::tempdir().unwrap();
        let absoluta = dir.path().join("n").to_string_lossy().into_owned();

        for directorio in [absoluta, "../fuera".into(), "n/../fuera".into()] {
            let mut manifiesto = manifiesto_minimo();
            manifiesto.layout.directorios = vec![directorio.clone()];
            manifiesto.layout.sin_iota.clear();

            let errores = cargar_bundle(dir.path(), manifiesto).unwrap_err();

            let error_ruta = errores
                .iter()
                .find(|error| error.regla == Regla::Io)
                .unwrap_or_else(|| panic!("{directorio}: {errores:?}"));
            assert_eq!(error_ruta.path.as_deref(), Some(directorio.as_str()));
            assert!(error_ruta.detalle.contains("ruta relativa segura"));
        }
    }

    #[test]
    fn archivo_no_utf8_es_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("n")).unwrap();
        std::fs::write(
            dir.path().join("n/x.md"),
            b"---\ntype: a\ntitle: \xff\n---\n",
        )
        .unwrap();
        let errores = cargar_bundle(dir.path(), manifiesto_minimo()).unwrap_err();
        assert_eq!(errores[0].regla, Regla::NoUtf8);
        assert_eq!(errores[0].path.as_deref(), Some("n/x"));
    }

    /// Crea `enlace` → `destino` como symlink de directorio o, si Windows
    /// no da el privilegio, como junction (`mklink /J`).
    #[cfg(windows)]
    fn enlazar_directorio(enlace: &Path, destino: &Path) {
        if let Err(err) = std::os::windows::fs::symlink_dir(destino, enlace) {
            eprintln!("aviso: sin privilegio para symlink ({err}); se usa una junction");
            let salida = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(enlace)
                .arg(destino)
                .output()
                .expect("cmd debe poder ejecutarse");
            assert!(
                salida.status.success(),
                "no se pudo crear la junction de prueba: {}",
                String::from_utf8_lossy(&salida.stderr)
            );
        }
    }
    #[cfg(unix)]
    fn enlazar_directorio(enlace: &Path, destino: &Path) {
        std::os::unix::fs::symlink(destino, enlace).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn cargar_bundle_rechaza_ancestro_junction_del_root() {
        let temp = tempfile::tempdir().unwrap();
        let ancestro = temp.path().join("ancestro");
        let raiz = ancestro.join("bundle");
        let reubicado = temp.path().join("reubicado");
        let externo = temp.path().join("externo");
        std::fs::create_dir_all(raiz.join("n")).unwrap();
        std::fs::create_dir_all(externo.join("bundle/n")).unwrap();
        std::fs::write(raiz.join("n/x.md"), doc("a", "Interior", "")).unwrap();
        std::fs::write(
            externo.join("bundle/n/x.md"),
            doc("a", "CANARY EXTERNO", ""),
        )
        .unwrap();
        std::fs::rename(&ancestro, &reubicado).unwrap();
        enlazar_directorio(&ancestro, &externo);

        let errores = cargar_bundle(&raiz, manifiesto_minimo()).unwrap_err();
        assert_eq!(errores.len(), 1, "{errores:?}");
        assert_eq!(errores[0].regla, Regla::Io);
        assert_eq!(errores[0].path, None);
        assert!(errores[0].detalle.contains("raíz del bundle"));
    }

    #[test]
    fn cargar_bundle_rechaza_enlace_que_sale_del_directorio_declarado() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("n")).unwrap();
        std::fs::write(dir.path().join("n").join("x.md"), doc("a", "X", "")).unwrap();
        // Un archivo fuera del layout que jamás debe entrar al bundle.
        std::fs::write(dir.path().join("fuera.md"), doc("a", "Fuera", "")).unwrap();
        enlazar_directorio(&dir.path().join("n").join("salida"), dir.path());

        let errores = cargar_bundle(dir.path(), manifiesto_minimo()).unwrap_err();
        let fuga = errores
            .iter()
            .find(|e| e.regla == Regla::Io && e.detalle.contains("fuera del directorio"))
            .unwrap_or_else(|| panic!("{errores:?}"));
        assert_eq!(fuga.path.as_deref(), Some("n/salida"));
        assert!(
            fuga.detalle.ends_with("resuelve fuera de n") && !fuga.detalle.contains("\\\\?\\"),
            "el mensaje debe nombrar el directorio declarado, no la ruta canónica: {}",
            fuga.detalle
        );
        assert!(
            !errores
                .iter()
                .any(|e| e.path.as_deref() == Some("fuera") || e.detalle.contains("fuera.md")),
            "no debe haber leído archivos de fuera del layout: {errores:?}"
        );
    }

    #[test]
    fn cargar_bundle_detecta_ciclo_de_symlinks_y_no_desborda_la_pila() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("n")).unwrap();
        std::fs::write(dir.path().join("n/x.md"), doc("a", "X", "")).unwrap();
        // `join("n").join("loop")` y no `join("n/loop")`: `mklink` toma
        // `/loop` como modificador si la ruta lleva barra.
        let enlace = dir.path().join("n").join("loop");

        #[cfg(windows)]
        {
            // Un symlink de directorio exige privilegio en Windows; una
            // junction (`mklink /J`) no, y `is_dir()`/`canonicalize` la
            // siguen igual, así que el ciclo se ejercita de verdad.
            if let Err(err) = std::os::windows::fs::symlink_dir(dir.path().join("n"), &enlace) {
                eprintln!("aviso: sin privilegio para symlink ({err}); se usa una junction");
                let salida = std::process::Command::new("cmd")
                    .args(["/c", "mklink", "/J"])
                    .arg(&enlace)
                    .arg(dir.path().join("n"))
                    .output()
                    .expect("cmd debe poder ejecutarse");
                assert!(
                    salida.status.success(),
                    "no se pudo crear la junction de prueba: {}",
                    String::from_utf8_lossy(&salida.stderr)
                );
            }
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("n"), &enlace).unwrap();
        }

        let errores = cargar_bundle(dir.path(), manifiesto_minimo()).unwrap_err();
        assert!(
            errores
                .iter()
                .any(|e| e.regla == Regla::Io && e.detalle.contains("ciclo")),
            "{errores:?}"
        );
    }

    #[test]
    fn cargar_bundle_permite_diamante_de_directorios_sin_error_ni_duplicados() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("n").join("real")).unwrap();
        std::fs::write(
            dir.path().join("n").join("real").join("doc.md"),
            doc("a", "Doc", ""),
        )
        .unwrap();
        // `join("n").join("alias")` y no `join("n/real")` como destino
        // directo: el alias es un directorio hermano de `real`, no su
        // ancestro, así que alcanzar el mismo físico por las dos rutas es
        // un diamante, no un ciclo.
        let alias = dir.path().join("n").join("alias");

        #[cfg(windows)]
        {
            if let Err(err) =
                std::os::windows::fs::symlink_dir(dir.path().join("n").join("real"), &alias)
            {
                eprintln!("aviso: sin privilegio para symlink ({err}); se usa una junction");
                let salida = std::process::Command::new("cmd")
                    .args(["/c", "mklink", "/J"])
                    .arg(&alias)
                    .arg(dir.path().join("n").join("real"))
                    .output()
                    .expect("cmd debe poder ejecutarse");
                assert!(
                    salida.status.success(),
                    "no se pudo crear la junction de prueba: {}",
                    String::from_utf8_lossy(&salida.stderr)
                );
            }
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("n").join("real"), &alias).unwrap();
        }

        let u = cargar_bundle(dir.path(), manifiesto_minimo()).unwrap();
        assert_eq!(
            u.len_nodos(),
            1,
            "el documento del diamante debe cargarse una sola vez, no por cada alias"
        );
    }

    #[test]
    fn archivo_no_utf8_no_oculta_los_errores_de_los_demas_archivos() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("n")).unwrap();
        std::fs::write(
            dir.path().join("n/x.md"),
            b"---\ntype: a\ntitle: \xff\n---\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("n/y.md"),
            doc("a", "Y", "rel: [\"[[nadie]]\"]\n"),
        )
        .unwrap();
        let errores = cargar_bundle(dir.path(), manifiesto_minimo()).unwrap_err();
        let reglas: Vec<(String, Regla)> = errores
            .iter()
            .map(|e| (e.path.clone().unwrap_or_default(), e.regla.clone()))
            .collect();
        assert!(
            reglas.contains(&("n/x".to_string(), Regla::NoUtf8)),
            "{reglas:?}"
        );
        assert!(
            reglas.contains(&("n/y".to_string(), Regla::TargetColgante)),
            "{reglas:?}"
        );
    }
}
