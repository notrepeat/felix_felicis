# 07 — Memoria documental: el porqué como grafo

Los capítulos anteriores fijaron qué es el universo (01), cuándo está en
contradicción (02), cómo se revisa (03), cómo se consulta (04) y cómo muta
(05). Este capítulo trata la pregunta que ninguno de ellos responde: **por
qué** cada nodo está ahí. La tesis es que el porqué no es prosa adjunta al
grafo — es grafo.

## El porqué como grafo, no como prosa

Cada nodo de $U$ carga su universo documental: por qué existe, qué pregunta
lo originó, qué resolución lo fijó, qué implica, qué lo desplazó. La decisión
de diseño es que **todos esos elementos son nodos** de la misma clase que
cualquier otro: nodos OKF (capítulo 01, capa de representación) — identidad
pura en el path, tipo y etiqueta en el frontmatter, y un cuerpo markdown
donde vive la prosa del razonamiento — unidos al nodo que documentan por
**aristas tipadas**: `justifica`, `responde`, `motiva`, `implica`,
`supersede`.

La alternativa habitual es la contraria: el porqué como texto libre dentro
del artefacto (un comentario, una sección "Rationale", un changelog). Esa
alternativa tiene un defecto estructural que este estudio ya nombró: el
cuerpo markdown queda **fuera de la garantía formal** (capítulos 01 y 06).
Un porqué escrito solo en prosa no puede ser exigido por invariante, no puede
ser recorrido por match, y no puede ser retirado con cascada controlada
cuando lo que justificaba desaparece. Al reificar el porqué como subgrafo,
las tres cosas pasan a estar disponibles: la existencia de justificación es
un átomo $\mathit{Edge}$ como cualquier otro, chequeable (capítulo 02),
consultable (capítulo 04) y revisable (capítulo 03). La prosa no desaparece
— vive en el cuerpo de los nodos de rationale — pero la **estructura** del
razonamiento (qué justifica qué, qué respondió a qué, qué desplazó a qué)
queda dentro del grafo formal.

## Base citada

Cuatro obras publicadas, en tres linajes — deliberación, decisión,
procedencia —, cada una probada en práctica, sostienen este diseño.

**IBIS.** W. Kunz y H. Rittel, "Issues as elements of information systems",
Working Paper 131, Institute of Urban and Regional Development, University of
California, Berkeley, 1970, proponen estructurar la deliberación de problemas
mal definidos como una red de **issues** (preguntas), **posiciones**
(respuestas candidatas) y **argumentos** (a favor y en contra), conectados
por relaciones tipadas. La idea central que tomamos: la deliberación no es un
documento lineal sino una red tipada, y las preguntas son ciudadanos de
primera clase.

**gIBIS.** J. Conklin y M. L. Begeman, "gIBIS: a hypertext tool for
exploratory policy discussion", *ACM Transactions on Office Information
Systems* 6(4), 1988, implementan IBIS como hipertexto en grafo — nodos
tipados, aristas tipadas, navegación por la red de deliberación — y lo
validan en uso real de diseño. gIBIS es el precedente directo de "rationale
como grafo tipado navegable": demuestra que la estructura IBIS sobrevive el
contacto con proyectos reales, y también documenta sus costes (capturar la
deliberación en la red exige disciplina en el momento de deliberar).

**ADR.** M. Nygard, "Documenting architecture decisions", entrada de blog,
2011 — hoy práctica estándar en ingeniería de software —, propone registrar
cada decisión de arquitectura como un documento corto con **contexto,
decisión y consecuencias**, versionado junto al artefacto que documenta, con
un ciclo de vida explícito (una decisión puede quedar *superseded* por otra).
De ADR tomamos dos cosas: la unidad "resolución con contexto y
consecuencias" y el principio de que el registro vive **junto al artefacto**
y comparte su ciclo de vida, no en un sistema aparte.

**W3C PROV-DM.** *PROV-DM: The PROV Data Model*, W3C Recommendation, 2013,
define un modelo formal de procedencia con **entidades**, **actividades** y
**agentes**, y relaciones tipadas entre ellos — entre otras,
`wasDerivedFrom` (una entidad se derivó de otra), `wasInvalidatedBy` (una
actividad invalidó una entidad) y `wasAttributedTo` (una entidad se atribuye
a un agente). PROV-DM aporta lo que IBIS y ADR no formalizan: la
**procedencia del cambio y del retiro** — qué se derivó de qué, qué invalidó
qué — como relaciones de un modelo de datos publicado, no como convención.

