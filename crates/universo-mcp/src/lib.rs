//! Adaptador MCP del motor. Único crate con runtime async; vacío en el
//! tema 01. `rmcp` se enlaza aquí desde el día uno para que el workspace
//! resuelva y compile el SDK fijado (`rmcp = "3.2"`) y su árbol (tokio)
//! quede en el lockfile. El guard «core sin tokio»
//! (`universo-core/tests/sin_tokio.rs`) inspecciona solo el árbol de
//! `universo-core`: este crate no participa en esa comprobación.
use rmcp as _;
