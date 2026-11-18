//! Búsqueda léxica sobre las notas de un bundle (brecha P3).
//!
//! Una nota coincide si **cada** término de la consulta empieza alguna palabra
//! de su título, tipo, atributos, valores de otros campos del frontmatter o
//! cuerpo, sin distinguir mayúsculas ni tildes («cache» encuentra «Caché» y
//! «cachear», no «escaché»). No hay embeddings, sinónimos ni raíces: es
//! búsqueda por palabras, no semántica. El puntaje suma, por término y campo,
//! las apariciones (hasta 5) por un peso: título 8, tipo y atributos 4, otros
//! campos 2, cuerpo 1. Se ordena por puntaje y luego por ruta.
//!
//! **Los resultados contienen datos del bundle** (títulos, rutas y fragmentos
//! del cuerpo): son para uso local.
use universo_core::{BundlePath, Estructura};

/// Campo de la nota donde apareció algún término.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Campo {
    Titulo,
    Tipo,
    Atributo,
    Frontmatter,
    Cuerpo,
}

impl Campo {
    pub fn nombre(self) -> &'static str {
        match self {
            Self::Titulo => "title",
            Self::Tipo => "type",
            Self::Atributo => "attribute",
            Self::Frontmatter => "frontmatter",
            Self::Cuerpo => "body",
        }
    }

    fn peso(self) -> u32 {
        match self {
            Self::Titulo => 8,
            Self::Tipo | Self::Atributo => 4,
            Self::Frontmatter => 2,
            Self::Cuerpo => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coincidencia {
    pub path: BundlePath,
    pub tipo: String,
    pub titulo: String,
    /// Campos con al menos un término, en orden de peso.
    pub campos: Vec<Campo>,
    pub puntaje: u32,
    /// Línea del cuerpo con el primer término, acotada.
    pub fragmento: Option<String>,
}

/// Resultado de una búsqueda: como mucho `limite` coincidencias y si había más.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultadoBusqueda {
    pub coincidencias: Vec<Coincidencia>,
    pub total: usize,
}

/// Minúsculas y sin tildes ni diéresis (latín), para comparar.
pub fn normalizar(texto: &str) -> String {
    texto
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            otro => otro,
        })
        // Marcas diacríticas combinantes (texto en forma NFD).
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .collect()
}

/// Términos normalizados en el orden de la consulta, sin repetidos.
fn terminos(consulta: &str) -> Vec<String> {
    let mut salida: Vec<String> = Vec::new();
    for termino in normalizar(consulta).split_whitespace() {
        if !salida.iter().any(|t| t == termino) {
            salida.push(termino.to_string());
        }
    }
    salida
}

/// Posiciones (en bytes) donde `termino` aparece al comienzo de una palabra
/// de `texto`, ambos ya normalizados.
fn comienzos<'a>(texto: &'a str, termino: &'a str) -> impl Iterator<Item = usize> + 'a {
    texto.match_indices(termino).filter_map(move |(i, _)| {
        let previo = texto[..i].chars().next_back();
        (!previo.is_some_and(char::is_alphanumeric)).then_some(i)
    })
}

const MAX_FRAGMENTO: usize = 160;
/// Caracteres de contexto antes del término cuando la línea no cabe entera.
const CONTEXTO_PREVIO: usize = 40;

/// Primera línea del cuerpo donde empieza una palabra con `termino`,
/// recortada a `MAX_FRAGMENTO` caracteres alrededor de la coincidencia.
fn fragmento(cuerpo: &str, termino: &str) -> Option<String> {
    for linea in cuerpo.lines() {
        let chars: Vec<char> = linea.trim().chars().collect();
        // Texto normalizado y, por cada carácter suyo, el índice del original.
        let mut normal = String::new();
        let mut origen = Vec::new();
        for (i, c) in chars.iter().enumerate() {
            for n in normalizar(&c.to_string()).chars() {
                normal.push(n);
                origen.push(i);
            }
        }
        let Some(byte) = comienzos(&normal, termino).next() else {
            continue;
        };
        let posicion = origen[normal[..byte].chars().count()];
        let inicio = if chars.len() <= MAX_FRAGMENTO {
            0
        } else {
            posicion
                .saturating_sub(CONTEXTO_PREVIO)
                .min(chars.len() - MAX_FRAGMENTO)
        };
        let fin = (inicio + MAX_FRAGMENTO).min(chars.len());
        let mut salida = String::new();
        if inicio > 0 {
            salida.push('…');
        }
        salida.extend(&chars[inicio..fin]);
        if fin < chars.len() {
            salida.push('…');
        }
        return Some(salida);
    }
    None
}

