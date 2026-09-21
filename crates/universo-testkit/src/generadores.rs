//! Generadores `proptest` compartidos por todas las suites del motor (plan 10).

use proptest::prelude::*;

/// Asignación `hijo → padre` en un mapa ι candidato (índices en `0..n`).
pub type Asignacion = (usize, usize);

/// Genera `(n, asignaciones)`: `n` nodos y una lista de asignaciones
/// hijo→padre con repeticiones, auto-padre y ciclos posibles, y cadenas
/// largas (a veces la lista es una cadena `1→0, 2→1, …` que produce
/// profundidad `n-1`, a veces esa misma cadena con una asignación final que
/// cierra el ciclo).
pub fn mapa_iota(nodos: std::ops::Range<usize>) -> impl Strategy<Value = (usize, Vec<Asignacion>)> {
    nodos.prop_flat_map(|n| {
        prop_oneof![
            proptest::collection::vec((0..n, 0..n), 0..(3 * n + 1)).prop_map(move |v| (n, v)),
            Just((n, (1..n).map(|i| (i, i - 1)).collect())),
            Just((
                n,
                (1..n)
                    .map(|i| (i, i - 1))
                    .chain(std::iter::once((0, n - 1)))
                    .collect()
            )),
        ]
    })
}

/// `true` si el mapa parcial `padre` (índices en `0..n`) es un bosque:
/// ningún nodo es su propio ancestro.
pub fn es_bosque(padre: &[Option<usize>]) -> bool {
    for inicio in 0..padre.len() {
        let mut visitados = vec![false; padre.len()];
        let mut actual = Some(inicio);
        while let Some(n) = actual {
            if visitados[n] {
                return false;
            }
            visitados[n] = true;
            actual = padre[n];
        }
    }
    true
}

/// Simulación de referencia, independiente de `Estructura`: aplica las
/// asignaciones en orden; una asignación con `hijo == padre` o que cerraría
/// un ciclo se descarta (y se cuenta como rechazada); una asignación válida
/// reemplaza al padre anterior. Devuelve `(padre_final, hubo_rechazo)`.
pub fn simular_iota(n: usize, asignaciones: &[Asignacion]) -> (Vec<Option<usize>>, bool) {
    let mut padre: Vec<Option<usize>> = vec![None; n];
    let mut hubo_rechazo = false;
    for &(hijo, candidato_padre) in asignaciones {
        if hijo == candidato_padre {
            hubo_rechazo = true;
            continue;
        }
        // ¿candidato_padre desciende de hijo (o es hijo)? Si sí, asignar
        // padre[hijo] = candidato_padre cerraría un ciclo.
        let mut cierra_ciclo = false;
        let mut actual = Some(candidato_padre);
        while let Some(n) = actual {
            if n == hijo {
                cierra_ciclo = true;
                break;
            }
            actual = padre[n];
        }
        if cierra_ciclo {
            hubo_rechazo = true;
            continue;
        }
        padre[hijo] = Some(candidato_padre);
    }
    (padre, hubo_rechazo)
}

// --- Generador de bundles OKF completos (plan 10) ------------------------
//
// `bundle_okf` produce bundles en memoria, válidos bajo
// `manifiesto_minimo()` (mismo manifiesto que usan todos los tests del
// motor: sortes `a`/`b`/`Retirado`/`Patrón`, aristas `rel` (simétrica),
// `sub`/`sup` (inversas), `ten` (mappings `with`/`scope`), predicado
// `Nivel` con dominio {bajo, alto}, layout de un único directorio `n` con
// `n/libre` sin ι), para la property de round-trip de
// `universo-okf/tests/roundtrip_prop.rs`.
//
// Decisión de diseño no fijada por la spec: cuando se incluye un nodo
// índice opcional (`n/index.md`, y/o `n/sub/index.md`), no participa como
// objetivo de `rel`/`sub`/`sup`/`ten` de los demás nodos (solo los stems
// `x1..x8` activos se ofrecen como objetivos); esto mantiene la resolución
// de objetivos determinista sin afectar las propiedades RT1/RT2, que no
// dependen de qué nodos concretos llevan aristas. Ambos índices comparten
// el mismo stem (`index`), lo que sería ambiguo para un wikilink — pero al
// no ser nunca objetivo de ninguno, `n/index.md` y `n/sub/index.md` pueden
// coexistir sin problema: la ambigüedad de stem solo importaría para
// resolver `[[index]]`, y nada apunta ahí.

