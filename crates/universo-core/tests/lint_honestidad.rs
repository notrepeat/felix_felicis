//! Lint de honestidad (plan 10, cap. 06 de conducta: cero promesas de
//! rendimiento sin análisis). Ningún archivo `.rs`/`.toml` del workspace
//! `motor/` puede contener alguna de las cadenas prohibidas listadas en
//! `motor/tests/honesty-lint/forbidden-strings.txt`: ese archivo lleva el
//! porqué de cada entrada y cambiarlo es, en sí mismo, un cambio revisable
//! (no algo que se ajusta para que este test pase).
//!
//! Solo usa `std`: es un test de `universo-core`, que no depende de nada
//! del workspace en su código de producción, así que su suite de tests
//! tampoco debería necesitar una dependencia nueva para un recorrido de
//! archivos tan simple.

use std::path::{Path, PathBuf};

/// Raíz del workspace `motor/` (dos niveles arriba de
/// `crates/universo-core`).
fn raiz_motor() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Ruta de la lista de cadenas prohibidas, relativa a la raíz del
/// workspace.
fn ruta_lista_prohibidas() -> PathBuf {
    raiz_motor().join("tests/honesty-lint/forbidden-strings.txt")
}

/// Ruta de este mismo archivo de test (para excluirlo del recorrido: cita
/// las cadenas prohibidas en su propia documentación y en sus asserts).
fn ruta_propia() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/lint_honestidad.rs")
}

/// Entradas no vacías y sin comentarios de la lista de cadenas prohibidas.
/// Falla ruidosamente si el archivo no existe: la lista es obligatoria,
/// nunca opcional.
fn cargar_entradas(ruta: &Path) -> Vec<String> {
    let texto = std::fs::read_to_string(ruta).unwrap_or_else(|err| {
        panic!(
            "falta la lista de cadenas prohibidas del lint de honestidad en {}: {err}",
            ruta.display()
        )
    });
    texto
        .lines()
        .map(str::trim)
        .filter(|linea| !linea.is_empty() && !linea.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Recorre `dir` recursivamente (salta `target/` y directorios ocultos),
/// acumulando en `encontrados` los archivos `.rs`/`.toml` cuyo path
/// canonicalizado no esté en `excluidos`.
///
/// `camino` lleva el canonicalizado de cada directorio ancestro de la
/// recursión actual (se empuja antes de recursar, se saca al volver): un
/// symlink que apunta a sí mismo o a un ancestro repetiría un
/// canonicalizado ya presente ahí, así que en vez de recursar sin límite
/// (desbordando la pila) esta función simplemente corta el recorrido en ese
/// directorio. Es un helper de test, no de `universo-okf::cargar_bundle`:
/// no hace falta reportar el ciclo como error, solo no desbordar la pila.
fn recorrer(
    dir: &Path,
    excluidos: &[PathBuf],
    encontrados: &mut Vec<PathBuf>,
    camino: &mut Vec<PathBuf>,
) {
    let entradas = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entrada in entradas.flatten() {
        let ruta = entrada.path();
        let nombre = entrada.file_name();
        let nombre = nombre.to_string_lossy();
        if nombre == "target" || nombre.starts_with('.') {
            continue;
        }
        if ruta.is_dir() {
            let canonico = ruta.canonicalize().unwrap_or_else(|_| ruta.clone());
            if camino.contains(&canonico) {
                continue;
            }
            camino.push(canonico);
            recorrer(&ruta, excluidos, encontrados, camino);
            camino.pop();
            continue;
        }
        let es_rs_o_toml = ruta
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "rs" || e == "toml");
        if !es_rs_o_toml {
            continue;
        }
        let canonico = ruta.canonicalize().unwrap_or_else(|_| ruta.clone());
        if excluidos.contains(&canonico) {
            continue;
        }
        encontrados.push(ruta);
    }
}

/// `entrada` aparece en `linea` (ambas ya en minúsculas) con la regla que le
/// corresponde según su forma:
///
/// - Entradas con `(` (las formas de notación O grande: `O(1)`, `O(n)`,
///   `O(log`, `O(n log`) solo exigen frontera a la IZQUIERDA: al inicio de
///   la línea o tras un carácter que no es alfanumérico ni `_`. Eso es lo
///   que evita que `O(n)` coincida dentro de `Reservado(n)` (un nombre de
///   variante seguido de un binding de una letra); la derecha ya cierra con
///   `)`, que no es alfanumérico, así que no hace falta exigirle nada más.
/// - El resto (palabras sueltas como «rápido» o «eficiente») son subcadena
///   simple: aparecen en cualquier posición, sin exigir frontera a ningún
///   lado. Así un compuesto como «ultrarrápido» o «supereficiente» sí se
///   marca (antes solo se comprobaba la izquierda, y ninguno de los dos
///   tiene frontera ahí tampoco, pero la intención de la regla es que
///   cualquier aparición del término cuenta, compuesto o no).
fn aparece_como_palabra(linea: &str, entrada: &str) -> bool {
    if entrada.contains('(') {
        linea.match_indices(entrada).any(|(inicio, _)| {
            linea[..inicio]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_')
        })
    } else {
        linea.contains(entrada)
    }
}

#[test]
fn la_frontera_de_palabra_evita_falsos_positivos() {
    assert!(aparece_como_palabra(
        "el check es o(n) en la práctica",
        "o(n)"
    ));
    assert!(aparece_como_palabra("o(n) al inicio", "o(n)"));
    assert!(!aparece_como_palabra(
        "errormanifiesto::predicadoreservado(n) => {",
        "o(n)"
    ));
    assert!(aparece_como_palabra("esto es rápido.", "rápido"));
    // Las entradas sin `(` son subcadena simple: un compuesto como
    // «ultrarrápido» sí se marca, aunque «rápido» no empiece en frontera.
    assert!(aparece_como_palabra("un cambio ultrarrápido", "rápido"));
}

#[test]
fn ningun_archivo_rs_o_toml_promete_rendimiento_sin_analisis() {
    let ruta_lista = ruta_lista_prohibidas();
    let entradas = cargar_entradas(&ruta_lista);
    assert!(
        !entradas.is_empty(),
        "la lista de cadenas prohibidas está vacía: {}",
        ruta_lista.display()
    );

    let excluidos: Vec<PathBuf> = [ruta_propia(), ruta_lista.clone()]
        .into_iter()
        .map(|p| p.canonicalize().unwrap_or(p))
        .collect();

    let mut archivos = Vec::new();
    let mut camino: Vec<PathBuf> = Vec::new();
    recorrer(&raiz_motor(), &excluidos, &mut archivos, &mut camino);
    assert!(
        !archivos.is_empty(),
        "el recorrido no encontró ningún archivo .rs/.toml bajo {}",
        raiz_motor().display()
    );

    let mut hallazgos: Vec<String> = Vec::new();
    for archivo in &archivos {
        let Ok(contenido) = std::fs::read_to_string(archivo) else {
            continue;
        };
        for (numero, linea) in contenido.lines().enumerate() {
            let linea_min = linea.to_lowercase();
            for entrada in &entradas {
                if aparece_como_palabra(&linea_min, &entrada.to_lowercase()) {
                    hallazgos.push(format!("{}:{}: {entrada}", archivo.display(), numero + 1));
                }
            }
        }
    }

    assert!(
        hallazgos.is_empty(),
        "el lint de honestidad encontró promesas de rendimiento sin análisis (cap. 06):\n{}",
        hallazgos.join("\n")
    );
}
