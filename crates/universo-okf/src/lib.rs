//! Adaptador bundle OKF ↔ estructura del motor.

pub mod bundle;
pub mod busqueda;
pub mod criterio;
pub mod delta;
pub mod diferencia;
pub mod documento;
pub mod errores;
pub mod explicacion_json;
pub mod manifiesto_toml;
pub mod serializar;
pub mod transaccion;

pub use bundle::{cargar_bundle, parsear_bundle};
pub use delta::{
    ErrorDeltaJson, LadoDelta, parsear_delta_json, parsear_delta_json_v1, parsear_delta_json_v2,
    serializar_delta_json_canonico, serializar_delta_json_v1, serializar_delta_json_v2,
};
pub use diferencia::{ErrorDiferencia, delta_desde_borrador};
pub use documento::{Documento, EntradaCruda, parsear_documento};
pub use errores::{
    ErrorBundle, ErrorDocumento, ErrorEntrada, ErrorEscritura, ErrorManifiestoArchivo, Regla,
};
pub use manifiesto_toml::{cargar_manifiesto, parsear_manifiesto};
pub use serializar::{
    DocumentoCandidato, ErrorSerializarCandidato, ParentOverride, escalar_salida,
    serializar_bundle, serializar_candidato, serializar_nodo,
};
