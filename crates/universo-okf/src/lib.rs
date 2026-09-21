//! Adaptador bundle OKF ↔ estructura del motor.

pub mod bundle;
pub mod documento;
pub mod errores;
pub mod manifiesto_toml;
pub mod serializar;

pub use bundle::{cargar_bundle, parsear_bundle};
pub use documento::{Documento, EntradaCruda, parsear_documento};
pub use errores::{
    ErrorBundle, ErrorDocumento, ErrorEntrada, ErrorEscritura, ErrorManifiestoArchivo, Regla,
};
pub use manifiesto_toml::{cargar_manifiesto, parsear_manifiesto};
pub use serializar::{escalar_salida, serializar_bundle, serializar_nodo};
