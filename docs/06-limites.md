# 06 — Límites: lo que la matemática no garantiza

Los capítulos anteriores demostraron lo que el universo $U$ puede garantizar
y bajo qué condiciones. Este capítulo hace el trabajo opuesto: reúne, en un
solo lugar, todo lo que la matemática de este estudio **no** garantiza. Nada
de lo que sigue es nuevo — cada límite fue enunciado en el capítulo que lo
carga; aquí se consolidan para que ningún lector pueda decir que la letra
pequeña estaba repartida.

## Consistencia no es verdad

$\mathit{Str}(U) \models I$ dice exactamente esto: la estructura presente
satisface los invariantes presentes. No dice nada sobre el mundo. La
contradicción del capítulo 02 es, por definición, **violación de un
invariante de primer orden sobre una estructura finita dada** — no
desacuerdo con la realidad, no falsedad de una afirmación en lenguaje
natural. Los invariantes son axiomas que alguien escribió; si el axioma es
malo, el sistema lo aplicará con el mismo rigor con que aplicaría uno bueno.
Un $I_1$ que prohibiera exactamente la dependencia correcta se haría cumplir
tras cada operación, con protocolo transaccional y todo (capítulo 05), y el
universo resultante sería consistente y estaría equivocado. Basura en los
axiomas, rigor en la basura. El check certifica coherencia interna respecto
de $I$; la calidad de $I$ es responsabilidad del diseñador y queda fuera de
todo teorema de este estudio.

## El check solo cubre lo axiomatizado

Corolario directo de la definición: un error en una dimensión sobre la que
ningún invariante habla **pasa limpio**. El check evalúa las oraciones de
$I$ y nada más que las oraciones de $I$; una propiedad deseable que nadie
escribió como invariante no existe para el motor. El capítulo 01 lo mostró
en pequeño con la instanciación de diseño de software: el esquema de efectos
prohíbe aristas hacia efecto mayor **según qué instancias del esquema estén
en $I$** — el patrón del capítulo 04 encuentra tranquilamente aristas de
Mediador a Adaptador porque ningún invariante vigente las prohíbe. La
cobertura del check es exactamente la extensión de $I$: ni un átomo más.

El propio tipado cae en este límite. El $\tau$ de la Definición 1 es una
variante debilitada de los grafos tipados de Ehrig et al. 2006, con
codominio plano $K$ y **sin la restricción de incidencia** que el grafo de
tipos impone sobre qué tipos de arista pueden conectar qué tipos de nodo
(capítulo 01). Esa restricción no viene dada por el formalismo: si se
quiere, hay que axiomatizarla a mano como invariantes en $I$ — y lo no
axiomatizado, como siempre, pasa limpio.

Y la extensión de $I$ tiene un precio propio, fijado en el capítulo 02:
el check es polinómico **con $I$ fijo y en FO con pocas variables** — el
exponente es el número de variables del peor invariante, así que una fórmula
con muchas variables encarece el check aunque siga siendo polinómica, y si
$I$ se trata como entrada variable el régimen es PSPACE (complejidad
combinada). Axiomatizar más dimensiones no es gratis: cada invariante nuevo
debe seguir siendo corto para que la garantía de tratabilidad se sostenga.

## El cuerpo markdown OKF escapa a la garantía

La capa de representación (capítulo 01) traza la frontera con precisión:
**frontmatter + links = átomos formales**, de ahí se leen $\mathit{Type}$,
$\mathit{HasLab}$, $\mathit{Parent}$ y $\mathit{Edge}$, y sobre eso corre el
check; **el cuerpo markdown = contenido humano anclado al nodo**, fuera de
la garantía formal. Una contradicción escrita en prosa dentro de dos cuerpos
no la detecta ningún teorema de este estudio: se detecta por juicio, no por
check. Una instancia puede complementar el motor con una auditoría semántica
de la prosa, pero esa auditoría no es el check formal del capítulo 02 ni lo
sustituye: conviven, teorema para la estructura, juicio para el cuerpo.

El capítulo 07 hereda este límite y lo enuncia para el rationale: lo que sus
invariantes ID1–ID3 garantizan es la **estructura** del porqué — que la
resolución exista como nodo, que esté conectada, que el registro porte
fecha —, mientras que el **contenido** en prosa de cada nodo de rationale
queda fuera, como todo cuerpo markdown. Una justificación estructuralmente
presente puede ser sustantivamente mala, y eso lo detecta el juicio, no el
check. Y la lección de coste de gIBIS sigue en pie: la red de rationale se
llena en el momento de deliberar o no se llena; los invariantes obligan a
que el esqueleto exista, no pueden obligar a que sea fiel.