Los tres linajes se complementan sin solaparse: IBIS/gIBIS cubren la deliberación
(pregunta → resolución), ADR cubre la decisión con sus consecuencias, PROV-DM
cubre la procedencia (derivación, invalidación, atribución).

## Tipos de rationale en U

La traducción de esa base al universo $U$ es **construcción nuestra**. Se
formula como una extensión de la instanciación — igual que $\mathit{Effect}$
en diseño de software (capítulo 01) — y respeta la separación de sortes del
tipado $\tau = \tau_V \uplus \tau_E$ de la Definición 1.

**Definición 8 (Extensión de rationale).** Una instanciación de $U$ **con
rationale** extiende sus alfabetos de tipos así:

- $K_V$ se extiende con $\{\mathit{Pregunta}, \mathit{Resolución},
  \mathit{RegistroSupersesión}\}$;
- $K_E$ se extiende con $\{\mathit{justifica}, \mathit{responde},
  \mathit{motiva}, \mathit{implica}, \mathit{supersede}\}$,

con la lectura direccional siguiente (fuente $\to$ destino):

| Arista | De → a | Lectura |
|---|---|---|
| `motiva` | Pregunta → nodo | esta pregunta originó este nodo |
| `responde` | Resolución → Pregunta | esta resolución responde a esta pregunta |
| `justifica` | Resolución → nodo | esta resolución justifica este nodo |
| `implica` | nodo → nodo | consecuencia declarada (las *consequences* de ADR) |
| `supersede` | desplazante → Registro → desplazado | supersesión reificada (abajo) |

La supersesión no es una arista suelta sino un **camino de dos aristas
`supersede` a través de un nodo $\mathit{RegistroSupersesión}$**: si $y$
desplaza a $x$, el registro $r$ satisface $\mathit{Edge}(e_1, y, r,
\mathit{supersede})$ y $\mathit{Edge}(e_2, r, x, \mathit{supersede})$. La
reificación existe para que la supersesión tenga identidad propia — un nodo
$r$ que porta la fecha y cuyo cuerpo markdown narra la causa — en lugar de
ser un hecho anónimo. Que este camino reificado sea la **única** forma
admitida de usar `supersede` no lo garantiza la definición por sí sola: lo
impone el invariante de buena formación ID3 (abajo).

El mapeo a PROV-DM: los nodos de $U$ son entidades; el
$\mathit{RegistroSupersesión}$ juega el papel de la **actividad** que efectuó
el cambio; el camino $y \to r \to x$ equivale a la conjunción
`wasInvalidatedBy` (la entidad $x$ fue invalidada por la actividad $r$) más
`wasDerivedFrom` (la entidad $y$ se derivó de $x$, a la que reemplaza). Es
decir: $\mathit{supersede} \approx \mathit{wasInvalidatedBy} +
\mathit{wasDerivedFrom}$, con el registro como pivote. Un campo de origen
definido por una instancia (por ejemplo, humano / proceso automatizado)
corresponde a `wasAttributedTo`. El mapeo es correspondencia conceptual,
marcada como **construcción nuestra**; no afirmamos conformidad formal con la
recomendación completa.

Dos átomos de atributo completan la extensión, ambos **construcción
nuestra** y análogos en factura a $\mathit{Effect}(v, g)$ del capítulo 01:

- $\mathit{Standing}(v, s)$ con $s \in \{\mathit{firm}, \mathit{practice},
  \mathit{hypothesis}\}$. En los capítulos 01 y 03 el standing vive en el
  frontmatter OKF e **induce** el preorden $\preceq$ sobre los átomos que
  mencionan a $v$; no es, por sí mismo, un átomo de la firma base. Para que
  un invariante pueda mencionarlo (ID1, abajo) hay que promoverlo
  honestamente a la firma: la instanciación con rationale añade el predicado
  $\mathit{Standing}$, leído del mismo campo de frontmatter que ya alimenta
  a $\preceq$. Una sola fuente, dos consumidores: el preorden y el check.
- $\mathit{Ts}(r, d)$ — el registro $r$ porta la fecha $d$, con $d$ una
  constante de fecha leída del frontmatter del registro. No se usa
  $\lambda$ para esto: la etiqueta identifica (capítulo 01) y cargarla
  además con datos conflaría nombrar con datar.

## Invariantes de completitud documental

Con la firma extendida, la consigna del diseño — "documentar deja de ser
disciplina y pasa a ser teorema" — se vuelve literal: la completitud
documental se escribe como oraciones de primer orden y entra en $I$. Los
tres invariantes son **construcción nuestra**.

