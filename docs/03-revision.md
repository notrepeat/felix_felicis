# 03 — Revisión: cómo lo nuevo gana

El capítulo 02 fijó qué es contradicción y cómo detectarla. Este capítulo
responde la pregunta siguiente: cuando la detección da positivo porque llegó
conocimiento nuevo, ¿qué cede? La respuesta se apoya en la teoría clásica de
revisión de creencias y se adapta —con costes que se nombran— a nuestra
estructura finita.

## El problema

Llega un **delta**: un conjunto finito de átomos nuevos $\Delta \subseteq
\mathit{At}(\Sigma)$ que se quiere incorporar a $U$. Dos cosas pueden salir
mal:

1. Incorporar $\Delta$ deja $\mathit{Str}(U') \not\models I$ — el universo
   extendido viola un invariante (Definición 2, capítulo 02). El ejemplo
   mínimo del capítulo 02 es exactamente esto: una sola arista nueva crea el
   testigo que $I_1$ prohíbe.
2. $\Delta$ contradice átomos vigentes de forma directa: por ejemplo, un
   $\mathit{HasLab}(v, l')$ nuevo choca con el $\mathit{HasLab}(v, l)$
   vigente cuando un invariante exige etiqueta única por elemento (λ es
   función), o un $\mathit{Parent}(v, p')$ nuevo choca con el
   $\mathit{Parent}(v, p)$ vigente (ι es mapa parcial). Este caso es un
   subcaso del anterior — el choque siempre se manifiesta como violación de
   algún invariante sobre la estructura combinada — pero conviene nombrarlo,
   porque es el caso típico de la operación diaria.

Hay una política trivial: rechazar todo delta que produzca contradicción. Es
segura y es inútil a largo plazo: un universo que solo sabe rechazar se
**fosiliza** — su conocimiento inicial queda congelado como verdad perpetua y
toda corrección posterior rebota contra él. Un sistema de conocimiento vivo
necesita lo contrario: poder **retirar conocimiento viejo para admitir el
nuevo**, y hacerlo con criterio, no al azar. Ese es exactamente el problema
que la teoría de revisión de creencias formalizó.

## Base: AGM

La referencia fundacional es C. Alchourrón, P. Gärdenfors y D. Makinson, "On
the logic of theory change: partial meet contraction and revision functions",
*Journal of Symbolic Logic* 50, 1985 (en adelante AGM 1985). Su marco: un
estado de creencias es un **belief set** $K$, un conjunto de oraciones
**cerrado bajo consecuencia lógica** ($K = \mathit{Cn}(K)$). Sobre $K$ se
definen tres operaciones: expansión ($K + \varphi$: añadir y cerrar),
**contracción** ($K \dot{-} \varphi$: retirar $\varphi$ sin añadir nada) y
**revisión** ($K * \varphi$: incorporar $\varphi$ manteniendo consistencia).
La contracción es la operación primitiva; la revisión se obtiene de ella por
la identidad de Levi: $K * \varphi = \mathit{Cn}((K \dot{-} \neg\varphi)
\cup \{\varphi\})$ — primero se hace sitio retirando $\neg\varphi$, luego se
expande con $\varphi$.

AGM 1985 postula qué debe cumplir cualquier contracción racional. Los **seis
postulados básicos de contracción**, con sus nombres estándar:

1. **Clausura** (closure): $K \dot{-} \varphi = \mathit{Cn}(K \dot{-}
   \varphi)$ — el resultado sigue siendo un belief set.
2. **Éxito** (success): si $\varphi \notin \mathit{Cn}(\emptyset)$, entonces
   $\varphi \notin K \dot{-} \varphi$ — la contracción de veras elimina
   $\varphi$, salvo que $\varphi$ sea un teorema lógico (los teoremas no se
   pueden retirar).
3. **Inclusión** (inclusion): $K \dot{-} \varphi \subseteq K$ — contraer
   nunca añade creencias.
4. **Vacuidad** (vacuity): si $\varphi \notin K$, entonces $K \dot{-}
   \varphi = K$ — retirar lo que no se cree no cambia nada.
5. **Recuperación** (recovery): $K \subseteq (K \dot{-} \varphi) + \varphi$
   — si tras contraer $\varphi$ se vuelve a expandir con $\varphi$, se
   recupera todo $K$: la contracción no pierde más de lo necesario.
6. **Extensionalidad** (extensionality): si $\varphi \leftrightarrow \psi \in
   \mathit{Cn}(\emptyset)$, entonces $K \dot{-} \varphi = K \dot{-} \psi$ —
   oraciones lógicamente equivalentes se contraen igual.

El resultado central de AGM 1985 es una caracterización: definen la
**contracción partial meet** — $K \dot{-} \varphi$ como la intersección de
una selección de los subconjuntos maximales de $K$ que no implican
$\varphi$ — y demuestran que **una operación satisface los seis postulados
básicos si y solo si es una contracción partial meet** (AGM 1985). Es decir:
los postulados no son deseos sueltos; caracterizan exactamente una
construcción.

Nótese lo que la caracterización deja abierto: la *selección*. Los
subconjuntos maximales que no implican $\varphi$ son en general **muchos**, y
partial meet exige una función de selección que elija entre ellos. Los
postulados no dictan cuál; cualquier selección da una contracción legítima.
Esta subdeterminación es estructural en AGM y reaparecerá abajo como nuestro
primer coste.

### Epistemic entrenchment

¿De dónde sale la función de selección? La respuesta clásica es de
P. Gärdenfors y D. Makinson, "Revisions of knowledge systems using epistemic
entrenchment", *Proc. TARK 1988*: un orden de **atrincheramiento epistémico**
$\leq$ sobre las oraciones — $\varphi \leq \psi$ se lee "$\psi$ está al menos
tan atrincherada como $\varphi$", es decir, ante la obligación de retirar una
de las dos, cede $\varphi$. El orden satisface postulados propios
(transitividad, dominancia: si $\varphi \vdash \psi$ entonces $\varphi \leq
\psi$; conjuntividad; minimalidad de lo no creído; maximalidad exclusiva de
los teoremas), y determina la contracción mediante una condición explícita:
$\psi$ sobrevive a la contracción de $\varphi$ sii $\psi \in K$ y ($\varphi <
\varphi \vee \psi$, o $\varphi$ es teorema).

El **teorema de representación** de Gärdenfors y Makinson 1988: las
contracciones generadas por órdenes de atrincheramiento que satisfacen esos
postulados son exactamente las contracciones que satisfacen los postulados
AGM de contracción **incluyendo los dos suplementarios** (los que gobiernan
la contracción de conjunciones), y viceversa — de toda contracción de esa
clase se puede extraer el orden que la genera. Nota de honestidad sobre la
fuerza exacta: la correspondencia es con la clase completa (básicos más
suplementarios), no con los seis básicos solos; los seis básicos solos
caracterizan partial meet sin restricción sobre la selección (AGM 1985), y
es el atrincheramiento el que aporta la selección coherente que los
suplementarios exigen.

La moraleja que nos llevamos: **un orden sobre las creencias es exactamente
la información que falta para que la revisión sea determinista**. Eso es lo
que $\preceq$ hace en $U$.

## Adaptación a grafos — construcción nuestra

AGM habla de teorías lógicamente cerradas e infinitas; nuestro universo es
una estructura finita con un conjunto finito de átomos vigentes. La
traducción no es automática y la construcción entera de esta sección es
**construcción nuestra** sobre la base citada arriba.

Primero hay que decir qué significa retirar un átomo, porque en un grafo la
retirada **cascada** — a diferencia de un belief set, donde quitar una
oración deja las demás intactas:

**Definición 3 (Retiro estructural).** Sea $A \subseteq \mathit{At}(\Sigma)$
un conjunto de átomos vigentes en $U$. El **retiro** de $A$ produce la
estructura $\mathit{Str}(U')$ que resulta de:

1. Por cada $\mathit{Type}(v, k) \in A$ con $v \in V$: eliminar el elemento
   $v$ de $V$ — sin tipo no hay elemento, pues $\tau$ es total — junto con
   todos sus átomos ($\mathit{HasLab}$, $\mathit{Parent}$ entrante — sus
   hijos pasan a raíz, como en el paso 3 — y saliente, y toda arista
   incidente a $v$: limpieza de aristas colgantes, pues $s$ y $t$ son
   totales).
2. Por cada $\mathit{Edge}(e, x, y, k) \in A$ (o $\mathit{Type}(e, k)$ con
   $e \in E$): eliminar la arista $e$ de $E$ con sus átomos.
3. Por cada $\mathit{Parent}(x, y) \in A$: dejar $\iota$ indefinida en $x$
   (el nodo pasa a raíz; sus descendientes no se tocan).
4. Por cada $\mathit{HasLab}(x, l) \in A$: el retiro aislado no es admisible
   — $\lambda$ es total, así que retirar la etiqueta exige o bien retirar el
   elemento (caso 1 o 2) o bien reemplazarla por un átomo
   $\mathit{HasLab}(x, l')$ nuevo; de dónde procede ese reemplazo lo fija la
   Definición 4, que es donde este paso se evalúa.

El **cierre por cascada** de $A$, $\mathit{casc}(A)$, es $A$ junto con todos
los átomos arrastrados por los pasos 1–2 (los átomos de los elementos
eliminados y de las aristas colgantes). Retirar $A$ es retirar
$\mathit{casc}(A)$: no existe el retiro parcial de un elemento.

**Adenda (2026-09-01, construcción propia; resuelve la tensión con ID2 del
capítulo 07).** El paso 1 admite una variante para el retiro **con acta de
supersesión**: en lugar de eliminar $v$ de $V$, la operación reemplaza
$\mathit{Type}(v, k)$ por $\mathit{Type}(v, \mathit{Retirado})$ —sorte
reservado en todo alfabeto $K_V$—, conserva $\mathit{HasLab}(v,
\lambda(v))$ y arrastra por cascada exactamente los mismos átomos que el
paso 1 (aristas incidentes, $\mathit{Parent}$, atributos de extensión). Así
$v$ conserva identidad para la arista $e_2$ del registro (ID2) sin
resucitar ninguno de sus átomos, y $s$, $t$ siguen totales. El invariante
ID4 del capítulo 07 impide que un nodo $\mathit{Retirado}$ participe en
otra relación que `supersede`. La eliminación literal de $v$ (paso 1 sin
variante) queda para los retiros sin acta. Detalle operativo en
`plan/07-procedencia.md`.

Con eso, la revisión:

**Definición 4 (Revisión de $U$ con un delta).** Dado $U$ consistente
($\mathit{Str}(U) \models I$) y un delta $\Delta \subseteq
\mathit{At}(\Sigma)$, llamamos **testigo mínimo** de violación a un conjunto
de átomos — vigentes y/o de $\Delta$ — **mínimo por inclusión** (como los
kernels de Hansson 1994) tal que ninguna estructura de firma $\Sigma$ que
satisfaga $I$ y respete la funcionalidad de $\tau$, $\lambda$ e $\iota$ (tipo
único, etiqueta única, a lo sumo un padre) e incidencia única de aristas
($s$, $t$: cada arista tiene origen y destino únicos) hace verdaderos todos
sus átomos.
La definición es sobre conjuntos de átomos, no sobre una "estructura
combinada": cuando $\Delta$ trae $\mathit{HasLab}(v, l')$ y rige
$\mathit{HasLab}(v, l)$ con $l \neq l'$, ninguna $\Sigma$-estructura
interpreta ambos a la vez ($\lambda$ es función; lo mismo con
$\mathit{Parent}$/$\iota$ y $\mathit{Type}$/$\tau$), y el par es directamente
un testigo de conflicto funcional. Nota de alcance: así se capturan las
violaciones de invariantes **universales**, que fallan por presencia de
átomos (como $I_1$); un invariante existencial falla por **ausencia** de
átomos y no tiene testigo de este tipo, de modo que la identificación
kernel = testigo de la Advertencia 2 vale solo en ese fragmento universal.
Una **revisión admisible** es un conjunto $R$ de átomos vigentes tal que:

1. **Éxito**: la estructura $\mathit{Str}(U')$ que resulta de retirar
   $\mathit{casc}(R)$ e incorporar $\Delta$ satisface $\mathit{Str}(U')
   \models I$. El retiro y la incorporación son **una sola operación
   simultánea** sobre el par $(R, \Delta)$: la estructura intermedia "ya
   retirado, aún no incorporado" no existe como estado, y en particular la
   admisibilidad del paso 4 de la Definición 3 se evalúa sobre el par — el
   $\mathit{HasLab}$ de reemplazo debe estar en $\Delta$, de modo que
   $\lambda$ nunca deja de ser total.
2. **Minimalidad**: ningún subconjunto propio de $R$ cumple 1 — no se retira
   nada superfluo (análogo en espíritu a recuperación/inclusión: perder lo
   mínimo).
3. **Prioridad**: la comparación es **local a cada conflicto**, no global a
   $\Delta$. Por cada testigo mínimo que $R$ golpea, el átomo retirado está
   estrictamente por debajo, según $\preceq$, de algún átomo de $\Delta$
   **presente en ese mismo testigo** — lo viejo solo cede ante lo que de
   veras lo contradice y está mejor atrincherado. La localidad cierra una
   trampa: si la comparación fuera contra $\Delta$ entero, bastaría colar en
   $\Delta$ un átomo de relleno muy atrincherado para autorizar retiros que
   el átomo realmente conflictivo — quizá el menos atrincherado del delta —
   no justificaría. $\preceq$ juega aquí el papel del orden de
   atrincheramiento de Gärdenfors y Makinson 1988, con dominio
   $\mathit{At}(\Sigma)$ en lugar de oraciones arbitrarias. Advertencia de
   honestidad: la prioridad se chequea sobre los átomos de $R$, pero los de
   $\mathit{casc}(R) \setminus R$ se retiran por arrastre estructural **sin
   peaje de prioridad** — no existe el retiro parcial de un elemento
   (Definición 3), y exigir prioridad también sobre la cascada volvería
   inadmisible casi cualquier revisión que toque un elemento con incidencias.

Si no existe ningún $R$ admisible — típicamente porque lo que estorba está
por encima de $\Delta$ en $\preceq$, o porque algún testigo mínimo se compone
solo de átomos de $\Delta$ (el delta es inconsistente con $I$ por sí mismo y
ningún retiro de lo vigente lo arregla) — el delta se **rechaza**: lo nuevo
no gana por ser nuevo, gana por estar mejor apoyado. La revisión es además
**todo-o-nada sobre $\Delta$** por decisión de diseño: o se incorpora el
delta completo o se rechaza completo; aceptar un subconjunto sería decidir
por el emisor qué quiso decir.

Dos advertencias honestas, que son los dos costes con nombre del veredicto:

**Advertencia 1: el mínimo no es único.** Puede haber varios $R$ admisibles
incomparables entre sí, igual que en AGM hay varios subconjuntos maximales
que no implican $\varphi$: la teoría entrega una **familia** de soluciones,
no una solución. Elegir exige información extra: un $\preceq$ total (o al
menos total sobre los átomos en conflicto) decide solo; con un preorden
parcial y empates, la elección queda fuera de la matemática y debe
resolverla otra cosa — normalmente, intervención humana o una política de la
instancia consumidora. Presentar la revisión como determinista sin haber
pagado ese precio sería falso.

**Advertencia 2: computar el retiro mínimo es duro en general.** Nuestro
problema es estructuralmente el de la **kernel contraction** de S. O.
Hansson, "Kernel contraction", *Journal of Symbolic Logic* 59, 1994: en
lugar de intersecar subconjuntos maximales inocentes (partial meet), se
computan los **kernels** — los subconjuntos mínimos del conjunto de creencias
que bastan para derivar lo indeseado — y una **función de incisión** corta al
menos un elemento de cada kernel. En nuestros términos: los kernels son
exactamente los testigos mínimos de la Definición 4 (capítulo 01: la
contradicción siempre es de conjuntos de átomos), y $R$ debe golpear todos
los testigos que involucran átomos vigentes. Eso es un problema de **conjunto de
golpeo mínimo** (minimal hitting set) sobre la familia de testigos, una
familia de problemas computacionalmente dura en general — NP-dura ya en sus
formulaciones básicas —, así que no hay algoritmo eficiente conocido para el
caso general. Lo que lo hace practicable en $U$ es el régimen de operación,
no un teorema: deltas **pequeños** (la operación diaria añade pocos átomos)
más invariantes **locales** (fórmulas cortas con pocas variables, la misma
condición que el capítulo 02 impone para el check barato) acotan los
testigos a vecindarios pequeños del delta, y la búsqueda del mínimo queda
acotada. Un delta enorme contra invariantes globales sale de ese régimen y
el sistema debe poder decir "no puedo resolver esto solo" en lugar de
prometer eficiencia que no tiene.

## Ejemplo de una política de prioridad epistémica

Una instancia de gestión de conocimiento puede inducir $\preceq$ desde su
frontmatter y resolver la Definición 4 así:

- **Standing**: $\mathit{firm} \succ \mathit{practice} \succ
  \mathit{hypothesis}$. Un átomo de un concepto `hypothesis` cede ante un
  delta apoyado en `practice`; nada por debajo de `firm` desplaza a un
  `firm`.
- **Origen**: decisión-humana $\succ$ inferencia-IA. Un átomo destilado por
  la IA nunca desplaza, por sí solo, uno fijado por decisión humana explícita
  del mismo standing.
- **Empate**: entre átomos del mismo standing y el mismo origen, lo nuevo
  gana **solo si trae rationale** — un registro del porqué del cambio, que
  es exactamente lo que el capítulo 07 (memoria documental) exige conservar
  como subgrafo de rationale; `log.md` es solo su vista cronológica
  derivada. Sin rationale, el empate se mantiene y la Advertencia 1 aplica:
  la elección sube al humano.

Esto instancia la moraleja de Gärdenfors y Makinson 1988: el sistema es
determinista exactamente hasta donde $\preceq$ es decisivo, y honesto sobre
el resto.

## Veredicto

**Fundado con condiciones.** La base es teoría publicada y sólida: los seis
postulados básicos de contracción y su caracterización por partial meet (AGM
1985), la generación de contracciones desde órdenes de atrincheramiento con
su teorema de representación (Gärdenfors y Makinson 1988) y la kernel
contraction (Hansson 1994). La adaptación a estructuras finitas con cascada
(Definiciones 3 y 4) es **construcción nuestra**, y carga dos costes con
nombre: (1) **no-unicidad** — la teoría entrega una familia de retiros
mínimos y elegir exige un $\preceq$ decisivo o intervención humana en los
empates; (2) **complejidad** — el retiro mínimo es un problema tipo
conjunto de golpeo mínimo, duro en general, practicable solo bajo el régimen
de deltas pequeños e invariantes locales que este estudio asume
explícitamente.
