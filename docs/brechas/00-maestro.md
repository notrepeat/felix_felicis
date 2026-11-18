# Brechas entre la propuesta de valor y el código

Fecha: 2026-11-17. Base: `codex/plan-demostracion` en `89764ab`.

Este documento contrasta, afirmación por afirmación, el párrafo con el que
describimos Felix Felicis contra lo que el **código** hace hoy, no contra lo que
dicen los documentos. Complementa el [plan maestro](../plan/00-plan-maestro.md):
el plan ordena fases; este análisis dice qué falta para que la promesa sea cierta.

## La promesa

> Lo que sabes y las reglas que consideras ciertas se guardan como un universo
> formal, y cualquier cambio, venga de ti o de un agente de IA, tiene que pasar
> por un verificador determinista antes de entrar. Si el cambio contradice tus
> reglas, Felix lo rechaza con un testigo concreto que muestra qué regla se rompe
> y dónde, y propone corregirlo en vez de dejarlo pasar en silencio. Lo que se
> admite se aplica de forma transaccional, con lock, control de concurrencia y
> recuperación ante fallos, así que el conocimiento nunca queda a medio escribir
> ni se contradice sin que lo sepas. […] Felix es la barrera verificable entre
> «la IA propuso algo» y «esto pasa a ser parte de lo que sé».

## Veredicto por afirmación

| # | Afirmación | Hoy | Tema |
|---|---|---|---|
| A1 | «Lo que sabes y tus reglas se guardan como universo formal» | **Parcial.** Grafo tipado, etiquetas, jerarquía y atributos de dominio cerrado sí; el texto, fechas, confianza, fuentes y atributos de arista no son representables. | [01](01-representacion-y-reglas.md) |
| A2 | «Verificador determinista» | **Cierto.** Evaluador Tarski con oráculo, corpus de 46 casos sin falsos admit, límites explícitos. | [01](01-representacion-y-reglas.md) |
| A3 | «Cualquier cambio tiene que pasar por el verificador» | **Falso.** Editar el Markdown a mano (persona o agente con acceso a archivos) se salta Felix; nada lo impide ni lo detecta al momento. | [04](04-agentes-y-control-obligatorio.md) |
| A4 | «Testigo concreto: qué regla y dónde» | **Parcial.** Al rechazar un delta solo se entrega fuente/línea/columna de la regla e índice de instancia; la CLI imprime solo conteos. El «dónde» concreto está redactado por política de privacidad. | [02](02-rechazo-testigos-y-reparacion.md) |
| A5 | «Propone corregirlo» | **Falso.** No existe generador de reparación; está excluido a propósito. | [02](02-rechazo-testigos-y-reparacion.md) |
| A6 | «Aplica de forma transaccional con lock, concurrencia y recovery» | **Mayormente cierto en Linux y Windows**, con lock advisory entre escritores Felix, CAS y recovery v2 cableados. macOS y otros Unix fallan cerrado. | [03](03-transacciones-y-durabilidad.md) |
| A7 | «Nunca queda a medio escribir» | **Mayormente cierto en Linux** (2026-11-17): fsync ordenado de stage y directorios, limpieza del stage tras preparación fallida, `status`/`recover`. Sin prueba real de caída del SO; en Windows no se sincronizan entradas de directorio. | [03](03-transacciones-y-durabilidad.md) |
| A8 | «Ni se contradice sin que lo sepas» | **Parcial.** `check` detecta después una violación de reglas, pero nadie lo ejecuta automáticamente; un editor que guarda tras un commit pisa el cambio sin aviso. | [03](03-transacciones-y-durabilidad.md), [04](04-agentes-y-control-obligatorio.md) |
| A9 | «Barrera entre "la IA propuso" y "lo que sé"» | **Parcial.** El MCP es de solo lectura y la CLI aplica bajo CAS, pero no hay orquestador obligatorio ni enforcement; el agente no tiene camino legítimo para publicar. | [04](04-agentes-y-control-obligatorio.md) |
| A10 | «Biblioteca personal de criterio» (uso diario) | **No todavía.** Sin comando para agregar conocimiento (deltas JSON a mano), sin crear notas nuevas por delta, sin búsqueda, sin historial ni procedencia de cambios, sin instalación. | [05](05-procedencia-y-vision.md), [06](06-uso-diario-y-release.md) |
| A11 | «Mejora a un agente» (lo dijimos como no demostrado) | **No demostrado.** Piloto neutral; instrumentación v2 corregida y probada (2026-11-17), aún sin conectar ni usar en un estudio. | [07](07-evidencia-de-utilidad.md) |

