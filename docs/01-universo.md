# 01 — El Universo: definición formal

Este capítulo define el objeto formal central del estudio. Todos los capítulos
posteriores (no-contradicción, revisión, recuperación, evolución, límites,
memoria documental) operan sobre las definiciones fijadas aquí.

## Definición del universo U

Fijamos primero la firma. Sea $\Sigma$ la firma bisortida (sorte de nodos,
sorte de aristas) con los símbolos de predicado $\mathit{Type}$,
$\mathit{Edge}$, $\mathit{Parent}$ y $\mathit{HasLab}$ (su lectura se da en
la sección siguiente) y con un símbolo de constante por cada elemento de
$V \cup E \cup K$ y por cada etiqueta en uso — todos conjuntos finitos,
porque la estructura es finita. Denotamos $\mathit{At}(\Sigma)$ al conjunto
(finito) de **oraciones atómicas** sobre $\Sigma$. Los átomos de conocimiento
son elementos de $\mathit{At}(\Sigma)$, con independencia de cuáles de ellos
valgan en un universo concreto.

**Definición 1 (Universo de conocimiento).** Un universo de conocimiento es una
tupla

$$U = \langle G, \tau, \lambda, \iota, I, \preceq \rangle$$

donde:

- $G = (V, E, s, t)$ es un **multigrafo dirigido finito**: $V$ es un conjunto
  finito de nodos, $E$ un conjunto finito de aristas, y $s, t : E \to V$ son
  las funciones fuente y destino. Al ser multigrafo, pueden existir varias
  aristas distintas entre el mismo par de nodos (distinguidas por su
  identidad y su tipo).
- $\tau : V \cup E \to K$ es el **tipado**: asigna a cada nodo y a cada arista
  un tipo del alfabeto finito $K = K_V \uplus K_E$ (tipos de nodo y tipos de
  arista, respectivamente). El tipado respeta las sortes:
  $\tau = \tau_V \uplus \tau_E$ con $\tau_V : V \to K_V$ y
  $\tau_E : E \to K_E$.
- $\lambda : V \cup E \to \mathit{Lab}$ es el **etiquetado semántico**: asigna
  a cada nodo y arista una etiqueta legible (nombre, descripción corta). La
  etiqueta identifica; no carga contenido lógico por sí misma. Aunque
  $\mathit{Lab}$ pueda ser infinito en abstracto, solo un número finito de
  etiquetas está en uso en cada universo (la estructura es finita), y solo
  esas aparecen como constantes de $\Sigma$.
- $\iota : V \rightharpoonup V$ es un **mapa parcial de padre** (mereología):
  $\iota(v) = p$ significa "el nodo $v$ es parte del nodo $p$". Se exige que
  el grafo de $\iota$ sea un **bosque**: ningún nodo es su propio ancestro,
  es decir $\forall v \in V : v \notin \mathit{descendientes}^{+}(v)$, donde
  $\mathit{descendientes}^{+}$ es la clausura transitiva de la relación
  hijo-de.
- $I$ es un **conjunto finito de invariantes**: oraciones de lógica de primer
  orden bisortida (sorte de nodos y sorte de aristas) sobre la **estructura
  subyacente** $\mathit{Str}(U) = \langle V, E, s, t, \tau, \lambda, \iota
  \rangle$. Los invariantes son la sede de la noción de contradicción: $U$
  está en contradicción sii $\mathit{Str}(U) \not\models I$ — la satisfacción
  es de la estructura subyacente, no de la tupla $U$, puesto que $I$ es
  componente de $U$ (capítulo 02).
- $\preceq \subseteq \mathit{At}(\Sigma) \times \mathit{At}(\Sigma)$ es un
  **preorden sobre los átomos** de conocimiento: la **prioridad epistémica**.
  Su dominio es todo $\mathit{At}(\Sigma)$, no solo los átomos actualmente
  verdaderos. Cuando una revisión obliga a retirar conocimiento, $\preceq$
  guía la elección de qué átomo cede; un preorden por sí solo no siempre
  decide, y el desempate exacto se define en el capítulo 03.

