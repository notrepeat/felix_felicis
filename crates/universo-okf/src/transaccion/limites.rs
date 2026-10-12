/// Presupuesto estructural compartido por el inventario y la validación del árbol raw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LimitesInventario {
    pub(crate) bytes_raw: usize,
    pub(crate) entradas: usize,
    pub(crate) profundidad: usize,
    pub(crate) bytes_rutas: usize,
}

impl LimitesInventario {
    pub(crate) const CONTRATO: Self = Self {
        bytes_raw: 128 * 1024 * 1024,
        entradas: 100_000,
        profundidad: 128,
        bytes_rutas: 16 * 1024 * 1024,
    };
}
