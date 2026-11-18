# 02 — Rechazo, testigos y reparación

Afirmaciones: A4 («rechaza con un testigo concreto: qué regla y dónde») y A5
(«propone corregirlo»). [Volver al maestro](00-maestro.md).

## Qué entrega hoy un rechazo

`proponer` devuelve uno de tres resultados
(`crates/universo-core/src/transaccion/propuesta.rs:23-30`, verificado):
`Admitida`, `RechazadaConflictos` o `RechazadaInvariantes`. En este último caso
solo viaja `ProcedenciaExplicacion`: índice de fuente, línea, columna e índice
de instancia (`veredicto.rs:51-60`, reportado).

| Canal | Qué ve el usuario | Evidencia |
|---|---|---|
| CLI `propose` (texto) | Solo `conflicts: N; failed_invariants: M` | `crates/universo-cli/src/propose.rs:156-159` (verificado) |
| CLI `propose` | No acepta `--explain` | `main.rs:27`, uso de `propose` (verificado) |
| MCP `propose`/`why` | Procedencia (fuente/línea/columna). `why` comparte argumentos y camino con `propose`, no explica más | `crates/universo-mcp/src/dominio.rs:68` (verificado) |
| CLI `check --explain` | Árbol abstracto de la regla, `presence_core` como conteo, `gap` como forma; sin rutas ni ligaduras | `main.rs:893-1023` (reportado) |

Conclusión: **«qué regla»** se responde a medias (posición en el archivo, sin
nombre); **«dónde»** no lo ve ningún humano por CLI ni MCP. La redacción es
deliberada (privacidad del bundle), y `check --explain` evalúa el bundle
existente, no el candidato de un delta.

## Reparación

**Ausente y excluida a propósito** (verificado):

- `crates/universo-core/src/veredicto/brecha.rs:3`: «Este extractor no propone
  reparaciones ni inventa elementos del dominio».
- `crates/universo-cli/tests/explain.rs:608` afirma que ningún objeto trae
  `repair`.

Lo más cercano son `gap` (obligación existencial sin testigo) y `presence_core`
(hechos mínimos que sostienen la falla): materia prima para una reparación, no
una reparación.

## Brechas

| # | Brecha | Tamaño | Compuerta |
|---|---|---|---|
| T1 | **Hecho** (2026-11-17): `propose --explain decide|enumerate` en simulación, con el formato redactado de `check` v2; test `crates/universo-cli/tests/propose_explain.rs`. `propose --explain`: testigo concreto sobre el **candidato**, opt-in y solo local, manteniendo la redacción por defecto | M | No, pero debe respetar la política de privacidad |
| T2 | **Parcial** (2026-11-17): id y mensaje sólo en el texto de `--explain`; el JSON v2 (claves cerradas) y el MCP no cambian sin decisión de esquema. Nombre y mensaje de la regla en el rechazo (depende de R1 en [01](01-representacion-y-reglas.md)) | S | No |
| T3 | `why` con contenido propio (p. ej. testigo y regla) o retirarlo | S | No |
| T4 | Conflicto `Ext` como rechazo y no como error técnico `add_conflict` (`propuesta.rs:191-202`, reportado) | S | No |
| T5 | Quitar `ErrorCandidato::UnsupportedOperation`, que ningún camino construye (`candidato.rs:35`, verificado) | S | No |
| T6 | Reparación sugerida: delta mínimo derivado de `presence_core`/`gap`, reverificado con `proponer` antes de mostrarse | L | **Sí**: elegir qué ceder es revisión (AGM), excluida hasta probar valor |

Con T1–T4, A4 se vuelve cierta. A5 solo se puede prometer después de una
decisión explícita sobre T6.
