# Protocolo prerregistrado de cuatro brazos (`felix-four-arms-v2`, contexto parcial)

Fijado el 2026-11-18, antes de cualquier corrida del estudio. Diseño y
decisiones: [propuesta 09](../docs/propuestas/09-estudio-cuatro-brazos.md) y
[propuesta 09b](../docs/propuestas/09b-contexto-parcial.md). El piloto 173
([protocolo](paired_methodology.md)) queda intacto y sus resultados no se
mezclan con estos.

**Versión 2 (contexto parcial).** La versión 1 daba a todos los brazos todos los
hechos; sus dos calibraciones (tareas v1 y v2) dieron 16 de 16 aciertos del
baseline (ver el registro de intentos). En la versión 2 cada brazo ve sólo los
hechos de radio 1 alrededor de los nodos que nombra el objetivo
(`visible_facts`, con una nota que lo dice), y en C el rechazo incluye un
**testigo localizado** (`propose --witness`): la primera asignación que incumple
la regla, con rutas del bundle. El oráculo sigue puntuando contra la base
completa. Los resultados van a `evaluation/results/four-arms-v2-<fase>/`.

## Qué se congela

[`four_arms/plan.json`](four_arms/plan.json) guarda el calendario completo (96
filas), la semilla de planificación 914, el alias de modelo, presupuestos,
textos de instrucción y el SHA-256 de cada fuente que decide el resultado:
tareas, claves del oráculo, generador, oráculo, runner, `run_paired.py` (de donde
se toman la captura acotada y el parseo), proxy MCP v2 y orquestador. Lo que el
plan no puede fijar porque depende de la máquina, como los SHA-256 de los
binarios `universo` y `universo-mcp`, la versión de Claude Code y el commit, se
escribe en `run.json` al empezar. Esos valores se **registran**, no se fijan de
antemano (los binarios `debug` no son reproducibles byte a byte), con tres
reglas:

- una reanudación con otros valores se rechaza
  (`run_manifest_changed_refusing_to_resume`), y cada reanudación deja en
  `run.json` el commit desde el que se hizo;
- el estudio 9c se niega a correr si sus binarios o su versión de Claude Code difieren
  de los de la calibración 9b;
- una corrida real con el árbol de trabajo sucio se rechaza, sin contar
  `evaluation/results/`.

Cada registro lleva `fase` y `plan_sha256`. `run_four_arms.py` se niega a correr si el plan no coincide con las
fuentes actuales (`plan_changed_refusing_to_run`), y un test lo comprueba en CI.
Cambiar cualquiera de esas fuentes obliga a regenerar el plan y a revisarlo de
nuevo antes de correr.

## Agente

El agente bajo estudio es Claude: Claude Code en modo no interactivo
(`claude -p --output-format stream-json`) con la suscripción de Claude del
usuario, dentro de bubblewrap. Su HOME es un temporal, así que no carga ajustes,
hooks, plugins, memoria ni CLAUDE.md del usuario (`--setting-sources ""`). No
tiene herramientas integradas (`--tools ""`): entrega el delta con la salida
estructurada (`--json-schema`), y en el brazo B dispone además de las cuatro
herramientas MCP de Felix por el proxy v2. El modelo es el
`configured_model_alias` del plan y el system prompt fijo queda en
`system_prompt`. El piloto 173 usó GPT vía Codex CLI; este estudio no usa
modelos de otro proveedor.

## Tareas

[`four_arms/tasks.json`](four_arms/tasks.json), **versión 3**, generadas por
[`four_arms_dataset.py`](four_arms_dataset.py) sobre el mismo bundle de la
versión 2: 24 nodos (14 decisiones, 4 fuentes, 6 personas), 53 aristas
(`revisa`, `propone`, `apoya`, `depende`), 200 hechos y siete reglas, entre ellas
revisores mínimos con `al-menos 2` (el proponente no cuenta) y dependencias
vigentes con cascadas de hasta cuatro pasos. Cada tarea declara los nodos que
nombra (`context_nodes`) y los hechos que verá el agente (`visible_facts`): los
de esos nodos y sus vecinos directos, más las aristas que los tocan.

| Familia | Tareas | Qué queda fuera de la vista |
|---|---|---|
| fuera_de_radio | `cascada_d02`, `retirar_revisor_d01`, `degradar_d01`, `verificar_dependientes` | la cascada sigue por decisiones que no se ven: `d04` en `cascada_d02` (se ve `d03`); `d03` en `retirar_revisor_d01` y `degradar_d01` (en la primera se ve `d04`, pero no su arista hacia `d03`); en `verificar_dependientes`, los revisores y proponentes de los dependientes |
| control | `degradar_f1`, `promover_d13`, `reasignar_d14`, `promover_d11` | nada: todo lo necesario es visible |