**ID1 (ningún firme huérfano de porqué).**

$$\mathit{ID1} \equiv \forall v\, \big( \mathit{Standing}(v, \mathit{firm})
\to \exists r\, \exists e\, \big( \mathit{Type}(r, \mathit{Resolución})
\wedge \mathit{Edge}(e, r, v, \mathit{justifica}) \big) \big)$$

— todo nodo con standing firme tiene al menos una resolución entrante que lo
justifica. Un conocimiento puede nacer como hipótesis sin papeles, pero no
puede **ascender a firme** sin que exista, como nodo, la resolución que lo
sostiene.

**ID2 (ningún retiro sin acta).**

$$\mathit{ID2} \equiv \forall r\, \Big( \mathit{Type}(r,
\mathit{RegistroSupersesión}) \to \big( \exists e_1\, \exists y\,
\mathit{Edge}(e_1, y, r, \mathit{supersede}) \wedge \exists e_2\, \exists
x\, \mathit{Edge}(e_2, r, x, \mathit{supersede}) \wedge \exists d\,
\mathit{Ts}(r, d) \big) \Big)$$

— todo registro de supersesión está conectado al nodo desplazante, al nodo
desplazado, y porta fecha. Un registro colgante (sin ambos extremos o sin
fecha) viola $I$.

**ID4 (el retirado solo supersede; construcción nuestra, adenda
2026-09-01).** Resuelve la tensión con la Definición 3 del capítulo 03: el
desplazado $x$ no sale de $V$, se retipa al sorte reservado
$\mathit{Retirado}$ (adenda a la Definición 3). Para que la lápida no siga
contando como conocimiento:

$$\mathit{ID4} \equiv \forall e\, \forall x\, \forall y\, \forall k\, \big(
\mathit{Edge}(e, x, y, k) \wedge (\mathit{Type}(x, \mathit{Retirado}) \vee
\mathit{Type}(y, \mathit{Retirado})) \to k = \mathit{supersede} \big)$$

— toda arista que toca un nodo retirado es una arista `supersede` de su
acta. ID4 viaja con ID1–ID3 en el $I$ por defecto de la instanciación con
rationale.

**ID3 (buena formación del supersede).** ID2 obliga a los registros, pero
nada de lo anterior prohíbe una arista `supersede` directa $y \to x$ entre
dos nodos ordinarios — exactamente el hecho anónimo que la reificación dice
prevenir — ni impide que $\mathit{Ts}$ feche nodos que no son registros.
ID3 cierra ambos huecos (**construcción nuestra**):

$$\mathit{ID3} \equiv \forall e\, \forall x\, \forall y\, \big(
\mathit{Edge}(e, x, y, \mathit{supersede}) \to \mathit{Type}(x,
\mathit{RegistroSupersesión}) \vee \mathit{Type}(y,
\mathit{RegistroSupersesión}) \big) \;\wedge\; \forall r\, \forall d\, \big(
\mathit{Ts}(r, d) \to \mathit{Type}(r, \mathit{RegistroSupersesión}) \big)$$

— toda arista `supersede` toca un registro en al menos uno de sus extremos,
y $\mathit{Ts}$ solo fecha registros. A diferencia de ID1 e ID2, **ID3 es
del fragmento universal**: falla por **presencia** de átomos (la arista
directa ilegal, el $\mathit{Ts}$ fuera de dominio), de modo que sus
violaciones sí tienen testigos mínimos en el sentido del capítulo 03 y la
maquinaria de retiro guiado por $\preceq$ las cubre — el contraste con ID1 e
ID2 se detalla en la sutileza 1.

Las tres son oraciones de FO sobre $\mathit{Str}(U)$ extendida con los
átomos de la Definición 8, con pocas variables cada una — exactamente el régimen que el
capítulo 02 exige para el check barato. **Ese es el punto entero de la
construcción**: la completitud documental la verifica el mismo motor de model
checking del capítulo 02, tras cada operación, con el mismo protocolo
transaccional del capítulo 05. No hay un "linter de documentación" aparte;
hay invariantes.

Tres sutilezas, dichas sin adorno:

