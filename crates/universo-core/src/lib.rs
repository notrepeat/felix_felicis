//! Núcleo síncrono del motor Universo de Conocimiento. No depende de tokio
//! ni de ningún otro crate del workspace: es el fondo hexagonal sobre el
//! que se construyen `universo-okf`, `universo-testkit` y `universo-cli`.
pub mod anexo;
pub mod atomos;
pub mod estructura;
pub mod formula;
pub mod ids;
pub mod manifiesto;
pub mod path;
pub mod sexpr;
pub mod tipos;
pub use anexo::{Anexo, Entrada};
pub use atomos::{Atomo, ErrorAtomo, IdArista, Sujeto};
pub use estructura::{Arista, ErrorEstructura, Estructura, Nodo};
pub use formula::{
    DominioEsquema, Formula, Sentencia, SorteVar, SujetoTermino, TerminoArista, TerminoConstExt,
    TerminoEtiqueta, TerminoNodo, TerminoSorte, Var,
};
pub use ids::{AristaId, NodoId};
pub use manifiesto::{
    DeclArista, DeclPredicado, ErrorManifiesto, Layout, Manifiesto, PREDICADOS_BASE,
    SORTES_RESERVADOS,
};
pub use path::{BundlePath, ErrorPath};
pub use sexpr::{ErrorAutoria, MotivoAutoria, parsear};
pub use tipos::{Etiqueta, Sorte};