La tupla completa $U$, como ensamblaje, es **construcción nuestra**. Cada
componente, en cambio, descansa sobre formalismo publicado: los grafos tipados
con morfismo de tipado sobre un grafo de tipos son el objeto estándar de la
teoría algebraica de transformación de grafos (H. Ehrig, K. Ehrig, U. Prange,
G. Taentzer, *Fundamentals of Algebraic Graph Transformation*, Springer, 2006,
cap. 2, donde un grafo tipado se define como un grafo junto con un morfismo a
un grafo de tipos fijo — nuestro $\tau$ con codominio plano $K$ es una
variante debilitada de esa construcción, sin la restricción de incidencia que
el grafo de tipos impone sobre qué tipos de arista pueden conectar qué tipos
de nodo). Las restantes componentes se
citan en las secciones que las desarrollan: $\iota$ en «Granularidad y
refinamiento» (grafos jerárquicos y bigrafos), $I$ en el capítulo 02 (teoría
de modelos finitos) y $\preceq$ en el capítulo 03 (epistemic entrenchment).

## El nodo es identidad, no conocimiento

Una decisión de diseño central: el nodo **no contiene** conocimiento. Un nodo
$v \in V$ porta exactamente tres cosas: su identidad (el elemento $v$ mismo),
su tipo $\tau(v)$ y su etiqueta $\lambda(v)$. Nada más.

La unidad mínima de conocimiento es el **átomo lógico**: un hecho básico
sobre la estructura de incidencia. Los átomos son las oraciones de
$\mathit{At}(\Sigma)$; los que **valen** en $U$ son los verdaderos en
$\mathit{Str}(U)$. Su lectura:

- $\mathit{Type}(v, k)$ — el nodo (o arista) $v$ tiene tipo $k$;
- $\mathit{Edge}(e, x, y, k)$ — existe la arista $e$ de tipo $k$ con
  $s(e) = x$ y $t(e) = y$; el cuarto argumento es azúcar notacional:
  $\mathit{Edge}(e, x, y, k)$ abrevia la incidencia de $e$ entre $x$ e $y$
  en conjunción con $\mathit{Type}(e, k)$;
- $\mathit{Parent}(x, y)$ — $\iota(x) = y$;
- $\mathit{HasLab}(x, l)$ — $\lambda(x) = l$ (el predicado se llama
  $\mathit{HasLab}$ para no colisionar con el conjunto $\mathit{Lab}$).

El conocimiento vive, por tanto, en la **estructura de incidencia** — qué
está conectado con qué, bajo qué tipo, dentro de qué todo — y no dentro de
los nodos.

Esta decisión tiene una consecuencia directa sobre la noción de
contradicción: **la contradicción es una propiedad de conjuntos de átomos,
nunca de un nodo aislado**. Un invariante $\varphi \in I$ cuantifica sobre
nodos y aristas; su violación es siempre testificada por una combinación de
átomos (una arista prohibida entre dos nodos de ciertos tipos, un padre
faltante, una etiqueta duplicada). Un nodo solo no puede contradecir nada,
igual que una palabra sola no puede ser falsa: la falsedad exige una
afirmación, y la afirmación mínima en $U$ es el átomo, que ya relaciona al
nodo con algo más (su tipo, su padre, otra arista, otro nodo).

## Granularidad y refinamiento

El mapa parcial $\iota$ dota a $U$ de una **mereología**: los nodos forman un
bosque de partes y todos, y un nodo puede ser, a la vez, hoja hoy y padre
mañana. Esto habilita la operación de **refinamiento** (refine): tomar un
nodo $v$ que hasta ahora era atómico y expandirlo en un subgrafo interno
$G_v$ cuyos nodos declaran $\iota(\cdot) = v$. El nodo $v$ conserva su
identidad, su tipo y todas sus aristas de nivel superior; lo que cambia es
que ahora tiene estructura interna.

