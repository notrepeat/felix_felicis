# Plan maestro — demostrar que `felix_felicis` funciona

El objetivo no es completar toda la visión antes de recibir evidencia. El
objetivo es demostrar, por capas, que Felix representa conocimiento sin
pérdida, detecta contradicciones formales, explica sus rechazos y mejora el
resultado de un agente en escenarios reproducibles.

## Estado global

| Campo | Valor |
|---|---|
| Estado | en curso |
| Versión de partida | `v0.1.0` |
| Próximo gate | `v0.2.0` |
| Progreso | 3/9 fases completas; fases 01–03 cerradas con evidencia reproducible. `cargo test --workspace` pasa en HEAD `ccd54b5`. |
| Foco actual | Fase 04: cerrar root canónico, orden de aristas/Ext/labels, API core/OKF y privacidad CLI. |
| Última actualización | 2026-10-10 |

Estados admitidos: `pendiente`, `en curso`, `bloqueado`, `completo`. Una fase
solo pasa a `completo` cuando cumple todos sus criterios de aceptación y deja
evidencia reproducible en su registro.

## Qué significa «funciona»

| Afirmación | Evidencia requerida |
|---|---|
| La representación es fiel | Carga, serialización y recarga conservan átomos y anexos. |
| El check es correcto | Un evaluador de referencia coincide con un corpus etiquetado y pruebas generativas. |
| El rechazo es explicable | Cada instancia falsa conserva un contraejemplo; `presence_core` y `gap` se emiten solo dentro de sus fragmentos acotados. |
| La operación es segura | Un candidato rechazado no modifica el universo persistido. |
| El sistema ayuda | Un agente con Felix mejora frente al mismo agente sin Felix en una evaluación pareada. |

## Ruta crítica

```mermaid
flowchart LR
    P1[01 Contrato de verdad] --> P2[02 Evaluador]
    P2 --> P3[03 Veredictos]
    P3 --> P4[04 Transacciones]
    P4 --> P5[05 Demo pública]
    P5 --> P6[06 Loop MCP]
    P6 --> P7[07 Validación empírica]
    P4 --> P8[08 Integración privada]
    P7 --> P9[09 Versionado]
    P8 --> P9
```

## Fases

| # | Fase | Estado | Depende de | Resultado observable |
|---|---|---|---|---|
| [01](01-contrato-de-verdad.md) | Contrato de verdad | completo | — | `validate` y `check` respetan contrato de comandos, códigos, canales, JSON v1 y privacidad; pruebas y CLI en `a9e1983`. |
| [02](02-evaluador-de-referencia.md) | Evaluador de referencia | completo | 01 | Parser, validación estática, expansión, Tarski, oráculo y `check` verificados; commits `2b5f721`–`a9e1983`. |
| [03](03-veredictos-explicables.md) | Veredictos explicables | completo | 02 | Criterios 01–17 cerrados con suites de `universo-core`/`universo-cli`; los conflictos funcionales del delta quedan asignados al gate de aceptación de fase 04. |
| [04](04-operacion-transaccional.md) | Operación transaccional | en curso | 03 | Ningún rechazo modifica el estado vigente. |
| [05](05-demo-publica.md) | Demo pública | pendiente | 01–04 | Una contradicción y su reparación se reproducen con un comando. |
| [06](06-loop-de-agentes-mcp.md) | Loop de agentes por MCP | pendiente | 05 | Un agente propone, recibe veredicto, corrige y readmite. |
| [07](07-validacion-empirica.md) | Validación empírica | pendiente | 05–06 | Hay mediciones públicas de corrección, utilidad y costo. |
| [08](08-integracion-privada.md) | Integración privada | pendiente | 04 | El bundle real valida el motor sin filtrarse al repositorio público. |
| [09](09-versionado-y-evidencia.md) | Versionado y evidencia | pendiente | 07–08 | Cada tag corresponde a capacidades demostradas y CI verde. |

## Reglas de progreso

- Cada fase se implementa en una rama `codex/<fase>` o equivalente.
- Cada cambio conserva pruebas y documentación junto al comportamiento.
- Ninguna casilla se marca por intención; se marca con un commit, prueba o
  artefacto enlazado en el registro de evidencia.
- Un bloqueo explica qué falta, quién puede resolverlo y qué trabajo puede
  continuar sin esa decisión.
- Las mediciones se publican aunque contradigan la hipótesis del proyecto.
- Revisión AGM, recuperación avanzada y DPO/SqPO quedan fuera hasta que la
  demo mínima pruebe valor.

