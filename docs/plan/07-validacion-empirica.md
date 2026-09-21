# 07 — Medir corrección, utilidad y costo

El objetivo es sustituir la afirmación «ayuda a los agentes» por resultados
reproducibles, incluidos los resultados negativos y los límites encontrados.

## Estado

| Campo | Valor |
|---|---|
| Estado | pendiente |
| Depende de | 05, 06 |
| Desbloquea | 09 |

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

## Verificación prevista

```powershell
cargo bench --workspace
python evaluation/run_paired.py
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| — | — | — | — |

## Siguiente fase

[09 — Versionado y evidencia](09-versionado-y-evidencia.md), junto con la
evidencia de la fase 08.