La propiedad que hace útil al refinamiento es de preservación, y hay que
enunciarla con cuidado: los cuantificadores de primer orden recorren todo
$V$, y refine **añade** nodos a $V$, de modo que un invariante puede romperse
aunque no mencione $\mathit{Parent}$. Ejemplo: "no hay etiquetas duplicadas",
$\forall x\, \forall y\, (\mathit{HasLab}(x, l) \wedge \mathit{HasLab}(y, l)
\to x = y)$ para cada etiqueta $l$ en uso (un esquema de oraciones, una por
constante de etiqueta), se rompe si un nodo interno nuevo repite una etiqueta
ya usada.
La condición correcta es sintáctica, por **relativización guardada**: decimos
que $\varphi \in I$ está *relativizado al nivel superior* respecto del
refinamiento de $v$ cuando todos sus cuantificadores de nodo están guardados
por $\mathit{Raiz}(x) \equiv \neg\exists p\, \mathit{Parent}(x, p)$ o por
$\mathit{Parent}(x, p)$ con $p$ un nodo fijo distinto de $v$, y todos sus
cuantificadores de arista exigen que ambos extremos satisfagan alguna de esas
guardas ($\mathit{Raiz}$ es una abreviatura: su cuantificador interno no
cuenta para esta condición). **Los invariantes así relativizados se preservan bajo refine**: la
operación solo añade nodos con padre $v$ (y aristas incidentes a ellos), así
que el rango de cada cuantificador guardado no cambia y los átomos que
$\varphi$ inspecciona quedan intactos. Todo invariante no relativizado debe
rechequearse tras la operación — este es exactamente el motivo por el que el
capítulo 05 exige check post-operación.

La base citada para grafos con jerarquía es doble. Primero, los **grafos
jerárquicos** con transformación: F. Drewes, B. Hoffmann, D. Plump,
"Hierarchical graph transformation", *Journal of Computer and System
Sciences* 64, 2002, definen grafos cuyos "frames" contienen grafos anidados a
profundidad arbitraria y extienden a ese contexto la transformación por doble
pushout (reescritura de grafos con semántica algebraica; capítulo 05).
Segundo, los **bigrafos**: R. Milner, *The Space and Motion of Communicating
Agents*, Cambridge University Press, 2009, formalizan exactamente la
coexistencia de dos estructuras sobre los mismos nodos — un bosque de
anidamiento (place graph) y un grafo de conexión (link graph) — como un solo
objeto matemático. Nuestra $\iota$ es un place graph de Milner sobre los
nodos de $G$ (el place graph ya es un bosque por definición), y nuestro $G$
es análogo al link graph — análogo, no idéntico: el link graph de Milner es
un hipergrafo sobre puertos, más general que nuestras aristas dirigidas.

La operación **refine** exacta sobre $U$ — con su condición de preservación
por relativización guardada enunciada arriba — es **construcción nuestra**
sobre esa base.

## Capa de representación: OKF

El universo $U$ es un objeto abstracto; en disco se serializa como bundle del
Open Knowledge Format (OKF v0.1: Google Cloud, "Introducing the Open
Knowledge Format", junio 2026;
cloud.google.com/blog/products/data-analytics/how-the-open-knowledge-format-can-improve-data-sharing).
El mapeo es normativo:

| Universo U | OKF |
|---|---|
| Nodo (identidad pura) | Archivo markdown — el path ES la identidad |
| $\tau$ (tipado) | Campo `type` (único obligatorio en OKF) |
| $\lambda$ (etiquetado) | `title` (`description` y `tags` quedan como metadatos fuera de la firma: decisión D3 del plan 01, 2026-09-01) |
| $\iota$ (mereología) | Jerarquía de directorios + `index.md` |
| Aristas tipadas | Links markdown entre conceptos + campo de tipo de arista |
| $\preceq$ (prioridad epistémica) | Campo `standing` (**construcción nuestra**: extensión del frontmatter) |
| Historial de revisión | `log.md` |
| $I$, check, revisión, match | No existen en OKF — son nuestro motor |

