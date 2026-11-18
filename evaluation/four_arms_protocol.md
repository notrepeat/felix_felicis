# Protocolo prerregistrado de cuatro brazos (`felix-four-arms-v1`)

Fijado el 2026-11-18, antes de cualquier corrida con un modelo real. Diseño y
decisiones: [propuesta 09](../docs/propuestas/09-estudio-cuatro-brazos.md). El
piloto 173 ([protocolo](paired_methodology.md)) queda intacto y sus resultados
no se mezclan con estos.

## Qué se congela

[`four_arms/plan.json`](four_arms/plan.json) guarda el calendario completo (96
filas), la semilla de planificación 914, el alias de modelo, presupuestos,
textos de instrucción y el SHA-256 de cada fuente que decide el resultado:
tareas, claves del oráculo, generador, oráculo, runner, `run_paired.py` (de donde
se toman la captura acotada y el parseo), proxy MCP v2 y orquestador. Lo que el
plan no puede fijar porque depende de la máquina, como los SHA-256 de los
binarios `universo` y `universo-mcp`, la versión de Codex CLI y el commit, se
escribe en `run.json` al empezar. Esos valores se **registran**, no se fijan de
antemano (los binarios `debug` no son reproducibles byte a byte), con tres
reglas:

- una reanudación con otros valores se rechaza
  (`run_manifest_changed_refusing_to_resume`), y cada reanudación deja en
  `run.json` el commit desde el que se hizo;
- el estudio 9c se niega a correr si sus binarios o su versión de Codex difieren
  de los de la calibración 9b;
- una corrida real con el árbol de trabajo sucio se rechaza, sin contar
  `evaluation/results/`.

Cada registro lleva `fase` y `plan_sha256`. `run_four_arms.py` se niega a correr si el plan no coincide con las
fuentes actuales (`plan_changed_refusing_to_run`), y un test lo comprueba en CI.
Cambiar cualquiera de esas fuentes obliga a regenerar el plan y a revisarlo de
nuevo antes de correr.

## Tareas

[`four_arms/tasks.json`](four_arms/tasks.json), generadas por
[`four_arms_dataset.py`](four_arms_dataset.py): ocho tareas sintéticas sobre un
mismo bundle de nueve nodos (cuatro decisiones, dos fuentes y tres personas) con
aristas `revisa` y `apoya`, un atributo ordenado (Confianza), una fecha
(Vigencia) y seis reglas, una de ellas de cardinalidad (`al-menos 2`).

| Familia | Tareas | Trampa prevista |
|---|---|---|
| promover | `promover_d1`, `renovar_d2` | en `promover_d1`, cambiar sólo el estado deja una decisión vigente con un solo revisor; `renovar_d2` es de control (el objetivo da todos los datos; el error plausible es no retirar los valores viejos) |
| cascada | `degradar_f1`, `retirar_revisor` | el cambio pedido viola una regla en otro nodo; el objetivo dice cómo resolverlo |
| imposible | `apoyo_fuente_debil`, `apoyo_de_persona` | las reglas impiden lo pedido; lo correcto es no cambiar nada |
| conservacion | `asegurar_revisores`, `extender_d4` | ya se cumple, o un reemplazo de valores sin efectos laterales |

Todos los brazos reciben el mismo objetivo, los hechos, las reglas en lenguaje
natural y las invariantes `.sx`. El brazo B recibe además una línea que anuncia
las herramientas MCP. Nunca se entregan las claves del oráculo ni las respuestas
de referencia.

## Oráculo

[`four_arms_scoring.py`](four_arms_scoring.py) usa sólo la biblioteca estándar
y no importa ni llama a Felix. Informa las mismas propiedades que el piloto:
`format_valid`, `structural_valid`, `constraints_valid`, `goal_satisfied`,
`preserved` y `success`, más `valid_state`, que es la conjunción de las tres
primeras. `test_four_arms_dataset.py` comprueba referencias correctas con
edición mínima, respuestas plausibles pero erróneas con su severidad, formatos no
canónicos e identidades inventadas. Como preflight de las fixtures, y no como
scoring, también comprueba que los hechos coinciden con el bundle que carga Felix
y que oráculo y motor coinciden en la validez de 23 deltas de prueba. Igual que
el motor, el oráculo trata como no-op añadir un hecho que ya existe, y exige que
una arista nueva tome el siguiente índice libre de su (origen, destino, clave).

## Brazos y rondas

| Brazo | Rondas | Fin | Resultado final |
|---|---|---|---|
| A_baseline | 1 | — | la respuesta |
| B_felix_opcional | 1 (hasta 8 llamadas MCP de lectura, por proxy v2) | — | la respuesta |
| C_obligatorio | hasta 3 | `admit` del orquestador | lo publicado, o ningún cambio si escala |
| D_revision | hasta 3 | el agente repite su delta | la última respuesta |

En C, un rechazo vuelve con id y mensaje de cada regla fallida. Un error de forma
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

Un **error de entorno** (falta bubblewrap, el login de Codex o un binario; no se
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
2. **9b, calibración:** `--agente codex --fase calibracion`, que corre sólo el
   baseline con 2 repeticiones (16 corridas). Si el baseline acierta todo, las
   tareas se reescriben y el plan se vuelve a congelar **antes** de 9c. La
   calibración se publica aparte y no entra en el análisis.
3. **9c, estudio:** `--agente codex --fase estudio`, las 96 filas del plan. Se
   publican todas, incluidos timeouts y errores; no se amplía la muestra ni se
   repiten corridas desfavorables.

9b y 9c necesitan cada una una decisión explícita, una instalación autenticada de
Codex CLI y Linux con bubblewrap. Con `--agente codex` el destino es fijo,
`evaluation/results/four-arms-v1-<fase>/`, para que no se pueda correr el
estudio en otro directorio y quedarse con la mejor corrida.

```sh
python evaluation/run_four_arms.py --agente guionizado --output /tmp/ensayo
python evaluation/run_four_arms.py --agente codex --codex-dir <dir> --fase calibracion
```

## Límites conocidos

- Un agente real puede ver en el prompt de C que Felix intervino. El brazo D lo
  controla sólo en parte: separa «tener otra oportunidad» de «recibir un
  veredicto», no el efecto de saber que hay un verificador.
- C termina en el primer `admit`, mientras que D termina cuando el agente repite
  su delta, así que el número de rondas puede diferir entre brazos. Se informa
  por brazo.
- Una sola fixture base: la generalización a otros dominios no se evalúa.
