//! AST de fórmulas de primer orden bisortidas sobre $\mathit{Str}(U)$
//! (spec tema 02 §3). Dos sortes de variable —nodo ($N$) y arista ($E$)—
//! con espacios de nombres separados, los cuatro predicados base del tema
//! 01 más los de extensión del manifiesto, guardas representadas de forma
//! explícita, y una extensión mínima de clausura transitiva sobre
//! `Parent`.

use crate::atomos::IdArista;
use crate::path::BundlePath;
use crate::tipos::{Etiqueta, Sorte};

/// Sorte de una variable de la firma bisortida: `N` (nodo) o `E` (arista).
/// No confundir con [`Sorte`], que es el alfabeto de tipos $K$ del tema 01;
/// aquí se nombra `SorteVar` justamente para mantener los dos conceptos
/// distinguibles en el mismo módulo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SorteVar {
    Nodo,
    Arista,
}

impl SorteVar {
    /// Nombre de la sorte en la sintaxis de autoría.
    pub fn como_str(self) -> &'static str {
        match self {
            SorteVar::Nodo => "N",
            SorteVar::Arista => "E",
        }
    }
}

/// Nombre de una variable, tal como se escribió en la S-expresión.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Var(String);

impl Var {
    /// Construye una variable con el nombre dado.
    pub fn nuevo(nombre: &str) -> Self {
        Self(nombre.to_string())
    }

    /// Nombre de la variable.
    pub fn como_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Var {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Término en posición de sorte $N$: variable ligada o constante de nodo
/// (escrita `@ruta/al/nodo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminoNodo {
    Var(Var),
    Constante(BundlePath),
}

/// Término en posición de sorte $E$: variable ligada o constante de arista
/// (escrita `@origen→destino#sorte#ordinal`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminoArista {
    Var(Var),
    Constante(IdArista),
}

/// Sujeto de un `HasLab`: $\lambda$ es total sobre nodos y sobre aristas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SujetoTermino {
    Nodo(TerminoNodo),
    Arista(TerminoArista),
}

/// Término en posición de tipo ($K$): literal, o metavariable de esquema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminoSorte {
    Literal(Sorte),
    Meta(Var),
}

/// Término en posición de etiqueta ($\lambda$): literal entre comillas, o
/// metavariable de esquema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminoEtiqueta {
    Literal(Etiqueta),
    Meta(Var),
}

/// Término en posición de constante de un predicado de extensión: literal,
/// o metavariable de esquema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminoConstExt {
    Literal(String),
    Meta(Var),
}

/// Fórmula de primer orden bisortida.
///
/// La **guarda** de un cuantificador es un campo propio y no azúcar ya
/// reescrita a `→`/`∧`: el detector de relativización guardada y la
/// restricción de rango por índices la necesitan reconocible en el AST.
/// Su semántica es la de la forma azucarada: `ParaTodo{g, φ}` ≡
/// `∀x̄ (g → φ)` y `Existe{g, φ}` ≡ `∃x̄ (g ∧ φ)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Formula {
    Type {
        sujeto: TerminoNodo,
        sorte: TerminoSorte,
    },
    Edge {
        arista: TerminoArista,
        origen: TerminoNodo,
        destino: TerminoNodo,
        sorte: TerminoSorte,
    },
    Parent {
        hijo: TerminoNodo,
        padre: TerminoNodo,
    },
    HasLab {
        sujeto: SujetoTermino,
        etiqueta: TerminoEtiqueta,
    },
    Ext {
        predicado: String,
        sujeto: TerminoNodo,
        constante: TerminoConstExt,
    },
    /// Clausura transitiva **estricta** de `Parent`: al menos un paso.
    TcParent {
        desde: TerminoNodo,
        hasta: TerminoNodo,
    },
    IgualNodo(TerminoNodo, TerminoNodo),
    IgualArista(TerminoArista, TerminoArista),
    No(Box<Formula>),
    Y(Vec<Formula>),
    O(Vec<Formula>),
    Implica(Box<Formula>, Box<Formula>),
    ParaTodo {
        ligaduras: Vec<(Var, SorteVar)>,
        guarda: Option<Box<Formula>>,
        cuerpo: Box<Formula>,
    },
    Existe {
        ligaduras: Vec<(Var, SorteVar)>,
        guarda: Option<Box<Formula>>,
        cuerpo: Box<Formula>,
    },
}

