# Conclusiones: cómo podría mejorar Felix Felicis el uso de IA

Fecha: 2026-11-17. Este documento reúne lo observado, sus límites y propuestas
para aprovechar mejor el sistema. **No se ejecutaron nuevas evaluaciones ni
consultas a modelos para redactarlo.** Las recomendaciones son hipótesis de
trabajo, no capacidades adicionales implementadas ni beneficios demostrados.

La conclusión principal es que **dar acceso opcional a Felix no mejoró el
resultado del piloto**: el modelo resolvió todas las tareas sin utilizarlo.
La oportunidad más defendible es probar Felix como verificador determinista y
controlador de cambios dentro de un flujo de trabajo, además de ofrecerlo como
herramienta consultable. Debemos medir por separado la seguridad del flujo, la
capacidad de razonamiento del agente y el costo de obtener un resultado útil.

## 1. Qué construimos y qué demuestra cada etapa

| Etapa | Resultado disponible | Lo que no acredita por sí solo |
|---|---|---|
| Fase 04: transacciones | Simulación, aplicación con lock y CAS, preservación de auxiliares, recovery e Intent v3. | Que una IA elija un objetivo correcto o formule bien las reglas. |
| Fase 06: acceso del agente | MCP local con `context`, `propose`, `check` y `why`; demo sintética que rechaza, explica y readmite una corrección. Code Mode local opcional. | Que un modelo real consulte las herramientas espontáneamente o mejore gracias a ellas. |
| Fase 07: evaluación | Corpus formal, matriz de costo del motor y piloto pareado con un modelo real, incluyendo resultados negativos. | Superioridad general, ahorro monetario o utilidad sobre repositorios privados y tareas reales. |

La [aplicación transaccional](operacion-transaccional.md) vuelve a evaluar bajo
lock y rechaza snapshots desactualizados. Una simulación previa no autoriza una
escritura posterior sin esa comprobación. El [servidor MCP](mcp.md) sólo consulta
y simula; no expone `apply`. Estas fronteras ya existen y no deben eliminarse
para facilitar la integración con agentes.

## 2. Resultados observados

### Piloto con un modelo real

Se prerregistraron **seis tareas sintéticas, dos repeticiones y dos brazos: 24
ejecuciones, doce por brazo**. Cada par recibió el mismo objetivo, hechos y reglas,
con el mismo modelo y presupuesto. El modelo configurado fue `gpt-6-astra`, esfuerzo
`low`, mediante Codex CLI `0.169.0`. No se obtuvo snapshot inmutable del proveedor
ni semilla de muestreo; la semilla 173 sólo organiza el experimento.

| Medida | Sin Felix | Con Felix disponible |
|---|---:|---:|
| Ejecuciones correctas según oracle independiente | 12/12 | 12/12 |
| Deltas finales inválidos | 0 | 0 |
| Objetivos incumplidos o cambios ajenos al objetivo | 0 | 0 |
| Llamadas a herramientas Felix | 0 | 0 |
| Mediana de tiempo de pared, segundos | 5.297 | 5.112 |
| `input_tokens` reportados, suma del brazo | 121996 | 122188 |
| `output_tokens` reportados, suma del brazo | 684 | 734 |
| Costo monetario observado | No disponible | No disponible |

La mediana de diferencias **pareadas** con−sin fue −0.011 segundos: seis pares
más rápidos y seis más lentos con Felix. No es evidencia de mejora de latencia.
Los 192 tokens de entrada adicionales equivalen a 16 por par; incluyen el contexto
efectivo del cliente. Los campos de caché y razonamiento se publican por separado
en el informe y no se suman entre sí para inventar un total. `cost_usd` es `null`,
no cero; una suscripción existente no permite deducir facturación por ejecución.

Tras cuatro registros sin llamadas se hizo un diagnóstico separado: se pidió
explícitamente un `check`, el modelo lo ejecutó y recibió `admit`. Confirma acceso
real a la herramienta, pero no forma parte de las 24 tareas ni demuestra mejora
en ellas. También se registraron por separado los probes de preparación y sus
fallos. No hubo rechazos MCP ni reparaciones asistidas observables en el piloto.

