# 05 — Procedencia, revisión y visión teórica

Afirmación: A10 en su parte de «biblioteca de criterio» y la descripción
general del proyecto («universo formal, check decidible, revisión AGM y
procedencia»). [Volver al maestro](00-maestro.md).

## Visión de `docs/01`–`07` frente al código

| Capítulo | Promesa | Estado |
|---|---|---|
| [01 Universo](../01-universo.md) | Tupla con grafo tipado, etiquetas, padre, invariantes y preorden ⪯ | Implementado y probado **salvo ⪯**, ausente (verificado) |
| [02 No-contradicción](../02-no-contradiccion.md) | Check FO decidible tras cada operación | Implementado y probado |
| [03 Revisión](../03-revision.md) | AGM: ceder lo menos atrincherado según ⪯ | **Solo documentado**: el sistema solo admite o rechaza |
| [04 Recuperación](../04-recuperacion.md) | Recuperación por patrones (subgrafos, treewidth acotado) | **Solo documentado**; existe `context` por vecindad |
| [05 Evolución](../05-evolucion.md) | Reglas DPO/SqPO con check tras cada regla | **Solo documentado** para DPO/SqPO (el delta es altas/retiros de átomos); el check transaccional sí existe |
| [06 Límites](../06-limites.md) | Lo que no se garantiza | No requiere código |
| [07 Memoria documental](../07-memoria-documental.md) | El porqué como grafo, rationale tipado, completitud, procedencia de la revisión | **Solo documentado** en el motor; posible como instancia (manifiesto + invariantes) |

Fuente: subagente de visión teórica (reportado); las ausencias de ⪯, `DPO`,
`SqPO` y marcas de tiempo en código de producción se verificaron con `rg`.

## Procedencia

Lo que el código llama «procedencia» es **la posición de la regla violada**
(`ProcedenciaExplicacion`: fuente, línea, columna, instancia). No se registra
**quién, cuándo, por qué ni con qué fuente** se agregó un átomo o se hizo un
cambio. El journal sirve para recovery y se retira tras el commit; no hay
historial consultable más allá de git (reportado).

## Consulta de criterio

No hay forma de preguntar «qué pienso sobre X» ni «qué reglas aplican a X»:
`context` exige la ruta exacta de un nodo y devuelve vecindad y todos los
invariantes. Desde 2026-11-18 hay búsqueda léxica en la CLI (`universo search`,
P3), que también encuentra valores de atributos; no hay búsqueda semántica
(P5) ni la hay en el MCP.

## Brechas

| # | Brecha | Tamaño | Compuerta |
|---|---|---|---|
| P1 | **Hecho** (2026-11-17): `propose --apply --author/--reason`, fecha UTC y sello en `.<raiz>.felix.log` bajo el lock; sin verificación de identidad. Procedencia mínima por cambio: autor (humano/agente), fecha, motivo y fuente, guardados junto al commit transaccional | M | No |
| P2 | **Hecho** (2026-11-18): cada apply guarda en el historial el delta canónico que aplicó (campo `delta`, hasta 256 KiB) y `universo show <raiz> <n>` lo muestra; `log` sigue sin datos del bundle. Historial consultable (`universo log`, `universo show <cambio>`) | M | No |
| P3 | **Hecho** (2026-11-18): `universo search <raiz> --manifiesto <m> <términos...>` devuelve las notas donde cada término empieza alguna palabra del título, tipo, atributos, valores de otros campos o cuerpo (sin distinguir mayúsculas ni tildes), ordenadas por un puntaje ponderado, con fragmento; texto o JSON; su salida incluye datos del bundle. Sólo palabras: sin embeddings, sin sinónimos, sin raíces. No está en el MCP. Original: Búsqueda léxica sobre etiquetas, atributos y cuerpos | M | No (pedida por el usuario el 2026-11-18 como búsqueda léxica; la semántica sigue en P5) |
| P4 | **Hecho** (2026-11-17): [`examples/memoria-documental`](../../examples/memoria-documental) con ID1–ID4 como reglas con nombre; test `crates/universo-cli/tests/memoria_documental.rs`. Plantilla de memoria documental (cap. 07) como manifiesto + invariantes de ejemplo, sin tocar el motor | S–M | No |
| P5 | Búsqueda semántica | L | Probablemente sí («recuperación avanzada») |
| P6 | Preorden ⪯ y revisión AGM | L | **Sí** |
| P7 | Recuperación por patrones | L | **Sí** |
| P8 | Evolución DPO/SqPO | L | **Sí** |

P1–P4 acercan el producto a «biblioteca de criterio» sin cruzar compuertas.
