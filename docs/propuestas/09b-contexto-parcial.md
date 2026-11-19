# Propuesta 09b — Estudio con contexto parcial

Estado: **aprobada; fase 1 hecha** (2026-11-18): tareas v3, regla de contexto, testigo localizado en Felix (`propose --witness`) y runner `felix-four-arms-v2`. Calibración (10 de 16, cumple el criterio) y estudio hechos: ver el [informe](../../evaluation/four_arms_report.md). Aprobada por Romeo tal como se recomendó: radio 1, testigo localizado y 96 corridas con Sonnet 5.5. Extiende la
[propuesta 09](09-estudio-cuatro-brazos.md); el protocolo
[`felix-four-arms-v1`](../../evaluation/four_arms_protocol.md) y sus
calibraciones quedan como están.

## Por qué

Tres calibraciones dieron un acierto total cuando el agente recibe **todos los
hechos y todas las reglas en el prompt**:

| Calibración | Modelo | Aciertos |
|---|---|---|
| Piloto 173 | GPT | 12 de 12 |
| Tareas v1 | Sonnet 5.5 | 16 de 16 |
| Tareas v2 (24 nodos, cascadas) | Sonnet 5.5 | 16 de 16 |

Con información completa no hay errores que Felix pueda evitar. Pero un agente
sobre una base de conocimiento real no ve toda la base: ve la nota que edita y
sus vecinas. Felix, en cambio, verifica contra la base completa. La hipótesis
nueva es si **Felix permite a un agente con contexto parcial evitar o reparar
violaciones que involucran hechos que no vio**.

## Diseño

**Contexto.** Cada tarea nombra uno o más nodos. Todos los brazos reciben
exactamente el mismo contexto:

- los hechos de esos nodos y de sus vecinos directos (radio 1 por cualquier
  arista, en ambos sentidos);
- todas las reglas, en lenguaje natural y en `.sx`;
- el objetivo.

El resto del bundle no se muestra. La selección es determinista, se calcula a
partir del objetivo, se congela en el plan y se publica por tarea.

**Tareas v3.** Se adaptan las tareas v2 para que la consecuencia que hay que
atender quede **fuera del radio 1**. Por ejemplo, la cadena `d04 → d03 → d02`:
al degradar `d02`, el agente ve que `d03` depende de `d02`, pero no ve que `d04`
depende de `d03`. Se quitan las tareas que exigen recorrer toda la base
(`renovar_vencimientos`), porque con contexto parcial no tienen solución justa.
Se mantienen tareas de control cuya respuesta sí está dentro del radio, para
detectar si un brazo empeora lo que ya se resolvía. El oráculo sigue puntuando
contra la base completa.

| Brazo | Recibe además del contexto | Puede recuperar lo que no ve |
|---|---|---|
| A. Baseline | nada | no |
| B. Felix opcional | herramientas MCP `context`, `check`, `propose` y `why`; `context` devuelve vecindad hasta radio 2 | sí, si decide explorar |
| C. Verificación obligatoria | el orquestador del paso 8: rechazo con la regla y, según la decisión 2, los nodos implicados | sí, por el diagnóstico |
| D. Revisión neutral | las mismas rondas que C, con instrucción neutral | no |

Se espera que A y D fallen las tareas cuya consecuencia está fuera del radio:
esa es la condición del experimento, no un hallazgo. Lo que se mide es si B y C
recuperan esos casos sin empeorar los de control, y a qué costo en rondas,
llamadas y tiempo.

**Calibración con criterio nuevo.** Primero corre sólo el baseline. Se sigue
con el estudio si el baseline falla al menos una tarea fuera de radio y acierta
las de control. Si acierta todo, el contexto no es tan parcial como se cree y se
revisa el diseño antes de seguir.

## Decisiones que necesita

1. **Radio del contexto.** Recomendación: radio 1 desde los nodos nombrados en
   el objetivo. Radio 0 (sólo los nodos nombrados) es más duro y menos realista.
2. **Qué dice Felix al rechazar en C.** Hoy el rechazo dice qué regla falló,
   pero no **dónde**: los nodos se ocultan a propósito, por privacidad (brecha
   T1 en [02](../brechas/02-rechazo-testigos-y-reparacion.md)). Con contexto
   parcial, eso deja a C sin forma de saber qué nodo revisar.
   - **Opción a, recomendada:** agregar a Felix un **testigo localizado**
     opt-in y sólo local (`propose --witness`), con las rutas de
     los nodos de la instancia que falla. Las rutas ya existen internamente
     (`AsignacionCanonica`). Cierra la parte pendiente de T1, y el MCP no
     cambia.
   - **Opción b:** usar el rechazo tal como es hoy. Mide a Felix sin cambios,
     pero probablemente C no pueda reparar casi nada.
3. **Tamaño.** Recomendación: 8 tareas × 4 brazos × 3 repeticiones = 96 corridas
   con Sonnet 5.5, como el diseño aprobado.

## Fases

1. **Diseño de tareas v3 y regla de contexto**, y, si se elige la opción 2a, el
   testigo localizado en Felix con sus tests. Sin LLM.
2. **Calibración** del baseline con contexto parcial: 16 corridas.
3. **Estudio**: 96 corridas e informe.

Cada fase se detiene y espera tu decisión antes de la siguiente.
