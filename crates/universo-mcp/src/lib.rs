//! Adaptador MCP de solo lectura; el motor y sus veredictos permanecen en core.
pub mod dominio;
mod servidor;
mod transporte;
mod uso;
mod worker;

pub use servidor::servir;
pub use uso::registrar_uso_en;
pub use worker::ejecutar_worker;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_JSON_DEPTH: usize = 32;
pub const TOOL_DEADLINE_MS: u64 = 5_000;
/// Plazo de `capture`, `reinforce` y `object`, que escriben: largo, para no
/// cortar un apply a mitad. Al vencer, la transacción queda pendiente y
/// `universo recover` la completa o la revierte.
pub const WRITE_DEADLINE_MS: u64 = 300_000;
