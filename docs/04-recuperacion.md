# 04 — Recuperación: patrones para tareas repetitivas

Los capítulos 02 y 03 fijaron cómo el universo se defiende (check) y cómo
cambia (revisión). Este capítulo responde a la pregunta de uso diario: ¿cómo
se **saca** conocimiento de $U$? La respuesta es por patrones — y trae un
límite de complejidad que hay que enunciar sin maquillaje antes de listar los
escapes que lo hacen practicable.

## Tarea repetitiva = patrón + match

Una tarea repetitiva sobre el universo ("encuéntrame todos los sitios donde
pasa X") se formaliza como un grafo pequeño que describe la situación X y la
enumeración de sus apariciones en $\mathit{Str}(U)$. Esta formalización sobre
$U$ es **construcción nuestra** (el emparejamiento de subgrafos en sí es
estándar; lo nuestro es su adaptación a la firma de $U$, con tipado
obligatorio, etiquetas parciales y mereología):

**Definición 5 (Patrón).** Un **patrón** es una tupla $P = \langle G_P,
\tau_P, \lambda_P, \iota_P \rangle$ donde $G_P = (V_P, E_P, s_P, t_P)$ es un
multigrafo dirigido finito pequeño, $\tau_P : V_P \cup E_P \to K$ asigna
tipos del mismo alfabeto $K$ de $U$ respetando las sortes, como en el
capítulo 01: $\tau_P = \tau_{P,V} \uplus \tau_{P,E}$ con $\tau_{P,V} : V_P
\to K_V$ y $\tau_{P,E} : E_P \to K_E$, $\lambda_P : V_P \cup E_P
\rightharpoonup \mathit{Lab}$ es un etiquetado **parcial** — donde
$\lambda_P$ no está definida, el elemento es un comodín: cualquier etiqueta
sirve — y $\iota_P : V_P \rightharpoonup V_P$ es un mapa parcial de padre
cuyo grafo es un bosque. Un patrón es, pues, un fragmento de universo sin
invariantes ni prioridad: describe una forma, no afirma nada.

**Definición 6 (Ocurrencia).** Una **ocurrencia** de $P$ en $U$ es un
morfismo inyectivo $m : G_P \to G$ — un par de funciones inyectivas
$m_V : V_P \to V$ y $m_E : E_P \to E$ que conmutan con fuente y destino
($s(m_E(e)) = m_V(s_P(e))$ y $t(m_E(e)) = m_V(t_P(e))$) — tal que:

1. **respeta $\tau$**: $\tau(m(x)) = \tau_P(x)$ para todo $x \in V_P \cup
   E_P$;
2. **respeta $\lambda$ parcialmente**: $\lambda(m(x)) = \lambda_P(x)$ donde
   $\lambda_P$ está definida; los comodines no imponen nada;
3. **respeta $\iota$**: donde $\iota_P(v) = p$, vale $\iota(m_V(v)) =
   m_V(p)$.

En las instanciaciones con átomos de atributo adicionales — como
$\mathit{Effect}$ en diseño de software (capítulo 01) — el patrón puede
declarar el atributo sobre sus elementos y la ocurrencia debe preservarlo,
igual que el tipo.

**Recuperar conocimiento es enumerar ocurrencias**: la respuesta a la tarea
repetitiva es el conjunto $\{m : m \text{ es ocurrencia de } P \text{ en }
U\}$, y cada $m$ señala un lugar concreto del universo donde la situación
buscada existe.

### Ejemplo: el patrón "método de pago"

En la instanciación de diseño de software (capítulo 01: $K_V = \{
\mathit{Sujeto}, \mathit{Objeto}, \mathit{Transformación}\}$, $K_E =
\{\delta\}$, efectos $\gamma$ con $\mathit{Puro} \sqsubset \mathit{Mediador}
\sqsubset \mathit{Adaptador}$), la tarea repetitiva "encuéntrame todos los
adaptadores de pago que un mediador usa y que reportan a auditoría" es el
patrón $P$ con tres nodos y dos aristas:

- $V_P = \{a, b, c\}$, todos de tipo $\mathit{Transformación}$, con
  atributos $\mathit{Effect}(a, \mathit{Mediador})$, $\mathit{Effect}(b,
  \mathit{Adaptador})$ y $\mathit{Effect}(c, \mathit{Adaptador})$;
- $\lambda_P(b) = \text{"metodo-de-pago"}$ y $\lambda_P(c) =
  \text{"auditoria"}$; el nodo $a$ es comodín — no importa **qué** mediador
  sea, importa que sea un mediador;
- $E_P = \{e_1, e_2\}$ de tipo $\delta$, con $e_1 : a \to b$ (el mediador
  usa al adaptador de pago) y $e_2 : b \to c$ (el adaptador de pago depende
  del adaptador de auditoría).

Cada ocurrencia $m$ de $P$ en $U$ entrega un triple concreto
(mediador, adaptador de pago, salida de auditoría) del diseño real. Si la
tarea exige que la auditoría esté a distancia mayor que una arista, un
patrón único no basta — un camino de longitud no acotada no es un grafo
finito fijo — y se usa una familia finita de patrones, uno por longitud
hasta una cota fija; la cota es parte del diseño de la consulta, no un
resultado. Nótese que este patrón busca estructura que los invariantes
**permiten**: $e_2$ va de Adaptador a Adaptador, y $e_1$ de Mediador a
Adaptador sube en $\sqsubset$, cosa que el esquema del capítulo 01 no
prohíbe (prohíbe las aristas hacia efecto *mayor* solo en su caso extremo
$I_1$ y, en general, según qué instancias del esquema estén en $I$). El
match no es el check: el check busca violaciones, el match busca presencia.

## La verdad incómoda

El problema recién definido es, en su forma general, **subgraph isomorphism**,
y es NP-completo. Con precisión: dados dos grafos $P$ y $G$, decidir si $G$
contiene un subgrafo isomorfo a $P$ es un problema NP-completo. La membresía
en NP es trivial — una ocurrencia candidata $m$ se verifica en tiempo
polinomial comprobando inyectividad, incidencias, tipos y etiquetas. La
NP-dureza se obtiene por reducción desde problemas clásicos que son casos
particulares: CLIQUE (¿contiene $G$ un clique de tamaño $k$? — tomar como
$P$ el grafo completo de $k$ nodos) y ciclo hamiltoniano (tomar como $P$ el
ciclo de $|V_G|$ nodos), ambos en la lista de los 21 problemas de Karp
(R. M. Karp, "Reducibility among combinatorial problems", 1972), dentro del
marco de NP-completitud fundado por Cook (S. A. Cook, "The complexity of
theorem-proving procedures", 1971). CLIQUE es sobre grafos no dirigidos y
nuestros grafos son multigrafos dirigidos; la adaptación es rutinaria —
representar cada arista no dirigida por el par de aristas dirigidas
simétricas, y el ciclo hamiltoniano dirigido está él mismo en la lista de
Karp 1972. Nuestras restricciones adicionales —
tipos, etiquetas, padres — no eliminan la dureza: el caso con un solo tipo,
todo comodín y $\iota$ vacía es exactamente el problema general.

La consecuencia hay que decirla sin eufemismo: **ningún índice hace
polinomial el emparejamiento de patrones en general**. Un índice puede
precomputar, ordenar y descartar; no puede, salvo colapso de clases de
complejidad que nadie ha demostrado, convertir un problema NP-completo en
uno polinomial para entradas arbitrarias. Todo sistema que prometa "consulta
de subgrafos rápida siempre" está prometiendo algo falso o está restringiendo
la entrada sin decirlo. Nosotros restringimos la entrada y lo decimos: esa es
la sección siguiente.

## Los escapes reales

La dureza es del caso general. El régimen de operación de $U$ no es el caso
general, y cada escape siguiente se apoya o en un resultado publicado o en un
argumento que se marca honestamente como lo que es.

**1. Patrones pequeños.** La explosión combinatoria es exponencial en
$|P|$, no en $|G|$: el algoritmo de fuerza bruta que prueba todas las
asignaciones inyectivas de $V_P$ en $V$ examina $O(n^{|V_P|})$ candidatas
(con $n = |V|$) y verifica cada una en tiempo polinomial. Con $|P|$ acotado
por una constante, eso es **polinomial en $|G|$** — de grado alto, pero
polinomial. Es un argumento elemental de conteo y no necesita cita. Nuestros
patrones de operación diaria tienen entre 3 y 10 nodos (el ejemplo de arriba
tiene 3), así que la recuperación cotidiana vive dentro de este escape. El
precio queda a la vista: $n^{10}$ es polinomial pero no es barato, y por eso
los escapes 3 y 4 existen — reducen la constante práctica, no el exponente.

**2. Complejidad parametrizada.** El escape anterior se puede afinar con
teoría real. La técnica de **color-coding** de Alon, Yuster y Zwick ("Color-
coding", *Journal of the ACM* 42(4), 1995) encuentra subgrafos de $k$
vértices cuyo patrón tiene treewidth acotado (treewidth: cuán cerca está un
grafo de ser un árbol; caminos y árboles tienen treewidth 1) en tiempo
$2^{O(k)} \cdot
\mathrm{poly}(n)$ — exponencial solo en el parámetro $k$, polinomial en el
tamaño del universo, y con el exponente de $n$ independiente de $k$ (a
diferencia de la fuerza bruta del escape 1). Ese es el molde del marco
**FPT** (fixed-parameter tractable) de la complejidad parametrizada, cuya
referencia de libro es R. G. Downey y M. R. Fellows, *Parameterized
Complexity*, Springer, 1999. La lectura para $U$: si el diseño se compromete
a patrones pequeños **y** de forma simple (caminos, árboles, grafos de
treewidth acotado — el ejemplo de arriba es un camino de 3 nodos, treewidth
1), la recuperación tiene garantías asintóticas publicadas, no solo el conteo
elemental del escape 1.

**3. Poda por tipos.** El tipado $\tau$ obligatorio corta el espacio de
candidatos por nodo: un nodo de patrón con $\tau_P(x) = \mathit{Sujeto}$
solo puede ir a nodos de tipo Sujeto, y las etiquetas no comodín cortan aún
más (una etiqueta única deja un solo candidato). En universos donde los
tipos reparten la población, el factor de ramificación por nodo de patrón
baja de $n$ a la talla de la clase de tipo correspondiente. No hay teorema
general aquí — un universo degenerado con todos los nodos del mismo tipo no
gana nada, y el peor caso asintótico no mejora — así que este escape es
**argumento de ingeniería, no teorema**: reduce el trabajo esperado en los
universos que de hecho construimos, y nada más.

**4. Weisfeiler-Leman como filtro.** El refinamiento de colores (1-WL:
iterar "el color de un nodo es el multiconjunto de colores de sus vecinos"
hasta estabilizar) es una prueba barata — tiempo polinomial de grado bajo —
que compara resúmenes estructurales. Su poder está medido: el refinamiento
de colores identifica **casi todos** los grafos, en el sentido de que la
proporción de grafos de $n$ vértices que no distingue de algún no-isomorfo
tiende a cero (L. Babai, P. Erdős, S. M. Selkow, "Random graph isomorphism",
*SIAM Journal on Computing* 9, 1980). La garantía es sobre grafos aleatorios
uniformes; nuestros universos son estructurados (tipados, ralos,
jerárquicos), régimen donde ese "casi todos" no transfiere — 1-WL ya falla
en todos los grafos regulares, donde el refinamiento no separa ningún nodo.
El límite honesto también está medido:
WL tiene falsos "quizá" — las construcciones de Cai, Fürer e Immerman ("An
optimal lower bound on the number of variables for graph identification",
*Combinatorica* 12, 1992) exhiben pares de grafos no isomorfos que la
versión $k$-dimensional de WL no distingue, para **cada** $k$ fijo. La
disciplina de uso que esto impone es asimétrica, con una precaución extra en
nuestro contexto: "resúmenes distintos implican no-isomorfismo" vale para
grafos completos comparados entre sí, pero al buscar un **subgrafo** los
colores de los nodos del universo se refinan también con aristas externas a
la ocurrencia, así que un nodo que sí participa en un match puede tener
resumen distinto del nodo de patrón correspondiente solo por tener vecinos de
más — descartar por desigualdad de resúmenes mataría matches verdaderos. La
noción sonora de "incompatible" es de **contención**, monótona: una región se
descarta cuando le falta con certeza estructura que el patrón exige — ningún
nodo suyo tiene un vecindario cuyos multiconjuntos de tipos **contengan**
los que el patrón requiere (los colores de un 1-WL de igualdad no sirven
aquí: divergen por los mismos vecinos externos; solo valdrían refinados con
la propia regla de contención en cada ronda) —; la comparación por igualdad de
resúmenes solo es legítima sobre candidatos ya recortados a exactamente
$|P|$ elementos que se testean por isomorfismo. Se descarta solo por
ausencia demostrable, nunca por desajuste de resúmenes completos; y si la
región pasa el filtro, no se ha demostrado nada y hay que correr el match de
verdad. **WL sirve para descartar rápido, nunca para confirmar.**

## Veredicto

**Fundado con condiciones.** La recuperación por patrones descansa en un
problema NP-completo (Cook 1971; Karp 1972) y ningún índice ni truco lo hace
polinomial en general; presentar lo contrario sería falso. Es tratable **si y
solo si** el diseño paga la condición: patrones pequeños (3–10 nodos, conteo
elemental) o de treewidth acotado (color-coding, Alon-Yuster-Zwick 1995;
marco FPT, Downey y Fellows 1999), con la poda por tipos como ganancia de
ingeniería sin teorema y WL como filtro de descarte que jamás confirma
(Babai-Erdős-Selkow 1980; límite de Cai-Fürer-Immerman 1992). Este estudio
se compromete a esa disciplina de diseño: los patrones de $U$ se mantienen
pequeños o de treewidth acotado, y toda consulta que salga de ese régimen
debe declararse cara en lugar de fingirse barata.
