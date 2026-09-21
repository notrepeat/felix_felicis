# 02 — No-contradicción: qué es y por qué es decidible

Este capítulo fija qué significa "contradicción" en el universo $U$ del
capítulo 01, y demuestra —citando los teoremas exactos— que detectarla es un
problema decidible y tratable, mientras que la pregunta vecina (¿son
satisfacibles mis invariantes en general?) no lo es.

## Qué es contradicción aquí

**Definición 2 (Contradicción).** Un universo $U = \langle G, \tau, \lambda,
\iota, I, \preceq \rangle$ está en **contradicción** sii

$$\mathit{Str}(U) \not\models I$$

es decir, sii al menos un invariante $\varphi \in I$ no es satisfecho por la
estructura subyacente finita $\mathit{Str}(U) = \langle V, E, s, t, \tau,
\lambda, \iota \rangle$.

Conviene subrayar lo que esta definición **no** es. No es contradicción entre
proposiciones arbitrarias sobre el mundo: no decidimos si dos afirmaciones en
lenguaje natural se contradicen, ni si el contenido de dos cuerpos markdown
es incompatible (capítulo 01, capa OKF: el cuerpo queda fuera de la garantía
formal). Contradicción es, exactamente, la **violación de un invariante de
primer orden sobre una estructura finita dada**. Esta reducción es
deliberada: cambia un problema semántico abierto (consistencia de
conocimiento en general) por un problema de model checking sobre un objeto
finito y concreto — y es precisamente lo que compra la decidibilidad de la
sección siguiente. La elección de esta reducción como noción oficial de
contradicción del sistema es **construcción nuestra** (la matemática que la
sostiene, no: se cita abajo).

## El teorema que lo hace posible

El hecho central es elemental de enunciar: **el model checking de lógica de
primer orden sobre una estructura finita dada es decidible**. Dada
$\mathit{Str}(U)$ (finita, explícita) y una oración $\varphi$ de FO, la
semántica de Tarski da directamente un algoritmo: los cuantificadores
recorren dominios finitos ($V$ y $E$), así que la evaluación termina siempre.
No hay que decidir validez lógica; hay que evaluar una fórmula sobre datos.

La complejidad exacta está establecida:

- **Complejidad combinada** (la fórmula y la estructura son ambas entrada):
  el problema es **PSPACE-completo**. La dureza viene de QBF (QBF, fórmulas
  booleanas cuantificadas: SAT con ∀/∃ anidados), cuya PSPACE-completitud es
  de L. J. Stockmeyer y A. R. Meyer, "Word problems requiring exponential
  time", *Proc. 5th ACM STOC*, 1973; la
  clasificación PSPACE de la evaluación de consultas de primer orden es de
  M. Y. Vardi, "The complexity of relational query languages", *Proc. 14th
  ACM STOC*, 1982.
- **Complejidad de datos** (la fórmula está FIJA, solo la estructura es
  entrada): **polinómica**. Sea $n = |V| + |E|$ el tamaño de la estructura.
  Para una fórmula fija con $k$ variables, la
  evaluación naïve corre en $O(n^{k})$
  (Vardi 1982, la noción misma de data complexity se articula en
  ese trabajo). La exposición de libro: L. Libkin, *Elements of Finite Model
  Theory*, Springer, 2004, cap. 6.

La implicación práctica para $U$ es directa. El conjunto $I$ de invariantes
está **fijo** durante la operación normal del sistema (cambia solo cuando el
diseñador lo edita), y los invariantes útiles en la práctica son fórmulas
cortas con pocas variables — $I_1$ del capítulo 01 usa tres ($x$, $y$, $e$).
Bajo ese régimen aplica la complejidad de datos: **el check tras cada
operación es polinómico en el tamaño del grafo**, con exponente acotado por
el número de variables del peor invariante. Esto no es una promesa de
velocidad absoluta — un invariante con muchas variables encarece el
exponente — sino un análisis: mantener $I$ en FO con pocas variables es la
condición que mantiene el check barato.

## El límite del otro lado

La decidibilidad anterior es estrecha por diseño, y hay que decir con
precisión qué queda fuera:

- **Satisfacibilidad de FO en general es indecidible.** No existe algoritmo
  que, dada una oración arbitraria de primer orden, decida si tiene algún
  modelo (equivalentemente, la validez es indecidible): A. Church, "A note
  on the Entscheidungsproblem", *J. Symbolic Logic* 1, 1936; A. Turing, "On
  computable numbers, with an application to the Entscheidungsproblem",
  *Proc. London Math. Soc.*, 1936.
