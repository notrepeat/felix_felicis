//! Núcleo síncrono del motor Universo de Conocimiento. No depende de tokio
//! ni de ningún otro crate del workspace: es el fondo hexagonal sobre el
//! que se construyen `universo-okf`, `universo-testkit` y `universo-cli`.
pub mod anexo;
pub mod atomos;
pub mod estructura;
pub mod evaluador;
pub mod formula;
pub mod ids;
pub mod invariante;
pub mod manifiesto;
pub mod path;
pub mod sexpr;
pub mod tipos;
pub use anexo::{Anexo, Entrada};
pub use atomos::{Atomo, ErrorAtomo, IdArista, Sujeto};
pub use estructura::{Arista, ErrorEstructura, Estructura, Nodo};
pub use evaluador::{
    AsignacionCanonica, ConstructorAtomico, ErrorEvaluacion, ErrorEvaluacionMedida,
    EvidenciaEvaluacion, InstanciaCuantificada, LigaduraCanonica,
    PRESUPUESTO_EVALUACION_POR_DEFECTO, PRESUPUESTO_EVIDENCIA_POR_DEFECTO, ReglaEvidencia,
    ResultadoTrazable, ResultadoTrazableMedido, TipoCuantificador, ValorLigadura, evaluar,
    evaluar_con_presupuesto, evaluar_trazable, evaluar_trazable_con_presupuesto,
    evaluar_trazable_con_presupuesto_total_medido, evaluar_trazable_con_presupuestos,
    evaluar_trazable_con_presupuestos_medidos,
};
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
pub use sexpr::{ErrorAutoria, MotivoAutoria, SentenciaFuente, parsear, parsear_programa};
pub use tipos::{Etiqueta, Sorte};