/// Busca en todas las notas de `estructura`. Una consulta sin términos no
/// devuelve nada.
pub fn buscar(estructura: &Estructura, consulta: &str, limite: usize) -> ResultadoBusqueda {
    let terminos = terminos(consulta);
    if terminos.is_empty() {
        return ResultadoBusqueda {
            coincidencias: Vec::new(),
            total: 0,
        };
    }
    let mut coincidencias = Vec::new();
    for (id, nodo) in estructura.nodos() {
        let mut campos_texto: Vec<(Campo, String)> = vec![
            (Campo::Titulo, normalizar(nodo.etiqueta.como_str())),
            (Campo::Tipo, normalizar(nodo.tipo.como_str())),
        ];
        let mut atributos: Vec<String> = estructura.atributos(id).values().cloned().collect();
        for valores in estructura.valores_multivalor(id).values() {
            atributos.extend(valores.iter().cloned());
        }
        campos_texto.push((Campo::Atributo, normalizar(&atributos.join(" "))));
        let anexo = estructura.anexo(id);
        if let Some(anexo) = anexo {
            // Sólo el valor de cada campo extra, no su clave.
            let crudo: Vec<&str> = anexo
                .passthrough
                .iter()
                .map(|e| e.crudo.split_once(':').map_or("", |(_, valor)| valor))
                .collect();
            campos_texto.push((Campo::Frontmatter, normalizar(&crudo.join(" "))));
            campos_texto.push((Campo::Cuerpo, normalizar(&anexo.cuerpo)));
        }
        let mut puntaje = 0;
        let mut campos = Vec::new();
        let todos = terminos.iter().all(|termino| {
            let mut aparece = false;
            for (campo, texto) in &campos_texto {
                let veces = comienzos(texto, termino).count() as u32;
                if veces > 0 {
                    aparece = true;
                    puntaje += campo.peso() * veces.min(5);
                    if !campos.contains(campo) {
                        campos.push(*campo);
                    }
                }
            }
            aparece
        });
        if !todos {
            continue;
        }
        campos.sort();
        coincidencias.push(Coincidencia {
            path: nodo.path.clone(),
            tipo: nodo.tipo.como_str().to_string(),
            titulo: nodo.etiqueta.como_str().to_string(),
            campos,
            puntaje,
            fragmento: anexo.and_then(|a| {
                terminos
                    .iter()
                    .find_map(|termino| fragmento(&a.cuerpo, termino))
            }),
        });
    }
    coincidencias.sort_by(|a, b| b.puntaje.cmp(&a.puntaje).then_with(|| a.path.cmp(&b.path)));
    let total = coincidencias.len();
    coincidencias.truncate(limite);
    ResultadoBusqueda {
        coincidencias,
        total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parsear_bundle, parsear_manifiesto};

    fn bundle() -> Estructura {
        let manifiesto = parsear_manifiesto(
            "version = \"t/1\"\n[nodos]\nsortes = [\"decision\", \"fuente\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n[[predicados]]\nnombre = \"Estado\"\nclave = \"estado\"\ndominio = [\"propuesta\", \"vigente\"]\n",
        )
        .unwrap();
        parsear_bundle(
            &[
                (
                    "n/cache.md".into(),
                    "---\ntype: decision\ntitle: Caché de lectura\nestado: vigente\ndescription: Reduce latencia\n---\nUsamos una caché local para las lecturas frecuentes.\n".into(),
                ),
                (
                    "n/cola.md".into(),
                    "---\ntype: decision\ntitle: Cola de eventos\nestado: propuesta\n---\nLa cola desacopla la escritura; también sirve de caché temporal.\n".into(),
                ),
                (
                    "n/informe.md".into(),
                    "---\ntype: fuente\ntitle: Informe de carga\n---\nMide la latencia bajo carga.\n".into(),
                ),
            ],
            manifiesto,
        )
        .unwrap()
    }

    fn rutas(resultado: &ResultadoBusqueda) -> Vec<String> {
        resultado
            .coincidencias
            .iter()
            .map(|c| c.path.to_string())
            .collect()
    }

    #[test]
    fn todos_los_terminos_sin_mayusculas_ni_tildes_y_por_relevancia() {
        let e = bundle();
        // «cache» está en el título de una y en el cuerpo de otra.
        let r = buscar(&e, "CACHE", 10);
        assert_eq!(rutas(&r), vec!["n/cache", "n/cola"]);
        assert_eq!(
            r.coincidencias[0].campos,
            vec![Campo::Titulo, Campo::Cuerpo]
        );
        assert_eq!(r.coincidencias[1].campos, vec![Campo::Cuerpo]);
        // Todos los términos: «cache» y «latencia» sólo juntos en n/cache.
        assert_eq!(rutas(&buscar(&e, "caché latencia", 10)), vec!["n/cache"]);
        // Tipo, atributo y campo extra del frontmatter.
        assert_eq!(rutas(&buscar(&e, "fuente", 10)), vec!["n/informe"]);
        let vigente = buscar(&e, "vigente", 10);
        assert_eq!(
            (rutas(&vigente), vigente.coincidencias[0].campos.clone()),
            (vec!["n/cache".to_string()], vec![Campo::Atributo])
        );
        assert_eq!(
            buscar(&e, "reduce", 10).coincidencias[0].campos,
            vec![Campo::Frontmatter]
        );
    }

    #[test]
    fn fragmento_limite_y_consultas_vacias() {
        let e = bundle();
        let r = buscar(&e, "latencia", 1);
        assert_eq!((r.total, r.coincidencias.len()), (2, 1));
        let informe = buscar(&e, "carga", 10);
        assert_eq!(
            informe.coincidencias[0].fragmento.as_deref(),
            Some("Mide la latencia bajo carga.")
        );
        assert_eq!(buscar(&e, "   ", 10).total, 0);
        assert_eq!(buscar(&e, "inexistente", 10).total, 0);
        assert_eq!(normalizar("Decisión ÑANDÚ"), "decision nandu");
    }

    fn una_nota(frontmatter: &str, cuerpo: &str) -> Estructura {
        let manifiesto = parsear_manifiesto(
            "version = \"t/1\"\n[nodos]\nsortes = [\"decision\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n",
        )
        .unwrap();
        parsear_bundle(
            &[(
                "n/a.md".into(),
                format!("---\ntype: decision\ntitle: Alfa\n{frontmatter}---\n{cuerpo}\n"),
            )],
            manifiesto,
        )
        .unwrap()
    }

    #[test]
    fn coincide_por_comienzo_de_palabra_no_por_subcadena() {
        let e = una_nota("", "Una parte del cachear.");
        assert_eq!(buscar(&e, "art", 10).total, 0);
        assert_eq!(buscar(&e, "parte", 10).total, 1);
        // Prefijo de palabra: «cache» encuentra «cachear».
        assert_eq!(buscar(&e, "cache", 10).total, 1);
    }

    #[test]
    fn la_clave_de_un_campo_extra_no_cuenta_solo_su_valor() {
        let e = una_nota("description: Reduce latencia\n", "Cuerpo.");
        assert_eq!(buscar(&e, "description", 10).total, 0);
        assert_eq!(buscar(&e, "latencia", 10).total, 1);
    }

    #[test]
    fn tildes_descompuestas_se_ignoran() {
        let e = una_nota("", "Una cafe\u{301}tera y un e\u{301}xito.");
        assert_eq!(buscar(&e, "cafetera", 10).total, 1);
        assert_eq!(buscar(&e, "éxito", 10).total, 1);
    }

    #[test]
    fn el_fragmento_rodea_al_primer_termino_de_la_consulta_que_esta_en_el_cuerpo() {
        let largo = format!("{} objetivo final", "relleno ".repeat(50));
        let e = una_nota("", &largo);
        let f = buscar(&e, "objetivo", 10).coincidencias[0]
            .fragmento
            .clone()
            .unwrap();
        assert!(f.contains("objetivo"), "{f}");
        assert!(f.starts_with('…'), "{f}");
        assert!(f.chars().count() <= 162, "{f}");
        // «alfa» va primero en la consulta pero sólo está en el título; el
        // fragmento usa «objetivo», que sí está en el cuerpo.
        let e = una_nota("", "Busca el objetivo.");
        let r = buscar(&e, "alfa objetivo", 10);
        assert_eq!(
            r.coincidencias[0].fragmento.as_deref(),
            Some("Busca el objetivo.")
        );
    }
}
