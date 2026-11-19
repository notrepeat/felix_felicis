# 11 — Medir si el criterio guardado cambia lo que hace el agente

La fase 07 y el estudio de cuatro brazos midieron si Felix evita que un agente
deje la base en un estado que viola las reglas. La fase 10 agregó otra cosa: un
repositorio con el **criterio** del dueño (decisiones, principios y
preferencias), consultable por MCP. Esta fase mide si consultar ese criterio
hace que el agente trabaje como el dueño quiere, y cuánto cuesta. Es el gate
`v0.5.0` de la [fase 09](09-versionado-y-evidencia.md) («evaluación
comparativa y benchmarks publicados»), movido desde `v0.4.0` por D5 de la
[fase 10](10-criterio-distribuible.md).

## Estado

| Campo | Valor |
|---|---|
| Estado | completo (2026-11-19): 144 corridas y [informe](../../evaluation/criterio_uso_report.md) publicados; resultado **neutral** por la regla (B − A = +0,22, IC95 [0,00; 0,47]); el MCP rinde igual que el prompt |
| Depende de | 10 tal como salió en `v0.4.0`; no requiere U10 (uso real), que sigue en privado |
| Desbloquea | `v0.5.0`; la decisión sobre capacidades avanzadas (fase 09) |

## Pregunta

Con el mismo modelo y la misma tarea, ¿un agente que puede consultar el
repositorio de criterio toma más decisiones acordes al criterio del dueño que
uno que no puede, sin inventar criterio que no existe?

No mide la detección de criterio en la conversación: eso ya lo mide U13
(`evaluation/run_criterio_deteccion.py`).

## Diseño propuesto

- **Universo ficticio.** `evaluation/criterio_uso/criterios.json`: 18
  criterios vigentes (12 que aplican a una tarea y 6 distractores) y 6
  candidatas, en cinco proyectos ficticios. El runner lo construye con la CLI
  real: captura cada vigente en un proyecto, lo refuerza en otro, lo promueve y
  captura las candidatas. Nada sale de la biblioteca privada.
- **Tareas.** 16 decisiones de código (`evaluation/criterio_uso/tasks.json`),
  cada una con un escenario corto y dos o tres opciones neutrales. En 12, un
  criterio vigente distinto indica cuál elegir, y la respuesta correcta queda
  repartida 4/4/4 entre `a`, `b` y `c`. En 4 de control ningún criterio
  aplica. El agente devuelve `opcion`, `justificacion` y `criterio_aplicado`
  (o `null`) con salida estructurada. Se puntúa contra el oráculo sin
  ejecutar código.
- **Brazos.**
  1. **A, sin criterio.** El agente sólo recibe la tarea.
  2. **B, criterio por MCP.** El agente tiene `universo-mcp --criterio`, sólo
     con `search` y `context` y un máximo de 6 llamadas, y la instrucción de
     consultarlo antes de decidir. Todos los universos del
     repositorio de prueba llevan `escritura_agentes = false`, así que el MCP
     no anuncia `capture`, `reinforce` ni `object`.
  3. **C, criterio en el prompt.** El texto de los criterios vigentes va en el
     prompt. Es el techo barato: si C rinde igual que B, el MCP no aporta
     sobre pegar el criterio.
- **Medidas, por corrida:**
  - si la decisión sigue el criterio que aplica (sí, no o no aplica), según
    un oráculo fijado antes de correr;
  - en las tareas de control, si el agente cita un criterio. Ninguno aplica
    allí, así que cualquier cita, exista o no en el repositorio, es criterio
    aplicado donde no corresponde;
  - las respuestas que faltan (timeout o fallo), que se informan aparte;
  - las llamadas MCP, los tokens y el tiempo;
  - los intentos de usar herramientas no permitidas. El MCP también anuncia
    `propose`, `check` y `why`; se quitan del contexto con `--disallowedTools`,
    y un intento se registra sin anular la fila, para no penalizar sólo a B.
