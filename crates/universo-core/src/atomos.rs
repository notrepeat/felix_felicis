//! Átomos canónicos y orden total sobre ellos (spec §3.5 y §3.6).
//!
//! Un `Atomo` es la proyección de un hecho elemental de la `Estructura`
//! (un `Type`, un `Edge`, una entrada de $\iota$ como `Parent`, un valor
//! de $\lambda$ como `HasLab` o un atributo de predicado como `Ext`) a su
//! forma canónica de texto. Esa forma canónica es a la vez el `Display`
//! del átomo y la entrada de `Atomo::parsear`, su inversa exacta.

use crate::path::{BundlePath, ErrorPath};
use crate::tipos::{Etiqueta, Sorte};

/// Identidad de una arista fuera de la `Estructura` que la contiene: la
/// tupla `(origen, sorte, destino, ordinal)` en forma de valor, tal como
/// aparece en la forma canónica de `Edge` y de `HasLab` sobre aristas.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IdArista {
    pub origen: BundlePath,
    pub sorte: Sorte,
    pub destino: BundlePath,
    pub ordinal: u32,
}

impl std::fmt::Display for IdArista {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}→{}#{}#{}",
            self.origen, self.destino, self.sorte, self.ordinal
        )
    }
}

impl IdArista {
    /// Inversa exacta de `Display`: `origen→destino#sorte#ordinal`.
    pub fn parsear(texto: &str) -> Result<IdArista, ErrorAtomo> {
        let (origen_str, resto) = texto
            .split_once('→')
            .ok_or_else(|| ErrorAtomo("el id de arista no contiene '→'".to_string()))?;
        let (destino_str, resto2) = resto
            .split_once('#')
            .ok_or_else(|| ErrorAtomo("el id de arista no contiene '#'".to_string()))?;
        let (sorte_str, ordinal_str) = resto2.rsplit_once('#').ok_or_else(|| {
            ErrorAtomo("el id de arista no tiene el segundo '#' del ordinal".to_string())
        })?;
        let origen = BundlePath::nuevo(origen_str)?;
        let destino = BundlePath::nuevo(destino_str)?;
        let sorte = Sorte::nuevo(sorte_str);
        let ordinal: u32 = ordinal_str
            .parse()
            .map_err(|_| ErrorAtomo(format!("ordinal inválido {ordinal_str:?}")))?;
        Ok(IdArista {
            origen,
            sorte,
            destino,
            ordinal,
        })
    }
}

/// Sujeto de un `HasLab`: $\lambda$ es total sobre nodos y sobre aristas.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sujeto {
    Nodo(BundlePath),
    Arista(IdArista),
}

impl std::fmt::Display for Sujeto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Sujeto::Nodo(p) => write!(f, "{p}"),
            Sujeto::Arista(id) => write!(f, "{id}"),
        }
    }
}

/// Átomo canónico: proyección de un hecho elemental de la `Estructura` a
/// su forma de texto (spec §3.6). `Edge` es azúcar de notación: no
/// enumera un `Type` aparte para la arista.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Atomo {
    Type {
        sujeto: BundlePath,
        sorte: Sorte,
    },
    Edge {
        id: IdArista,
        origen: BundlePath,
        destino: BundlePath,
        sorte: Sorte,
    },
    Parent {
        hijo: BundlePath,
        padre: BundlePath,
    },
    HasLab {
        sujeto: Sujeto,
        etiqueta: Etiqueta,
    },
    Ext {
        predicado: String,
        sujeto: BundlePath,
        constante: String,
    },
}

/// Motivo de rechazo de `Atomo::parsear` o `IdArista::parsear`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorAtomo(pub String);

impl std::fmt::Display for ErrorAtomo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ErrorAtomo {}

impl From<ErrorPath> for ErrorAtomo {
    fn from(err: ErrorPath) -> Self {
        ErrorAtomo(err.to_string())
    }
}

/// Escapa una etiqueta para su forma canónica entre comillas: `\` y `"`
/// son los únicos caracteres escapados.
pub(crate) fn escapar_etiqueta(texto: &str) -> String {
    let mut resultado = String::with_capacity(texto.len());
    for c in texto.chars() {
        match c {
            '\\' => resultado.push_str("\\\\"),
            '"' => resultado.push_str("\\\""),
            otro => resultado.push(otro),
        }
    }
    resultado
}

