# Propuesta 09 — Estudio prerregistrado de cuatro brazos

Estado: **9a hecha** (2026-11-18): protocolo congelado en [`evaluation/four_arms_protocol.md`](../../evaluation/four_arms_protocol.md), ensayo completo sin LLM. **9b y 9c pendientes de decisión.** Corresponde al paso 9
del [maestro](../brechas/00-maestro.md), a E3–E6 de la
[brecha 07](../brechas/07-evidencia-de-utilidad.md) y a la §5 de las
[conclusiones](../conclusiones-ia-felix-felicis.md#5-cómo-evaluar-la-hipótesis-siguiente-sin-cambiar-el-resultado-anterior).

## Por qué un estudio nuevo

El piloto 173 fue neutral por dos motivos que este estudio corrige: las tareas
no dejaban margen (12/12 éxitos sin Felix) y usar Felix era opcional (cero
llamadas). El piloto queda intacto: ni sus archivos con hash, ni su runner, ni
su informe cambian, y sus resultados no se mezclan con los nuevos.

## Brazos

| Brazo | Qué recibe el agente | Pregunta |
|---|---|---|
| A. Baseline | Hechos, reglas y objetivo; sin Felix | ¿Qué resuelve el modelo solo? |
| B. Felix opcional | A + las cuatro herramientas MCP de lectura | ¿Las adopta y cuándo? |
| C. Verificación obligatoria | A, pero cada delta pasa por el orquestador del paso 8, que devuelve el rechazo redactado y da hasta 3 intentos | ¿El flujo reduce estados inválidos aceptados y facilita reparar? |
| D. Control de revisión | A, con las mismas oportunidades que C, pero la respuesta es neutral («revisa tu propuesta contra las reglas») y no dice si fue admitida | ¿La mejora de C viene del veredicto o de tener otra oportunidad? |

C y D reciben el mismo número máximo de rondas y el mismo presupuesto de tiempo.
D nunca recibe un falso `admit`: sólo una instrucción neutral, igual en cada
ronda.

## Tareas y oráculo

- **Tareas nuevas**, sintéticas y escritas antes de correr nada, con margen de
  error para el baseline: dependencias cruzadas (cambiar un tipo obliga a
  cambiar aristas o atributos), contradicciones que el objetivo provoca si se
  ejecuta de forma literal, reglas con cardinalidad (`al-menos`), orden y fechas,
  y casos donde no hay que cambiar nada.
- Propuesta: **8 tareas en 4 familias**. Un **sondeo de calibración** con el
  baseline (2 repeticiones, descartado del análisis y publicado aparte) muestra
  si hay margen. Si el baseline acierta todo, se reescriben las tareas **antes**
  de congelar, nunca después de ver los brazos.
- **Oráculo independiente** en Python estándar, sin el motor de Felix, como en
  el piloto, extendido a las nuevas formas de regla. Se prueba contra deltas de
  referencia que no se entregan al agente.

## Medidas prerregistradas

- **Primaria de seguridad**: estados inválidos aceptados como resultado final,
  según el oráculo.
- **Primaria de utilidad**: éxito final (formato, estructura, reglas, objetivo
  y conservación), según el oráculo.
- **Secundarias**: éxito del primer delta, reparación tras un rechazo, rondas
  usadas, escalados, tiempo de pared y tokens reportados por el proveedor
  (`null` si no los reporta).
- **Atribución**: si C supera a A en éxito final pero no en primer delta, el
  beneficio se atribuye al control del flujo, no a un razonamiento mejor. Si C
  no supera a D, no se atribuye al veredicto de Felix.

## Diseño de ejecución

- 8 tareas × 4 brazos × 3 repeticiones = **96 ejecuciones**, con orden
  aleatorizado por una semilla de planificación fijada en el plan congelado.
- El mismo modelo, configuración, sandbox y presupuesto por brazo. Proxy MCP v2
  (`felix-mcp-ledger-v2`) en el brazo B.
- Runner nuevo (`evaluation/run_four_arms.py`) y protocolo
  `felix-four-arms-v1`; el runner del piloto no se modifica.
- **Criterio de detención prerregistrado**: se corren las 96 y se publican todas,
  incluidos timeouts y errores. No se amplía la muestra por un resultado
  negativo ni se repiten ejecuciones desfavorables.
- Análisis descriptivo y pareado por tarea; con 8 tareas no se afirma
  significancia general.

## Fases y compuertas

1. **9a, preparación (sin LLM):** tareas, oráculo y sus tests, runner con un
   agente falso determinista para probar los cuatro brazos de punta a punta,
   plan congelado con hashes. Termina en un commit que congela el protocolo.
2. **9b, calibración:** el sondeo del baseline. Necesita tu instalación
   autenticada del agente y consume cuota.
3. **9c, ejecución:** las 96 corridas y el informe. Necesita tu sí explícito
   tras revisar el plan congelado.

Cada fase se detiene y espera tu decisión antes de la siguiente.

## Decisiones tomadas

Aprobadas por Romeo el 2026-11-18 tal como se recomendaron: sólo 9a por ahora,
Codex CLI con `gpt-6-astra` y sandbox de solo lectura, 8 × 4 × 3 = 96 corridas
y 3 rondas en C y D.

Cambio respecto de la propuesta, decidido al implementar 9a: en C, los errores
de forma del delta (por ejemplo `add_conflict`) también vuelven al agente para
reparar (opción `reparar_errores_de_delta` del orquestador). Sin esa opción, C
perdería las rondas de reparación ante un error de formato mientras D las
conserva, y la comparación C frente a D no sería justa.

## Decisiones que necesitaba

1. **Alcance de hoy:** recomendación, sólo 9a; 9b y 9c, cada una con su propia
   aprobación.
2. **Modelo y harness:** recomendación, el mismo del piloto (Codex CLI con
   `gpt-6-astra`, sandbox de solo lectura), para reutilizar el aislamiento ya
   revisado. La alternativa es otro proveedor como un brazo más, pero eso
   duplica el costo.
3. **Tamaño:** recomendación, 8 tareas × 4 brazos × 3 repeticiones (96
   corridas). Con 2 repeticiones serían 64.
4. **Rondas de C y D:** recomendación, 3, igual que el orquestador.
