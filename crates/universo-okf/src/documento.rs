//! Parser de un documento OKF individual: frontmatter (subconjunto YAML
//! propio) + cuerpo verbatim. Ver spec §4.1. Reproduce la gramática de
//! `tools/okf_model.py` (`frontmatter`, `parse_link_list`, `parse_tension`,
//! `link_slug`), pero no la importa: es un parser propio porque el
//! passthrough debe conservar bytes de entradas que el motor no interpreta,
//! y un parser YAML genérico las re-serializaría.
//!
//! Divergencias deliberadas: una línea vacía dentro de una entrada se
//! conserva como continuación (Python cerraba el bloque) para no perder
//! bytes del passthrough; una línea de columna 0 que no es entrada
//! (comentario `#`, clave que empieza por dígito, línea vacía antes de la
//! primera entrada) es `FrontmatterMalformado` donde Python la ignoraba
//! (spec §4.1).

use crate::errores::{ErrorDocumento, ErrorEntrada};

/// Documento OKF parseado: entradas del frontmatter en orden y cuerpo
/// verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Documento {
    pub entradas: Vec<EntradaCruda>,
    pub cuerpo: String,
}

/// Una entrada de frontmatter tal como está en el archivo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntradaCruda {
    pub clave: String,
    /// Texto tras `clave:` en la primera línea, recortado de blancos, con
    /// sus comillas si las tiene.
    pub valor: String,
    /// La entrada completa (primera línea + continuaciones), `\r`
    /// eliminado, terminada en `\n`.
    pub crudo: String,
    /// Línea (1-based, contando la línea `---` de apertura como línea 1)
    /// donde empieza la entrada.
    pub linea: usize,
}

/// Quita el terminador de línea (`\n` y, si lo hay, el `\r` que lo precede)
/// de una línea producida por `str::split_inclusive('\n')`.
fn quitar_terminador(linea: &str) -> &str {
    let sin_salto = linea.strip_suffix('\n').unwrap_or(linea);
    sin_salto.strip_suffix('\r').unwrap_or(sin_salto)
}

/// `true` si `linea` pertenece por continuación a la entrada en curso:
/// vacía, indentada (empieza con espacio o tab) o de lista (empieza con
/// `-`). Reproduce la regla de `tools/okf_model.py::frontmatter`, que solo
/// sigue leyendo un bloque mientras la línea empieza con `" "`, `"\t"` o
/// `"-"`.
fn es_continuacion(linea: &str) -> bool {
    linea.is_empty() || linea.starts_with(' ') || linea.starts_with('\t') || linea.starts_with('-')
}

/// Si `linea` es el inicio de una entrada (`^[A-Za-z_][A-Za-z0-9_-]*:`),
/// devuelve el índice en bytes del `:`.
fn inicio_entrada(linea: &str) -> Option<usize> {
    let mut it = linea.char_indices();
    let (_, primero) = it.next()?;
    if !(primero.is_ascii_alphabetic() || primero == '_') {
        return None;
    }
    for (i, c) in it {
        if c == ':' {
            return Some(i);
        }
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            continue;
        }
        return None;
    }
    None
}