## Las cinco brechas que más pesan

1. **Felix no es el único camino de escritura** (A3, A8, A9). Sin un control que
   impida o detecte cambios fuera de Felix, la garantía depende de la disciplina.
2. **No se puede agregar conocimiento nuevo cómodamente** (A10): un delta no
   puede crear un nodo (`crates/universo-core/src/transaccion/tipos.rs:1-2`,
   `nodos.rs:62`), y los deltas se escriben en JSON a mano.
3. **El rechazo no es accionable para un humano** (A4, A5): solo línea de la
   regla y conteos; no hay reparación sugerida.
4. **Durabilidad incompleta** (A7): falta fsync de archivos del stage y de
   directorios; un stage huérfano bloquea; no hay comando `recover`/`status`.
5. **La representación no cubre «criterio»** (A1): sin fechas/vigencia,
   confianza, fuente ni atributos multivaluados o de arista.

## Hoja de ruta propuesta

Ordenada por valor/costo. La columna «Compuerta» indica si cruza una regla
escrita del plan y por tanto **requiere decisión explícita de Romeo**.

| Orden | Trabajo | Tamaño | Compuerta |
|---|---|---|---|
| 1 | ~~Corregir proxy v2 (bytes, latencia, llamadas en vuelo) y meter los tests Python en CI~~ **Hecho** 2026-11-17; falta confirmar el CI de Windows ([07](07-evidencia-de-utilidad.md)) | S | No |
| 2 | ~~Durabilidad: fsync de archivos del stage y de directorios padre; limpieza o diagnóstico del stage huérfano; `universo status`/`recover`~~ **Hecho** 2026-11-17 (Windows: sin fsync de directorios; sin prueba de caída del SO) ([03](03-transacciones-y-durabilidad.md)) | M | No |
| 3 | Guardia de escritura: hook pre-commit/CI que corre `validate`+`check` sobre el bundle, y detección de cambios fuera de Felix (comparar contra el último token admitido) | S–M | No |
| 4 | Rechazo legible: id y mensaje por regla en `.sx`; `propose --explain` con testigo concreto opt-in y local | M | No (respetar la política de redacción por defecto) |
| 5 | Crear/borrar nodos (notas) por delta, con transacción y replay | L | No, pero toca contratos de fase 04 |
| 6 | Ergonomía: `universo init`, generar el delta desde una nota editada, búsqueda léxica, historial con procedencia mínima (quién/cuándo/por qué) | M–L | No |
| 7 | Representación de criterio: fechas/vigencia, confianza, fuente, multivalor | L | Decisión de diseño (cambia OKF y semántica) |
| 8 | Prototipo de orquestador obligatorio (prioridad 2 de las conclusiones) | M | **Sí**: no se inicia sin decisión |
| 9 | Estudio prerregistrado de cuatro brazos (prioridad 3) | M | **Sí** |
| 10 | Reparación sugerida (delta mínimo) | L | **Sí**: roza revisión AGM, excluida hasta probar valor |
| 11 | Fase 08 (licencia AGPL) y 09 (tag `v0.2.0`) | S–M | **Sí** |
| — | AGM con preorden, recuperación por patrones, DPO/SqPO | L | **Sí**, excluidas por el plan |

Los puntos 1–6 hacen verdaderas A3 (como detección), A4, A7, A8 y gran parte de
A10 sin cruzar ninguna compuerta. A5 («propone corregirlo») y A11 dependen de
decisiones.

## Párrafo honesto para hoy

> Felix Felicis guarda tu conocimiento estructurado y tus reglas como un universo
> formal y verifica cada propuesta de cambio con un evaluador determinista: si
> contradice una regla, la rechaza indicando qué regla falló, y si la admite, la
> aplica con lock, control de concurrencia y recuperación ante fallos del
> proceso. Hoy esa barrera es de uso voluntario: protege los cambios que pasan
> por ella, pero todavía no impide los que se hacen a mano.

## Método y confianza

Cinco subagentes analizaron el código por tema (tres con Opus: verificador,
transacciones, agentes; dos con Sonnet: visión teórica, uso diario). Ninguno
tuvo shell, así que usaron búsqueda estática en vez del grafo de Graphify. Cada
hallazgo citado en estos documentos se marca como:

- **verificado**: comprobado directamente en el código durante este análisis;
- **reportado**: viene de un subagente y no se volvió a comprobar línea a línea.

Un subagente leyó mal el README y afirmó que la CLI no puede aplicar; es falso:
`universo propose … --apply --expect-snapshot` existe
(`crates/universo-cli/src/propose.rs:7`). Se corrigió en estos documentos.
