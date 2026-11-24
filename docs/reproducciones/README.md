# Reproducciones con bundles ficticios

Casos mínimos que reproducen las fricciones de la
[propuesta 14](../propuestas/14-uso-diario-por-agentes.md). Salieron de usar el
motor `universo` 0.8.0 sobre un universo privado de unas 230 notas y 290
aristas. Aquí no hay nada de ese universo: todas las notas son `n/xNNNN`, con
títulos y cuerpos inventados, y las reglas son de juguete con la misma forma.

```sh
python docs/reproducciones/reproducir.py           # todos los casos
python docs/reproducciones/reproducir.py R01 R07   # algunos
```

Necesita `universo` y `universo-mcp` en el `PATH`. Escribe sólo en un temporal.
`generar.py` crea el universo ficticio con el tamaño pedido (notas de tipo `x`,
aristas `k` deterministas, atributos `marca` y `orden`).

## Casos

Resultados con `universo 0.8.0` (2026-11-24, Linux).

| Caso | Issue | Qué muestra | Resultado observado |
|---|---|---|---|
| R01 | I1 | `check --explain` de una regla trivial sobre aristas | 20 notas y 30 aristas: consume 528 400 de 1 000 000. 30 notas y 45 aristas: `explanation_budget_exceeded`, con `status: error` y `summary: null` aunque la regla se cumple |
| R02 | I2 | 8 nodos incumplen dos reglas | `check` da sólo conteos (`failed: 2`); `--witness` da un nodo (`n/x0001`); `--explain enumerate` da un objeto por regla y ninguno dice qué nodo. Sin publicar: `check --witness` da los 8 |
| R03 | I3 | Comparar `orden` de los dos extremos de una arista | `(>= Orden b a)` es `invariant_syntax`. Con `esquema` sobre el dominio: 20, 100 y 400 instancias, 6, 20 y 80 ms; el conteo de fallas cuenta instancias del esquema, no aristas |
| R04 | I4 | Ciclo `x0001 → x0002 → x0003 → x0001` | `(tc Edge k a a)` es `invariant_syntax` sin decir por qué; la regla desplegada a dos pasos lo admite |
| R05 | I5 | Ocho reglas con `(exists ((g E)) (Edge g b a j))` en el cuerpo | `check` admite en 31 ms; `propose` con delta vacío da `evaluation_budget_exhausted`. Con la arista como guarda (y un cuerpo de relleno `(= a a)`) admite en 3 ms |
| R06 | I6 | `mcp-config` en un universo suelto | `criterio_not_found`, aunque `universo-mcp --root … --manifiesto … --invariantes …` arranca |
| R07 | I7, I8 | Arranque y deuda previa | Tras `init`, agregar una nota da `seal: mismatch`; una regla con `@n/x0001` no deja sellar el bundle vacío (`seal` dice sólo `invalid_invariant_statement`); con dos nodos que ya fallan, `seal` se niega, `guard` falla siempre y un delta con una nota correcta se rechaza con testigo en `n/x0001` |
| R08 | — | Carga masiva por `delta` | 200 notas y 300 aristas: delta v2 de 53 KB, apply en 0,6 s, `guard: pass`. El camino existe; falta documentarlo como arranque |
| R10 | I10 | Dos `tools/call` seguidas al MCP | La segunda vuelve como error JSON-RPC `-32603 server_busy`, sin indicación de reintento |
| R11 | I11 | Error de sintaxis frente a rechazo | Los dos salen con código 1 |
| R12 | I12 | `escritura_agentes = false` en `vigente` | Con la CLI, un autor `agente` aplica reglas vacías a `vigente` y luego promueve un criterio que antes se rechazaba |

Los casos I9 (vistas generadas desde el bundle) e I13 a I16 (mensajes, tipado de
aristas, cardinalidad exacta, apply por MCP) se describen en la propuesta: son
funciones que faltan, no fallos que se puedan correr.

## Reportar un caso nuevo

Reduce el fallo a la orden, el código y la forma de la regla o del delta.
Reconstrúyelo con `generar.py` o con notas `n/x…` escritas a mano. Nunca copies
notas, títulos, rutas ni ids de regla de un universo real.
