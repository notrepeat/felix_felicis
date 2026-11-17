# 07 — Medir corrección, utilidad y costo

El objetivo es sustituir la afirmación «ayuda a los agentes» por resultados
reproducibles, incluidos los resultados negativos y los límites encontrados.

## Estado

| Campo | Valor |
|---|---|
| Estado | en curso |
| Depende de | 05, 06 |
| Desbloquea | 09 |

## Corte de trabajo: protocolo fijado antes del piloto

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
El cierre documental espera la publicación y gate del informe final.

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

- [ ] El corpus cubre todas las formas del AST y clases de veredicto.
- [ ] No hay falsos `admit` en los casos etiquetados.
- [ ] Los experimentos guardan prompts, semillas y versiones de modelo.
- [ ] La comparación con/sin Felix usa entradas idénticas.
- [ ] El informe publica resultados favorables, neutros y adversos.
- [ ] Los benchmarks pueden repetirse desde un clon limpio.

## Regla de decisión

Si Felix no mejora corrección o capacidad de reparación, no se avanza a
revisión AGM ni recuperación avanzada. Primero se explica el fallo del loop
mínimo y se revisa la hipótesis.

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
| 2026-11-17 | Informe final, commit pendiente | 24 tareas reales, seis escenarios × dos brazos × dos repeticiones; oracle y hashes recalculados por revisión independiente. | 12/12 éxitos por brazo; cero llamadas Felix. No se demuestra beneficio ni se avanza a AGM. |

## Siguiente fase

[09 — Versionado y evidencia](09-versionado-y-evidencia.md), junto con la
evidencia de la fase 08.