Fuentes: [informe pareado](../evaluation/paired-report.md),
[plan previo](../evaluation/results/pilot-173/plan.json),
[resumen](../evaluation/results/pilot-173/summary.json) y
[metodología](../evaluation/paired_methodology.md).

### Motor y transporte: mediciones distintas

El corpus formal obtuvo coincidencia exacta en 46 casos etiquetados, cubriendo
14 variantes de fórmula, más tres casos adicionales de conflicto funcional,
sin falsos admit en los casos etiquetados. La matriz operativa produjo 65 admisiones y **cinco agotamientos
de presupuesto con tres variables**, de 70 muestras. Esos errores son límites
reales conservados en el informe, no admisiones lentas ni motivos para aumentar
la cuota automáticamente. [Corpus y costo operativo](../evaluation/formal-and-performance.md).

La comparación Code Mode usó un agente determinista, **sin LLM**, sobre una
fixture sintética. Fueron doce muestras: tres repeticiones frías y cálidas por
modo, con binario debug y ejecución local Linux.

| Medida | MCP directo | Code Mode local |
|---|---:|---:|
| Mediana fría, ms | 14.722 | 416.736 |
| Mediana cálida, ms | 11.471 | 423.098 |
| Invocaciones exteriores | 4 | 1 |
| Llamadas MCP internas | 4 | 4 |
| Bytes JSON en frontera del agente, sin catálogo | 1354 | 1130 |

El catálogo upstream medido aparte ocupa 1792 bytes en ambos casos. Cada ejecución
Code Mode crea un nuevo sandbox; «cálida» sólo reutiliza el cliente MCP. La capa
compone y filtra resultados, pero añadió aproximadamente 400 ms localmente en
este caso. Menos invocaciones exteriores o bytes **no demuestra ahorro de tokens,
dinero o latencia de un modelo**. Se mantiene MCP directo por defecto y Code Mode
opcional. [Comparación y muestras](codemode-comparacion.md).

La CI valida Rust y el corpus en Linux/Windows. El piloto real, los benchmarks,
las pruebas Python de evaluación y Code Mode son evidencia local; no se afirma
que la CI ejecute inferencia autenticada ni que Code Mode tenga soporte Windows.

El ensayo midió la oferta de verificación para microrreparaciones autocontenidas.
No midió memoria entre sesiones, continuidad de conocimiento, recuperación de
contexto en una base grande ni integración privada: la fase 08 sigue pendiente.
Esos posibles objetivos de producto necesitan evaluaciones propias; no deben
confundirse con acertar un delta pequeño cuyos datos ya están en el prompt.

## 3. Por qué no apareció la mejora esperada

Esperábamos observar menos errores o mejores reparaciones al ofrecer verificación.
Lo observado fue éxito máximo en ambos brazos y ausencia de uso. Las siguientes
son **inferencias plausibles, no causas demostradas**:

| Hipótesis | Evidencia compatible | Cómo distinguirla en otro estudio |
|---|---|---|
| Las tareas ofrecían poco margen de mejora. | Tres nodos, toda la información visible y 100 % de éxito sin Felix. | Tareas nuevas con dependencias más largas y errores reales del baseline, seleccionadas antes de comparar brazos. |
| Ya habíamos entregado el contexto que Felix podría recuperar. | El prompt contenía hechos y reglas completos. | Comparar acceso selectivo a una misma base, con igualdad de información disponible, sin entregarla toda de entrada. |
| La consulta opcional no compensaba su esfuerzo percibido. | Cero llamadas pese a capacidad real comprobada. | Separar disponibilidad opcional de una verificación obligatoria del flujo. |
| El modelo podía resolver directamente este conjunto. | El alias configurado acertó todos los casos. | Comparar modelos o presupuestos distintos bajo protocolos independientes; no asumir que todos se comportarán igual. |
| El descubrimiento diferido de herramientas o su presentación influyó. | El acceso existía, pero no hubo adopción espontánea. | Comparar presentación explícita frente a descubrimiento, manteniendo tareas y presupuestos iguales. |