- **Restringirse a modelos finitos no rescata nada; lo empeora en un sentido
  preciso.** El teorema de Trakhtenbrot (B. A. Trakhtenbrot, 1950)
  establece que la satisfacibilidad sobre modelos **finitos** es indecidible
  (para firmas con al menos un símbolo de relación binaria — nuestro
  $\Sigma$ las tiene). La satisfacibilidad finita es recursivamente
  enumerable (se pueden enumerar las estructuras finitas y chequear cada
  una), de modo que su complemento — y con él la **validez finita**, que le
  corresponde vía negación — **no es siquiera semidecidible**: no hay sistema
  de prueba completo para las oraciones válidas en todos los modelos finitos.

La consecuencia para $U$: **no podemos decidir, en general, si un conjunto
$I$ de invariantes es satisfacible por ALGÚN grafo**. Solo podemos chequear
grafos concretos. La línea divisoria, en las preguntas del usuario del
sistema:

- "¿Existe algún diseño que cumpla todos mis axiomas?" — **fuera** del
  alcance decidible (es satisfacibilidad finita de $I$; Trakhtenbrot).
- "¿Este diseño cumple?" — **dentro** (es model checking de $I$ sobre
  $\mathit{Str}(U)$; decidible y polinómico con $I$ fijo).

El sistema, por tanto, nunca certificará que unos axiomas son coherentes en
abstracto; certificará, tras cada operación, que el grafo presente los
satisface.

## Por qué el check es obligatorio tras cada operación

Podría pensarse que un universo consistente sigue consistente mientras "solo
se añada" — que la contradicción exigiría borrar o mutar algo. Es falso, y
la razón es elemental: **los invariantes con cuantificación universal no se
preservan bajo extensiones de la estructura**. Añadir nodos o aristas puede
crear exactamente el contraejemplo que $\forall$ prohíbe.

Ejemplo mínimo con $I_1$ del capítulo 01 (instanciación de diseño de
software):

$$I_1 \equiv \forall x\, \forall y\, \neg \exists e\, \big(
\mathit{Effect}(x, \mathit{Puro}) \wedge \mathit{Effect}(y,
\mathit{Adaptador}) \wedge \mathit{Edge}(e, x, y, \delta) \big)$$

Sea $\mathit{Str}(U)$ con dos nodos de tipo Transformación: $a$ con
$\mathit{Effect}(a, \mathit{Puro})$ y $b$ con $\mathit{Effect}(b,
\mathit{Adaptador})$, y **sin aristas**. Se cumple $\mathit{Str}(U) \models
I_1$ vacuamente (la sorte de aristas es vacía, así que ningún $\exists e$
puede testificar nada). Una sola operación de extensión — añadir la arista $e_1$
con $\mathit{Edge}(e_1, a, b, \delta)$ — produce $\mathit{Str}(U') \not\models
I_1$: el par $(a, b)$ que antes no testificaba nada ahora testifica la
violación. Nada se borró; solo se añadió. Por eso el check corre **después
de cada operación**, no solo tras las destructivas (el capítulo 05 fija el
protocolo exacto).

Nota de honestidad: esta observación es **elemental** — un contraejemplo de
dos nodos y una arista; no requiere el teorema de Rossman ni ninguna
maquinaria de teoría de modelos finitos. El resultado profundo en esta
vecindad es el teorema de preservación por homomorfismos de Rossman: una
fórmula de FO es preservada bajo homomorfismos sobre todas las estructuras
(y también restringido a las finitas) sii es equivalente a una fórmula
existencial-positiva (B. Rossman, "Homomorphism preservation theorems",
*J. ACM* 55(3), 2008; el caso clásico es anterior — la contribución de
Rossman es que sobrevive restringido a las finitas). Ese teorema aplica a **FO puro**, y **no se extiende
gratis a FO+TC** (primer orden con clausura transitiva, que $U$ necesita
p. ej. para la condición de bosque de $\iota$): el documento previo generado
por IA que este estudio reemplaza invocaba Rossman como si cubriera también
esos invariantes con clausura transitiva, y eso es un error que aquí queda
corregido. Para nuestro argumento no hace falta Rossman en ninguna
dirección: la no-preservación de los $\forall$ bajo extensiones se muestra
con el contraejemplo de arriba, y la preservación positiva que sí usamos
(relativización guardada bajo refine) se probó directamente en el
capítulo 01.

## Veredicto

**Fundado con condiciones.** Chequear un grafo concreto contra $I$ es
decidible (semántica de Tarski sobre estructura finita) y tratable con $I$
fijo: polinómico en $n = |V| + |E|$ con exponente el número de variables
(Vardi 1982;
Libkin 2004, cap. 6). Las condiciones: (1) la expresividad de $I$ debe
mantenerse en FO con pocas variables — fórmulas con muchas variables
encarecen el exponente polinómico; solo si $I$ se trata como entrada
variable aplica el régimen PSPACE de la complejidad
combinada; (2) la satisfacibilidad global de $I$ — si existe grafo alguno
que cumpla los axiomas — es indecidible en general (Trakhtenbrot 1950) y
queda fuera del sistema: el check responde "¿este grafo cumple?", nunca
"¿mis axiomas son coherentes?".