Todos los brazos reciben el mismo objetivo, los mismos hechos visibles, la nota
de contexto, las reglas en lenguaje natural y las invariantes `.sx`. El brazo B
recibe además una línea que anuncia las herramientas MCP; `context` le permite
ver más de la base. Nunca se entregan las claves del oráculo ni las respuestas
de referencia.

**Criterio de calibración (v2).** Se sigue con el estudio si el baseline falla al
menos una tarea fuera de radio y acierta las de control. Si acierta todo, se
revisa el diseño. Si falla las de control, las tareas están mal planteadas.

## Oráculo

[`four_arms_scoring.py`](four_arms_scoring.py) usa sólo la biblioteca estándar
y no importa ni llama a Felix. Informa las mismas propiedades que el piloto:
`format_valid`, `structural_valid`, `constraints_valid`, `goal_satisfied`,
`preserved` y `success`, más `valid_state`, que es la conjunción de las tres
primeras. `test_four_arms_dataset.py` comprueba referencias correctas con
edición mínima, respuestas plausibles pero erróneas con su severidad, formatos no
canónicos e identidades inventadas. Como preflight de las fixtures, y no como
scoring, también comprueba que los hechos coinciden con el bundle que carga Felix
y que oráculo y motor coinciden en la validez de 30 deltas de prueba. Igual que
el motor, el oráculo trata como no-op añadir un hecho que ya existe, y exige que
una arista nueva tome el siguiente índice libre de su (origen, destino, clave).

## Brazos y rondas

| Brazo | Rondas | Fin | Resultado final |
|---|---|---|---|
| A_baseline | 1 | — | la respuesta |
| B_felix_opcional | 1 (hasta 8 llamadas MCP de lectura, por proxy v2) | — | la respuesta |
| C_obligatorio | hasta 3 | `admit` del orquestador | lo publicado, o ningún cambio si escala |
| D_revision | hasta 3 | el agente repite su delta | la última respuesta |

En C, un rechazo vuelve con el id y el mensaje de la regla fallida y con el
testigo localizado: la instancia que la incumple, con rutas (por ejemplo
`e=n/d04→n/d03#depende#0, a=n/d04, b=n/d03`). Un error de forma
del delta vuelve con su código; sólo se devuelven los de una lista blanca
(`CODIGOS_DE_DELTA` del orquestador), y cualquier otro código escala. En D cada
ronda recibe la misma instrucción neutral, que no dice si la propuesta es válida.
En C y D el historial se acumula igual: cada propuesta previa, seguida de su
respuesta (veredicto de Felix o instrucción neutral). Cada ronda es una
invocación nueva del agente con ese historial en el prompt, con 120 s de límite.

**Fallo técnico.** La regla es la misma en los cuatro brazos. Es fallo técnico
cuando:

- alguna ronda no se completa (timeout, sin delta, límite de salida, MCP no
  disponible en B, una herramienta distinta de las cuatro de Felix);
- el runner falla;
- en C, el orquestador termina por algo distinto de publicar o de agotar los
  intentos (incluido `incierto`).

Esas filas se publican y se cuentan por brazo (`technical_failures`) y por
causa (`technical_failure_causes`), pero quedan fuera de los endpoints
primarios: su `score` es `null`. Una fila fallida no se repite al reanudar.

Un **error de entorno** (falta bubblewrap, el login de Claude o un binario; no se
puede lanzar el proceso) no es un fallo técnico del brazo. El runner lo comprueba
antes de la primera fila (`preflight`) y, si aparece durante una fila, aborta sin
escribirla, para que `--resume` la corra al arreglar el entorno. Un fallo del
proveedor (límite de uso o cuota, turno fallido sin respuesta del modelo) es
también un error de entorno.

**Registro de intentos.** El 2026-11-18 un primer intento de calibración falló
en sus 16 filas por límite de uso de la suscripción, sin respuesta del modelo.
Ese intento llevó a la regla anterior. Se conserva sin cambios en
[`results/four-arms-v1-calibracion-intento-1-sin-cuota/`](results/four-arms-v1-calibracion-intento-1-sin-cuota/README.md)
y no entra en ningún análisis.

El mismo día, con el agente ya cambiado a Claude (`claude-sonnet-5-5`, Claude
Code 2.1.327), la calibración se completó en
[`results/four-arms-v1-calibracion-tareas-v1/`](results/four-arms-v1-calibracion-tareas-v1/summary.json):
**16 de 16 aciertos del baseline al primer delta**, sin fallos técnicos y sin
estados inválidos, entre 3 y 7 s por corrida. Según este protocolo, las tareas
no dejan margen y deben reescribirse, con el plan congelado de nuevo, antes de
9c. Esas tareas (v1) no se usan en el estudio.