/// Dominio finito sobre el que un esquema instancia su metavariable
/// (spec §4.4). Todos salen del `Manifiesto` salvo `Etiquetas`, que sale de
/// las etiquetas en uso en la estructura vigente.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DominioEsquema {
    /// `Lab`: etiquetas en uso.
    Etiquetas,
    /// `KV`: sortes de nodo declaradas.
    SortesNodo,
    /// `KE`: claves de arista declaradas.
    SortesArista,
    /// `Dom.P`: dominio cerrado del predicado de extensión `P`.
    DominioPredicado(String),
}

/// Lo que un autor escribe: o una oración, o un esquema de oraciones que el
/// motor instancia sobre dominios finitos al cargarlo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sentencia {
    Oracion(Formula),
    Esquema {
        ligaduras: Vec<(Var, DominioEsquema)>,
        cuerpo: Formula,
    },
}

/// Prefijo de los nombres de variable que genera el motor al expandir
/// azúcar; ningún autor puede escribirlo (ver `sexpr`).
pub(crate) const PREFIJO_GENERADO: char = '%';

impl std::fmt::Display for TerminoNodo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminoNodo::Var(v) => write!(f, "{v}"),
            TerminoNodo::Constante(p) => write!(f, "@{p}"),
        }
    }
}

impl std::fmt::Display for TerminoArista {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminoArista::Var(v) => write!(f, "{v}"),
            TerminoArista::Constante(id) => write!(f, "@{id}"),
        }
    }
}

impl std::fmt::Display for SujetoTermino {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SujetoTermino::Nodo(t) => write!(f, "{t}"),
            SujetoTermino::Arista(t) => write!(f, "{t}"),
        }
    }
}

impl std::fmt::Display for TerminoSorte {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminoSorte::Literal(s) => write!(f, "{s}"),
            TerminoSorte::Meta(v) => write!(f, "{v}"),
        }
    }
}

impl std::fmt::Display for TerminoConstExt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminoConstExt::Literal(c) => write!(f, "{c}"),
            TerminoConstExt::Meta(v) => write!(f, "{v}"),
        }
    }
}

impl std::fmt::Display for TerminoEtiqueta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminoEtiqueta::Literal(e) => {
                write!(f, "\"{}\"", crate::atomos::escapar_etiqueta(e.como_str()))
            }
            TerminoEtiqueta::Meta(v) => write!(f, "{v}"),
        }
    }
}

impl std::fmt::Display for DominioEsquema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DominioEsquema::Etiquetas => write!(f, "Lab"),
            DominioEsquema::SortesNodo => write!(f, "KV"),
            DominioEsquema::SortesArista => write!(f, "KE"),
            DominioEsquema::DominioPredicado(p) => write!(f, "Dom.{p}"),
        }
    }
}

/// Escribe una lista de ligaduras en la forma `((x N) (e E))`.
fn escribir_ligaduras(
    f: &mut std::fmt::Formatter<'_>,
    ligaduras: &[(Var, SorteVar)],
) -> std::fmt::Result {
    write!(f, "(")?;
    for (indice, (variable, sorte)) in ligaduras.iter().enumerate() {
        if indice > 0 {
            write!(f, " ")?;
        }
        write!(f, "({variable} {})", sorte.como_str())?;
    }
    write!(f, ")")
}

