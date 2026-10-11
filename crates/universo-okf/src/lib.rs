//! Adaptador bundle OKF ↔ estructura del motor.

pub mod bundle;
pub mod delta;
pub mod documento;
pub mod errores;
pub mod manifiesto_toml;
pub mod serializar;
pub mod transaccion;

pub use bundle::{cargar_bundle, parsear_bundle};
pub use delta::{ErrorDeltaJson, LadoDelta, parsear_delta_json_v1, serializar_delta_json_v1};
pub use documento::{Documento, EntradaCruda, parsear_documento};
pub use errores::{
    ErrorBundle, ErrorDocumento, ErrorEntrada, ErrorEscritura, ErrorManifiestoArchivo, Regla,
};
pub use manifiesto_toml::{cargar_manifiesto, parsear_manifiesto};
pub use serializar::{
    DocumentoCandidato, ErrorSerializarCandidato, ParentOverride, escalar_salida,
    serializar_bundle, serializar_candidato, serializar_nodo,
};