/// Pool fijo de stems para los nodos "normales" del bundle generado.
const STEMS: [&str; 8] = ["x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8"];
/// Directorios de layout de `manifiesto_minimo()` bajo los que se puede
/// colocar un nodo normal.
const DIRS: [&str; 3] = ["n", "n/sub", "n/libre"];
/// Valores de `type:` en disco para las sortes no reservadas.
const TIPOS: [&str; 3] = ["a", "b", "pattern"];
/// Dominio del predicado `Nivel` de `manifiesto_minimo()`.
const NIVELES: [&str; 2] = ["bajo", "alto"];

/// Alfabeto de texto libre para `title` y `scope`: letras, dígitos y una
/// muestra de los símbolos que aparecen en bundles reales (incluida al
/// menos una letra acentuada y una flecha, sin `\n`/`\r`).
fn alfabeto_texto() -> Vec<char> {
    let mut cs: Vec<char> = ('a'..='z').chain('A'..='Z').chain('0'..='9').collect();
    cs.extend([
        ' ', ':', '"', '\\', ',', '#', '-', '[', ']', '{', '}', '\'', '!', '%', '@', '`', '|', '>',
        '&', '*', 'é', '→',
    ]);
    cs
}

/// Cadena de `rango.0..=rango.1` caracteres tomados de `alfabeto_texto()`.
fn texto_libre(rango: std::ops::RangeInclusive<usize>) -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(alfabeto_texto()), rango)
        .prop_map(|cs| cs.into_iter().collect())
}

/// Igual que `texto_libre`, pero descarta las cadenas que quedarían vacías
/// tras recortar blancos: el `scope` de un mapping se rechaza con
/// `EtiquetaAusente` si es puro blanco (spec §4.1, «etiqueta ausente o
/// vacía»), así que un `scope` compuesto solo de espacios no es un bundle
/// válido y no debe generarse.
fn texto_libre_no_blanco(rango: std::ops::RangeInclusive<usize>) -> impl Strategy<Value = String> {
    texto_libre(rango).prop_filter("no puede quedar vacío tras recortar blancos", |s| {
        !s.trim().is_empty()
    })
}