La división de trabajo es deliberada: OKF aporta la sintaxis portable
(markdown + frontmatter YAML, path como identidad, links como aristas);
nuestro universo aporta la semántica que OKF no define — invariantes, check
de consistencia, revisión.

Hay una tensión que conviene dejar documentada. En $U$, el nodo es identidad
pura; pero el archivo OKF que lo representa tiene un **cuerpo markdown
libre**, que evidentemente contiene contenido. La resolución es trazar la
frontera de la garantía formal: **frontmatter + links = átomos formales**,
cubiertos por el check ($\mathit{Type}$, $\mathit{HasLab}$, $\mathit{Parent}$ y
$\mathit{Edge}$ se leen exactamente de ahí); **el cuerpo markdown = contenido
humano anclado al nodo**, que queda fuera de la garantía formal. Una
contradicción escrita en prosa dentro de dos cuerpos no la detecta ningún
teorema de este estudio; se detecta por juicio (auditoría), no por check. El
alcance exacto de este límite se desarrolla en el capítulo 06.

## Instanciación: diseño de software

Ejemplo de instanciación de $U$ para arquitectura de software:

- Tipos de nodo: $K_V = \{\mathit{Sujeto}, \mathit{Objeto},
  \mathit{Transformación}\}$ — quién actúa, sobre qué dato, mediante qué
  operación.
- Tipos de arista: $K_E = \{\delta\}$, donde $\delta$ es la dependencia
  ("$x$ usa a $y$").
- Efectos: la instanciación extiende la firma con una función parcial
  $\gamma : V_{\mathit{Transformación}} \rightharpoonup \{\mathit{Puro},
  \mathit{Mediador}, \mathit{Adaptador}\}$ (donde
  $V_{\mathit{Transformación}}$ son los nodos de tipo Transformación), con
  átomo asociado $\mathit{Effect}(v, g)$. Los efectos no son tipos de nodo:
  son un atributo propio de esta instanciación, ordenado por
  $\mathit{Puro} \sqsubset \mathit{Mediador} \sqsubset \mathit{Adaptador}$
  — de menor a mayor contacto con el mundo exterior.
- Invariante de ejemplo, en lógica de primer orden:

  $$I_1 \equiv \forall x\, \forall y\, \neg \exists e\, \big(
  \mathit{Effect}(x, \mathit{Puro}) \wedge \mathit{Effect}(y,
  \mathit{Adaptador}) \wedge \mathit{Edge}(e, x, y, \delta) \big)$$

  — "ningún nodo de efecto Puro tiene arista de dependencia $\delta$ hacia
  uno de efecto Adaptador". $I_1$ es el caso extremo del esquema que
  $\sqsubset$ permite escribir en general: "ningún nodo tiene arista
  $\delta$ hacia otro de efecto estrictamente mayor". Un commit que
  introduzca esa arista deja $\mathit{Str}(U) \not\models I_1$:
  contradicción estructural, rechazada por el check.

## Instanciaciones consumidoras

Una instancia concreta define su vocabulario en un manifiesto: tipos de nodo,
tipos de arista, predicados de extensión, layout e invariantes. También define
la política que induce $\preceq$ sobre sus átomos. El motor no incorpora esas
decisiones de dominio ni necesita que el bundle viva dentro de este
repositorio.

El ejemplo ficticio incluido usa decisiones y principios como tipos de nodo.
Su propósito es mostrar la frontera: el check razona sobre los átomos formales,
mientras que el contenido libre del cuerpo markdown sigue requiriendo juicio
externo (capítulo 06).

## Veredicto

**Fundado.** Cada componente de $U$ es formalismo estándar publicado —
multigrafos tipados (Ehrig et al. 2006), jerarquía mereológica sobre grafos
(Drewes et al. 2002; Milner 2009), invariantes como oraciones de primer orden
sobre estructuras finitas, preórdenes de prioridad — y lo único que es
nuestro es el ensamblaje de esas piezas en una sola tupla, marcado como tal.
