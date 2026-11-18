# Informe del estudio de cuatro brazos con contexto parcial (`felix-four-arms-v2`)

Fecha: 2026-11-18. Protocolo prerregistrado:
[`four_arms_protocol.md`](four_arms_protocol.md). Datos crudos:
[`results/four-arms-v2-estudio/`](results/four-arms-v2-estudio/) (96 filas,
`run.json` con commit, binarios y versión de Claude Code). Agente:
`claude-sonnet-5-5` vía Claude Code 2.1.327, con la suscripción de Claude.

Se corrieron las 96 filas del plan congelado y se publican todas. No hubo
fallos técnicos, así que el análisis sobre todas las filas y el de sensibilidad
coinciden.

## Resultado principal

| Brazo | Éxito final | Estado final inválido | Éxito del primer delta | Reparadas | Rondas | Tiempo medio | Tokens medios |
|---|---|---|---|---|---|---|---|
| A. Baseline | 14 / 24 | **9** | 14 | 0 | 24 | 5,5 s | 4 468 |
| B. Felix opcional (MCP) | 12 / 24 | **9** | 12 | 0 | 24 | 5,0 s | 5 077 |
| C. Verificación obligatoria + testigo | **21 / 24** | **0** | 12 | **9** | 39 | 10,0 s | 7 680 |
| D. Revisión neutral | 12 / 24 | **9** | 13 | 0 | 49 | 12,1 s | 9 494 |

Por familia (éxitos / estados inválidos sobre 12 corridas cada una):

| Brazo | Control (todo visible) | Fuera de radio |
|---|---|---|
| A | 12 / 0 | 2 / 9 |
| B | 12 / 0 | 0 / 9 |
| C | 12 / 0 | **9 / 0** |
| D | 12 / 0 | 0 / 9 |

Por tarea (`o` éxito, `I` estado inválido, `x` fallo con estado válido; una letra
por repetición):

| Tarea | Familia | A | B | C | D |
|---|---|---|---|---|---|
| `cascada_d02` | fuera de radio | III | III | ooo | III |
| `retirar_revisor_d01` | fuera de radio | III | III | ooo | III |
| `degradar_d01` | fuera de radio | III | III | ooo | III |
| `verificar_dependientes` | fuera de radio | oxo | xxx | xxx | xxx |
| `degradar_f1` | control | ooo | ooo | ooo | ooo |
| `promover_d13` | control | ooo | ooo | ooo | ooo |
| `reasignar_d14` | control | ooo | ooo | ooo | ooo |
| `promover_d11` | control | ooo | ooo | ooo | ooo |

## Lectura según las reglas de atribución prerregistradas

1. **Seguridad.** Con verificación obligatoria ningún estado inválido llegó al
   final (0 de 24). En los otros tres brazos llegaron 9 de 24, todos en las
   cascadas que el agente no podía ver.
2. **Utilidad.** C resolvió las 9 cascadas fuera de radio, A, B y D ninguna. En
   las tareas de control los cuatro brazos acertaron todo: C no empeoró lo que
   ya se resolvía.
3. **Es el flujo, no un mejor primer intento.** El primer delta de C acertó 12
   veces, como el de los demás (12 a 14). La mejora viene entera de la
   reparación: 9 corridas reparadas, 6 en tres rondas y 3 en dos. El agente fue
   siguiendo la cascada con el testigo.
4. **Es el veredicto, no la segunda oportunidad.** D tuvo las mismas rondas y
   una instrucción neutral de revisar. Usó 49 rondas, más que C, y no reparó
   ninguna: sin saber qué falla ni dónde, el agente repite su propuesta. La
   diferencia entre C y D se atribuye al veredicto de Felix con el testigo
   localizado.
5. **Felix opcional no se usó.** En B el MCP estaba disponible en las 24
   corridas y el agente no hizo ninguna llamada. Es lo mismo que en el piloto
   173, con otro proveedor: un agente que cree tener información suficiente no
   consulta la herramienta, ni siquiera con un aviso de que ve sólo una parte de
   la base.

## Lo que el estudio no muestra

- **Felix no detecta intenciones.** En `verificar_dependientes` lo correcto era
  no cambiar nada. Pero en 10 de 12 corridas el agente registró revisores que no
  podía ver para «asegurar» la regla, inventando hechos que el objetivo no
  mencionaba. El estado resultante es válido, así que en C Felix lo publicó.
  Felix impide violar las reglas, no garantiza que el cambio respete la
  intención del objetivo.
- **El costo es real.** C usó un 63 % más de rondas que A, casi el doble de
  tiempo (10 s frente a 5,5 s) y un 72 % más de tokens, más la verificación
  local de Felix.
- **El testigo revela datos.** El brazo C recibe rutas de nodos que no estaban
  en su contexto, que es justo lo que le permite reparar. En un uso real, eso
  sólo es aceptable si el agente puede ver esa parte de la base.
- **Alcance limitado.** Ocho tareas sintéticas sobre un único bundle de 24
  nodos, un modelo, tres repeticiones y un radio de contexto fijo. Las
  diferencias son descriptivas y pareadas por tarea, no una prueba de
  significancia general. Con información completa (calibraciones v1 y v2), el
  mismo modelo no se equivocó: el beneficio aparece cuando el agente no ve la
  base entera.

## Qué cambia para Felix

La afirmación A11 («mejora a un agente») pasa de «no demostrada» a
**demostrada de forma acotada**. Con contexto parcial y verificación
obligatoria con testigo localizado, Felix evitó todos los estados inválidos y
permitió reparar cascadas que el agente no veía, en este diseño sintético. No
mejora el primer intento ni detecta cambios que cumplan las reglas pero
contradigan la intención. Usado de forma opcional, no se usa.
