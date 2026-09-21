//! Carga de un bundle OKF a una `Estructura` (spec §3.3, §3.4, §4.2).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use universo_core::{
    Anexo, BundlePath, Entrada, ErrorEstructura, Estructura, Etiqueta, Manifiesto, NodoId, Sorte,
};

use crate::documento::{EntradaCruda, parsear_documento};
use crate::errores::{ErrorBundle, ErrorDocumento, ErrorEntrada, Regla};

/// Un archivo ya localizado como `BundlePath`, con su contenido crudo.
struct ArchivoBundle {
    path: BundlePath,
    contenido: String,
}

/// Último segmento del path (nombre de archivo con extensión), aceptando
/// `/` o `\` como separador: mismo criterio que usa `cargar_bundle` para
/// comparar contra `layout.ignorados`.
fn nombre_archivo(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Orden determinista por (path, línea); todo `Err` del cargador sale
/// ordenado así.
fn ordenar_errores(errores: &mut [ErrorBundle]) {
    errores.sort_by(|a, b| (a.path.clone(), a.linea).cmp(&(b.path.clone(), b.linea)));
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

/// Carga pura desde memoria: `archivos` = (path relativo del archivo CON
/// extensión `.md` y separador `/`, contenido). Acumula todos los errores
/// encontrados; si hay al menos uno, nunca devuelve una `Estructura` a
/// medias.
pub fn parsear_bundle(
    archivos: &[(String, String)],
    manifiesto: Manifiesto,
) -> Result<Estructura, Vec<ErrorBundle>> {
    let mut errores: Vec<ErrorBundle> = Vec::new();

    let mut estructura = match Estructura::nueva(manifiesto) {
        Ok(e) => e,
        Err(err) => {
            let detalles = match err {
                universo_core::ErrorEstructura::Manifiesto(errs) => errs,
                _ => unreachable!("Estructura::nueva solo falla por manifiesto inválido"),
            };
            let mut errores: Vec<ErrorBundle> = detalles
                .into_iter()
                .map(|e| error(None, None, Regla::Estructura, e.to_string()))
                .collect();
            ordenar_errores(&mut errores);
            return Err(errores);
        }
    };

    // Paso 0: saltar los archivos cuyo nombre (último segmento del path,
    // con extensión) está en `layout.ignorados` — mismo criterio que
    // aplica `cargar_bundle` al recorrer disco.
    let ignorados = estructura.manifiesto().layout.ignorados.clone();
    let archivos: Vec<&(String, String)> = archivos
        .iter()
        .filter(|(path_original, _)| !ignorados.iter().any(|i| i == nombre_archivo(path_original)))
        .collect();

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

    // Detectar colisiones de case (incluye duplicados exactos) entre todos
    // los pares de archivos resueltos.
    let mut por_case: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, archivo) in resueltos.iter().enumerate() {
        por_case
            .entry(archivo.path.clave_case())
            .or_default()
            .push(i);
    }
    for indices in por_case.values() {
        if indices.len() < 2 {
            continue;
        }
        for &i in indices {
            let otros: Vec<&str> = indices
                .iter()
                .filter(|&&j| j != i)
                .map(|&j| resueltos[j].path.como_str())
                .collect();
            errores.push(error(
                Some(resueltos[i].path.como_str()),
                None,
                Regla::ColisionDeCase,
                format!("colisiona con: {}", otros.join(", ")),
            ));
        }
    }

    struct NodoCargado {
        id: NodoId,
        path: BundlePath,
        /// Entradas de arista pendientes de resolver (clave, entrada).
        aristas_pendientes: Vec<(String, EntradaCruda)>,
    }

    let mut cargados: Vec<NodoCargado> = Vec::new();

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
            match estructura
                .manifiesto()
                .sorte_desde_disco(&entrada_type.valor_escalar())
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
            if let Some(decl) = estructura.manifiesto().predicado_por_clave(&entrada.clave) {
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
            if estructura.manifiesto().es_sorte_arista(&entrada.clave) {
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

        cargados.push(NodoCargado {
            id,
            path: archivo.path.clone(),
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
            let decl = estructura
                .manifiesto()
                .arista(clave)
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
                let candidatos = stems.get(&slug).cloned().unwrap_or_default();
                if candidatos.is_empty() {
                    errores.push(error(
                        Some(&path_str),
                        Some(entrada.linea),
                        Regla::TargetColgante,
                        format!("[[{slug}]] desde clave {clave}"),
                    ));
                    continue;
                }
                if candidatos.len() > 1 {
                    let lista: Vec<&str> = candidatos.iter().map(BundlePath::como_str).collect();
                    errores.push(error(
                        Some(&path_str),
                        Some(entrada.linea),
                        Regla::StemAmbiguo,
                        format!(
                            "[[{slug}]] desde clave {clave} candidatos: {}",
                            lista.join(", ")
                        ),
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

    // Paso 7: ι, nodo padre implícito por directorio.
    let sin_iota = estructura.manifiesto().layout.sin_iota.clone();
    let es_sin_iota = |dir: &str| -> bool {
        sin_iota.iter().any(|s| {
            dir == s
                || dir
                    .strip_prefix(s.as_str())
                    .is_some_and(|r| r.starts_with('/'))
        })
    };
    for nodo in &cargados {
        let Some(dir) = nodo.path.directorio() else {
            continue;
        };
        let es_index = nodo.path.stem() == "index";
        // El directorio relevante para "sin ι" es siempre el propio
        // directorio del nodo (para `d/sub/index` es `d/sub`).
        if es_sin_iota(dir) {
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
            if let Err(err) = estructura.asignar_padre(nodo.id, padre_id) {
                errores.push(error(
                    Some(nodo.path.como_str()),
                    None,
                    Regla::Estructura,
                    err.to_string(),
                ));
            }
        }
    }

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
    let mut errores: Vec<ErrorBundle> = Vec::new();
    let mut archivos: Vec<(String, String)> = Vec::new();

    for directorio in &manifiesto.layout.directorios {
        let base = raiz.join(directorio);
        if !base.is_dir() {
            continue;
        }
        let base_canonica = match std::fs::canonicalize(&base) {
            Ok(c) => c,
            Err(err) => {
                errores.push(error(Some(directorio), None, Regla::Io, err.to_string()));
                continue;
            }
        };
        let mut encontrados: Vec<std::path::PathBuf> = Vec::new();
        let mut camino: Vec<PathBuf> = Vec::new();
        let mut procesados: HashSet<PathBuf> = HashSet::new();
        let recorrido = Recorrido {
            raiz,
            base_declarada: directorio,
            base_canonica: &base_canonica,
        };
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
            let relativa = ruta
                .strip_prefix(raiz)
                .unwrap_or(&ruta)
                .to_string_lossy()
                .replace('\\', "/");
            if manifiesto
                .layout
                .ignorados
                .iter()
                .any(|i| i == nombre_archivo(&relativa))
            {
                continue;
            }
            match std::fs::read(&ruta) {
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

/// Path relativo a `raiz`, con separador `/`, tal como se reporta en los
/// `ErrorBundle` de esta función.
fn relativa_a_raiz(raiz: &Path, dir: &Path) -> String {
    dir.strip_prefix(raiz)
        .unwrap_or(dir)
        .to_string_lossy()
        .replace('\\', "/")
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

    if !canonico.starts_with(ctx.base_canonica) {
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
                Some(&dir.to_string_lossy()),
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
    use universo_core::BundlePath;
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
        let u = cargar_bundle(vacio.path(), manifiesto_minimo()).unwrap();
        assert_eq!(
            u.len_nodos(),
            0,
            "directorios ausentes se saltan en silencio"
        );
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