Con las tareas v2 (24 nodos, cascadas y proponente excluido) la calibración se
repitió el mismo día en
[`results/four-arms-v1-calibracion/`](results/four-arms-v1-calibracion/summary.json):
de nuevo **16 de 16 aciertos al primer delta**, sin fallos técnicos, entre 4 y 7 s
por corrida, incluidas las cascadas de cuatro pasos, el proponente que no cuenta
como revisor y la tarea imposible. Junto con el piloto 173 (12 de 12 con GPT),
es la tercera vez que un modelo actual resuelve todas las tareas cuando recibe
todos los hechos y las reglas en el prompt.

## Medidas

- **Primarias**: sobre las filas sin fallo técnico, `final_invalid_state` (el
  estado final que el oráculo considera inválido) y `final_success`. Se informan
  también como proporción de todas las filas del brazo, con los fallos técnicos
  a la vista.
- **Sensibilidad prerregistrada**: como la exposición a fallos técnicos difiere
  por brazo (C y D hacen hasta tres llamadas; B añade el MCP), se publica además
  un análisis sobre todas las filas en el que un fallo técnico cuenta como
  fracaso (`final_success_all_rows`) y, en el peor caso, como estado inválido
  (`final_invalid_state_worst_case`). Una conclusión que cambie entre los dos
  análisis se informa como no robusta.
- **Secundarias**: `first_delta_success`, `repaired` (primer delta incorrecto y
  final correcto), rondas, escalados, ejecuciones no completadas, tiempo de pared
  y uso que reporte el proveedor (`null` si no lo reporta).
- **Atribución**: si C supera a A en éxito final pero no en primer delta, el
  beneficio es del control del flujo. Si C no supera a D, no se atribuye al
  veredicto de Felix.
- Análisis descriptivo y pareado por tarea y repetición. Con ocho tareas no se
  afirma significancia general.

## Fases

1. **9a, preparación (este commit):** dataset, oráculo, runner, plan congelado y
   ensayo completo con `--agente guionizado` (96 filas, sin LLM).
2. **9b, calibración:** `--agente claude --fase calibracion`, que corre sólo el
   baseline con 2 repeticiones (16 corridas). Si el baseline acierta todo, las
   tareas se reescriben y el plan se vuelve a congelar **antes** de 9c. La
   calibración se publica aparte y no entra en el análisis.
3. **9c, estudio:** `--agente claude --fase estudio`, las 96 filas del plan. Se
   publican todas, incluidos timeouts y errores; no se amplía la muestra ni se
   repiten corridas desfavorables.

9b y 9c necesitan cada una una decisión explícita, Claude Code con la sesión de
la suscripción de Claude iniciada y Linux con bubblewrap. Con `--agente claude` el destino es fijo,
`evaluation/results/four-arms-v1-<fase>/`, para que no se pueda correr el
estudio en otro directorio y quedarse con la mejor corrida.

```sh
python evaluation/run_four_arms.py --agente guionizado --output /tmp/ensayo
python evaluation/run_four_arms.py --agente claude --fase calibracion
```

## Límites conocidos

- **Presupuesto de Felix.** En un bundle de este tamaño, cada regla sobre una
  arista y dos nodos recorre unas 30 000 combinaciones. `propose` reparte un
  presupuesto semántico agregado de 1 000 000 unidades entre todas las reglas, y
  `check` uno igual por regla. Con una regla de tipos por arista, `propose` no
  podía admitir ningún cambio que `check` sí aceptaba, así que los tipos van en
  una sola regla equivalente. La explicación redactada de un rechazo puede
  agotar también su propio presupuesto. En ese caso el orquestador identifica las
  regla fallida simulando el delta contra cada regla por separado
  (`fallback: per_rule`). Como la explicación `decide` normal, devuelve sólo la
  primera regla fallida en orden de fuente, con su id y su mensaje, para no
  darle a C más diagnóstico en unas tareas que en otras. En el
  brazo B, el `propose` del MCP devuelve el veredicto y la línea de la regla, sin
  esa ayuda.

- Un agente real puede ver en el prompt de C que Felix intervino. El brazo D lo
  controla sólo en parte: separa «tener otra oportunidad» de «recibir un
  veredicto», no el efecto de saber que hay un verificador.
- C termina en el primer `admit`, mientras que D termina cuando el agente repite
  su delta, así que el número de rondas puede diferir entre brazos. Se informa
  por brazo.
- Una sola fixture base: la generalización a otros dominios no se evalúa.