No conocemos la motivación interna del modelo. No puede atribuirse causalmente
el resultado a una de estas hipótesis sólo con el transcript. Seis tareas con
dos repeticiones no son 24 tareas independientes. La muestra pequeña, el techo
de éxito y la ausencia de snapshot/semilla de inferencia limitan generalización
y reproducción exacta; no justifican una prueba de significancia ingenua.

**No concluimos que Felix sea inútil. Tampoco concluimos que mejore la IA o que
ahorre costos.** Demostramos que este protocolo de disponibilidad opcional no
produjo beneficio observable y no llegó a poner a prueba una reparación asistida.

## 4. Cómo podríamos optimizar el uso de IA

### Separar generación, validación y autoridad de escritura

**Propuesta de arquitectura, no cambio implementado por este documento:**

1. El modelo expresa la intención como delta sobre un snapshot identificado.
2. El orquestador llama al verificador de forma determinista; no depende de que
   el modelo recuerde consultarlo.
3. Ante rechazo, entrega diagnóstico acotado y procedencia al modelo para una
   reparación con presupuesto explícito. Ante error técnico, timeout o cuota
   agotada, detiene o escala; nunca los interpreta como admisión.
4. Sólo un ejecutor autorizado puede intentar aplicar un candidato admitido,
   usando el contrato transaccional existente: lock, CAS y reevaluación. No hay
   ruta alternativa de escritura directa del agente.
5. Se comprueba el objetivo y la ausencia de regresiones con criterios externos
   a la mera satisfacción de invariantes.

Este flujo podría reducir la aceptación de cambios inválidos incluso si no mejora
el razonamiento del modelo. Son beneficios diferentes que deben medirse por
separado. Una regla incompleta puede admitir un cambio contrario a la intención:
el verificador no descubre automáticamente todos los requisitos relevantes.

### Reducir contexto y reservar inferencia para decisiones útiles

**Propuestas a probar:** recuperar hechos por necesidad, mostrar primero conteos
y procedencia, y solicitar valores sólo cuando hagan falta y exista autorización.
Evitar repetir el bundle completo o diagnósticos idénticos en cada turno; conservar
un snapshot/versionado explícito y recapturar cuando pueda haber cambiado.

Hoy `context` usa vecindad acotada y entrega todos los invariantes de manera
conservadora; no implementa selección semántica mínima. Reducir fórmulas sin una
regla de relevancia sólida podría ocultar restricciones y producir ahorro aparente.
Primero medir qué parte del contexto se repite, luego experimentar con selección
preservando cobertura. Cualquier caché deberá invalidarse por versión/snapshot, respetar cuotas y
conservar la reevaluación y replay exigidos para publicar; nunca convertir un
veredicto de un estado anterior en autorización sobre un estado nuevo.

Asignar al código determinista parseo, validación y serialización; reservar el
modelo para interpretar objetivos y proponer correcciones. Se podría probar un
modelo más pequeño o menor presupuesto con escalamiento acotado cuando falle,
pero todavía no hay evidencia de que esa combinación mantenga calidad o reduzca
costo. Detener reintentos repetidos sin progreso evita convertir un límite del
motor en un bucle caro de inferencia.

Mantener Code Mode opcional para secuencias dependientes y filtrado de resultados
intermedios. No adoptarlo globalmente por haber reducido cuatro invocaciones a
una: con cuatro herramientas locales fue más lento, y falta medir el caso con
latencia real de inferencia antes de atribuirle una ventaja global.

### Dónde tendría sentido buscar utilidad

Como hipótesis de aplicación: cambios con varias restricciones cruzadas,
migraciones de tipos/jerarquías, modificaciones concurrentes donde importe CAS,
y mantenimiento de conocimiento con reglas explícitas y objetivos auditables.
Felix podría aportar una barrera verificable entre propuesta y publicación.
No garantiza verdad factual, vigencia temporal, cumplimiento normativo ni cobertura
de requisitos que no hayan sido representados y validados.

## 5. Cómo evaluar la hipótesis siguiente sin cambiar el resultado anterior

Estas son **propuestas para un protocolo futuro**, no experimentos ejecutados ni
autorización para iniciarlos ahora. Conservar el piloto actual intacto y crear
otro conjunto, otra versión de protocolo y un oracle independiente.

