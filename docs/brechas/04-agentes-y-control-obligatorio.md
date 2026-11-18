# 04 — Agentes, MCP y control obligatorio

Afirmaciones: A3 («cualquier cambio tiene que pasar por el verificador») y A9
(«barrera verificable entre "la IA propuso" y "lo que sé"»).
[Volver al maestro](00-maestro.md).

## Qué existe

| Capacidad | Estado | Evidencia |
|---|---|---|
| MCP con cuatro herramientas `context`, `propose`, `check`, `why`, todas `readOnlyHint` | Implementado y probado | `crates/universo-mcp/src/dominio.rs:32,57` (verificado) |
| `context`: vecindad inducida de profundidad ≤2 desde un nodo conocido, ≤256 nodos, devuelve **todos** los invariantes | Implementado | `dominio.rs:178-212` (reportado) |
| `why` usa los mismos argumentos y camino que `propose` | Implementado | `dominio.rs:68` (verificado) |
| Test que comprueba que no existe `apply` por MCP y que el bundle queda intacto | Probado | `crates/universo-mcp/tests/loop_mcp.rs:176-196` (reportado) |
| Apply autorizado solo por CLI, con CAS | Implementado y probado | ver [03](03-transacciones-y-durabilidad.md) |
| Harness determinista de fase 06 (agente scriptado sin LLM) y Code Mode local opcional | Implementado (demo) | `tools/demo_mcp.py`, `tools/codemode-local/` (reportado) |

## La brecha central

**Un agente o una persona con acceso a archivos puede escribir el Markdown
directamente y saltarse Felix.** El lock es advisory
(`crates/universo-okf/src/transaccion/lock.rs:7`, verificado) y la propia
documentación lo reconoce (`docs/mcp.md:84-87`, verificado): la ausencia de
herramientas de escritura «no es un sandbox». En el piloto se evitó solo por el
sandbox de solo lectura del runner (`evaluation/run_paired.py`).

No hay enforcement de ningún tipo (verificado salvo donde se indica):

- `.git/hooks` contiene solo ejemplos `*.sample`.
- CI corre tests, fmt, clippy y frontera pública; no valida bundles de usuario
  ni exige que cada cambio pase por Felix (`.github/workflows/ci.yml:19-25`).
- No hay watcher, servidor único escritor ni orquestador obligatorio
  propose → rechazo → reparación → apply autorizado (reportado).
- En el piloto, usar Felix era opcional por instrucción («you may use them»).

## Brechas

| # | Brecha | Tamaño | Compuerta |
|---|---|---|---|
| G1 | **Hecho** (2026-11-17, `universo guard`/`seal`, [guardia](../guardia.md)). Guardia del bundle: hook pre-commit y job de CI que corren `validate` + `check` sobre el repositorio de conocimiento | S | No |
| G2 | **Hecho** (2026-11-17, `universo guard`/`seal`, [guardia](../guardia.md)). Detección de escrituras fuera de Felix: comparar el bundle con el token del último estado admitido y exigir pasar por `propose` (ver D5 en [03](03-transacciones-y-durabilidad.md)) | M | No |
| G3 | Camino legítimo para que un agente publique: `apply` autorizado por el orquestador o por confirmación humana, nunca por el modelo solo | M | Decisión de autoridad |
| G4 | Orquestador obligatorio (prioridad 2 de las [conclusiones](../conclusiones-ia-felix-felicis.md#6-prioridades-y-decisión)) en harness aislado | M | **Sí**: no se inicia sin decisión |
| G5 | `context` con selección de invariantes relevantes que preserve cobertura | M | No, pero medir antes (riesgo de ahorro aparente) |
| G6 | `why` con explicación propia o retirarlo (ver T3 en [02](02-rechazo-testigos-y-reparacion.md)) | S | No |

G1 y G2 hacen que A3 pase de «falso» a «detectado»; solo G3 + G4 la hacen
«obligatorio» para agentes. Para una persona que edita a mano, lo realista es
detección inmediata (G1/G2), no prohibición.