/// Inversa de `escapar_etiqueta` sobre el interior (sin las comillas) de
/// una cadena entre comillas. Rechaza escapes desconocidos y comillas sin
/// escapar dentro del interior.
fn desescapar_etiqueta(interior: &str) -> Result<String, ErrorAtomo> {
    let mut resultado = String::with_capacity(interior.len());
    let mut chars = interior.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('\\') => resultado.push('\\'),
                Some('"') => resultado.push('"'),
                _ => return Err(ErrorAtomo("escape inválido en la etiqueta".to_string())),
            },
            '"' => {
                return Err(ErrorAtomo(
                    "comilla sin escapar dentro de la etiqueta".to_string(),
                ));
            }
            otro => resultado.push(otro),
        }
    }
    Ok(resultado)
}

/// Divide el cuerpo de un átomo (el texto entre `(` y `)`) en exactamente
/// `n` argumentos planos, separados por `, `. Solo válido para campos que
/// no pueden contener `, ` (paths, sortes, constantes: spec §3.6).
fn split_args_planos(cuerpo: &str, n: usize) -> Result<Vec<&str>, ErrorAtomo> {
    let partes: Vec<&str> = cuerpo.split(", ").collect();
    if partes.len() != n {
        return Err(ErrorAtomo(format!(
            "se esperaban {n} argumentos y se encontraron {}",
            partes.len()
        )));
    }
    Ok(partes)
}

fn parsear_haslab(cuerpo: &str) -> Result<Atomo, ErrorAtomo> {
    let idx = cuerpo
        .find(", ")
        .ok_or_else(|| ErrorAtomo("HasLab exige 2 argumentos".to_string()))?;
    let sujeto_str = &cuerpo[..idx];
    let resto = &cuerpo[idx + 2..];
    if resto.len() < 2 || !resto.starts_with('"') || !resto.ends_with('"') {
        return Err(ErrorAtomo(
            "el segundo argumento de HasLab debe ser una cadena entre comillas".to_string(),
        ));
    }
    let interior = &resto[1..resto.len() - 1];
    let texto = desescapar_etiqueta(interior)?;
    let etiqueta = Etiqueta::nuevo(&texto);
    let sujeto = if sujeto_str.contains('→') {
        Sujeto::Arista(IdArista::parsear(sujeto_str)?)
    } else {
        Sujeto::Nodo(BundlePath::nuevo(sujeto_str)?)
    };
    Ok(Atomo::HasLab { sujeto, etiqueta })
}

impl std::fmt::Display for Atomo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Atomo::Type { sujeto, sorte } => write!(f, "Type({sujeto}, {sorte})"),
            Atomo::Edge {
                id,
                origen,
                destino,
                sorte,
            } => write!(f, "Edge({id}, {origen}, {destino}, {sorte})"),
            Atomo::Parent { hijo, padre } => write!(f, "Parent({hijo}, {padre})"),
            Atomo::HasLab { sujeto, etiqueta } => {
                write!(
                    f,
                    "HasLab({sujeto}, \"{}\")",
                    escapar_etiqueta(etiqueta.como_str())
                )
            }
            Atomo::Ext {
                predicado,
                sujeto,
                constante,
            } => write!(f, "{predicado}({sujeto}, {constante})"),
        }
    }
}

impl PartialOrd for Atomo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Atomo {
    /// Orden total = orden lexicográfico por bytes de la forma canónica
    /// (spec §3.5).
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_string().cmp(&other.to_string())
    }
}