| Brazo propuesto | Pregunta que responde |
|---|---|
| Baseline sin Felix | ¿Qué resuelve el modelo con la información disponible? |
| Felix opcional | ¿Adopta espontáneamente la verificación y cuándo? |
| Verificación Felix obligatoria por el orquestador | ¿Reduce estados inválidos aceptados y facilita reparación el flujo completo? |
| Control de revisión sin veredicto semántico | ¿La mejora se debe al verificador o simplemente a una segunda oportunidad/instrucción de revisar? |

El control debe usar presupuesto comparable y una respuesta neutral, no un falso
`admit`. Igualar oportunidades de revisión y límites; registrar diferencias
reales de tiempo y tokens. Si el pipeline obligatorio mejora la aceptación final
pero no el primer delta, atribuir el beneficio al control del flujo, no a un
razonamiento inicial superior.

Incluir tareas nuevas con contradicciones, dependencias cruzadas y cambios de
estado. Las restricciones temporales deben representarse explícitamente con el
vocabulario disponible; no se supone una lógica temporal implementada. Cuando
falte información, definir en el oracle si corresponde consultar, abstenerse o
escalar, en vez de premiar una respuesta inventada. Dar a todos los brazos acceso
a los mismos hechos, aunque difieran los medios de verificación. No escoger
retrospectivamente sólo casos donde Felix gane ni reutilizar el piloto conocido
como conjunto principal de una nueva afirmación de mejora. No ajustar modelos,
prompts o selección adaptativa usando las respuestas del conjunto reservado para
evaluar; separar desarrollo y evaluación antes de probar las alternativas.

Medir éxito del objetivo, validez formal, preservación de hechos ajenos, severidad
de regresiones, falsos admit en casos etiquetados, reparación tras rechazo y
cobertura de uso de herramientas. Separar primer intento y salida final; contar
rechazos útiles y falsos bloqueos. Publicar tiempo de pared, llamadas, tamaño de
contexto, campos de tokens efectivamente reportados, errores por cuotas y costo
monetario sólo si se observa. Separar diagnósticos y preparación de los pares,
con su costo visible. Los fallos durante ejecuciones prerregistradas permanecen
en el denominador y se reportan; no se excluyen como si fueran sólo preparación.

Prerregistrar un criterio de continuar/detener según la aplicación: beneficio
mínimo útil, tolerancia a regresiones, presupuesto de tiempo/contexto y respuesta
ante agotamiento. **No fijamos aquí umbrales numéricos arbitrarios.** Si sólo se
obtiene más uso de herramientas sin mayor calidad o menor costo a igual calidad,
no hay evidencia para expandir el sistema. Las réplicas deben analizarse por tarea
y no inflar artificialmente el tamaño de muestra.

## 6. Prioridades y decisión

| Prioridad propuesta | Trabajo acotado | Evidencia necesaria antes de ampliar |
|---|---|---|
| 1 | Instrumentar adopción, primer delta, rechazo, reparación y estado final; reutilizar el motor existente. | Saber dónde ocurre un error y cuánto cuesta corregirlo. |
| 2 | Prototipar el control determinista de verificación y publicación en un harness aislado. | Menos cambios inválidos aceptados sin rutas de escritura que eludan el control. |
| 3 | Prerregistrar el estudio con baseline, opcional, obligatorio y control de revisión. | Mejora atribuible, preservación del objetivo y costo total medido. |
| 4 | Sólo después, probar contexto selectivo, presupuesto/modelo y batching Code Mode. | Calidad comparable o mejor y ahorro observado, no sólo menos llamadas exteriores. |

No hace falta reescribir el motor para probar estas hipótesis. El trabajo de las
fases 04 y 06 aporta componentes reutilizables; la fase 07 mostró qué afirmación
no podemos sostener todavía. **La evaluación está completada, pero el gate de
beneficio sigue sin superarse.** No avanzar a AGM ni recuperación avanzada con
esta evidencia, ni iniciar automáticamente las fases privadas o un nuevo estudio.
[Estado y regla del plan](plan/07-validacion-empirica.md).
