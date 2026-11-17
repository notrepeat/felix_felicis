# Piloto pareado real: disponibilidad de Felix

**No se demostró mejora de corrección ni reparación en este piloto.** Los dos
brazos resolvieron correctamente las doce ejecuciones asignadas a cada uno. El
modelo no utilizó las herramientas Felix en ninguna de las doce ejecuciones
con acceso a ellas. Se midió disponibilidad accesible y elección de no usarla;
no se midió el efecto de una verificación o reparación asistida efectiva.

Esto no demuestra inutilidad general de Felix. Las seis tareas pequeñas y
completamente especificadas produjeron un techo de éxito para este modelo. La
explicación de que sean demasiado fáciles es una hipótesis consistente con el
resultado, no una causa demostrada. Conforme a la regla del plan, **no se avanza
a revisión AGM ni recuperación avanzada** sobre esta evidencia.

## Diseño y trazabilidad

El prerregistro está publicado en `e076e74`, anterior a los resultados de tarea.
El [plan](results/pilot-173/plan.json) selecciona seis tareas sintéticas entre
las doce de `tasks.json`, una por familia, y fija 24 ejecuciones: dos brazos y
dos repeticiones. Se invierte el orden de los brazos entre repeticiones y se
mantienen todos los resultados. [Protocolo y oracle](paired_methodology.md).

El modelo configurado fue `gpt-6-astra`, esfuerzo `low`, mediante Codex CLI
`0.169.0` autenticado con una suscripción existente. El proveedor no entregó un
identificador inmutable de snapshot ni una semilla de muestreo; esos campos son
`null`. La semilla 173 corresponde a selección y planificación, no al muestreo
del modelo. Las entradas de cada par tienen exactamente el mismo hash de prompt.
El tratamiento es la presencia de cuatro herramientas MCP, no una diferencia
en los hechos, reglas u objetivo suministrados.

Cada brazo dispuso de 120 segundos y, cuando estaban disponibles, hasta ocho
llamadas MCP. Los 24 terminaron normalmente: no hay intentos omitidos, timeouts,
cambios retrospectivos de tarea ni ejecuciones sustituidas por mejores respuestas.
Los otros seis casos del dataset no se evaluaron y no acreditan generalización.

Después de los primeros cuatro registros sin llamadas se ejecutó un diagnóstico
separado de disponibilidad: se pidió explícitamente un `check`, el modelo lo
invocó y obtuvo `admit`. El [smoke de capacidad](results/pilot-173/capability-smoke.json)
está fuera de las 24 ejecuciones y de sus totales; no resolvió tareas ni cambió
el prompt, modelo, selección o presupuesto del piloto. Se publica únicamente
su evidencia sintética, sin metadatos privados de sesión.

## Corrección y utilidad de tarea

[Resultados individuales](results/pilot-173/results.jsonl) y
[resumen reproducible](results/pilot-173/summary.json).

| Medida | Sin Felix | Con Felix disponible |
|---|---:|---:|
| Ejecuciones completas | 12/12 | 12/12 |
| Éxito del objetivo y preservación, oracle independiente | 12/12 | 12/12 |
| Deltas finales inválidos estructuralmente o por reglas | 0 | 0 |
| Objetivos incumplidos | 0 | 0 |
| Regresiones en hechos ajenos | 0 | 0 |
| Ediciones extra sobre el mínimo de cada tarea | 0 | 0 |
| Llamadas MCP | 0 | 0 |
| Propuestas rechazadas observadas por MCP | no observable | 0 |
| Éxitos posteriores a rechazo MCP observado | no observable | 0 |

Los doce pares tienen éxito en ambos brazos. No hay pares donde sólo Felix tenga
éxito ni donde sólo el baseline lo tenga. Cada una de las seis tareas tuvo dos
respuestas correctas por brazo. El oracle Python evalúa intención, conservación,
estructura y reglas sin llamar al motor; una mera admisión de Felix no decide
éxito de tarea. Un estado válido con objetivo incumplido se puntuaría como fallo.