impl Atomo {
    /// Inversa exacta de `Display` (round-trip textual: spec §3.6).
    /// Estricto: cualquier forma que no calce exactamente con la
    /// gramática de un `Type`/`Edge`/`Parent`/`HasLab`/`Ext` se rechaza.
    pub fn parsear(texto: &str) -> Result<Atomo, ErrorAtomo> {
        let apertura = texto
            .find('(')
            .ok_or_else(|| ErrorAtomo("el átomo no contiene '('".to_string()))?;
        if apertura == 0 {
            return Err(ErrorAtomo(
                "el átomo no tiene nombre de predicado".to_string(),
            ));
        }
        if !texto.ends_with(')') {
            return Err(ErrorAtomo("el átomo no termina en ')'".to_string()));
        }
        let nombre = &texto[..apertura];
        let cuerpo = &texto[apertura + 1..texto.len() - 1];
        match nombre {
            "Type" => {
                let args = split_args_planos(cuerpo, 2)?;
                let sujeto = BundlePath::nuevo(args[0])?;
                let sorte = Sorte::nuevo(args[1]);
                Ok(Atomo::Type { sujeto, sorte })
            }
            "Edge" => {
                let args = split_args_planos(cuerpo, 4)?;
                let id = IdArista::parsear(args[0])?;
                let origen = BundlePath::nuevo(args[1])?;
                let destino = BundlePath::nuevo(args[2])?;
                let sorte = Sorte::nuevo(args[3]);
                if id.origen != origen || id.destino != destino || id.sorte != sorte {
                    return Err(ErrorAtomo(
                        "el id de la arista no coincide con origen, destino o sorte".to_string(),
                    ));
                }
                Ok(Atomo::Edge {
                    id,
                    origen,
                    destino,
                    sorte,
                })
            }
            "Parent" => {
                let args = split_args_planos(cuerpo, 2)?;
                let hijo = BundlePath::nuevo(args[0])?;
                let padre = BundlePath::nuevo(args[1])?;
                Ok(Atomo::Parent { hijo, padre })
            }
            "HasLab" => parsear_haslab(cuerpo),
            _ => {
                let args = split_args_planos(cuerpo, 2)?;
                let sujeto = BundlePath::nuevo(args[0])?;
                Ok(Atomo::Ext {
                    predicado: nombre.to_string(),
                    sujeto,
                    constante: args[1].to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forma_canonica_y_parseo_inverso() {
        let casos = [
            "Type(rules/x, rule)",
            "Edge(rules/x→values/y#related#0, rules/x, values/y, related)",
            "Parent(rules/x, rules/index)",
            "HasLab(rules/x, \"Título con \\\"comillas\\\" y \\\\\")",
            "HasLab(rules/x, \"a, b\")",
            "HasLab(rules/x→values/y#tension#1, \"\")",
            "Standing(rules/x, firm)",
            "Nivel(n/x, alto)",
        ];
        for c in casos {
            let a = Atomo::parsear(c).unwrap_or_else(|e| panic!("{c}: {e}"));
            assert_eq!(a.to_string(), c);
        }
        assert!(matches!(
            Atomo::parsear("HasLab(rules/x, \"Título con \\\"comillas\\\" y \\\\\")").unwrap(),
            Atomo::HasLab { etiqueta, .. } if etiqueta.como_str() == "Título con \"comillas\" y \\"
        ));
        assert!(matches!(
            Atomo::parsear("HasLab(rules/x→values/y#tension#1, \"\")").unwrap(),
            Atomo::HasLab {
                sujeto: Sujeto::Arista(_),
                ..
            }
        ));
    }
    #[test]
    fn parseo_rechaza_formas_malas() {
        for malo in [
            "Edge(x, y)",
            "Type(rules/x)",
            "Type(rules/x, rule, extra)",
            "HasLab(rules/x, sin comillas)",
            "HasLab(rules/x, \"sin cierre)",
            "Edge(rules/x→values/y#related#0, rules/x, values/z, related)",
            "Edge(rules/x→values/y#related#x, rules/x, values/y, related)",
            "Parent(rules/x, rules/ y)",
            "Type(rules/x, rule",
            "",
            "(a, b)",
        ] {
            assert!(Atomo::parsear(malo).is_err(), "debería rechazar {malo:?}");
        }
    }
    #[test]
    fn orden_total_es_lexicografico_por_bytes() {
        let mut atomos: Vec<Atomo> = [
            "Type(b, a)",
            "Parent(a, b)",
            "Edge(a→b#r#0, a, b, r)",
            "HasLab(a, \"x\")",
        ]
        .iter()
        .map(|s| Atomo::parsear(s).unwrap())
        .collect();
        atomos.sort();
        let textos: Vec<String> = atomos.iter().map(ToString::to_string).collect();
        let mut esperado = textos.clone();
        esperado.sort();
        assert_eq!(textos, esperado);
        assert_eq!(textos[0], "Edge(a→b#r#0, a, b, r)");
    }
}