## Checklist maestro

- [x] 01 — El contrato estructural de `validate` y la aceptación semántica de `check` están implementados y verificados conjuntamente.
- [x] 02 — El evaluador de referencia ejecuta todo el AST soportado, con pruebas por variante, oráculo y límites de recursos documentados.
- [x] 03 — Los veredictos explican contradicciones y vacíos dentro de fragmentos acotados, con privacidad y presupuesto verificados.
- [ ] 04 — Las propuestas son transaccionales.
- [ ] 05 — La demo pública corre en CI.
- [ ] 06 — El loop MCP funciona de extremo a extremo.
- [ ] 07 — La evaluación comparativa tiene resultados publicados.
- [ ] 08 — La integración privada pasa sin exponer datos.
- [ ] 09 — Se publica el tag correspondiente con evidencia enlazada.

## Próximo paso

Foco actual: cerrar el contrato de [04 — Operación transaccional](04-operacion-transaccional.md).
La fase 03 preserva el baseline JSON v1; solo la salida solicitada usa v2.
El presupuesto medido de 1.000.000 se aplica al pase explicativo separado, no
al trabajo total del proceso. No se exponen asignaciones universales sensibles
ni resultados parciales por agotamiento de recurso.

## Evidencia de las fases cerradas

| Commit | Hito verificado |
|---|---|
| `2b5f721d66bef62e753b2481df8c7865da130d38` | Parser de cero o más sentencias con ubicaciones de fuente. |
| `602120977c3b0949730efd7a926c113879afb42c` | Validación estática tipada de invariantes. |
| `0b3df1162368c0cf1f5adbf4cc577897f146f1fa` | Evaluador Tarski y oráculo independiente con pruebas generativas. |
| `b8786dbc56bcf63a72fe540bf65ee6975484a861` | Expansión determinista de esquemas con límite de instancias. |
| `a9e198347ea40835b3686a16a349150fcbb7ff3c` | CLI `check`, JSON v1, canales y compatibilidad de `validate`. |

## Evidencia de fase 03 (fase completada)

| Commit / estado | Hito |
|---|---|
| `1a923bc` | Conteos medidos de evaluación/evidencia para trazas. |
| `5f28991` | Agregación de contraejemplos con selección `decide`/`enumerate`. |
| `cf0f752` | Extracción de cores de presencia subconjunto-minimales en fragmento operacional. |
| `df06f11` | Gap positivo integrado en el núcleo. |
| `4d8b238` | CLI `--explain` opt-in, JSON v2 y texto redactado. |
| `fd637ce5e82e17f6d75713cd0ce59f5a9e83af2d` | Casos límite de presencia/gap, replay semántico y forma positiva recursiva. |
| `ccd54b5ac1fd68178faca48389d3de2313054c6d` | Errores presupuestarios v2 sin parciales, orden `decide` y procedencia exacta. |

Verificación de fase 02 registrada: `cargo test -p universo-core` (108 unitarias y 7 de
integración); `cargo test -p universo-cli` (8 pruebas de `check` y 14 de CLI);
clippy en ambos crates, `cargo fmt --all -- --check` y los dos ejemplos
públicos pasan. Límites: 32 niveles, 64 ligaduras por cuantificador,
100.000 instancias expandidas y 1.000.000 de unidades deterministas de
evaluación; no constituyen SLA de latencia o memoria. La fase 03 tiene un
presupuesto explicativo global de 1.000.000 de unidades dentro del pase opt-in,
separado del check baseline y no representativo del límite total del proceso.
Verificación final de fase 03 en HEAD `ccd54b5`: `cargo test --workspace`
pasó 251 pruebas y 0 fallos. En detalle, `cargo test -p universo-core` pasó
(151 unitarias y 7 complementarias); `cargo test -p universo-cli` pasó
(1 unitaria, 8 `check`, 14 CLI y 12 explicación). Clippy, `cargo fmt --all
-- --check`, `python tools/check_public_boundary.py` y `git diff --check`
también pasan. El checklist de 17 criterios está completo, incluida la
asignación explícita de conflictos funcionales de delta al gate de aceptación
de fase 04; no afirma que fase 04 ya esté implementada. La explicación opt-in revela estructura lógica
abstracta y valores booleanos por instancia, ordinales/posiciones de fuente,
conteos derivados y métricas deterministas. Errores v2 redactan paths y
locations; errores legacy v1 conservan paths por compatibilidad. Un error de
presupuesto de explicación devuelve status `error` y summary nulo.