/// Recorta un único par de comillas (`"..."` o `'...'`) que envuelvan por
/// completo la cadena; si no las hay, devuelve la cadena tal cual.
fn recortar_comillas(s: &str) -> &str {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Desescapa `\"` → `"` y `\\` → `\` dentro de un escalar entre comillas
/// dobles; cualquier otra secuencia con `\` se deja tal cual.
fn desescapar(s: &str) -> String {
    let mut salida = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('"') => {
                    salida.push('"');
                    chars.next();
                }
                Some('\\') => {
                    salida.push('\\');
                    chars.next();
                }
                _ => salida.push('\\'),
            }
        } else {
            salida.push(c);
        }
    }
    salida
}

/// Escalar de un texto ya recortado de blancos externos: `"…"` se
/// desescapa (`\"`, `\\`), `'…'` se recorta y el resto es literal. Reglas
/// del §4.1 aplicadas a cualquier escalar del documento, no solo al valor
/// completo de una entrada: un campo de un mapping (`destino`/`etiqueta`
/// de la lista de mappings de una arista con etiqueta, spec §4.1) es un
/// escalar en miniatura y sigue las mismas reglas — antes de esta
/// corrección `lista_mappings` solo recortaba comillas
/// (`recortar_comillas`) sin desescapar, así que un `scope` con `\"` o
/// `\\` literales no hacía round-trip contra `escalar_salida`, que sí
/// escapa esos caracteres al serializar (hallado por la property de
/// round-trip del bundle sintético, plan 10).
fn escalar_desde_texto(s: &str) -> String {
    let v = s.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        desescapar(&v[1..v.len() - 1])
    } else if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

/// Slug de un `[[wikilink]]`, o `None` si `s` no tiene esa forma. Reproduce
/// `WIKILINK_RE = ^\[\[\s*([^\[\]]+?)\s*\]\]$` de `tools/okf_model.py`.
fn enlace_slug(s: &str) -> Option<String> {
    if s.len() < 4 || !s.starts_with("[[") || !s.ends_with("]]") {
        return None;
    }
    let interior = s[2..s.len() - 2].trim();
    if interior.is_empty() || interior.contains('[') || interior.contains(']') {
        return None;
    }
    Some(interior.to_string())
}

impl EntradaCruda {
    /// Las líneas de `crudo` sin el salto final, con la primera línea
    /// reemplazada por `valor` (equivalente al `block` de
    /// `parse_link_list`/`parse_tension` en `tools/okf_model.py`: el texto
    /// tras `clave:` más las líneas de continuación, sin la clave).
    fn lineas_bloque(&self) -> Vec<&str> {
        let mut lineas: Vec<&str> = self.crudo.split('\n').collect();
        if lineas.last() == Some(&"") {
            lineas.pop();
        }
        lineas
    }

    /// Escalar de esta entrada: `valor` recortado, con `"…"` desescapado
    /// (`\"`, `\\`) o `'…'` simplemente recortado; el resto, literal.
    pub fn valor_escalar(&self) -> String {
        escalar_desde_texto(&self.valor)
    }

    /// Lista de wikilinks (flow `["[[a]]", …]` o bloque `- "[[a]]"`).
    /// Reproduce `parse_link_list` + `link_slug` de `tools/okf_model.py`:
    /// el bloque es `valor` (la clave ya recortada) más las líneas de
    /// continuación crudas, sin recortar.
    pub fn lista_wikilinks(&self) -> Result<Vec<String>, ErrorEntrada> {
        let lineas = self.lineas_bloque();
        let continuaciones: &[&str] = if lineas.len() > 1 { &lineas[1..] } else { &[] };
        let mut bloque = vec![self.valor.as_str()];
        bloque.extend(continuaciones.iter().copied());

        let texto = bloque.join("\n");
        let texto = texto.trim();

        let items: Vec<&str> = if let Some(resto) = texto.strip_prefix('[') {
            let fin = resto.rfind(']');
            let contenido = match fin {
                Some(fin) => &resto[..fin],
                None => resto,
            };
            contenido.split(',').collect()
        } else {
            bloque
                .iter()
                .filter_map(|linea| linea.trim().strip_prefix('-'))
                .collect()
        };

        let mut resultado = Vec::new();
        for item in items {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let item = recortar_comillas(item);
            match enlace_slug(item) {
                Some(slug) => resultado.push(slug),
                None => return Err(ErrorEntrada::LinkMalformado(item.to_string())),
            }
        }
        Ok(resultado)
    }

    /// Lista de mappings `- destino: [[x]]\n  etiqueta: texto`. Reproduce
    /// `parse_tension` de `tools/okf_model.py`, generalizada a nombres de
    /// campo configurables.
    pub fn lista_mappings(
        &self,
        destino: &str,
        etiqueta: &str,
    ) -> Result<Vec<(String, String)>, ErrorEntrada> {
        let valor_en_clave = self.valor.trim();
        if !valor_en_clave.is_empty() {
            // Una lista de mappings debe ser una secuencia de bloque bajo
            // la clave. Si hay texto en la propia línea de la clave, se
            // distingue la forma flow real (`[...]`/`{...}`), no admitida
            // porque el campo de etiqueta puede contener comas, de un
            // escalar simple (`ten: algo`), que no es ninguna forma de
            // lista de mappings.
            if valor_en_clave.starts_with('[') || valor_en_clave.starts_with('{') {
                return Err(ErrorEntrada::FlowNoAdmitido);
            }
            return Err(ErrorEntrada::ValorEnLineaDeClave);
        }
        let lineas = self.lineas_bloque();
        let continuaciones: &[&str] = if lineas.len() > 1 { &lineas[1..] } else { &[] };

        let mut mapeos: Vec<Vec<(String, String)>> = Vec::new();
        for linea in continuaciones {
            let recortada = linea.trim();
            if recortada.is_empty() {
                continue;
            }
            let mut restante = recortada;
            if let Some(tras_guion) = recortada.strip_prefix('-') {
                let tras_guion = tras_guion.trim();
                if tras_guion.starts_with('{') {
                    return Err(ErrorEntrada::FlowNoAdmitido);
                }
                mapeos.push(Vec::new());
                if tras_guion.is_empty() {
                    continue;
                }
                restante = tras_guion;
            }
            if mapeos.is_empty() {
                continue;
            }
            if let Some(idx) = restante.find(':') {
                let clave = restante[..idx].trim().to_string();
                // El valor de un campo (`destino`/`etiqueta`) es un
                // escalar en miniatura: mismas reglas de comillas y
                // desescape que `valor_escalar` (corrección de la
                // property de round-trip, ver doc de
                // `escalar_desde_texto`).
                let valor = escalar_desde_texto(&restante[idx + 1..]);
                mapeos
                    .last_mut()
                    .expect("mapeos no está vacío")
                    .push((clave, valor));
            }
        }

        let mut resultado = Vec::with_capacity(mapeos.len());
        for mapeo in mapeos {
            let buscar = |clave: &str| -> Option<&str> {
                mapeo
                    .iter()
                    .find(|(k, _)| k == clave)
                    .map(|(_, v)| v.as_str())
            };
            let destino_val = buscar(destino).ok_or(ErrorEntrada::DestinoAusente)?;
            let slug = enlace_slug(recortar_comillas(destino_val.trim()))
                .ok_or_else(|| ErrorEntrada::LinkMalformado(destino_val.to_string()))?;
            let etiqueta_val = buscar(etiqueta).ok_or(ErrorEntrada::EtiquetaAusente)?;
            if etiqueta_val.trim().is_empty() {
                return Err(ErrorEntrada::EtiquetaAusente);
            }
            resultado.push((slug, etiqueta_val.to_string()));
        }
        Ok(resultado)
    }
}

/// Parsea un documento OKF completo: frontmatter (subconjunto YAML propio)
/// + cuerpo verbatim. Ver spec §4.1 / `tools/okf_model.py::frontmatter`.
pub fn parsear_documento(texto: &str) -> Result<Documento, ErrorDocumento> {
    let mut iter = texto.split_inclusive('\n');
    let primera = iter.next().ok_or(ErrorDocumento::FrontmatterAusente)?;
    if quitar_terminador(primera) != "---" {
        return Err(ErrorDocumento::FrontmatterAusente);
    }

    let mut offset = primera.len();
    let mut fm_lineas: Vec<(usize, String)> = Vec::new();
    let mut numero = 2usize;
    let mut cierre: Option<usize> = None;
    for linea in iter {
        offset += linea.len();
        if quitar_terminador(linea) == "---" {
            cierre = Some(offset);
            break;
        }
        fm_lineas.push((numero, quitar_terminador(linea).to_string()));
        numero += 1;
    }
    let inicio_cuerpo = cierre.ok_or(ErrorDocumento::FrontmatterSinCierre)?;
    let cuerpo = texto[inicio_cuerpo..].to_string();

    let mut entradas = Vec::new();
    let mut i = 0;
    while i < fm_lineas.len() {
        let (linea_num, texto_linea) = &fm_lineas[i];
        let idx_colon = match inicio_entrada(texto_linea) {
            Some(idx) => idx,
            None => return Err(ErrorDocumento::FrontmatterMalformado { linea: *linea_num }),
        };
        let clave = texto_linea[..idx_colon].to_string();
        let valor = texto_linea[idx_colon + 1..].trim().to_string();
        let mut crudo_lineas = vec![texto_linea.clone()];
        let linea_entrada = *linea_num;
        i += 1;
        while i < fm_lineas.len() {
            let (num_j, linea_j) = &fm_lineas[i];
            if es_continuacion(linea_j) {
                crudo_lineas.push(linea_j.clone());
                i += 1;
            } else if inicio_entrada(linea_j).is_some() {
                break;
            } else {
                return Err(ErrorDocumento::FrontmatterMalformado { linea: *num_j });
            }
        }
        let mut crudo = crudo_lineas.join("\n");
        crudo.push('\n');
        entradas.push(EntradaCruda {
            clave,
            valor,
            crudo,
            linea: linea_entrada,
        });
    }

    Ok(Documento { entradas, cuerpo })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errores::{ErrorDocumento, ErrorEntrada};

    const DOC: &str = "---\ntype: rule\ntitle: Hola: mundo\ngenerated: { by: human:example, at: 2026-07-22T00:00:00Z }\nverified:\n  - { by: x, kind: human }\n\nrelated: [\"[[a]]\", \"[[b]]\"]\ntension:\n  - with:  \"[[c]]\"\n    scope: \"uno, dos\"\n---\n# Cuerpo\n\ntexto\n";

    #[test]
    fn separa_entradas_y_cuerpo_verbatim() {
        let d = parsear_documento(DOC).unwrap();
        let claves: Vec<&str> = d.entradas.iter().map(|e| e.clave.as_str()).collect();
        assert_eq!(
            claves,
            [
                "type",
                "title",
                "generated",
                "verified",
                "related",
                "tension"
            ]
        );
        assert_eq!(d.entradas[1].valor_escalar(), "Hola: mundo");
        assert_eq!(
            d.entradas[3].crudo,
            "verified:\n  - { by: x, kind: human }\n\n"
        );
        assert_eq!(d.entradas[3].linea, 5);
        assert_eq!(d.entradas[0].linea, 2);
        assert_eq!(d.cuerpo, "# Cuerpo\n\ntexto\n");
    }

    #[test]
    fn acepta_crlf_y_conserva_cuerpo_con_crlf() {
        let doc = DOC.replace('\n', "\r\n");
        let d = parsear_documento(&doc).unwrap();
        assert_eq!(d.entradas[0].valor_escalar(), "rule");
        assert_eq!(
            d.entradas[3].crudo,
            "verified:\n  - { by: x, kind: human }\n\n"
        );
        assert_eq!(d.cuerpo, "# Cuerpo\r\n\r\ntexto\r\n");
    }

    #[test]
    fn cuerpo_vacio_y_sin_salto_final() {
        assert_eq!(parsear_documento("---\ntype: a\n---").unwrap().cuerpo, "");
        assert_eq!(parsear_documento("---\ntype: a\n---\n").unwrap().cuerpo, "");
        assert_eq!(
            parsear_documento("---\ntype: a\n---\nx").unwrap().cuerpo,
            "x"
        );
        assert!(parsear_documento("---\n---\n").unwrap().entradas.is_empty());
    }

    #[test]
    fn escalares_con_comillas() {
        let d = parsear_documento("---\ntitle: \"a \\\"b\\\" \\\\ c\"\nx: 'y'\nz:\n---\n").unwrap();
        assert_eq!(d.entradas[0].valor_escalar(), "a \"b\" \\ c");
        assert_eq!(d.entradas[1].valor_escalar(), "y");
        assert_eq!(d.entradas[2].valor_escalar(), "");
    }

    #[test]
    fn listas_de_wikilinks_flow_y_bloque() {
        let d = parsear_documento(
            "---\nrelated: [\"[[a]]\", [[ b ]]]\nderived_from:\n  - \"[[c]]\"\n  - [[d]]\nvacia: []\n---\n",
        )
        .unwrap();
        assert_eq!(d.entradas[0].lista_wikilinks().unwrap(), ["a", "b"]);
        assert_eq!(d.entradas[1].lista_wikilinks().unwrap(), ["c", "d"]);
        assert!(d.entradas[2].lista_wikilinks().unwrap().is_empty());
        let m = parsear_documento("---\nrelated: [\"a\"]\n---\n").unwrap();
        assert_eq!(
            m.entradas[0].lista_wikilinks().unwrap_err(),
            ErrorEntrada::LinkMalformado("a".into())
        );
        let m = parsear_documento("---\nrelated: [\"[[a]]\", \"[[b\"]\n---\n").unwrap();
        assert_eq!(
            m.entradas[0].lista_wikilinks().unwrap_err(),
            ErrorEntrada::LinkMalformado("[[b".into())
        );
    }

    #[test]
    fn listas_de_mappings() {
        let d = parsear_documento(
            "---\ntension:\n  - with:  \"[[c]]\"\n    scope: \"uno, dos\"\n  - with: [[d]]\n    scope: tres\n---\n",
        )
        .unwrap();
        let ms = d.entradas[0].lista_mappings("with", "scope").unwrap();
        assert_eq!(
            ms,
            vec![
                ("c".to_string(), "uno, dos".to_string()),
                ("d".into(), "tres".into())
            ]
        );
        let f =
            parsear_documento("---\ntension:\n  - { with: \"[[c]]\", scope: x }\n---\n").unwrap();
        assert_eq!(
            f.entradas[0].lista_mappings("with", "scope").unwrap_err(),
            ErrorEntrada::FlowNoAdmitido
        );
        let flow_en_clave =
            parsear_documento("---\nten: [{with: \"[[x]]\", scope: y}]\n---\n").unwrap();
        assert_eq!(
            flow_en_clave.entradas[0]
                .lista_mappings("with", "scope")
                .unwrap_err(),
            ErrorEntrada::FlowNoAdmitido
        );
        let escalar_en_clave = parsear_documento("---\nten: algo\n---\n").unwrap();
        assert_eq!(
            escalar_en_clave.entradas[0]
                .lista_mappings("with", "scope")
                .unwrap_err(),
            ErrorEntrada::ValorEnLineaDeClave
        );
        let sin = parsear_documento("---\ntension:\n  - with: [[c]]\n---\n").unwrap();
        assert_eq!(
            sin.entradas[0].lista_mappings("with", "scope").unwrap_err(),
            ErrorEntrada::EtiquetaAusente
        );
        let vacia =
            parsear_documento("---\ntension:\n  - with: [[c]]\n    scope: \"\"\n---\n").unwrap();
        assert_eq!(
            vacia.entradas[0]
                .lista_mappings("with", "scope")
                .unwrap_err(),
            ErrorEntrada::EtiquetaAusente
        );
        let sin_destino = parsear_documento("---\ntension:\n  - scope: x\n---\n").unwrap();
        assert_eq!(
            sin_destino.entradas[0]
                .lista_mappings("with", "scope")
                .unwrap_err(),
            ErrorEntrada::DestinoAusente
        );
        let mal = parsear_documento("---\ntension:\n  - with: c\n    scope: x\n---\n").unwrap();
        assert_eq!(
            mal.entradas[0].lista_mappings("with", "scope").unwrap_err(),
            ErrorEntrada::LinkMalformado("c".into())
        );
        let ninguna = parsear_documento("---\ntension:\n---\n").unwrap();
        assert!(
            ninguna.entradas[0]
                .lista_mappings("with", "scope")
                .unwrap()
                .is_empty()
        );
        // El campo de etiqueta de un mapping es un escalar como cualquier
        // otro (spec §4.1): entre comillas dobles se desescapan `\"` y
        // `\\`, igual que `valor_escalar`.
        let escapado =
            parsear_documento("---\ntension:\n  - with: [[c]]\n    scope: \"a\\\"b\\\\c\"\n---\n")
                .unwrap();
        assert_eq!(
            escapado.entradas[0]
                .lista_mappings("with", "scope")
                .unwrap(),
            vec![("c".to_string(), "a\"b\\c".to_string())]
        );
    }

    #[test]
    fn errores_de_forma() {
        assert_eq!(
            parsear_documento("# sin frontmatter").unwrap_err(),
            ErrorDocumento::FrontmatterAusente
        );
        assert_eq!(
            parsear_documento("").unwrap_err(),
            ErrorDocumento::FrontmatterAusente
        );
        assert_eq!(
            parsear_documento("---\n  indentado: x\n---\n").unwrap_err(),
            ErrorDocumento::FrontmatterMalformado { linea: 2 }
        );
        assert_eq!(
            parsear_documento("---\ntype: a\nno es entrada\n---\n").unwrap_err(),
            ErrorDocumento::FrontmatterMalformado { linea: 3 }
        );
        assert_eq!(
            parsear_documento("---\ntype: a\n").unwrap_err(),
            ErrorDocumento::FrontmatterSinCierre
        );
    }
}
