//! Operaciones OKF sobre snapshots transaccionales.

// U4 prepara estas piezas internas; U5/U6 las conectan al guard transaccional.
#[allow(dead_code)]
pub(crate) mod captura;
#[allow(dead_code)]
pub(crate) mod inventario;
pub mod lock;
pub(crate) mod raiz;
pub mod snapshot;