- **Protocolo.** Como en la fase 07 y el estudio de cuatro brazos:
  - el plan, las tareas y el oráculo se fijan con hash antes de observar
    respuestas;
  - tres repeticiones por tarea y brazo, como en los cuatro brazos (la fase
    07 usó dos);
  - el resultado se publica aunque sea neutral o negativo;
  - el oráculo no se entrega al agente.
- **Benchmarks del repositorio.** El tiempo de `search --universo todos`,
  `resumen`, `capturar` y `promover` con 100, 1 000 y 10 000 notas, para saber
  hasta dónde escala el uso diario.

## Regla de decisión

Se fija antes de correr. La adherencia es la proporción de decisiones que
siguen el criterio en las 12 tareas con criterio (36 por brazo). Las
comparaciones son pareadas: por tarea, el promedio de sus repeticiones en
cada brazo, sólo con tareas respondidas en ambos, y un intervalo bootstrap al
95 % que remuestrea tareas (10 000 muestras, semilla fija).

- **El criterio ayuda** si B supera a A en al menos 15 puntos de adherencia,
  el intervalo de la diferencia excluye el cero, y la tasa de criterio citado
  en las tareas de control de B no supera la de A.
- **El MCP aporta sobre el prompt** si la diferencia pareada B − C no baja de
  −5 puntos.
  Si C supera a B por más, la recomendación es pegar el criterio en el
  prompt, no consultarlo por MCP.
- Cualquier otro resultado es neutral y se publica igual. Con 36
  observaciones por brazo, diferencias menores no se distinguen del azar.

## Criterios de aceptación

- [x] Plan, tareas, universo y oráculo publicados con hash antes de la
      primera corrida (`evaluation/criterio_uso/plan.json`, 144 filas).
- [x] Corridas de los tres brazos completas, o cortadas con la causa y lo ya
      medido conservado.
- [x] Informe con la adherencia al criterio, el criterio citado en control,
      las respuestas que faltan y el costo por brazo, con intervalos,
      publicado aunque sea neutral.
- [x] Benchmarks de las cuatro operaciones en los tamaños previstos
      (`evaluation/results/bench-criterio.json`; ver Hallazgos).
- [x] Tests sin LLM del runner y del puntaje (oráculo y agente guionizado),
      en CI: `evaluation/test_criterio_uso.py` y
      `evaluation/test_bench_criterio.py`.

## Decisiones

| # | Decisión | Opciones | Recomendación |
|---|---|---|---|
| E1 | Correr con un LLM real | sí / sólo el diseño | sí, con tu OK explícito antes de la primera corrida (dado el 2026-11-19) |
| E2 | Modelo | uno / dos | `claude-sonnet-5-5`, el mismo de U13; un segundo modelo sólo si el resultado lo pide |
| E3 | Brazo C (criterio en el prompt) | incluir / omitir | incluir: sin él no se sabe si el MCP aporta |
| E4 | Agente | Claude Code aislado con bwrap / API directa | Claude Code aislado, como U13 y los cuatro brazos |
| E5 | Tamaño | 16 tareas × 3 brazos × 3 repeticiones (144 corridas) / la mitad | 144: con menos, una corrida mueve demasiado el resultado |

## Riesgos

- **Tareas a la medida.** Si las tareas se diseñan para que el criterio
  gane, el resultado no dice nada. Mitigación: cada tarea se fija antes de
  correr, y el brazo C separa el valor del criterio del valor del MCP.
- **El modelo ya sigue buenas prácticas.** Si el criterio coincide con lo que
  el modelo haría igual, no hay diferencia que medir. Mitigación: los puntos
  de decisión eligen preferencias defendibles pero no por defecto.
- **Costo.** Unas 144 corridas en tu suscripción. Un corte por cuota conserva
  lo medido.

## Fuera de alcance

- Medir con tu criterio real: eso es U10, en privado.
- Escritura de agentes (`capture`) durante las tareas.
- Modelos de otros proveedores.

