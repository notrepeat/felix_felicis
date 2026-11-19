# Informe de la evaluación del criterio (`felix-criterio-uso-v1`, fase 11)

Corrida del 2026-11-19 con `claude-sonnet-5-5` (Claude Code aislado con
bubblewrap). Se ejecutaron las 144 filas del plan prerregistrado
(`evaluation/criterio_uso/plan.json`, huella `3999a8b9330c…`) sobre el commit
`08caf1a`, con `universo 0.4.0`. No hubo cortes, filas sin respuesta ni intentos
de usar herramientas no permitidas. Las filas están en
`evaluation/results/criterio-uso-claude/runs.jsonl` y el resumen en
`summary.json`. Antes del plan se hizo una sola llamada de comprobación de
entorno del brazo B, fuera del plan y no contada aquí.

## Resultado principal

| Brazo | Adherencia al criterio | Criterio citado en control | Tiempo mediano | Tokens medianos (con caché) | Llamadas MCP (mediana; máx.) |
|---|---|---|---|---|---|
| A. Sin criterio | 28 / 36 (0,78) | 0 / 12 | 3,4 s | 1 620 | — |
| B. Criterio por MCP | **36 / 36** (1,00) | 0 / 12 | 6,3 s | 6 789 | 3; 6 |
| C. Criterio en el prompt | **36 / 36** (1,00) | 0 / 12 | 3,3 s | 2 111 | — |

Las diferencias son pareadas por tarea, con intervalo bootstrap al 95 % que
remuestrea las 12 tareas:

| Comparación | Diferencia | IC95 |
|---|---|---|
| B − A | +0,22 | [0,00; 0,47] |
| C − A | +0,22 | [0,00; 0,47] |
| B − C | 0,00 | [0,00; 0,00] |

## Lectura según la regla prerregistrada

- **«El criterio ayuda»: no se cumple.** B supera a A en 22 puntos, más que
  los 15 exigidos, y no cita criterio en las tareas de control. Pero el
  intervalo de la diferencia no excluye el cero: su límite inferior es
  exactamente 0. El resultado es **neutral** por la regla, aunque la dirección
  es clara.
- **«El MCP aporta sobre el prompt»: se cumple.** B rinde igual que C.

## Por qué el intervalo toca el cero

A eligió por su cuenta la opción del criterio en 9 de las 12 tareas. Sólo tres
tareas separaron a los brazos:

| Tarea | Criterio | A | B | C |
|---|---|---|---|---|
| `t03` formato de fecha | fechas en UTC con `Z` | 0 / 3 (eligió hora local con offset) | 3 / 3 | 3 / 3 |
| `t07` tipo del precio | montos como enteros en la unidad mínima | 0 / 3 (eligió `NUMERIC(12,2)`) | 3 / 3 | 3 / 3 |
| `t05` ubicación de tests | tests junto al código | 1 / 3 | 3 / 3 | 3 / 3 |

Al remuestrear 12 tareas, la probabilidad de no tomar ninguna de esas tres es
(9/12)¹² ≈ 3 %, por encima del 2,5 % del límite inferior. Por eso el
intervalo llega a 0. Es el riesgo que el plan nombró, «el modelo ya sigue
buenas prácticas»: la mayoría de los criterios ficticios coincidían con lo que
el modelo elige sin ayuda. Con este diseño, la evaluación tenía poca potencia.

## Lo que sí muestra

- **Consultado por MCP, el criterio se encuentra y se aplica.** En las 36
  decisiones con criterio, B eligió lo que el criterio indica. En 32 citó el
  criterio vigente correcto palabra por palabra, a partir de búsquedas por
  palabras en los títulos (`search`, mediana de 3 llamadas).
- **No se inventó criterio.** Ningún brazo citó un criterio en las tareas de
  control. En `k01` (color del botón), B eligió azul dos veces y verde una,
  sin citar nada.
- **El costo del MCP es real.** Frente al criterio en el prompt, B usó unas 3
  veces más tokens y el doble de tiempo, para la misma adherencia. Con 18
  criterios, pegarlos en el prompt es más barato. El MCP se justifica cuando
  el criterio no cabe o no conviene ponerlo entero en el contexto, algo que
  este estudio no midió.

## Lo que el estudio no muestra

- No mide un repositorio grande, donde pegar todo el criterio en el prompt
  deja de ser viable. Con 18 criterios, C es el techo barato.
- Las tareas son decisiones de opción múltiple, no cambios de código reales;
  el agente no tuvo que descubrir que había una decisión.
- Un modelo, tres repeticiones, criterio y tareas escritos por la misma mano
  que la skill y el runner.

## Qué sigue

Una segunda versión debería tener más potencia y decidirse antes de correrla.
Dos opciones:

1. Elegir criterios que el modelo no sigue por defecto, verificándolo antes
   con una calibración del brazo A.
2. Medir un repositorio con cientos de criterios, donde C deja de ser viable.

Ninguna de las dos se ha corrido.
