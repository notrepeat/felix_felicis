# 05 — Evolución: reescritura con check obligatorio

Los capítulos anteriores fijaron el objeto ($U$, capítulo 01), su noción de
contradicción y el check que la detecta (capítulo 02), la revisión que decide
qué cede cuando lo nuevo choca con lo vigente (capítulo 03) y la recuperación
por patrones (capítulo 04). Falta la pieza dinámica: **cómo cambia el grafo**.
Este capítulo fija la semántica de las operaciones de mutación y el protocolo
que las hace seguras.

## Operaciones como reglas de reescritura

La decisión central del capítulo: **toda mutación de $U$ — añadir, borrar,
clonar, fusionar, refinar — es la aplicación de una regla de reescritura de
grafos**. Una regla es una tupla

$$p = \langle L \leftarrow K \rightarrow R \rangle$$

donde $L$ (*left*) es el subgrafo que la regla exige encontrar, $R$ (*right*)
es lo que queda tras aplicarla, y $K$ (la *interfaz*) es la parte común: lo
que la regla preserva. Lo que está en $L$ pero no en $K$ se borra; lo que está
en $R$ pero no en $K$ se crea. Aplicar la regla requiere además un **match**
$m : L \to G$ — un morfismo de grafos que localiza una ocurrencia de $L$ en
el grafo $G$ de $U$, respetando tipos (el match del capítulo 04 es el caso
inyectivo de este objeto, usado allí en modo lectura y aquí en modo
escritura).

**Definición 7 (Operación sobre $U$).** Una operación sobre $U$ es un par
$(p, m)$ con $p = \langle L \leftarrow K \rightarrow R \rangle$ una regla
sobre grafos tipados por el alfabeto $K_V \uplus K_E$ del capítulo 01 y
$m : L \to G$ un match. (Advertencia notacional: la $K$ de la interfaz de la
regla es la letra estándar de la literatura de reescritura y no tiene
relación con el alfabeto de tipos $K$ de la Definición 1; el contexto
desambigua.) **Ninguna
mutación del grafo ocurre fuera de una regla.** Esta clausura es
**construcción nuestra** como decisión de diseño sobre $U$; los formalismos
de reescritura que le dan semántica se citan en las dos secciones siguientes.

La clausura es lo que hace **auditable** la evolución: si toda mutación es un
par $(p, m)$, entonces el historial del universo es una secuencia finita de
pares regla-match, cada uno con su verificación (sección del check, abajo).
No hay ediciones "a mano" sin rastro: un cambio que no se exprese como regla
no existe para el sistema. El costo de la decisión es simétrico y hay que
decirlo: toda operación que se quiera soportar debe primero formularse como
regla, incluidas las incómodas.

## DPO para lo conservador

La semántica por defecto de las operaciones es el **doble pushout** (DPO),
el enfoque algebraico original de la reescritura de grafos (H. Ehrig,
M. Pfender, H. J. Schneider, "Graph-grammars: an algebraic approach", *Proc.
14th Annual Symposium on Switching and Automata Theory*, 1973; el tratamiento
de libro es Ehrig, Ehrig, Prange, Taentzer, *Fundamentals of Algebraic Graph
Transformation*, Springer, 2006 — el mismo libro citado en el capítulo 01
para grafos tipados).

Glosario mínimo para el lector ingeniero, sin teoría de categorías previa:
un **pushout** es el pegado de dos grafos a lo largo de una parte común — la
unión que identifica exactamente lo compartido y nada más. "Doble pushout"
significa que la aplicación de la regla se descompone en dos pasos: primero
se resuelve el grafo intermedio $D$ — el **complemento de pushout**: el $D$
tal que pegar $L$ y $D$ sobre $K$ reconstruye $G$; operativamente, $G$
**menos** lo que la regla borra —, y luego se pega $R$ sobre $D$ a lo largo
de la interfaz $K$. En
pseudocódigo: `D = G - (L - K); H = D + (R - K)`, donde el "+" y el "−" son
pegados y recortes que respetan la estructura, no operaciones de conjuntos
sueltas.

DPO impone la **condición de pegado** (*gluing condition*) para que el paso
intermedio exista. Su parte operativa es la **condición de aristas
colgantes** (*dangling condition*): **no se puede borrar un nodo mientras
existan en $G$ aristas incidentes a él que la regla no borra también**. Si el
match toca un nodo a eliminar que tiene aristas no contempladas por $L$, la
regla simplemente **no es aplicable** en ese match. (La otra parte, la
condición de identificación, prohíbe que el match colapse en un mismo
elemento algo que la regla borra con algo que preserva.)

