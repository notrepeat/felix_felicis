# 00 — Índice y veredicto global

La pregunta del proyecto: **¿podemos crear un universo matemáticamente
definible que funcione como matriz de conocimiento, que no permita almacenar
conocimiento bajo contradicción y que facilite recuperarlo para tareas
repetitivas?** La respuesta de este estudio es que sí, con condiciones
estructurales explícitas: el universo se define como una tupla de formalismo
publicado ([01](01-universo.md)); la no-contradicción se reduce a model
checking de invariantes de primer orden sobre una estructura finita, decidible
y polinómico con $I$ fijo ([02](02-no-contradiccion.md)); la revisión ante
conflicto se adapta de la teoría AGM con dos costes con nombre
([03](03-revision.md)); la recuperación por patrones es tratable solo bajo
disciplina de diseño ([04](04-recuperacion.md)); la evolución es reescritura
de grafos con check transaccional obligatorio ([05](05-evolucion.md)); el
porqué de cada nodo se reifica como grafo chequeable
([07](07-memoria-documental.md)); y todo lo que la matemática **no**
garantiza queda consolidado en un capítulo propio ([06](06-limites.md)).

## Tabla de capítulos

| Capítulo | Pregunta que responde | Veredicto |
|---|---|---|
| [01 — El Universo](01-universo.md) | ¿Qué es, formalmente, el universo $U$? | Fundado |
| [02 — No-contradicción](02-no-contradiccion.md) | ¿Qué es contradicción y por qué detectarla es decidible? | Fundado con condiciones |
| [03 — Revisión](03-revision.md) | Cuando lo nuevo choca con lo vigente, ¿qué cede y con qué criterio? | Fundado con condiciones |
| [04 — Recuperación](04-recuperacion.md) | ¿Cómo se saca conocimiento de $U$ para tareas repetitivas? | Fundado con condiciones |
| [05 — Evolución](05-evolucion.md) | ¿Cómo cambia el grafo sin quedar roto ni inconsistente? | Fundado para la semántica de operaciones; fundado con condiciones para la auto-reparación |
| [06 — Límites](06-limites.md) | ¿Qué es exactamente lo que la matemática no garantiza? | — (es el veredicto negativo del proyecto) |
| [07 — Memoria documental](07-memoria-documental.md) | ¿Por qué está cada nodo ahí, y cómo se audita ese porqué? | Fundado con condiciones |

## Notación global

$$U = \langle G, \tau, \lambda, \iota, I, \preceq \rangle$$

- $G = (V, E, s, t)$ — multigrafo dirigido finito: nodos, aristas y sus funciones de incidencia ([01](01-universo.md)).
- $\tau$ — tipado: asigna a cada nodo y arista un tipo del alfabeto finito $K = K_V \uplus K_E$ ([01](01-universo.md)).
- $\lambda$ — etiquetado semántico: etiqueta legible que identifica, sin carga lógica propia ([01](01-universo.md)).
- $\iota$ — mapa parcial de padre (mereología): bosque de partes y todos que habilita el refinamiento ([01](01-universo.md)).
- $I$ — conjunto finito de invariantes: oraciones de primer orden sobre la estructura subyacente, sede de la noción de contradicción ([01](01-universo.md)).
- $\preceq$ — preorden de prioridad epistémica sobre los átomos: guía qué cede en una revisión ([01](01-universo.md)).

## Veredicto global

**Fundado con condiciones.** El estudio sostiene que el universo propuesto es
matemáticamente viable, y que su viabilidad depende de tres condiciones
estructurales que el diseño debe pagar de forma permanente:

1. **$I$ se mantiene en FO con pocas variables** ([02](02-no-contradiccion.md)):
   el check tras cada operación es polinómico con exponente acotado por el
   número de variables del peor invariante; fórmulas con muchas variables
   encarecen el exponente, y tratar $I$ como entrada variable cambia el
   régimen a PSPACE.
2. **Los patrones se mantienen pequeños o de treewidth acotado**
   ([04](04-recuperacion.md)): el emparejamiento general es NP-completo y
   ningún índice lo hace polinómico; la recuperación es tratable solo dentro
   de ese régimen, y toda consulta que salga de él debe declararse cara.
3. **$\preceq$ y la calidad de los axiomas son curaduría humana**
   ([03](03-revision.md), [06](06-limites.md)): ningún teorema dice qué debe
   estar más atrincherado ni si un axioma es bueno; el orden lo pone el
   diseñador, los empates suben al humano, y un $I$ malo se hace cumplir con
   el mismo rigor que uno bueno.

Lo que queda fuera, dicho en una línea: el sistema no genera corrección, la
filtra — la verdad de los axiomas, la prosa de los cuerpos, la calidad
sustantiva de las justificaciones y la elección de $\preceq$ quedan del lado
del juicio humano, y el inventario completo de esos límites es el
[capítulo 06](06-limites.md) entero.

## Relación con las instancias consumidoras

El motor es deliberadamente independiente de cualquier base de conocimiento
concreta. Una instancia aporta su manifiesto, su bundle OKF, sus invariantes y
su política de prioridad; esos datos no forman parte del motor. Los ejemplos
de este repositorio son ficticios y sirven únicamente para documentación y
pruebas. Frontmatter y links portan los átomos formales que el motor chequea;
el cuerpo markdown sigue siendo contenido humano anclado al nodo, fuera de la
garantía formal ([06](06-limites.md)).

## Plan de demostración

La implementación y su evidencia de funcionamiento se siguen en el
[`plan maestro`](plan/00-plan-maestro.md). Ese plan separa cada afirmación
que el proyecto debe demostrar, sus entregables y su criterio de cierre.
