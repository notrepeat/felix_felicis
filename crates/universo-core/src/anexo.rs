//! Blob opaco por nodo, fuera de $\mathit{Str}(U)$: existe solo para el
//! round-trip con disco (cap. 01: el cuerpo queda fuera de la garantía
//! formal; el frontmatter no mapeado se conserva sin interpretar).

/// Entrada de frontmatter que el motor no interpreta. `crudo` es el texto
/// completo de la entrada (línea `clave: …` y sus continuaciones), con
/// salto final `\n` y sin `\r`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entrada {
    pub clave: String,
    pub crudo: String,
}

/// Cuerpo markdown byte a byte más las entradas de passthrough en orden.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Anexo {
    pub cuerpo: String,
    pub passthrough: Vec<Entrada>,
}