1. **ID1 e ID2 fallan por ausencia, no por presencia.** Ambos tienen forma
   $\forall\exists$: su violación es un átomo que **falta** (la arista
   `justifica`, la fecha), no un conjunto de átomos presentes. El capítulo
   03 advirtió que el aparato de testigos mínimos cubre el fragmento
   universal — el que falla por presencia — así que estas violaciones no
   tienen testigo de ese tipo. El check las **detecta** igual (model
   checking evalúa cualquier oración FO), pero la reparación no es un retiro
   guiado por $\preceq$: es una obligación de **añadir** (crear la
   resolución, completar el registro) o de renunciar al ascenso a firme. El
   modo por defecto sigue siendo el del capítulo 05: la operación que deja
   $\mathit{Str}(U') \not\models \mathit{ID1}$ se rechaza entera. ID3 es el
   contraste exacto: universal, falla por presencia, y sus violaciones caen
   de lleno en el aparato de testigos y retiro del capítulo 03.
2. **ID1 protege también contra el retiro de justificaciones.** Si una
   revisión retira la única resolución que justificaba un nodo firme, el
   check posterior falla ID1 y la transacción se rechaza — o bien la
   revisión incluye en su delta la degradación del standing o una
   justificación sustituta. El porqué no puede desaparecer en silencio
   mientras su justificado siga firme.
3. **La fecha es existencia, no aritmética.** $\mathit{Ts}$ permite exigir
   que la fecha *exista* y permite compararla por igualdad; ordenar fechas
   ("todo registro es posterior a lo que registra") exigiría añadir a la
   firma un orden sobre las constantes de fecha. Es una extensión posible y
   barata — un predicado binario más, interpretado sobre un dominio finito
   de fechas en uso — pero este capítulo no la necesita y no la incluye:
   ID2 solo pide la fecha, no su coherencia temporal.

## La revisión emite procedencia

El contrato con el capítulo 03 es este: **toda contracción produce su
$\mathit{RegistroSupersesión}$ en la misma transacción**.

La mecánica ya existe y no hay que inventar nada nuevo: el capítulo 05
estableció que la revisión se expresa como aplicación de reglas — la revisión
elige, la reescritura ejecuta, el check confirma — y que el protocolo es
transaccional: el candidato $H$ no reemplaza a $G$ hasta que el check pasa.
Basta entonces con que la regla de revisión que retira $\mathit{casc}(R)$ e
incorpora $\Delta$ (Definición 4) incluya en su lado derecho, además, el
nodo $r$ de tipo $\mathit{RegistroSupersesión}$, sus dos aristas
`supersede` y su átomo $\mathit{Ts}(r, d)$. El check que corre sobre el
candidato evalúa ID2 junto con el resto de $I$: una revisión que retirara
sin acta produciría un candidato sin registro válido — y si el sistema
declara además el invariante de que todo retiro deja registro (una
formulación por regla, no expresable como oración estática sobre un solo
estado, y por eso impuesta como **obligación del esquema de regla**,
construcción nuestra), la ausencia del acta hace fallar la transacción
entera. No hay estado observable en que el conocimiento se fue y el porqué
de su ida no está.

Una pregunta natural es si esto abre un regreso infinito: si documentar
exige nodos, ¿quién documenta a los documentadores? No hay regreso: emitir
un $\mathit{RegistroSupersesión}$ o una $\mathit{Resolución}$ no exige, por
sí mismo, rationale adicional — ID1 solo obliga a los nodos con
$\mathit{Standing}(v, \mathit{firm})$, y los nodos de rationale no portan
standing firme.

Esto invierte la relación habitual entre historial y fuente. En OKF, el
historial de revisión se serializa en `log.md` (capítulo 01, tabla de
mapeo). La posición de este capítulo: **`log.md` es una vista cronológica
derivada del subgrafo de registros, nunca la fuente**. Se regenera ordenando
los nodos $\mathit{RegistroSupersesión}$ por su $\mathit{Ts}$; si `log.md` y
el subgrafo discrepan, manda el subgrafo. La razón es la de todo este
estudio: lo que está en el grafo formal es chequeable e invariantable; un
archivo de texto acumulativo no lo es. Esto también instancia el empate del
capítulo 03: "lo nuevo gana solo si trae rationale" significa, ahora con
precisión, que el delta ganador viene acompañado de su subgrafo de rationale
— no de una promesa de escribirlo después.

## Auditoría adversarial como consulta

Una auditoría adversarial hace cuatro preguntas a cada pieza de
conocimiento: **cómo** se llegó a ella, **por qué** está, **cuándo** cambió
por última vez, **para qué** sirve. En un sistema donde el porqué es prosa,
las cuatro se responden leyendo texto con fe. Aquí las cuatro son
**consultas de camino sobre el subgrafo de rationale** — patrones en el
sentido de la Definición 5 (capítulo 04), con su maquinaria de match y sus
costes ya analizados allí. Para un nodo auditado $v$ fijo (constante, no
variable):

- **¿Por qué existe $v$?** — resoluciones justificantes:
  $$\exists r\, \exists e\, \big( \mathit{Type}(r, \mathit{Resolución})
  \wedge \mathit{Edge}(e, r, v, \mathit{justifica}) \big)$$
  Como patrón: un nodo $r$ de tipo Resolución con arista `justifica` hacia
  $v$. Las ocurrencias devuelven los nodos cuyos cuerpos narran la
  justificación.
- **¿Cómo se llegó a $v$?** — la deliberación completa, pregunta incluida:
  $$\exists q\, \exists r\, \exists e_1\, \exists e_2\, \big(
  \mathit{Type}(q, \mathit{Pregunta}) \wedge \mathit{Type}(r,
  \mathit{Resolución}) \wedge \mathit{Edge}(e_1, r, q, \mathit{responde})
  \wedge \mathit{Edge}(e_2, r, v, \mathit{justifica}) \big)$$
  El camino $q \leftarrow r \to v$ reconstruye el arco IBIS: qué pregunta se
  planteó, qué resolución la respondió, qué fijó esa resolución. Añadiendo
  la arista `motiva` ($q \to v$) se recupera también el origen directo.
- **¿Cuándo cambió, y qué desplazó a qué?** — el historial de supersesión
  alrededor de $v$:
  $$\exists r\, \exists e_1\, \exists e_2\, \exists x\, \exists d\, \big(
  \mathit{Edge}(e_1, v, r, \mathit{supersede}) \wedge \mathit{Edge}(e_2, r,
  x, \mathit{supersede}) \wedge \mathit{Ts}(r, d) \big)$$
  — "$v$ desplazó a $x$ en la fecha $d$" (y la consulta simétrica, con $v$
  en el extremo desplazado, responde "qué desplazó a $v$"). Encadenando el
  patrón se recorre la genealogía completa hacia atrás.
- **¿Para qué sirve $v$?** — consecuencias declaradas:
  $$\exists e\, \exists w\, \mathit{Edge}(e, v, w, \mathit{implica})$$
  Las ocurrencias son los nodos que $v$ dice implicar — las *consequences*
  de ADR, ahora recorribles.

Los cuatro patrones son pequeños (dos a cuatro nodos), con tipos en cada
elemento — exactamente el caso donde la poda por tipado del capítulo 04
muerde más — y con un extremo anclado en la constante $v$, lo que reduce el
match a explorar el vecindario de $v$. El auditor no lee prosa con fe: pide
caminos, y la prosa la lee **después**, ya localizada, en los cuerpos de los
nodos que el match devolvió.

El requisito de auditoría exige un **guard con dientes**: un control que
puede negarse, no uno que decora. Un guard sin estructura que consultar solo
puede pedir fe o leer prosa; un guard sobre una instancia con rationale puede
ejecutar las cuatro consultas y
**rechazar** — vía ID1, ID2 y el check — lo que no las responde. Los dientes
del guard son, literalmente, los invariantes de completitud documental.

## Veredicto

**Fundado con condiciones.** La base es publicada y probada en práctica:
IBIS da la deliberación como red tipada (Kunz y Rittel 1970) y gIBIS la
valida como hipertexto en grafo en uso real (Conklin y Begeman 1988); ADR da
la unidad decisión-contexto-consecuencias versionada junto al artefacto
(Nygard 2011, hoy práctica estándar); PROV-DM da la procedencia formal del
cambio y del retiro (W3C, 2013). El esqueleto tipado que los une —
Definición 8, la reificación de la supersesión, los átomos
$\mathit{Standing}$ y $\mathit{Ts}$, los invariantes ID1, ID2 e ID3, la
obligación de acta por regla de revisión y las cuatro consultas de auditoría
— es **construcción nuestra** sobre esa base, y hereda las garantías de los
capítulos 02, 03, 04 y 05 exactamente en la medida en que se mantiene dentro
de sus regímenes (invariantes FO cortos, patrones pequeños anclados,
protocolo transaccional). Las condiciones: primero, lo que estos teoremas
garantizan es la **estructura** del porqué — que exista, que esté conectado,
que tenga fecha —, mientras que el **contenido** en prosa de cada nodo de
rationale queda fuera de la garantía formal, como todo cuerpo markdown
(capítulo 06): una justificación estructuralmente presente puede ser
sustantivamente mala, y eso lo detecta el juicio, no el check. Segundo, la
lección de coste de gIBIS sigue en pie: la red de rationale se llena en el
momento de deliberar o no se llena; los invariantes obligan a que el
esqueleto exista, no pueden obligar a que sea fiel.