Esta es la propiedad que queremos por defecto: **DPO falla ruidosamente en
lugar de dejar un grafo roto**. La alternativa — borrar el nodo y dejar
aristas sin origen o sin destino — produciría un objeto que ni siquiera es un
grafo según la Definición 1 ($s, t : E \to V$ son funciones totales). Con
DPO, el error se manifiesta como "regla no aplicable" antes de tocar nada, y
el universo queda intacto. Para las operaciones conservadoras — añadir un
nodo, añadir una arista, borrar una arista, borrar un nodo explícitamente
desconectado, y el refine del capítulo 01 (que solo añade nodos internos y
aristas incidentes a ellos) — DPO es exactamente el comportamiento correcto,
y es el **modo de operación por defecto** de $U$.

## SqPO para fusión y clonación

DPO no puede expresar dos operaciones que $U$ necesita: **clonar** un nodo
(duplicarlo con su contexto) y **fusionar** dos nodos en uno. Para ellas se
adopta el **sesqui-pushout** (SqPO: A. Corradini, T. Heindel, F. Hermann,
B. König, "Sesqui-pushout rewriting", *Proc. ICGT 2006*, LNCS 4178,
Springer, 2006).

En SqPO la regla tiene la misma forma $\langle L \leftarrow K \rightarrow R
\rangle$, pero los dos morfismos de la regla pueden ser **no inyectivos**, y
cada lado codifica una operación distinta:

- **$l : K \to L$ no inyectivo ⇒ clonación.** Si dos elementos de $K$ se
  envían al mismo elemento de $L$, la construcción del paso intermedio — el
  **complemento de pullback final** (FPC) — **duplica** la estructura
  matcheada: el nodo se copia tantas veces como preimágenes tenga, junto con
  **todas sus aristas incidentes en $G$** — incluso las que la regla no
  menciona. La intuición del FPC, en una línea: es la
  manera **más grande** (más conservadora, la que retiene más contexto) de
  quitar de $G$ lo que la regla quita, y esa maximalidad es la que arrastra
  copias del contexto cuando $l$ colapsa elementos.
- **$r : K \to R$ no inyectivo ⇒ fusión.** Si dos elementos de $K$ se envían
  al mismo elemento de $R$, el pushout final **cocienta**: identifica en el
  resultado los elementos correspondientes de $G$. Dos nodos pasan a ser
  uno, y las aristas de ambos quedan conectadas al nodo fusionado.

Los papeles no son intercambiables: la clonación vive en el lado izquierdo
($l$) vía el complemento de pullback final, y la fusión en el lado derecho
($r$) vía el pushout. **Nota de corrección**: el documento previo generado
por IA que este estudio reemplaza tenía estos papeles invertidos (atribuía la
clonación a $r$ y la fusión a $l$); la asignación correcta es la de arriba y
es la del paper de Corradini et al. 2006.

Un glosario más para el lector ingeniero: un **pullback** es el dual del
pushout — en lugar de pegar dos grafos por su parte común, calcula la parte
común de dos grafos vistos dentro de un tercero (una intersección
generalizada). Un **complemento de pullback** para $K \to L \to G$ es un
grafo intermedio $D$ con $K \to D \to G$ que completa el cuadrado como
pullback — es decir, un candidato a "$G$ menos lo borrado"; el complemento de
pullback **final** es el mayor de esos candidatos, único salvo isomorfismo
cuando existe.

Y la condición de existencia hay que enunciarla con la precisión del paper,
no más fuerte: en categorías de grafos, **el complemento de pullback final
existe para $l : K \to L$ arbitrario cuando el match $m : L \to G$ es mónico
(inyectivo)**. No afirmamos que exista para matches arbitrarios: con matches
no inyectivos la existencia no está garantizada en general, y Corradini et
al. desarrollan justamente el caso mónico como el bien comportado. Por eso
$U$ **restringe las operaciones SqPO a matches inyectivos**: la restricción
no es una preferencia de estilo sino la hipótesis bajo la cual la
construcción que usamos está probada.

El uso de SqPO exclusivamente para clone y merge sobre $U$, con DPO como
defecto para todo lo demás, es **construcción nuestra** (una política de
selección de formalismo, no un teorema).

## Por qué check tras cada regla

El capítulo 02 demostró con un ejemplo mínimo que **los invariantes
universales no se preservan bajo extensiones**: añadir una sola arista puede
crear el contraejemplo que un $\forall$ prohíbe, sin borrar nada. La
reescritura no cambia ese hecho — una regla DPO o SqPO garantiza que el
resultado es un **grafo bien formado** (sin aristas colgantes, con tipado
total), pero no garantiza que satisfaga $I$. Bien formado y consistente son
propiedades distintas; la regla asegura la primera, el check la segunda.

