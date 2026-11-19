# 07 — Medir corrección, utilidad y costo

El objetivo es sustituir la afirmación «ayuda a los agentes» por resultados
reproducibles, incluidos los resultados negativos y los límites encontrados.

## Estado

| Campo | Valor |
|---|---|
| Estado | completo |
| Depende de | 05, 06 |
| Desbloquea | 09 |

## Cierre verificado: evaluación neutral (2026-11-17)

El [protocolo pareado](../../evaluation/paired_methodology.md) y el
[plan de 24 ejecuciones](../../evaluation/results/pilot-173/plan.json) se fijan
antes de observar respuestas a tareas. Se seleccionan seis tareas sintéticas,
una por familia, con dos brazos y dos repeticiones. Dataset y oracle tienen
hashes separados; el oracle no se entrega al agente. El [piloto real completo](../../evaluation/paired-report.md) produjo 12/12
éxitos por brazo y cero llamadas a Felix: no demuestra mejora de corrección
ni reparación. La disponibilidad real se comprobó aparte y los diagnósticos
no se mezclan con las 24 tareas.

El [corpus y benchmark](../../evaluation/formal-and-performance.md) ya tienen
evidencia local: 46 casos etiquetados con coincidencia exacta, 14 variantes AST,
tres clases de conflicto y cero falsos admit. La matriz de 14 puntos × cinco
repeticiones conserva 65 admisiones y cinco agotamientos del presupuesto de
evaluación; estos últimos son resultados adversos, no admisiones lentas.
La CI del prerregistro `e076e74`, ejecución `44150586320`, pasó: Linux
1503/0/19 y Windows 1479/0/19, 46 suites por plataforma. El piloto autenticado
y las mediciones de rendimiento son evidencia local, no tareas ejecutadas en CI.
También pasaron localmente 19 pruebas Python de evaluación; el workflow actual
no las ejecuta.
El informe final `3f2cfe6adc63a731ab5f4de6531aec6a3599bb1f` pasó CI
`44152198690` en Linux y Windows. La fase se cierra porque la evaluación se
realizó y publicó, **no porque se haya demostrado beneficio**. El gate de
mejora sigue sin superarse; no se inicia AGM, recuperación avanzada ni fase 08.

## Tres evaluaciones

### Corrección formal

Corpus etiquetado que cubre cada operador, predicado y clase de violación. En
este corpus determinista se exige admisión o rechazo exacto, no una métrica
estadística aproximada.

### Utilidad para agentes

Evaluación pareada: mismo modelo, prompt, escenario y presupuesto; una rama
sin Felix y otra con Felix. Se registran propuestas inválidas, reparación,
iteraciones, contexto consumido y tiempo total.

### Costo operativo

Matriz de benchmarks por cantidad de nodos, aristas, invariantes, variables y
tamaño del delta. Se publican distribuciones y entorno de ejecución, sin fijar
promesas antes de medir.

## Criterios de aceptación

- [x] El corpus cubre todas las formas del AST y clases de veredicto.
- [x] No hay falsos `admit` en los casos etiquetados.
- [x] Los experimentos guardan prompts, semilla de planificación 173, alias configurado `gpt-6-astra` y Codex CLI `0.169.0`; snapshot inmutable del proveedor y semilla de muestreo no disponibles se registran como `null`.
- [x] La comparación con/sin Felix usa entradas idénticas.
- [x] El informe publica resultados favorables, neutros y adversos.
- [x] Los benchmarks pueden repetirse desde un clon limpio.

## Correspondencia de evidencia

- [Corpus etiquetado y pruebas exhaustivas de variantes](../../evaluation/formal-and-performance.md):
  46 casos y tres conflictos funcionales; etiquetas exactas, cero falsos admit.
- [Plan previo](../../evaluation/results/pilot-173/plan.json),
  [respuestas y prompts](../../evaluation/results/pilot-173/results.jsonl) y
  [resumen recalculable](../../evaluation/results/pilot-173/summary.json): 24
  registros, hashes idénticos de entrada en cada par, sin fallos omitidos.
- [Informe completo](../../evaluation/paired-report.md): resultados favorables,
  neutros y adversos; 12/12 éxitos por brazo, cero uso de Felix y ninguna mejora
  atribuida. El smoke separado confirma una llamada real accesible.
- [Matriz operativa](../../evaluation/benchmark-results.json): 70 muestras y
  fuentes identificadas por hash; cinco agotamientos de presupuesto conservados.
  Las recetas desde clon limpio están documentadas.

La CI cubre Rust/corpus y gates del repositorio. Las 19 pruebas Python, el piloto
real y los benchmarks son evidencia local Linux, revisada independientemente;
no se presentan como ejecutados en CI ni como resultados de agente en Windows.
Las semillas/versiones no observadas son una limitación explícita de reproducción
remota, no valores inventados para completar el checklist.

## Regla de decisión

Si Felix no mejora corrección o capacidad de reparación, no se avanza a
revisión AGM ni recuperación avanzada. Primero se explica el fallo del loop
mínimo y se revisa la hipótesis. Este piloto no mostró mejora: el modelo resolvió
todas las tareas sin usar las herramientas. La regla permanece activa después
del cierre de mediciones; no autoriza expandir esas funciones.

## Verificación reproducible

```powershell
cargo fetch --locked
python evaluation/run_formal.py --output evaluation/formal-results.json
python evaluation/benchmark_matrix.py --repetitions 5 --output evaluation/benchmark-results.json
python evaluation/run_paired.py --help
```

El piloto necesita una instalación autenticada de Codex CLI indicada mediante
`--codex-dir`; no se incluyen credenciales en el repositorio. Las opciones y
versiones efectivas se conservan en su plan, sin inventar snapshot del proveedor,
semilla de muestreo o costo monetario no observados.

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-11-17 | `e076e74`, CI `44150586320` | Corpus formal 46 casos + 3 conflictos; Rust Linux 1503/0/19 y Windows 1479/0/19, 46 suites. Matriz local de 70 muestras. | Gate aprobado; cinco agotamientos del presupuesto publicados. |
| 2026-11-17 | `3f2cfe6`, CI `44152198690` | 24 tareas reales, seis escenarios × dos brazos × dos repeticiones; oracle y hashes recalculados por revisión independiente. | 12/12 éxitos por brazo; cero llamadas Felix. No se demuestra beneficio ni se avanza a AGM. |

## Siguiente fase

[09 — Versionado y evidencia](09-versionado-y-evidencia.md), junto con la
evidencia de la fase 08.