## La satisfacibilidad global de $I$ es incomprobable

El capítulo 02 fue explícito sobre el otro lado de su decidibilidad: la
satisfacibilidad de primer orden sobre modelos **finitos** es indecidible
(teorema de Trakhtenbrot, B. A. Trakhtenbrot, 1950 — aplica a nuestra firma,
que tiene símbolos de relación binaria), y la validez finita no es siquiera
semidecidible. La consecuencia práctica, en las preguntas del usuario:

- "¿Existe algún diseño que cumpla todos mis axiomas?" — **fuera** del
  alcance decidible.
- "¿Este diseño cumple?" — **dentro**: model checking sobre
  $\mathit{Str}(U)$, decidible y polinómico con $I$ fijo.

El sistema nunca certificará que unos axiomas son coherentes en abstracto.
Un $I$ insatisfacible — que ningún grafo puede cumplir — no será denunciado
por ningún algoritmo del sistema; solo se manifestará como rechazo de todo
candidato, y distinguir "mis axiomas son incumplibles" de "aún no encontré
el diseño" queda a cargo del humano.

## La revisión no es única, y computarla es dura

El capítulo 03 cargó su adaptación de AGM con dos costes con nombre, más dos
advertencias de alcance que aquí se consolidan:

**No-unicidad.** La teoría entrega una **familia** de revisiones admisibles,
no una solución: puede haber varios retiros mínimos $R$ incomparables, igual
que en AGM hay varios subconjuntos maximales que no implican $\varphi$.
Elegir exige información extra — un $\preceq$ decisivo sobre los átomos en
conflicto. Y elegir $\preceq$ es una **decisión extra-matemática**: ningún
teorema de este estudio dice qué debe estar más atrincherado; el orden lo
pone el diseñador de cada instancia, y con empates la elección sube al
humano. Presentar la revisión como determinista sin haber
pagado ese precio sería falso.

**Todo-o-nada sobre $\Delta$.** Por decisión de diseño (capítulo 03), el
delta se incorpora completo o se rechaza completo; aceptar un subconjunto
sería decidir por el emisor qué quiso decir. El sistema no negocia deltas
parciales.

**Complejidad.** Computar el retiro mínimo es un problema tipo conjunto de
golpeo mínimo sobre la familia de testigos (la estructura de la kernel
contraction de Hansson 1994), NP-duro ya en sus formulaciones básicas. Lo
que lo hace practicable es el **régimen de operación** — deltas pequeños,
invariantes locales —, no un teorema; un delta enorme contra invariantes
globales sale del régimen y el sistema debe decir "no puedo resolver esto
solo" en lugar de prometer eficiencia que no tiene.

**La cascada se retira sin peaje de prioridad.** La prioridad de la
Definición 4 se chequea sobre los átomos de $R$, pero los de
$\mathit{casc}(R) \setminus R$ — lo arrastrado por el retiro estructural —
caen **sin** pasar por $\preceq$: no existe el retiro parcial de un
elemento, y exigir prioridad también sobre la cascada volvería inadmisible
casi cualquier revisión que toque un elemento con incidencias. Quien retira
un nodo retira todo lo que cuelga de él, esté atrincherado como esté.

**Los testigos cubren solo el fragmento universal.** La identificación
kernel = testigo mínimo vale para invariantes que fallan por **presencia**
de átomos. Un invariante existencial — o de forma $\forall\exists$, como
ID1 e ID2 del capítulo 07 — falla por **ausencia**, no tiene testigo de ese
tipo, y su violación queda fuera del aparato de retiro guiado por
$\preceq$: el check la detecta igual, pero la reparación es una obligación
de añadir (o de rechazar la transacción entera, que es el modo por defecto),
no un retiro que la maquinaria del capítulo 03 pueda decidir.

**No todo compromiso cabe en una oración sobre un estado.** El capítulo 07
lo dijo para "todo retiro deja registro": esa exigencia relaciona dos
estados (antes y después de la operación) y **no es expresable como oración
FO estática sobre un solo estado**, así que el check no puede verificarla;
se impone como **obligación del esquema de regla** de revisión — el lado
derecho de la regla incluye el acta —, una garantía de construcción de
reglas, no un teorema del motor de model checking.

## La recuperación exige disciplina de diseño

El capítulo 04 lo dijo sin eufemismo: el emparejamiento de patrones es, en
general, subgraph isomorphism, NP-completo, y **ningún índice lo hace
polinomial** para entradas arbitrarias. La tratabilidad es condicional y la
condición la paga el diseño:

- **Patrones pequeños o de treewidth acotado.** La operación diaria vive en
  patrones de 3–10 nodos (conteo elemental, polinómico de grado alto — y
  $n^{10}$ es polinomial pero no es barato) o de forma simple con garantías
  FPT (color-coding). Una consulta que salga de ese régimen debe declararse
  cara en lugar de fingirse barata; un camino de longitud no acotada ni
  siquiera es un patrón único — es una familia con cota fija, y la cota es
  parte del diseño de la consulta, no un resultado.
- **La poda por tipos es ingeniería, no teorema.** Reduce el trabajo
  esperado en los universos que de hecho construimos; el peor caso
  asintótico no mejora, y un universo degenerado con todos los nodos del
  mismo tipo no gana nada.
- **WL descarta, nunca confirma.** El filtro Weisfeiler-Leman impone una
  disciplina asimétrica: se descarta una región solo por **ausencia
  demostrable** de estructura que el patrón exige — nunca por desajuste de
  resúmenes completos, porque en búsqueda de subgrafos los vecinos externos
  a la ocurrencia refinan los colores y descartar por desigualdad mataría
  matches verdaderos —, y si la región pasa el filtro no se ha demostrado
  nada: hay que correr el match de verdad. Además, su garantía de "casi
  todos" es sobre grafos aleatorios uniformes y no transfiere a nuestros
  universos estructurados.

## La terminación no es gratis

Dos disciplinas del capítulo 05 son obligaciones permanentes, no
propiedades heredadas:

**Check tras cada operación, con excepciones solo por teorema.** Una regla
DPO o SqPO garantiza que el resultado es un grafo bien formado, no que
satisfaga $I$: bien formado y consistente son propiedades distintas, y los
invariantes universales no se preservan bajo extensiones (el contraejemplo
de dos nodos y una arista del capítulo 02). Por eso el protocolo es
transaccional y el check corre después de **cada** regla. La única excepción
está probada, no confiada: bajo refine, se preservan los invariantes
relativizados por guardas al nivel superior (capítulo 01); todo lo demás se
rechequea.

**SqPO solo con match inyectivo.** Clonar y fusionar están garantizados bajo
la hipótesis exacta del capítulo 05: el complemento de pullback final está
probado para match **mónico** (inyectivo). Con match no inyectivo la
existencia no está garantizada en general, y por eso $U$ restringe las
operaciones SqPO a matches inyectivos — la restricción es la hipótesis del
teorema, no una preferencia de estilo, y fuera de ella no hay garantía.

**Cada regla de auto-reparación debe exhibir su medida.** El orden de
multiconjuntos de Dershowitz y Manna 1979 da el **marco** de terminación, no
la terminación de ninguna regla concreta. Por cada regla de auto-reparación
que se añada hay que exhibir su medida — qué multiconjunto sobre qué orden
base bien fundado — y probar que cada aplicación la decrece estrictamente.
La prueba es por regla; no se hereda ni se asume. Una regla sin medida
probada es una regla cuya terminación no está establecida, y este estudio no
la declara segura. A la fecha, $U$ no fija ningún conjunto de reglas de
auto-reparación: el modo por defecto ante un check fallido es rechazar, que
termina trivialmente.

## Lo que este proyecto NO promete

El universo no **genera** corrección: la **filtra**. Todo lo que los
capítulos 02 a 05 y 07 construyen es un mecanismo para que un error ya
axiomatizado no vuelva a entrar: convierte "no repetir errores conocidos" en
teorema — el check rechaza lo que viola $I$, la revisión retira con criterio,
la reescritura no deja estados rotos, el rationale no desaparece en
silencio. Los errores nuevos — el axioma malo, la dimensión sin invariante,
la prosa incoherente, la justificación vacía, la elección equivocada de
$\preceq$ — quedan exactamente donde siempre estuvieron: del lado del juicio
humano. El sistema hace cumplir lo que se le dijo; decidir qué decirle sigue
siendo el trabajo difícil.

Extensiones descartadas del núcleo, registradas como investigación futura y
no como capacidades:

- **Sheaves celulares** — descartado del núcleo; investigación futura.
- **Lógica temporal (GITL)** — descartado del núcleo; investigación futura.
- **Estrategias MSO** — descartado del núcleo; investigación futura.

Este capítulo no lleva sección de veredicto: **06 es, en sí mismo, el
veredicto negativo del proyecto** — la lista completa de lo que ningún otro
capítulo puede prometer.
