# 06 — Cerrar el loop de propuesta y corrección por MCP

El objetivo es que un agente consulte contexto, proponga un delta, reciba un
veredicto estructurado y corrija su propuesta sin acceso directo de escritura
al bundle.

## Estado

| Campo | Valor |
|---|---|
| Estado | completo |
| Depende de | 05 |
| Desbloquea | 07 |

Contrato implementado y límites: [guía MCP](../mcp.md). Se añadió por decisión
del usuario una comparación con [Code Mode local opcional](../codemode-comparacion.md),
sin mover bundles a la nube. Cierre verificado en `6f9eb9950a0a08e66c54c521e7bf2437c6b6cd2b`: CI
`44147088907` pasó en Linux (1501/0/19) y Windows (1477/0/19), 45 suites
por plataforma. El SDK opcional se verificó localmente
en Linux y por revisión independiente; esa integración no se ejecuta en CI.

## Herramientas mínimas

| Tool | Responsabilidad |
|---|---|
| `context` | Recuperar el subgrafo relevante y sus invariantes. |
| `propose` | Presentar un delta sin persistirlo. |
| `check` | Evaluar estado o candidato y devolver el veredicto. |
| `why` | Explicar un testigo o la procedencia disponible. |

## Entregables

1. Servidor MCP por `stdio` con contratos JSON versionados.
2. Adaptador fino: la lógica permanece en `universo-core`.
3. Límites de tamaño, tiempo y profundidad explícitos.
4. Harness que ejecuta la demo completa como cliente MCP.
5. Transcript sintético sin información personal.

## Criterios de aceptación

- [x] MCP y CLI producen el mismo veredicto para la misma entrada.
- [x] La conexión MCP no concede escritura del bundle: sólo consultas y `propose` sin persistencia. No se concede al agente una herramienta externa de filesystem.
- [x] Un rechazo contiene datos suficientes para una segunda propuesta.
- [x] La propuesta corregida es admitida en el escenario público.
- [x] Reiniciar el servidor no cambia el resultado.
- [x] Los errores de protocolo se distinguen de los veredictos del dominio.

## Correspondencia entre criterios y evidencia

- `crates/universo-mcp/tests/loop_mcp.rs` usa un cliente stdio real: obtiene la
  corrección de `context` y `why`, distingue rechazo normal de errores de
  herramienta/protocolo, readmite y reinicia el servidor con respuesta idéntica.
- `tools/demo_mcp.py` compara rechazo y admisión con el ejecutable CLI usando
  las mismas entradas; comprueba contenido y namespace intactos.
- Los cuatro contratos no admiten rutas del host, `apply` ni campos de escritura.
  Las pruebas intentan esa capacidad ausente; el servidor sólo accede a las rutas
  fijadas por el operador. Esto delimita la API, no revoca permisos externos de
  filesystem que otro sistema conceda al agente.
- [Transcript sintético](../fixtures/mcp-transcript.json), [guía y límites](../mcp.md)
  y [comparación Code Mode](../codemode-comparacion.md) completan los entregables.
  El harness es un agente determinista; no se afirma evaluación de un LLM.

## Fuera de alcance

- Revisión automática AGM.
- Destilación de conversaciones.
- Recuperación general de subgrafos costosos.

## Verificación reproducible

```powershell
cargo test -p universo-mcp
cargo test --workspace --test loop_mcp
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-11-17 | `8ac2c69` | MCP directo: workspace Rust 1501/0/19, 45 suites; fmt/clippy y demo con CLI real, reinicio y bytes intactos. | CI `44146216036` pasa: Linux 1501/0/19 y Windows 1477/0/19, 45 suites por plataforma. |
| 2026-11-17 | `6f9eb99` | SDK Cloudflare real local: aislamiento 8/8 y comparación A/B de 12 muestras. | Linux local y revisión independiente pasan; el workflow CI no ejecuta este adaptador. Sin inferencias de tokens o utilidad LLM. |
| 2026-11-17 | `6f9eb99`, CI `44147088907` | Workspace Rust/MCP: Linux 1501/0/19 y Windows 1477/0/19, 45 suites por OS; fmt, clippy y frontera pública. | Aprobado; fase 06 completa. |

CI final `44147088907` corresponde al SHA completo indicado arriba; valida el
workspace Rust y MCP en Linux/Windows. La suite Code Mode 8/8 y las 12 muestras
A/B son evidencia local Linux separada, repetida por el revisor. Las cotas y
límites de privacidad/aislamiento permanecen los documentados, sin promesas de
ahorro de tokens, utilidad de modelos o soporte Windows del adaptador opcional.

## Siguiente fase

[07 — Validación empírica](07-validacion-empirica.md).