impl Formula {
    /// Si esta fórmula es exactamente la expansión del azúcar `Raiz` —una
    /// negación de un existencial de `Parent` cuya variable la generó el
    /// motor— devuelve el hijo guardado. Solo el motor produce nombres con
    /// el prefijo generado, así que el reconocimiento no puede confundir la
    /// expansión con algo que un autor haya escrito a mano.
    fn como_raiz(&self) -> Option<&TerminoNodo> {
        let Formula::No(interior) = self else {
            return None;
        };
        let Formula::Existe {
            ligaduras,
            guarda: None,
            cuerpo,
        } = interior.as_ref()
        else {
            return None;
        };
        let [(ligada, SorteVar::Nodo)] = ligaduras.as_slice() else {
            return None;
        };
        if !ligada.como_str().starts_with(PREFIJO_GENERADO) {
            return None;
        }
        let Formula::Parent {
            hijo,
            padre: TerminoNodo::Var(padre),
        } = cuerpo.as_ref()
        else {
            return None;
        };
        (padre == ligada).then_some(hijo)
    }
}

impl std::fmt::Display for Formula {
    /// Forma canónica: la misma sintaxis de autoría, con un solo espacio de
    /// separación y sin comentarios. El azúcar `Raiz` se reimprime como
    /// `Raiz` para que el texto canónico se vuelva a parsear (la variable
    /// que genera su expansión lleva el prefijo reservado, que ningún autor
    /// puede escribir).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(hijo) = self.como_raiz() {
            return write!(f, "(Raiz {hijo})");
        }
        match self {
            Formula::Type { sujeto, sorte } => write!(f, "(Type {sujeto} {sorte})"),
            Formula::Edge {
                arista,
                origen,
                destino,
                sorte,
            } => write!(f, "(Edge {arista} {origen} {destino} {sorte})"),
            Formula::Parent { hijo, padre } => write!(f, "(Parent {hijo} {padre})"),
            Formula::HasLab { sujeto, etiqueta } => write!(f, "(HasLab {sujeto} {etiqueta})"),
            Formula::Ext {
                predicado,
                sujeto,
                constante,
            } => write!(f, "({predicado} {sujeto} {constante})"),
            Formula::TcParent { desde, hasta } => write!(f, "(tc Parent {desde} {hasta})"),
            Formula::IgualNodo(a, b) => write!(f, "(= {a} {b})"),
            Formula::IgualArista(a, b) => write!(f, "(= {a} {b})"),
            Formula::No(interior) => write!(f, "(not {interior})"),
            Formula::Y(partes) => escribir_n_arias(f, "and", partes),
            Formula::O(partes) => escribir_n_arias(f, "or", partes),
            Formula::Implica(antecedente, consecuente) => {
                write!(f, "(=> {antecedente} {consecuente})")
            }
            Formula::ParaTodo {
                ligaduras,
                guarda,
                cuerpo,
            } => escribir_cuantificador(f, "forall", ligaduras, guarda.as_deref(), cuerpo),
            Formula::Existe {
                ligaduras,
                guarda,
                cuerpo,
            } => escribir_cuantificador(f, "exists", ligaduras, guarda.as_deref(), cuerpo),
        }
    }
}

fn escribir_n_arias(
    f: &mut std::fmt::Formatter<'_>,
    cabeza: &str,
    partes: &[Formula],
) -> std::fmt::Result {
    write!(f, "({cabeza}")?;
    for parte in partes {
        write!(f, " {parte}")?;
    }
    write!(f, ")")
}

fn escribir_cuantificador(
    f: &mut std::fmt::Formatter<'_>,
    cabeza: &str,
    ligaduras: &[(Var, SorteVar)],
    guarda: Option<&Formula>,
    cuerpo: &Formula,
) -> std::fmt::Result {
    write!(f, "({cabeza} ")?;
    escribir_ligaduras(f, ligaduras)?;
    if let Some(guarda) = guarda {
        write!(f, " :guarda {guarda}")?;
    }
    write!(f, " {cuerpo})")
}

impl std::fmt::Display for Sentencia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Sentencia::Oracion(formula) => write!(f, "{formula}"),
            Sentencia::Esquema { ligaduras, cuerpo } => {
                write!(f, "(esquema (")?;
                for (indice, (variable, dominio)) in ligaduras.iter().enumerate() {
                    if indice > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "({variable} {dominio})")?;
                }
                write!(f, ") {cuerpo})")
            }
        }
    }
}