/// Escalar entre comillas dobles con `\"`/`\\` escapados: misma forma que
/// `escalar_salida` de `universo-okf` (que `testkit` no puede importar:
/// `okf` depende de `core`, no al revés), así que cualquier `title`/`scope`
/// generado hace round-trip exacto contra el desescape del parser
/// (`EntradaCruda::valor_escalar` / los campos de una lista de mappings).
fn escalar_citado(valor: &str) -> String {
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

/// Cuerpo de un nodo: 0..=60 caracteres imprimibles, con 0..=3 saltos de
/// línea (`\n` o `\r\n`, a elección), salto final opcional, o vacío.
fn cuerpo_estrategia() -> impl Strategy<Value = String> {
    let alfabeto_cuerpo: Vec<char> = ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain([' ', '.', ',', '!', '?'])
        .collect();
    (
        proptest::collection::vec(proptest::sample::select(alfabeto_cuerpo), 0..=60),
        proptest::bool::ANY,
        0usize..=3,
        proptest::bool::ANY,
        proptest::bool::ANY,
    )
        .prop_map(|(chars, crlf, n_saltos_pedidos, salto_final, vacio)| {
            if vacio {
                return String::new();
            }
            let salto = if crlf { "\r\n" } else { "\n" };
            let len = chars.len();
            let n_saltos = if len < 2 {
                0
            } else {
                n_saltos_pedidos.min(len - 1)
            };
            let paso = if n_saltos == 0 {
                len + 1
            } else {
                (len / (n_saltos + 1)).max(1)
            };
            let mut texto = String::new();
            let mut saltos_puestos = 0usize;
            for (i, c) in chars.into_iter().enumerate() {
                texto.push(c);
                if saltos_puestos < n_saltos && (i + 1) % paso == 0 && i + 1 < len {
                    texto.push_str(salto);
                    saltos_puestos += 1;
                }
            }
            if salto_final {
                texto.push_str(salto);
            }
            texto
        })
}

/// Plantilla generada para un nodo (normal o el índice opcional): todo lo
/// que hace falta para escribir su documento OKF, salvo la resolución de
/// los objetivos de arista (que depende de qué otros nodos están activos
/// en el bundle concreto).
#[derive(Clone, Debug)]
struct PlantillaNodo {
    tipo: &'static str,
    titulo: String,
    retirado: bool,
    nivel: Option<&'static str>,
    con_extra: bool,
    con_tags: bool,
    con_generated: bool,
    con_verified: bool,
    /// Índices crudos (sin acotar): se resuelven contra la lista de
    /// objetivos válidos con `idx % objetivos.len()`.
    rel_idx: Vec<usize>,
    sub_idx: Vec<usize>,
    sup_idx: Vec<usize>,
    /// Igual que los anteriores, pero cada mapping lleva además su propio
    /// `scope`.
    ten: Vec<(usize, String)>,
    cuerpo: String,
    /// Si es `true`, todo el bloque de frontmatter (incluida la línea de
    /// cierre `---`) se emite con `\r\n` en vez de `\n`. El cuerpo no se
    /// toca: ya tiene su propia elección de salto de línea vía
    /// `cuerpo_estrategia`. El parser normaliza el `\r` del frontmatter al
    /// leerlo (`EntradaCruda::crudo` lo elimina), así que RT1/RT2 siguen
    /// valiendo con esta variante.
    frontmatter_crlf: bool,
}

fn plantilla_nodo() -> impl Strategy<Value = PlantillaNodo> {
    let parte_a = (
        proptest::sample::select(&TIPOS[..]),
        texto_libre(0..=40),
        proptest::bool::ANY,
        proptest::option::of(proptest::sample::select(&NIVELES[..])),
        proptest::bool::ANY,
        proptest::bool::ANY,
        proptest::bool::ANY,
        proptest::bool::ANY,
    );
    let parte_b = (
        proptest::collection::vec(0usize..1000, 0..=3),
        proptest::collection::vec(0usize..1000, 0..=2),
        proptest::collection::vec(0usize..1000, 0..=2),
        proptest::collection::vec((0usize..1000, texto_libre_no_blanco(1..=30)), 0..=2),
        cuerpo_estrategia(),
        proptest::bool::ANY,
    );
    (parte_a, parte_b).prop_map(
        |(
            (tipo, titulo, retirado, nivel, con_extra, con_tags, con_generated, con_verified),
            (rel_idx, sub_idx, sup_idx, ten, cuerpo, frontmatter_crlf),
        )| PlantillaNodo {
            tipo,
            titulo,
            retirado,
            nivel,
            con_extra,
            con_tags,
            con_generated,
            con_verified,
            rel_idx,
            sub_idx,
            sup_idx,
            ten,
            cuerpo,
            frontmatter_crlf,
        },
    )
}

/// Escribe, si hay objetivos e índices, la entrada `clave: ["[[a]]", …]`
/// (lista de wikilinks en estilo flow) en `fm`.
fn escribir_lista_wikilinks(fm: &mut String, clave: &str, idxs: &[usize], objetivos: &[&str]) {
    if idxs.is_empty() || objetivos.is_empty() {
        return;
    }
    let items: Vec<String> = idxs
        .iter()
        .map(|&idx| format!("\"[[{}]]\"", objetivos[idx % objetivos.len()]))
        .collect();
    fm.push_str(&format!("{clave}: [{}]\n", items.join(", ")));
}

/// Documento OKF completo (frontmatter + cuerpo) de un nodo, resolviendo
/// sus aristas contra `objetivos` (los otros stems activos del bundle).
fn documento_nodo(p: &PlantillaNodo, objetivos: &[&str]) -> String {
    let mut fm = String::new();
    fm.push_str("---\n");
    fm.push_str(&format!("type: {}\n", p.tipo));
    fm.push_str(&format!("title: {}\n", escalar_citado(&p.titulo)));
    if p.retirado {
        fm.push_str("status: deprecated\n");
    }
    if let Some(nivel) = p.nivel {
        fm.push_str(&format!("nivel: {nivel}\n"));
    }
    if p.con_extra {
        fm.push_str("extra: 1\n");
    }
    if p.con_tags {
        fm.push_str("tags: [x, y]\n");
    }
    if p.con_generated {
        fm.push_str("generated: { by: h, at: t }\n");
    }
    if p.con_verified {
        fm.push_str("verified:\n  - { by: h, kind: human }\n");
    }
    escribir_lista_wikilinks(&mut fm, "rel", &p.rel_idx, objetivos);
    escribir_lista_wikilinks(&mut fm, "sub", &p.sub_idx, objetivos);
    escribir_lista_wikilinks(&mut fm, "sup", &p.sup_idx, objetivos);
    if !p.ten.is_empty() && !objetivos.is_empty() {
        fm.push_str("ten:\n");
        for (idx, scope) in &p.ten {
            let objetivo = objetivos[idx % objetivos.len()];
            fm.push_str(&format!(
                "  - with: \"[[{objetivo}]]\"\n    scope: {}\n",
                escalar_citado(scope)
            ));
        }
    }
    fm.push_str("---\n");
    if p.frontmatter_crlf {
        // Solo el bloque de frontmatter (línea de cierre `---` incluida):
        // el cuerpo, ya empujado aparte, conserva su propia elección de
        // salto de línea.
        fm = fm.replace('\n', "\r\n");
    }
    fm.push_str(&p.cuerpo);
    fm
}

/// Bundles OKF completos, válidos bajo `manifiesto_minimo()`, como listas
/// de archivos en memoria `(path relativo con extensión `.md`, contenido)`
/// (plan 10, spec §4.4): 1 a 8 nodos con stems distintos bajo `n/`,
/// `n/sub/` o `n/libre/`, un `n/index.md` y un `n/sub/index.md` opcionales
/// (pueden coexistir: ver la nota de diseño más arriba), aristas `rel`/
/// `sub`/`sup`/`ten` entre los nodos activos, atributos, marca `Retirado`,
/// passthrough variado y un `n/log.md` opcional (que `layout.ignorados`
/// debe ignorar siempre).
pub fn bundle_okf() -> impl Strategy<Value = Vec<(String, String)>> {
    let permutacion = Just((0..STEMS.len()).collect::<Vec<usize>>()).prop_shuffle();
    (
        1..=STEMS.len(),
        permutacion,
        proptest::bool::ANY,
        proptest::bool::ANY,
        proptest::bool::ANY,
        proptest::collection::vec(proptest::sample::select(&DIRS[..]), STEMS.len()),
        proptest::collection::vec(plantilla_nodo(), STEMS.len()),
        plantilla_nodo(),
        plantilla_nodo(),
    )
        .prop_map(
            |(
                n,
                perm,
                incluir_index,
                incluir_index_sub,
                incluir_log,
                dirs,
                plantillas,
                plantilla_index,
                plantilla_index_sub,
            )| {
                let activos: Vec<usize> = perm[..n].to_vec();
                let mut archivos = Vec::new();

                if incluir_index {
                    archivos.push((
                        "n/index.md".to_string(),
                        documento_nodo(&plantilla_index, &[]),
                    ));
                }
                if incluir_index_sub {
                    archivos.push((
                        "n/sub/index.md".to_string(),
                        documento_nodo(&plantilla_index_sub, &[]),
                    ));
                }

                for (pos, &slot) in activos.iter().enumerate() {
                    let stem = STEMS[slot];
                    let dir = dirs[slot];
                    let ruta = format!("{dir}/{stem}.md");
                    let objetivos: Vec<&str> = activos
                        .iter()
                        .enumerate()
                        .filter(|&(p, _)| p != pos)
                        .map(|(_, &s)| STEMS[s])
                        .collect();
                    let contenido = documento_nodo(&plantillas[slot], &objetivos);
                    archivos.push((ruta, contenido));
                }

                if incluir_log {
                    archivos.push((
                        "n/log.md".to_string(),
                        "contenido cualquiera, ignorado por layout.ignorados\n".to_string(),
                    ));
                }

                archivos
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn es_bosque_acepta_cadena_simple() {
        assert!(es_bosque(&[None, Some(0), Some(1)]));
    }

    #[test]
    fn es_bosque_rechaza_ciclo_mutuo() {
        assert!(!es_bosque(&[Some(1), Some(0)]));
    }

    #[test]
    fn es_bosque_rechaza_auto_padre() {
        assert!(!es_bosque(&[Some(0)]));
    }

    #[test]
    fn simular_iota_construye_cadena() {
        assert_eq!(
            simular_iota(3, &[(1, 0), (2, 1), (0, 2)]),
            (vec![None, Some(0), Some(1)], true)
        );
    }

    #[test]
    fn simular_iota_ignora_auto_padre() {
        assert_eq!(
            simular_iota(2, &[(1, 0), (1, 1)]),
            (vec![None, Some(0)], true)
        );
    }

    #[test]
    fn simular_iota_reasigna_padre() {
        assert_eq!(
            simular_iota(3, &[(2, 0), (2, 1)]),
            (vec![None, None, Some(1)], false)
        );
    }
}
