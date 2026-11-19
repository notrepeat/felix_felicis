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
| Estado | propuesto (2026-11-19), pendiente de tu revisión |
| Depende de | 10 tal como salió en `v0.4.0`; no requiere U10 (uso real), que sigue en privado |
| Desbloquea | `v0.5.0`; la decisión sobre capacidades avanzadas (fase 09) |

## Pregunta

Con el mismo modelo y la misma tarea, ¿un agente que puede consultar el
repositorio de criterio toma más decisiones acordes al criterio del dueño que
uno que no puede, sin inventar criterio que no existe?

No mide la detección de criterio en la conversación: eso ya lo mide U13
(`evaluation/run_criterio_deteccion.py`).

## Diseño propuesto

- **Universo ficticio.** Un repositorio de criterio creado con
  `universo init --git`, con unos 20 criterios vigentes y candidatos
  inventados (por ejemplo, «antes de crear un componente, buscar si existe»,
  «las fechas se guardan en UTC»), cada uno confirmado en proyectos
  ficticios. Nada sale de la biblioteca privada.
- **Tareas.** Unas 12 tareas de código pequeñas, cada una con un punto de
  decisión donde un criterio vigente indica qué hacer y la opción por defecto
  de un modelo es otra. Más 4 de control, donde ningún criterio aplica, para
  medir si el agente inventa uno.
- **Brazos.**
  1. **A, sin criterio.** El agente sólo recibe la tarea.
  2. **B, criterio por MCP.** El agente tiene `universo-mcp --criterio` y la
     instrucción de consultarlo antes de decidir. Todos los universos del
     repositorio de prueba llevan `escritura_agentes = false`, así que el MCP
     no anuncia `capture`, `reinforce` ni `object`.
  3. **C, criterio en el prompt.** El texto de los criterios vigentes va en el
     prompt. Es el techo barato: si C rinde igual que B, el MCP no aporta
     sobre pegar el criterio.
- **Medidas, por corrida:**
  - si la decisión sigue el criterio que aplica (sí, no o no aplica), según
    un oráculo fijado antes de correr;
  - en las tareas de control, si el agente cita o inventa un criterio
    inexistente;
  - el éxito de la tarea (sus tests);
  - las llamadas MCP, los tokens y el tiempo.
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
siguen el criterio en las 12 tareas con criterio (36 por brazo). Se compara
por tarea, pareando brazos, con un intervalo bootstrap al 95 % sobre las
tareas.

- **El criterio ayuda** si B supera a A en al menos 15 puntos de adherencia,
  el intervalo de la diferencia excluye el cero, y B no inventa más criterio
  que A en las tareas de control.
- **El MCP aporta sobre el prompt** si B no queda más de 5 puntos bajo C.
  Si C supera a B por más, la recomendación es pegar el criterio en el
  prompt, no consultarlo por MCP.
- Cualquier otro resultado es neutral y se publica igual. Con 36
  observaciones por brazo, diferencias menores no se distinguen del azar.

## Criterios de aceptación

- [ ] Plan, tareas, universo y oráculo publicados con hash antes de la
      primera corrida.
- [ ] Corridas de los tres brazos completas, o cortadas con la causa y lo ya
      medido conservado.
- [ ] Informe con la adherencia al criterio, el criterio inventado, el éxito
      y el costo por brazo, con intervalos, publicado aunque sea neutral.
- [ ] Benchmarks de las cuatro operaciones en los tres tamaños.
- [ ] Tests sin LLM del runner y del puntaje (oráculo y agente falso), en CI.

## Decisiones

| # | Decisión | Opciones | Recomendación |
|---|---|---|---|
| E1 | Correr con un LLM real | sí / sólo el diseño | sí, con tu OK explícito antes de la primera corrida |
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

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-11-19 | — | Plan propuesto | Pendiente de revisión |