No hubo uso de herramientas ni rechazos que exigieran reparación en el piloto;
por tanto, cero reparaciones observadas no es una tasa de capacidad de reparación.
Tampoco se reconstruyen supuestas propuestas o razonamientos internos del brazo
sin Felix a partir de su respuesta final.

## Tiempo y consumo reportado

| Tiempo de pared por ejecución, segundos | Sin Felix | Con Felix disponible |
|---|---:|---:|
| Mínimo | 4.421 | 4.132 |
| Mediana | 5.297 | 5.112 |
| Máximo | 8.340 | 7.438 |

La mediana de las **diferencias pareadas** con−sin es −0.011 s. Seis pares son
más rápidos con Felix y seis más lentos. Es una observación descriptiva de un
host y servicio remoto, no evidencia de mejora de latencia. Las medianas por
brazo no sustituyen al cálculo pareado.

| Campo de uso reportado por CLI, suma de doce ejecuciones | Sin Felix | Con Felix disponible |
|---|---:|---:|
| `input_tokens` | 121996 | 122188 |
| `cached_input_tokens` | 32768 | 24576 |
| `cache_write_input_tokens` | 0 | 0 |
| `output_tokens` | 684 | 734 |
| `reasoning_output_tokens` | 48 | 94 |

Se conservan los campos tal como fueron reportados y **no se suman entre sí**
para fabricar un total o inferir el contrato de inclusión de subcampos. Hay
192 tokens de entrada reportados adicionales con Felix, 16 por par, aun sin
llamadas. Los valores incluyen el contexto efectivo del cliente, no sólo el
texto de la tarea; no son una medida aislada del costo de verificar invariantes.
No se atribuyen diferencias de salida o caché a una causa probada.

`cost_usd` permanece `null`: no se observó facturación por ejecución. Una
suscripción existente no implica costo marginal cero ni autoriza convertir
estos contadores a dinero con una tarifa de otro producto. El diagnóstico de
capacidad tiene su propio uso registrado y no se mezcla con los totales pareados.
Además, [los probes de preparación](results/pilot-173/setup-probes.json) registran
tres consultas exitosas de disponibilidad/aislamiento y dos intentos fallidos
de configuración, todos fuera de las 24 tareas. Los fallos no reportaron uso;
se conserva como desconocido, no cero. Los totales de la tabla no representan
el costo completo de preparar y diagnosticar la infraestructura.

## Incertidumbre y decisión

Son doce pares correspondientes a sólo seis tareas, con dos repeticiones
relacionadas por tarea. No se aplican intervalos binomiales ingenuos ni pruebas
de significancia que traten las 24 respuestas como tareas independientes. El
conjunto es pequeño, sintético, de tres nodos y con toda la información relevante
visible: no permite estimar utilidad sobre repositorios reales, modelos más
pequeños, tareas ambiguas ni situaciones donde el agente ya cometa errores.

El resultado favorable es completar las tareas sin regresiones en ambos brazos.
El resultado neutro es ausencia de mejora pareada; el resultado adverso para la
hipótesis de asistencia es falta de adopción espontánea de la herramienta en
este escenario. La prueba de capacidad descarta una indisponibilidad básica
como explicación de las cero llamadas, pero no explica por qué el modelo eligió
resolver directamente.

La decisión es conservar el resultado y revisar la hipótesis del loop mínimo
antes de expandir funciones avanzadas. No se cambian estos casos ni se obliga a
usar herramientas después de ver el resultado para producir una mejora aparente.
Cualquier estudio distinto requerirá otro protocolo y resultados separados.

## Trayectorias por ejecución (derivadas después)

`trajectory.py` reconstruye, para cada ejecución registrada, la adopción de
herramientas, el primer delta, los rechazos, la reparación y el estado final.
Puntúa con el oracle independiente cada delta que el agente envió a Felix y lo
compara con el veredicto de Felix. La comparación es solo sobre validez
(formato, estructura y restricciones), porque Felix no conoce el objetivo de la
tarea. También clasifica dónde aparece el error: `none`, `repaired`,
`persisted`, `introduced_later` o `no_delta`. No llama a modelos ni al motor y
no modifica los resultados.

