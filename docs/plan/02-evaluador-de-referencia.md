# 02 — Ejecutar la semántica completa del AST

El objetivo es disponer de un evaluador deliberadamente simple y correcto,
basado en la semántica de Tarski. Será el oráculo contra el que se comparará
cualquier optimización futura.

## Estado

| Campo | Valor |
|---|---|
| Estado | completo |
| Depende de | 01 |
| Desbloquea | 03 |

El parser, la validación estática, la expansión determinista, el evaluador
Tarski, el oráculo y la CLI `check` tienen evidencia reproducible y commits
registrados abajo.

## Decisiones de contrato vigentes

| Tema | Decisión |
|---|---|
| Firma de tipos | `Type(v,k)` solo tipa nodos. `Edge(e,x,y,k)` es la relación tipada de arista que incorpora identidad, extremos y tipo. |
| Variables | Los ambientes de asignación N/E son separados, pero cada nombre conserva una sola sorte en toda la sentencia. No se permite sombrear un nombre activo ni cambiarlo de sorte; alcances hermanos pueden reutilizarlo con la misma sorte. |
| Fuente | Un archivo contiene cero o más S-expresiones top-level; cada sentencia comienza con contexto independiente. |
| Constantes ausentes | Una constante de nodo o arista que no designa un elemento de la estructura correspondiente es error de autoría, no falsedad. |
| Resultado | Esta fase decide verdad/falsedad de fórmulas. Enumerar veredictos explicables y extraer testigos corresponde a fase 03. |
| Programa vacío | Un archivo vacío o solo con comentarios admite cero invariantes, con todos los conteos en cero; eso no equivale a una política sustantiva. |
| Privacidad | Un `reject` expone conteos, no asignaciones ni átomos testigo. Errores de datos, autoría y recursos son `error`, nunca rechazo lógico. Los campos `path` pueden revelar rutas locales; trata todos los diagnósticos como potencialmente sensibles y no publiques salidas privadas. |
| Recursos | Límites por defecto: 32 niveles de fórmula, 64 ligaduras por cuantificador y 100.000 instancias expandidas. `evaluar` conserva 1.000.000 de unidades deterministas por llamada; el `check` CLI baseline reinicia ese límite por fórmula. La cuota agregada de 1.000.000 que `proponer` comparte entre todas las fórmulas expandidas pertenece a fase 04 y está documentada desde `09aee6e`. Agotar una cota de esta fase produce `error` de tipo `resource`. Estas cuotas no son SLA de latencia o memoria ni promesas de rendimiento. |

## Punto de partida

- [x] Existe un AST FO bisortido.
- [x] Existe parser de S-expresiones con errores de autoría y múltiples sentencias top-level.
- [x] El grafo expone nodos, aristas, padres, etiquetas y atributos.
- [x] Existe validación estática tipada de sentencias contra manifiesto y estructura.
- [x] Existe evaluación Tarski de fórmulas sobre una estructura y un modo CLI `check` que compone la carga, autoría, expansión y evaluación.

## Entregables

1. Entorno de asignación separado para variables de nodo y arista. Integrado.
2. Evaluación de predicados base, extensiones e igualdad. Integrada.
3. Evaluación de conectivos, cuantificadores, guardas y `TC[Parent]`. Integrada.
4. Instanciación finita y determinista de esquemas. Integrada.
5. Modo `decidir` para fórmulas, con validación previa a la evaluación. Integrado.
6. Oráculo independiente y pruebas de verdad/falsedad para cada variante de `Formula`. Integrados.
7. CLI `check` con JSON v1, separación de errores y sin testigos en esta fase. Integrada; verificación registrada abajo.

## Criterios de aceptación

- [x] Cada variante de `Formula` tiene al menos un caso verdadero y uno falso.
- [x] Las sortes incorrectas se rechazan antes de evaluar.
- [x] Constantes ausentes y símbolos fuera de firma producen errores tipados, incluso en ramas que el cortocircuito no visitaría.
- [x] Los esquemas producen el conjunto determinista esperado de oraciones.
- [x] `TC[Parent]` es clausura positiva estricta y coincide con recorridos explícitos en bosques generados.
- [x] Pruebas generativas comparan el evaluador con un oráculo independiente.
- [x] El evaluador directo de AST respeta las cotas de 32 niveles y 64 ligaduras por cuantificador antes de evaluar o expandir.
- [x] No se introduce una optimización sin conservar el evaluador de referencia.
- [x] La CLI distingue `admit`/`reject`/`error`, informa conteos deterministas y no expone testigos en `reject`.

## Fuera de alcance

- Enumeración de veredictos y extracción de testigos; pertenece a la fase 03.
- Check incremental.
- SLA de latencia/memoria ni benchmarks de rendimiento. Las cotas deterministas limitan instancias expandidas y trabajo de evaluación; no caracterizan recursos de hardware ni tiempos de respuesta.

## Verificación prevista

```powershell
cargo test -p universo-core
cargo clippy -p universo-core --all-targets -- -D warnings
cargo fmt --all -- --check
cargo test -p universo-cli
cargo run -p universo-cli -- check examples/knowledge-base --manifiesto manifests/example.toml --invariantes examples/knowledge-base/invariants.sx --format json
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-10-10 | `2b5f721d66bef62e753b2481df8c7865da130d38` — `feat(core): parse invariant programs` | Parser de cero o más S-expresiones top-level, contextos independientes y ubicaciones de fuente; integrado en la suite core. | Base de autoría integrada. |
| 2026-10-10 | `602120977c3b0949730efd7a926c113879afb42c` — `feat(core): validate invariant programs` | En ese hito, `cargo test -p universo-core` (90 unitarias + 7 de integración; 0 doc tests), clippy core, fmt y diff check pasaron. | Validador estático integrado; la evaluación llegó en commits posteriores. |
| 2026-10-10 | `0b3df1162368c0cf1f5adbf4cc577897f146f1fa` — `feat(core): evaluate invariant formulas` | Evaluación Tarski de las variantes de fórmula; oráculo independiente, pruebas generativas, cotas de profundidad/ligaduras y presupuesto determinista de evaluación. | API de evaluación integrada. |
| 2026-10-10 | `b8786dbc56bcf63a72fe540bf65ee6975484a861` — `feat(core): expand invariant schemas deterministically` | Expansión determinista, límite de instancias y errores de recursos. En la verificación final, `cargo test -p universo-core`: 108 unitarias + 7 de integración; clippy core y fmt pasan. | Expansión y suite core aprobadas. |
| 2026-10-10 | `a9e198347ea40835b3686a16a349150fcbb7ff3c` — `feat(cli): check invariant programs` | `cargo test -p universo-cli`: 8/8 pruebas `check`, 14/14 CLI; clippy CLI pasa. Ejemplo normal `admit`/0 (3/3); fixture falsa `reject`/1 (1/1); JSON determinista, canales, errores tipados, compatibilidad validate y sin testigos. | Integración CLI aprobada; fases 01 y 02 cerradas. |
| 2026-10-10 | — (verificación documental del árbol de trabajo) | Se ejecutaron ambos comandos de ejemplo con el binario CLI: el normal devuelve `admit`/0 y el fixture devuelve `reject`/1. Guard público, enlaces Markdown y `git diff --check` pasan. | Documentación y ejemplos verificados. |

## Cierre y siguiente paso

La fase está completa y se cerró junto con la fase 01. La siguiente capacidad,
enumeración de veredictos y extracción de testigos, pertenece a
[03 — Veredictos explicables](03-veredictos-explicables.md).
