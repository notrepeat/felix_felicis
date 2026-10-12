//! Operaciones OKF sobre snapshots transaccionales.

// U4 prepara estas piezas internas; U5/U6 las conectan al guard transaccional.
#[allow(dead_code)]
pub(crate) mod anclado;
#[allow(dead_code)]
pub(crate) mod captura;
#[allow(dead_code)]
pub(crate) mod inventario;
#[allow(dead_code)]
pub(crate) mod journal;
pub(crate) mod limites;
pub mod lock;
pub(crate) mod raiz;
pub mod snapshot;
pub(crate) mod stage;
