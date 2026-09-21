//! Tipos de valor elementales de la estructura $\mathit{Str}(U)$ (spec
//! §3.2): `Sorte` (elemento de $K$, el alfabeto de tipos de nodo o de
//! arista) y `Etiqueta` ($\lambda$, la función de etiquetado total).

/// Nombre de tipo de nodo o de arista: elemento del alfabeto $K$. Para
/// nodos identifica la sorte declarada en `Manifiesto::sortes_nodo`; para
/// aristas es la clave de una `DeclArista` (D5: la clave ES el tipo).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Sorte(String);

impl Sorte {
    /// Construye una `Sorte` a partir de su nombre, sin validación: la
    /// pertenencia a $K_V$ o $K_E$ la decide el `Manifiesto`.
    pub fn nuevo(nombre: &str) -> Self {
        Self(nombre.to_string())
    }

    /// Nombre de la sorte tal como se declaró.
    pub fn como_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Sorte {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Valor de la función de etiquetado $\lambda$, total sobre nodos y
/// aristas. En nodos es el `title` del frontmatter; en aristas es la
/// cadena vacía salvo que la arista declare un campo de etiqueta
/// (`tension` → `scope`). Puede ser vacía.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Etiqueta(String);

impl Etiqueta {
    /// Construye una `Etiqueta` a partir de su texto; admite cadena vacía.
    pub fn nuevo(texto: &str) -> Self {
        Self(texto.to_string())
    }

    /// Texto de la etiqueta.
    pub fn como_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Etiqueta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
