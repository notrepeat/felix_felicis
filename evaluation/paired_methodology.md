# Protocolo pareado de utilidad, fase 07

Este protocolo se fija antes de ejecutar el agente. No sustituye al corpus de
corrección formal ni al benchmark operativo. Se publicarán resultados favorables,
neutros y adversos, incluidos timeouts, salidas no parseables y ausencia de mejora.

## Datos y selección

`tasks.json` contiene doce tareas enteramente sintéticas en seis familias:
cambio de tipo, etiquetas, parentesco, atributos de extensión, cambios compuestos
y conservación sin cambios necesarios. Cada tarea tiene tres nodos y presenta
el mismo estado canónico, manifiesto, invariantes, reglas y objetivo a ambos
brazos. No se copian datos privados. Las extensiones canónicas se escriben como
`Maturity(n/a, draft)` o `Owner(n/a, red)`, no como `Ext(...)`.

Hashes de los archivos completos congelados antes de resultados (el campo
`dataset_sha256` del plan usa además una serialización JSON canónica y puede
diferir; `source_sha256` conserva estos hashes de bytes):

- `tasks.json`: `b42e1b07beef427da365680d1a4c1709cc9fc357739a3f38cebf3339025e9bc0`.
- `oracle.json`: `6a6c6e744f9c5dffe2c5ea3bed2cf18d26be34c3b5813f1d63aec6430b978ee2`.

El piloto principal usa una tarea por familia: selección
estratificada con semilla 173, sin mirar resultados. Son `release_c`,
`owner_copy`, `label_swap`, `noop_owner`, `parent_rotate` y `type_a`, según
`results/pilot-173/plan.json`, escrito antes de las corridas. Dos
repeticiones y dos brazos producen **24 ejecuciones, 12 pares**. La semilla de
planificación 173 determina selección y orden de ejecución, no aleatoriedad del proveedor.
Los dos primeros pares sirven también de comprobación de disponibilidad y se
conservan en el reporte; no se reemplazan por intentos más favorables.

Las otras seis tareas quedan sin evaluar en este piloto, no se denominan una
prueba ciega ni acreditan generalización. No se amplía la muestra por observar
un resultado negativo. Si cambia el modelo o el proveedor, se registra una nueva
corrida y no se mezclan sus resultados silenciosamente.

## Comparabilidad y aislamiento

El mismo modelo identificado, prompt de tarea, información inicial y presupuesto
se utiliza en ambos brazos. El brazo con Felix puede consultar las cuatro
herramientas de lectura/simulación MCP; no recibe hechos o reglas omitidos al
brazo sin Felix. El tratamiento es acceso a verificación y explicación, no acceso
privilegiado a la respuesta. No se concede escritura del bundle.

`oracle.json`, este generador y el código de scoring sólo pertenecen al evaluador.
No se montan ni se incluyen en el directorio de trabajo o herramientas del agente.
El sandbox temporal debe contener únicamente entradas sintéticas autorizadas.
Que las respuestas estén publicadas en el repositorio no permite entregarlas al
modelo durante la medición. El runner registra los prompts efectivos y controles
aplicados; no basta con pedirle al modelo que no lea el oracle.

El presupuesto predefinido es 120 segundos por brazo y ocho llamadas MCP como
máximo cuando están disponibles. Los presupuestos y herramientas efectivos se
registran por corrida. La ausencia de un parámetro de semilla del proveedor se
reporta como tal; no se presenta una semilla de planificación como semilla de
muestreo del modelo. No se afirma reproducibilidad bit a bit de una inferencia
remota aunque los artefactos experimentales sean reproducibles.

## Oráculo independiente

`scoring.py` usa exclusivamente la biblioteca estándar Python. No importa el
motor Felix ni llama su CLI para decidir éxito. Aplica el delta a un conjunto de
hechos con reglas específicas de este corpus: sintaxis, cardinalidad, dominio,
aciclicidad de Parent y condición de negocio ready→accepted. Comprueba además
el objetivo pedido y la conservación de hechos fuera de las claves autorizadas.

Se informa por separado:

- `format_valid`: contrato delta, átomos, duplicados, solapamientos y retiros existentes.
- `structural_valid`: identidad conocida, tipos/títulos/atributos únicos y jerarquía acíclica.
- `constraints_valid`: condición de negocio independiente.
- `goal_satisfied`: hechos requeridos y ausencias requeridas por la tarea.
- `preserved`: ausencia de cambios ajenos al objetivo.
- `success`: conjunción de las cinco propiedades anteriores.
- `edits`, `minimum_edits`, `extra_edits` y `unrelated_changes`: tamaño y desviación
  del cambio mínimo definido para esta tarea, sin premiar respuestas incorrectas.
- `severity`: no evaluable, violación estructural/de negocio, regresión, objetivo
  incumplido o ninguna. No representa daño real: el dataset es sintético.

Un delta formalmente válido puede incumplir el objetivo: **admitirlo no es un
falso admit formal de Felix**, porque el motor no recibe el objetivo como una
invariante. Esta distinción evita que la utilidad se reduzca tautológicamente a
satisfacer las invariantes del propio sistema. También evita atribuir al motor
un detector general de intención que no implementa.

`test_scoring.py` verifica explícitamente estado válido con objetivo incumplido,
objetivo logrado con regresión ajena, conflicto de tipo, ciclo y violación de
madurez. `verify_dataset.py` comprueba por separado consistencia de las fixtures:
los hechos presentados coinciden con el bundle cargado, la base es admisible y
existe un objetivo correcto admitido. Ese preflight no decide el resultado del
agente y sus deltas de referencia no se entregan al modelo.

## Informe previsto

Se conservan todos los pares y se publican conteos de éxito, fallos de formato,
seguridad del estado, objetivo y regresiones por brazo/familia/repetición. La
diferencia pareada distingue ambos correctos, sólo Felix correcto, sólo baseline
correcto y ambos incorrectos. En doce pares pequeños y correlacionados por tarea
las diferencias son descriptivas, no una prueba de superioridad general.

Para el brazo Felix se registra el historial de consultas, propuestas rechazadas
y reparaciones hasta la respuesta final. No se inventa una trayectoria interna
para el baseline si su interfaz sólo entrega una salida final. Iteraciones de
herramienta, tiempo de pared y contexto/tokens realmente reportados por el
proveedor se mantienen separados. Dinero y consumo no observado se registran
como no disponibles; una suscripción autenticada no implica coste marginal cero.

No se compara Code Mode contra MCP aquí: ambos son interfaces de fase 06. La
pregunta de este piloto es si el acceso a Felix mejora la tarea del mismo agente.
La regla del plan se mantiene: si no mejora corrección o reparación, no se avanza
a AGM ni recuperación avanzada; primero se explica el resultado y se revisa la
hipótesis.
