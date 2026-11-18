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
invariantes; no hay búsqueda léxica, por atributo ni semántica (reportado).

## Brechas

| # | Brecha | Tamaño | Compuerta |
|---|---|---|---|
| P1 | Procedencia mínima por cambio: autor (humano/agente), fecha, motivo y fuente, guardados junto al commit transaccional | M | No |
| P2 | Historial consultable (`universo log`, `universo show <cambio>`) | M | No |
| P3 | Búsqueda léxica sobre etiquetas, atributos y cuerpos | M | No (ambigua respecto de «recuperación avanzada»: confirmar) |
| P4 | Plantilla de memoria documental (cap. 07) como manifiesto + invariantes de ejemplo, sin tocar el motor | S–M | No |
| P5 | Búsqueda semántica | L | Probablemente sí («recuperación avanzada») |
| P6 | Preorden ⪯ y revisión AGM | L | **Sí** |
| P7 | Recuperación por patrones | L | **Sí** |
| P8 | Evolución DPO/SqPO | L | **Sí** |

P1–P4 acercan el producto a «biblioteca de criterio» sin cruzar compuertas.
