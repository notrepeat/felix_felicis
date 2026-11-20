# Propuesta 13 — Verificar el trabajo, no sólo la biblioteca

Estado: **descartada** (2026-11-20, decisión de Romeo). La medición privada
cayó en la primera fila de la regla de decisión: los puntos 1 y 2 cubren menos
de un cuarto de los incumplimientos y el juez del punto 3 no pasa el umbral. No
se construye; el camino es entregar el criterio como contexto. No se implementó
nada. El documento se conserva como registro de la hipótesis y de su resultado.

Límites de esa medición: las etiquetas de referencia las puso un juez
automático, no el dueño; el juez recibió la regla correcta para cada turno, y
no una lista entre la que elegir; y se corrió una sola redacción del prompt,
que juzga la regla tal como está escrita.

## Por qué

Felix verifica hoy los cambios al repositorio de criterio: que cada nota tenga
la forma debida y que el universo no se contradiga. El objetivo de quien lo usa
es otro: que el trabajo de sus agentes salga según su criterio. Las mediciones
publicadas dicen dónde está la distancia entre las dos cosas:

| Medición | Resultado | Lo que implica |
|---|---|---|
| [Cuatro brazos](../../evaluation/four_arms_report.md), brazo B | Con Felix opcional por MCP, cero llamadas en 24 corridas | Lo que depende de que el agente decida consultar no ocurre |
| [Cuatro brazos](../../evaluation/four_arms_report.md), brazo C | Verificación obligatoria con testigo: 0 estados inválidos y 9 de 9 cascadas reparadas | Lo que funciona es el rechazo obligatorio que dice qué falla y dónde |
| [Fase 11](../../evaluation/criterio_uso_report.md) | Criterio en el prompt y criterio por MCP: 36 de 36 los dos | Entregar el criterio no es el problema difícil cuando cabe en el contexto |

La hipótesis de esta propuesta es que el resultado del brazo C se puede llevar
del repositorio de criterio al trabajo: una comprobación obligatoria sobre lo
que el agente hace, con un testigo que le permita reparar.

## Diseño

Un criterio se puede incumplir en tres lugares distintos, y cada uno se
comprueba de otra manera.

| Punto | Qué se comprueba | Cómo | Ejemplo ficticio |
|---|---|---|---|
| 1. Acción | Lo que el agente está por ejecutar | Regla determinista sobre la llamada a la herramienta, antes de que corra | «No se publica en la rama principal sin instrucción explícita» |
| 2. Artefacto | Lo que el agente dejó escrito | Comprobación ejecutable sobre el diff, al terminar el turno | «Las fechas se guardan en UTC con `Z`» |
| 3. Conducta | Cómo respondió el agente | Juez con rúbrica sobre la respuesta, obligado a citar el fragmento que incumple | «No se declara terminado un cambio sin haber corrido las pruebas» |

En los tres, el rechazo vuelve al agente con el id y el mensaje del criterio y
con el testigo: la llamada, el archivo y la línea, o la cita. El agente repara
dentro de un presupuesto de rondas, igual que en el
[orquestador](08-orquestador-obligatorio.md).

**Nivel de cada criterio.** Cada criterio declara cómo se comprueba:
`ejecutable` (puntos 1 y 2), `juzgado` (punto 3) o `consultivo` (sin
comprobación; sólo se entrega como contexto). El nivel es un atributo de la
nota, con dominio cerrado, y la comprobación vive junto a ella.

**Qué hace Felix y qué no.** Felix guarda el criterio, su nivel, su
comprobación y la evidencia de cada vez que se cumplió o se incumplió. El
veredicto sobre el trabajo no lo da el evaluador formal: lo dan la regla sobre
la acción, la comprobación sobre el diff o el juez. El evaluador sigue
garantizando lo de hoy, la coherencia del repositorio.

**Fallar cerrado sólo en lo determinista.** Los puntos 1 y 2 bloquean. El punto
3 empieza sin bloquear: registra el rechazo y lo muestra, hasta que su
precisión esté medida (ver más abajo). Un juez que se equivoca y bloquea es
peor que la falta que evita.

## Lo que esta propuesta no resuelve

- **El punto 3 es el menos sólido.** Depende de un modelo como juez. La cita
  obligatoria acota las invenciones, pero no la inestabilidad entre corridas.
- **Los puntos 1 y 2 cubren sólo los criterios que dejan rastro en una acción o
  en un archivo.** Los criterios sobre cómo razonar o cómo comunicar quedan en
  el punto 3 o en `consultivo`.
- **No detecta intenciones**, igual que el resto de Felix: un trabajo puede
  pasar todas las comprobaciones y no ser lo que se pidió.
- **El costo es real.** En el brazo C la verificación obligatoria usó un 63 %
  más de rondas y un 72 % más de tokens.

## Cómo se decide si vale la pena

Antes de construir nada se mide, fuera de este repositorio, sobre correcciones
reales del dueño a sus agentes. Esos datos son privados: aquí sólo se registra
la decisión que resulte, nunca los casos ni sus cifras
([frontera pública](../../PUBLIC_BOUNDARY.md)).

Sobre un conjunto de turnos ya clasificados a mano como «el agente incumplió un
criterio» o «el agente no lo incumplió»:

1. Para cada incumplimiento, anotar qué punto lo habría detenido: acción,
   artefacto, conducta o ninguno.
2. Correr el juez del punto 3 sobre todos los turnos, tres veces.

Criterios de decisión, fijados antes de mirar los resultados:

| Resultado | Decisión |
|---|---|
| Los puntos 1 y 2 juntos cubren menos de un cuarto de los incumplimientos, y el juez no pasa el umbral | No construir; el camino es entregar el criterio como contexto |
| El juez detecta al menos dos tercios de los incumplimientos, rechaza menos de un décimo de los turnos correctos y coincide consigo mismo en al menos nueve de cada diez | Construir los tres puntos |
| Sólo los puntos 1 y 2 pasan | Construir esos dos y dejar la conducta como `consultivo` |

## Decisiones pendientes

1. **Medir antes de construir.** Recomendación: sí. Usa datos que ya están
   clasificados, y puede descartar la propuesta entera.
2. **Dónde vive la comprobación de un criterio.** Recomendación: en el
   repositorio de criterio, junto a la nota, y no en cada proyecto. Así un
   criterio se comprueba igual en todos lados y su evidencia se acumula en un
   solo lugar.
3. **Si el punto 3 puede llegar a bloquear.** Recomendación: no en esta fase.
   Se registra y se muestra; bloquear se decide con datos de uso.