El protocolo es por tanto **transaccional**:

$$\text{regla } (p, m) \;\to\; \text{grafo candidato } H \;\to\;
\text{check } (\mathit{Str}(U') \models I\,?) \;\to\;
\text{admitir} \;/\; \text{rechazar}$$

El candidato $H$ no reemplaza a $G$ hasta que el check pasa. Si falla, la
operación completa se descarta y $U$ queda como estaba: nunca hay un estado
intermedio observable que viole $I$. El costo del check es el del model
checking del capítulo 02, mantenido barato por la disciplina de invariantes
de cuantificación acotada que ese capítulo fija.

Dos interacciones con capítulos anteriores acotan cuándo el check puede
ahorrarse trabajo:

- **Con refine (capítulo 01):** los invariantes **relativizados por guardas**
  al nivel superior se preservan bajo refine — está probado allí, porque
  refine solo añade nodos con padre $v$ y el rango de los cuantificadores
  guardados no cambia. Para esa operación específica, esos invariantes no
  necesitan rechequearse; **todo lo demás se rechequea**. La excepción es por
  teorema, no por confianza.
- **Con la revisión (capítulo 03):** la revisión no es un mecanismo aparte
  que mute el grafo por su cuenta. El retiro estructural (Definición 3) y la
  revisión con delta (Definición 4) se expresan como aplicación de reglas —
  retirar un átomo es una regla de borrado (un esquema instanciado por
  estado: su lado $L$ se computa de $\mathit{casc}(A)$ sobre el $G$ vigente,
  no una regla fija de catálogo); la cascada $\mathit{casc}(A)$ es
  la clausura que evita exactamente las aristas colgantes que DPO prohíbe —
  más la maquinaria de prioridad $\preceq$ que decide **qué** regla aplicar.
  La revisión elige; la reescritura ejecuta; el check confirma.

## Terminación del mantenimiento

Si el sistema incluye **reglas de auto-reparación** — reglas que se disparan
automáticamente para restaurar invariantes tras un fallo del check, en lugar
de rechazar la operación — aparece un riesgo nuevo: que la reparación no
termine (una regla repara algo y rompe otra cosa, cuya reparación rompe lo
primero). Para descartar ese riesgo se exige una **medida estrictamente
decreciente por aplicación**.

El marco citado es el **orden de multiconjuntos** de N. Dershowitz y
Z. Manna ("Proving termination with multiset orderings", *Communications of
the ACM* 22(8), 1979): si el orden base sobre los elementos es bien fundado
(sin cadenas descendentes infinitas), el orden inducido sobre los
multiconjuntos finitos de esos elementos también es bien fundado. La utilidad
práctica: una reparación típica **reemplaza un problema grande por varios
problemas estrictamente menores** (por ejemplo, una violación de invariante
por varias obligaciones locales más pequeñas), y eso — quitar un elemento y
añadir finitos elementos todos menores que él — es exactamente el paso que el
orden de multiconjuntos declara decreciente. Con la medida decreciendo en un
orden bien fundado, ninguna secuencia de reparaciones es infinita.

Honestidad requerida sobre el alcance: **el teorema da el marco, no la
terminación de ninguna regla concreta**. Para cada regla de auto-reparación
que se añada a $U$ hay que exhibir su medida (qué multiconjunto sobre qué
orden base) y probar que cada aplicación la decrece estrictamente. Esa prueba
es **por regla**, no se hereda ni se asume: una regla sin medida probada es
una regla cuya terminación no está establecida, y este estudio no la declara
segura. A la fecha, $U$ no fija ningún conjunto concreto de reglas de
auto-reparación; el modo por defecto ante un check fallido es **rechazar**
(transaccional, sección anterior), que termina trivialmente.

## Veredicto

**Fundado para la semántica de operaciones; fundado con condiciones para la
auto-reparación.** DPO (Ehrig-Pfender-Schneider 1973; Ehrig et al. 2006) y
SqPO (Corradini et al. 2006) son formalismos publicados con las garantías
exactas que se usan aquí — condición de pegado para el borrado seguro,
complemento de pullback final con match mónico para clonar, pushout no
inyectivo para fusionar — y lo nuestro es la clausura "toda mutación es una
regla" y la política de selección, marcadas como tales. La auto-reparación
dispone del marco de terminación de Dershowitz y Manna 1979, pero cada regla
concreta que se introduzca queda **pendiente de su prueba de medida
individual**; hasta entonces, el sistema opera en modo rechazo transaccional.