En el piloto 173 la [derivación](results/pilot-173/trajectory.json) repite lo
que ya estaba publicado: cero adopción, primer delta igual al final y `none` en
las 24 ejecuciones. No es un análisis prerregistrado ni agrega evidencia de
beneficio. Su utilidad está en los estudios futuros. Tiene límites: no reparte
tokens ni latencia por llamada, porque el registro no los guarda, y en el brazo
sin Felix no se ve ninguna reparación interna.

## Reproducción

```sh
python -m unittest discover -s evaluation -p 'test_*.py'
python evaluation/verify_dataset.py --binary target/debug/universo
python evaluation/summarize_paired.py evaluation/results/pilot-173
python evaluation/trajectory.py evaluation/results/pilot-173
python evaluation/run_paired.py --help
```

Para una corrida nueva en Linux con bubblewrap, sustituye `/ruta/distribucion-codex`
por la distribución que contiene `bin/codex`, con una sesión existente del operador.
Usa un directorio de captura nuevo fuera del repositorio; no captures sobre los
resultados publicados ni publiques el ledger raw, que puede contener metadatos
privados de sesión del proveedor.

```sh
python evaluation/run_paired.py --codex-dir /ruta/distribucion-codex --model gpt-6-astra --seed 173 --timeout 120 --output /tmp/felix-new-study --plan-only
python evaluation/run_paired.py --codex-dir /ruta/distribucion-codex --model gpt-6-astra --seed 173 --timeout 120 --output /tmp/felix-new-study --pilot
python evaluation/run_paired.py --codex-dir /ruta/distribucion-codex --model gpt-6-astra --seed 173 --timeout 120 --output /tmp/felix-new-study --resume
python evaluation/export_public.py /tmp/felix-new-study/results.jsonl evaluation/results/new-study/results.jsonl
cp /tmp/felix-new-study/plan.json evaluation/results/new-study/plan.json
python evaluation/summarize_paired.py evaluation/results/new-study
```

`--pilot` ejecuta los cuatro primeros registros ya incluidos en el plan de 24;
`--resume` conserva esos registros y continúa los veinte restantes. Los fallos
se conservan y detienen la ejecución para diagnóstico, no se borran para cambiar
el resultado. El exportador aplica una lista permitida antes de cualquier
publicación; no convierte datos privados arbitrarios en datos sintéticos. La
receta usa exclusivamente las fixtures públicas de este estudio.

Los artefactos permiten repetir el scoring y el resumen exactamente. La
inferencia remota no se promete idéntica bit a bit por las limitaciones de
snapshot y semilla ya indicadas. El [corpus formal y matriz operativa](formal-and-performance.md)
son mediciones separadas del motor, no sustitutos de esta evaluación de agentes.

## Verificación del checkpoint

El checkpoint prerregistrado `e076e74` pasó CI `44150586320`: Linux 1503/0/19 y
Windows 1479/0/19, 46 suites por plataforma. La CI verifica Rust y el corpus
formal; **no ejecuta los scripts Python de `evaluation` ni inferencia autenticada
del piloto**. Localmente pasaron 19 pruebas Python: runner 5, scorer 8, resumen 4
y exportador 2. El piloto se ejecutó
localmente en Linux y sus 24 puntajes, hashes pareados y preservación del plan
fueron recalculados por revisión independiente. El exportador público elimina
metadatos de sesión mediante lista permitida y tiene pruebas específicas.

El informe publicado `3f2cfe6` pasó después CI `44152198690` en Linux y Windows
con el mismo alcance: Rust/corpus y gates del repositorio, no inferencia remota
ni scripts Python de evaluación. El cierre de fase 07 registra la evaluación
realizada y su resultado neutral; no supera el gate de beneficio para funciones
avanzadas.