## Hallazgos de los benchmarks (2026-11-19)

Mediana de tres repeticiones, binario `release` 0.4.0 en el commit
`f976bc8` (con el arreglo de abajo), Linux x86_64, notas en `candidatos`
(`evaluation/results/bench-criterio.json`):

| Notas | `search --universo todos` | `resumen` | `capturar` | `promover` |
|---|---|---|---|---|
| 100 | 0,003 s | 0,004 s | 0,28 s | 0,31 s |
| 1 000 | 0,018 s | 0,039 s | 2,6 s | 3,1 s |
| 5 000 | 0,10 s | 0,25 s | 16 s | 17 s |
| 10 000 | — | — | — | — (`seal` agota el presupuesto) |

1. **Costo cuadrático de una guarda con extremo ligado (corregido).** En
   `forall x … (al-menos 1 ((e E) (y N)) :guarda (Edge e x y k) …)`, la
   restricción de rango por aristas recorría todas las aristas por cada `x`.
   Con las plantillas de criterio, entre 500 y 1 000 notas (medición manual
   con el mismo generador, antes del arreglo) bastaban para agotar el
   presupuesto de evaluación: desde ahí, ninguna escritura entraba. Ahora, si
   un extremo ya está fijado, se recorren sólo sus aristas salientes o
   entrantes (`crates/universo-core/src/evaluador.rs`; regresión en
   `crates/universo-core/tests/rango_aristas_ligado.rs` y casos nuevos en el
   test de propiedades contra el oráculo).
2. **Tope por presupuesto.** Con el arreglo, el costo de las reglas es
   lineal, pero el presupuesto fijo de 1 000 000 de unidades se agota entre
   5 859 y 5 937 notas en un universo (búsqueda binaria manual con el mismo
   generador; no está en el JSON). Sobra para un repositorio personal; para
   más, habría que hacer configurable el presupuesto o abaratar las reglas.
3. **`capturar` y `promover` crecen con el tamaño del universo.** Simular una
   captura con 1 000 notas tarda 0,1 s; aplicarla, 3,4 s (medición manual). El apply de la fase
   04 escribe el bundle completo en un directorio de staging y hace `fsync`
   de cada archivo antes de publicarlo de forma atómica: unos 3 ms por nota en
   este equipo. Es el precio de su garantía de atomicidad; reducirlo (por
   ejemplo, staging sólo de lo que cambia) toca el núcleo transaccional y
   queda como trabajo aparte: propuesta en la
   [fase 12](12-apply-incremental.md).

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-11-19 | — | Plan propuesto | Aprobado tal como se recomendó (E2 a E5) |
| 2026-11-19 | — | Dataset, runner, prerregistro (`plan.json`, 144 filas) y benchmarks | Benchmarks publicados; guarda cuadrática corregida en el evaluador |
| 2026-11-19 | `08caf1a` | 144 corridas con `claude-sonnet-5-5`, sin cortes ni filas sin respuesta ([informe](../../evaluation/criterio_uso_report.md)) | Adherencia A 28/36, B 36/36, C 36/36; cero criterio citado en control. Neutral por la regla: el IC95 de B − A toca el cero porque sólo 3 de 12 tareas separaron a los brazos. El MCP aporta sobre el prompt (B = C), a unas 3 veces más tokens |

## Cómo correr

```sh
# Sin LLM: recorre el plan con respuestas deterministas.
python3 evaluation/run_criterio_uso.py --agente guionizado
# Con Claude (144 llamadas, requiere tu OK, E1):
cargo build -j 2 -p universo-cli -p universo-mcp
python3 evaluation/run_criterio_uso.py --agente claude
# Benchmarks del repositorio:
cargo build --release -j 2 -p universo-cli
python3 evaluation/bench_criterio.py
```

El runner se niega a correr con Claude si `plan.json` no coincide con las
fuentes actuales. Un corte por cuota conserva lo medido; al volver a correr se
completan las filas que faltan.
