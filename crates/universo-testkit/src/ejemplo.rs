//! Rutas al bundle de ejemplo público incluido en el workspace.

use std::path::PathBuf;

/// Raíz del workspace público.
pub fn raiz_workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("no se pudo resolver la raíz del workspace")
}

/// Bundle OKF ficticio usado en documentación y pruebas de integración.
pub fn bundle() -> PathBuf {
    raiz_workspace().join("examples/knowledge-base")
}

/// Manifiesto del bundle ficticio.
pub fn manifiesto() -> PathBuf {
    raiz_workspace().join("manifests/example.toml")
}
