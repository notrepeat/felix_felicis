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
| A1 | «Lo que sabes y tus reglas se guardan como universo formal» | **Mayormente cierto** (2026-11-18): grafo tipado, etiquetas, jerarquía y atributos con dominio cerrado, valores ordenados, fechas ISO, atributos con varios valores y cardinalidad (`al-menos`). Faltan atributos de arista (fuera de alcance) y el texto de las notas no se razona. | [01](01-representacion-y-reglas.md) |
| A2 | «Verificador determinista» | **Cierto.** Evaluador Tarski con oráculo, corpus de 46 casos sin falsos admit, límites explícitos. | [01](01-representacion-y-reglas.md) |
| A3 | «Cualquier cambio tiene que pasar por el verificador» | **Detectado** (2026-11-17): editar a mano se sigue pudiendo, pero `universo guard` (y el hook de pre-commit de ejemplo) lo detecta comparando con el sello del último estado admitido (bytes, estructura, manifiesto e invariantes; no permisos ni fechas). No hay prohibición ni orquestador obligatorio. | [04](04-agentes-y-control-obligatorio.md) |
| A4 | «Testigo concreto: qué regla y dónde» | **Cierto en local** (2026-11-18): el rechazo trae id y mensaje de la regla (CLI y MCP), y `propose --witness` da la asignación que la incumple, con rutas. La explicación redactada de `--explain` sigue sin decir dónde, y en bases grandes puede agotar su presupuesto. | [02](02-rechazo-testigos-y-reparacion.md) |
| A5 | «Propone corregirlo» | **Falso.** No existe generador de reparación; está excluido a propósito. | [02](02-rechazo-testigos-y-reparacion.md) |
| A6 | «Aplica de forma transaccional con lock, concurrencia y recovery» | **Mayormente cierto en Linux y Windows**, con lock advisory entre escritores Felix, CAS y recovery v2 cableados. macOS y otros Unix fallan cerrado. | [03](03-transacciones-y-durabilidad.md) |
| A7 | «Nunca queda a medio escribir» | **Mayormente cierto en Linux** (2026-11-17): fsync ordenado de stage y directorios, limpieza del stage tras preparación fallida, `status`/`recover`. Sin prueba real de caída del SO; en Windows no se sincronizan entradas de directorio. | [03](03-transacciones-y-durabilidad.md) |
| A8 | «Ni se contradice sin que lo sepas» | **Mayormente cierto si se usa la guardia** (2026-11-17): `guard` combina `check`, estado transaccional y sello; `check` avisa de transacciones pendientes. Sigue sin ejecutarse solo: depende de instalar el hook o el job de CI. | [03](03-transacciones-y-durabilidad.md), [04](04-agentes-y-control-obligatorio.md) |
| A9 | «Barrera entre "la IA propuso" y "lo que sé"» | **Demostrada como prototipo** (2026-11-18): el MCP es de solo lectura, el orquestador (`tools/orquestador.py`) verifica cada delta y sólo su ejecutor publica con CAS, y en el estudio con contexto parcial no dejó pasar ningún estado inválido. Falta un aislamiento del agente por el sistema operativo; hoy es por construcción. | [04](04-agentes-y-control-obligatorio.md) |
| A10 | «Biblioteca personal de criterio» (uso diario) | **Mejor, todavía no** (2026-11-18): `init`, guardia, notas nuevas y edición desde un borrador por delta, historial con autor, motivo y delta (`log`, `show`), reglas con nombre. Probado contra la biblioteca privada real en la fase 08. Búsqueda léxica con `universo search` (P3). Instalación con `cargo install` y workflow de binarios de release (U6); falta publicar una versión con binarios. | [05](05-procedencia-y-vision.md), [06](06-uso-diario-y-release.md) |
| A11 | «Mejora a un agente» (lo dijimos como no demostrado) | **Demostrado de forma acotada** (2026-11-18, [informe](../../evaluation/four_arms_report.md)). Con información completa el agente no se equivoca y Felix no aporta (tres calibraciones con acierto total). Con contexto parcial y verificación obligatoria con testigo localizado, se pasa de 9 a 0 estados inválidos en 24 corridas y se reparan las 9 cascadas ocultas. La revisión neutral no repara ninguna. Felix opcional (MCP) no se usó. Felix no detecta cambios que cumplan las reglas pero inventen hechos. | [07](07-evidencia-de-utilidad.md) |

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
| 3 | ~~Guardia de escritura: hook pre-commit/CI que corre `validate`+`check` sobre el bundle, y detección de cambios fuera de Felix (comparar contra el último token admitido)~~ **Hecho** 2026-11-17 ([guardia](../guardia.md)) | S–M | No |
| 4 | ~~Rechazo legible: id y mensaje por regla en `.sx`; `propose --explain` con testigo concreto opt-in y local~~ **Hecho en lo que no requiere decisión** 2026-11-17: R1, T1 y T2 en texto. Id y mensaje en el MCP **hechos** 2026-11-18 (campos añadidos a `felix-mcp-v1`), y testigo localizado en la CLI (`propose --witness`). Pendiente de decisión: T3 `why`, T4, T5 (el plan 04 conserva la variante) | M | No (respetar la política de redacción por defecto) |
| 5 | **Hecho** 2026-11-18: crear notas con `felix-delta-v2` (simulación, apply, recovery); sin borrado ([propuesta](../propuestas/05-crear-y-retirar-notas.md)). **Propuesta escrita, pendiente de decisión** 2026-11-17 ([propuesta](../propuestas/05-crear-y-retirar-notas.md)): cambia el formato del delta y la semántica de replay. Original: Crear/borrar nodos (notas) por delta, con transacción y replay | L | No, pero toca contratos de fase 04 |
| 6 | **Hecho** 2026-11-18: `universo init`, historial con procedencia mínima (`log`, `show`), delta desde un borrador editado (`universo delta`) y búsqueda léxica (`universo search`). Original: Ergonomía: `universo init`, generar el delta desde una nota editada, búsqueda léxica, historial con procedencia mínima (quién/cuándo/por qué) | M–L | No |
| 7 | Representación de criterio: fechas/vigencia, confianza, fuente, multivalor | L | Decisión de diseño (cambia OKF y semántica) |
| 8 | **Prototipo hecho** 2026-11-18 ([propuesta](../propuestas/08-orquestador-obligatorio.md), `tools/orquestador.py`): verificación obligatoria, reparación con presupuesto, publicación sólo por el ejecutor con CAS, detección de escrituras externas; agentes guionizados, sin LLM. Original: Prototipo de orquestador obligatorio (prioridad 2 de las conclusiones) | M | Decidido 2026-11-18 |
| 9 | **Hecho** 2026-11-18 ([propuestas 09](../propuestas/09-estudio-cuatro-brazos.md) y [09b](../propuestas/09b-contexto-parcial.md), [protocolo](../../evaluation/four_arms_protocol.md), [informe](../../evaluation/four_arms_report.md)): estudio de 96 corridas con Claude Sonnet 5.5 y contexto parcial; C 21/24 y 0 inválidos, frente a 9 inválidos en A, B y D | M | Decidido 2026-11-18 |
| 10 | Reparación sugerida (delta mínimo) | L | **Sí**: roza revisión AGM, excluida hasta probar valor |
| 11 | **Hecho** 2026-11-18: fase 08 corrida contra la instancia privada (motor fijado en `v0.2.0`; revisión de licencia AGPL de la integración) y `v0.2.0` publicada como tag y release ([notas](../release/v0.2.0.md)) | S–M | Decidido 2026-11-18 |
| — | AGM con preorden, recuperación por patrones, DPO/SqPO | L | **Sí**, excluidas por el plan |

Los puntos 1–6 hacen verdaderas A3 (como detección), A4, A7, A8 y gran parte de
A10 sin cruzar ninguna compuerta. A5 («propone corregirlo») y A11 dependen de
decisiones.

## Párrafo honesto para hoy

> Felix Felicis guarda tu conocimiento estructurado y tus reglas como un universo
> formal y verifica cada propuesta de cambio con un evaluador determinista: si
> contradice una regla, la rechaza diciendo qué regla falló y, si se pide, en qué
> nodos. Si la admite, la aplica con lock, control de concurrencia y
> recuperación ante fallos del proceso. Puesto como paso obligatorio entre un
> agente y la base, evitó en un estudio controlado todos los estados inválidos
> que el agente dejaba cuando no veía la base entera, y le permitió repararlos.
> No mejora al agente cuando éste ya ve todo, y no detecta cambios que cumplan
> las reglas pero contradigan la intención. Fuera de ese flujo, la barrera es
> voluntaria: `guard` detecta los cambios hechos a mano, pero no los impide.

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
