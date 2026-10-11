# 03 — Explicar los resultados sin cambiar su semántica

La fase 03 añade evidencia a `check`; no redefine qué cuenta como admisión,
rechazo o error. Un `gap` es un componente opcional de la explicación de un
rechazo por ausencia, no un veredicto que pueda convertirlo en admisión.

## Estado

| Campo | Valor |
|---|---|
| Estado | completo |
| Depende de | 02 |
| Desbloquea | 04, 05 |
| Progreso | Los 17 criterios de aceptación están demostrados por los commits `1a923bc`–`ccd54b5`; `cargo test --workspace` (251/0), clippy, formato y guard de frontera pública se verificaron sobre HEAD `ccd54b5`. `git diff --check` validó además los seis documentos del working tree. |

La fase 02 conserva su salida v1 por defecto. El CLI ya ofrece
`check --explain decide|enumerate` (commits `df06f11` y `4d8b238`); la salida
explicativa usa JSON v2 o una sección redactada de texto. El gate de fase 03
está cerrado con los criterios de aceptación de abajo.

## Contrato acordado

| Concepto | Semántica |
|---|---|
| Estado v1 | Se conservan `admit`, `reject` y `error` con su significado actual. Si una o más instancias son falsas, el resultado sigue siendo `reject`, incluso si fallan por ausencia. `error` sigue reservado para datos, autoría y recursos. |
| Objeto de explicación | El núcleo produce un objeto por instancia fallida, vinculado a su fuente e instancia, con `counterexample` y componentes opcionales `presence_core` y `gap`. En el JSON v2 opt-in, `kind` es siempre `counterexample`; los componentes no son veredictos ni objetos separados. |
| `counterexample` | Componente genérico de por qué una instancia de fórmula es falsa en el snapshot finito evaluado. No afirma minimalidad ni una contradicción global de los axiomas. |
| `presence_core` | Un único subconjunto-minimal de hechos positivos para una asignación de esa instancia fallida, solo en el fragmento operacional definido abajo. Si existen varios cores incomparables, se devuelve uno elegido por orden canónico; no se enumeran todos. No se intenta decidir satisfacibilidad FO global ni se generaliza a toda fórmula universal. |
| `gap` | Componente opcional de un objeto que ya corresponde a una instancia falsa; nunca es un `status`, no modifica `reject` y no propone ni ejecuta una reparación. Solo se produce dentro del fragmento descrito abajo. |
| Fuera del fragmento de `gap` | Se conserva el `counterexample` y `gap` queda `null` en JSON v2. `presence_core` se decide independientemente, según su propio fragmento; no se salta la instancia. |
| Conflicto funcional | Colisiones de tipo, etiqueta, padre o incidencia al combinar un delta con el estado vigente pertenecen a fase [04 — Operación transaccional](04-operacion-transaccional.md), no al `presence_core` de una evaluación estática. |

### Fragmento de `gap`

La obligación existencial debe ser una fórmula monotónica positiva de la
gramática `P`:

```text
P ::= Type | Edge | Parent | HasLab | Ext | TcParent | igualdad positiva
    | (Y P ... P) | (O P ... P) | (Existe ligaduras [guarda P] P)
```

Las guardas y cuerpos de cuantificadores existenciales que formen parte de
`P` también deben pertenecer a `P`. No se admite `No`, `Implica` ni `ParaTodo`
dentro de la obligación. Una igualdad solo es admisible como átomo positivo;
su negación queda fuera.

El `gap` se limita a dos casos: (1) una instancia falsa cuyo cuantificador
exterior es `Existe`; (2) una instancia falsa cuyo cuantificador exterior es
`ParaTodo`, si se encuentra una ligadura canónica con guarda ausente o ya
evaluada verdadera cuyo cuerpo es un `Existe` falso y cuya obligación
existencial pertenece a `P`. La guarda del `ParaTodo` exterior puede ser
cualquier fórmula admitida por el evaluador: no forma parte de la obligación
positiva, y solo una guarda verdadera puede exigirla. Si la guarda es falsa,
ese binding es vacuo y no genera `gap`.

En el segundo caso se explica solo la primera ligadura falsificante y
aplicable en orden canónico: se saltan guardas falsas y cuerpos existenciales
verdaderos. La salida resume la forma completa de la obligación con
constructores y aridades allowlisted; no selecciona un átomo como “lo que
falta” cuando `Y`, `O` o cuantificadores anidados no justifican esa lectura.
No inventa un nodo, arista, ID, etiqueta ni otro candidato a testigo.

Fuera de esos casos —incluidas obligaciones con `No`, `Implica` o
`ParaTodo`— el `gap` es `null`; el objeto conserva su `counterexample` y no
se reclasifica ni se convierte en reparación.

### Minimalidad por inclusión

`presence_core` significa *subconjunto-minimal*, no conjunto de cardinalidad
mínima. En la primera versión, el fragmento operacional se limita a una
instancia expandida de la forma
`forall x̄ :guarda G (not (and A₁ ... Aₘ))`, con `m ≥ 1`: `G` (si se
especifica) es una conjunción no vacía de átomos positivos directos, y cada
`Aᵢ` también lo es; sus predicados solo pueden ser `Type`, `Edge`, `Parent`,
`HasLab` o `Ext`. Sin guarda, `G` es verdadero. No se admiten igualdad,
`TC[Parent]`, cuantificadores anidados, disyunción ni negación fuera de la
forma indicada. El core pertenece a una asignación universal concreta e
incluye los hechos positivos necesarios para la guarda y el patrón prohibido.
Quitar cualquier hecho debe dejar de sostener esa asignación concreta, aunque
otra asignación todavía pueda violar la misma instancia. Puede haber varios
cores incomparables; no se promete unicidad ni que se elija el menor número
de átomos. La salida elige uno de ellos en orden canónico por instancia; no
crea objetos de explicación adicionales por los otros cores. Esta noción no es el
testigo formal de revisión con delta de `docs/03-revision.md` ni un algoritmo
general de satisfacibilidad.

### Privacidad y serialización

Las explicaciones de texto y JSON se redactan por defecto: no incluyen paths
de nodos o archivos, IDs de arista, etiquetas, valores de extensión, texto de
fórmulas, términos ni nombres de variables. `counterexample` sí publica
estructura lógica abstracta, predicados allowlisted y booleanos por instancia;
los objetos también publican ordinales y posiciones de fuente, conteos
derivados y métricas deterministas. `presence_core` se serializa solo como
`{atom_count: N}`, nunca como hechos sensibles. `FormaBrecha` solo puede
resumir constructores y aridades de una lista permitida, sin copiar sus
argumentos. Puede publicar tamaños de dominios existentes y cantidades de
candidatos/bindings examinados, que son conteos derivados del bundle. No
inventa una identidad de testigo. Una asignación universal real solo se expone
mediante un accessor explícitamente sensible
(`asignacion_universal_sensible()`), nunca por la salida redactada
predeterminada. Los ordinales de fuente/instancia son referencias de ejecución,
asignadas en orden canónico.

`Debug` y `Display` de los objetos del núcleo tampoco incluyen IDs o
argumentos sensibles; el serializador CLI no debe serializar evidencia cruda
ni usar `Debug`.

Sin `--explain`, los errores de `check` mantienen el JSON v1 por compatibilidad:
`errors[].path` puede contener una ruta local o absoluta y sigue siendo
información potencialmente sensible. Con `--explain`, JSON v2 fuerza `path` y
`location` a `null`, y el texto omite las rutas; conserva `kind`, `code`, un
mensaje genérico y, cuando aplica, línea/columna. No publiques salidas de
ejecuciones privadas. Si el pase opt-in se completa, no altera los valores de
`status` ni el resumen completo del pase baseline. Si agota el presupuesto,
devuelve `status: error`, `summary: null` y ninguna explicación parcial; el
resumen baseline no se serializa en esa respuesta de error.

El orden de fuentes, instancias, bindings y explicaciones será canónico. El
JSON determinista puede incluir conteos y unidades de trabajo deterministas,
pero no duración de pared. Si se conserva telemetría de duración, será
opcional y fuera del JSON cuya igualdad de bytes se promete.

### Integración optativa en CLI (commit `4d8b238`)

La interfaz recomendada es:

```text
universo check <raiz> --manifiesto <toml> --invariantes <archivo> [--format text|json] [--explain decide|enumerate]
```

Sin `--explain`, `check` mantiene exactamente el JSON v1/texto, canales,
códigos de salida y resumen actuales. Con el flag, JSON usa
`schema_version: 2`; conserva `command`, `status`, `summary` y `errors`, y
añade `explanations` con `mode`, `objects`, `truncated`, `reason` y `budget`.
Cada objeto admite solo `kind`, `source_index`, `line`, `column`,
`instance_index`, `counterexample`, `presence_core` y `gap`; `kind` vale
`counterexample`. `presence_core` es `null` o `{atom_count: N}`. En texto se
agrega después del estado una línea `EXPLANATIONS mode=... objects=...
truncated=...` y resúmenes seguros. Los errores de uso siguen siendo salida de
uso con código 2; no se transforma ayuda o un error de argumentos en respuesta
JSON v2. En errores de ejecución opt-in, JSON v2 conserva los campos
`kind`, `code`, `message`, `path`, `line`, `column`, `location`, pero `path` y
`location` son `null`; texto envía el diagnóstico genérico a stderr sin path.
Estos errores usan `status: error`, `summary: null` y `explanations: null`.

`counterexample` es un árbol de reglas con `rule` y `value`, y solo datos
estructurales allowlisted: un átomo agrega el nombre de predicado; negación,
conectivos e implicación agregan sus hijos; cuantificadores agregan
`search_exhausted` e instancias `[value, guard, body]`. No se serializan
asignaciones ni argumentos de átomos. `gap`, si aplica, usa `{class, counts,
form}`; sus conteos son `available_nodes`, `available_edges`,
`existential_candidates_evaluated` (solo el `Existe` principal resumido) y
`universal_bindings_examined`. `form` conserva la forma positiva completa
mediante constructores/aridades allowlisted. `presence_core` también debe ser
una proyección redactada, nunca la lista de átomos sensibles del accessor.

El CLI ejecuta primero el `check` base completo para conservar el estado y el
resumen; después, si se pidió, corre el pase explicativo medido. El límite de
1.000.000 de unidades es único dentro de ese pase: cubre evaluación trazable,
evidencia, extracción de core/gap, agregación y materialización de objetos del
núcleo. No incluye la proyección/redacción JSON o texto ni la escritura de
salida; no se reinicia por fórmula y no es un límite del trabajo total de la
invocación, porque el baseline corre por separado. `budget` registra
`{limit, consumed, semantic_units, evidence_units, aggregator_units}`. No hay
flag para cambiar el límite. Si el pase explicativo agota el presupuesto antes
de completar el modo solicitado, JSON v2 devuelve `status: error`,
`summary: null`, error de tipo `resource` y ninguna lista parcial. El
truncamiento deliberado de `enumerate` solo se informa al detectar el resultado
101.

## Límites de enumeración

- `decidir` devuelve el objeto de la primera instancia fallida en orden
  canónico, aunque `gap` sea `null`; no salta fallos posteriores. El
  `status` y el resumen semántico vienen del pase completo base de `check`,
  no de la selección de explicación. Si el pase de explicación agota el
  presupuesto antes de completar el objeto, la respuesta opt-in es
  `status: error`, `summary: null`, error `resource` y sin objeto parcial.
- `enumerar` usa un límite máximo de 100 objetos. Al detectar una instancia
  fallida número 101 devuelve los primeros
  100 objetos y marca explícitamente `truncated: true, reason: result_limit`;
  nunca presenta ese prefijo como enumeración completa. `truncated` describe
  la lista de explicaciones y no añade ni sustituye un valor de `status`.
- Si el pase explicativo se agota antes de saber si la enumeración terminó,
  devuelve `status: error`, `summary: null` y ninguna lista parcial. El pase
  base de `check` se ejecuta por separado antes de explicar; así se conserva
  su estado/resumen actuales aunque la explicación tenga su propio resultado.

El presupuesto predeterminado del pase opt-in de explicación es 1.000.000 de
unidades deterministas, único para el recorrido trazable, evaluación de las
fórmulas visitadas, evidencia, extracción de `presence_core`/`gap`, agregación
y materialización de objetos del núcleo. No cubre la proyección/serialización
JSON o texto ni la escritura de salida. No se reinicia por fórmula o instancia.
El pase base de `check` se ejecuta aparte y no se suma a este límite: el 1M no
es un límite del trabajo total o del tiempo de pared de toda la invocación. El conteo de
100 resultados y el presupuesto son límites distintos; el único resultado
parcial permitido es la enumeración marcada `truncated` al detectar el 101.
El presupuesto no puede configurarse por CLI.

## Criterios de aceptación (Done)

- [x] Pruebas confirman que `admit`, `reject` y `error` conservan semántica;
  un `gap` junto con una violación por presencia mantiene `reject`.
- [x] Sin `--explain`, `check` conserva byte por byte la salida v1; con el flag,
  solo JSON opt-in pasa a v2 y texto añade el resumen redactado.
- [x] El JSON v2 no serializa evidencia cruda ni incluye asignaciones; objetos,
  forma, core y contraejemplo usan proyecciones allowlisted.
- [x] El presupuesto de 1.000.000 se mide para el pase explicativo separado,
  no se reinicia dentro de él y no se presenta como límite de toda la llamada.
- [x] Cada explicación se vincula de forma determinista a su ordinal de fuente
  e instancia, incluidas las instancias de esquemas.
- [x] Cada `counterexample` reproduce la falsedad en el mismo snapshot; no se
  presenta como prueba de inconsistencia global ni como core mínimo.
- [x] Cada `presence_core` se emite solo para una instancia del fragmento
  operacional; eliminar cualquier átomo invalida su condición de core. Si hay
  varios, se selecciona uno determinísticamente y los demás no multiplican
  los objetos de explicación.
- [x] Un `gap` es un componente de una instancia ya falsa y nunca altera el
  estado ni propone una reparación.
- [x] `gap` solo aplica a `Existe` falso, o a `ParaTodo` falso con la primera
  asignación falsificante y aplicable cuyo cuerpo `Existe` falso contiene una
  obligación de la gramática positiva; se saltan guardas falsas y cuerpos
  existenciales verdaderos.
- [x] Una obligación fuera del fragmento conserva su `counterexample` con
  `gap: null`; no inventa identidades de testigo ni un `kind` alternativo.
- [x] Texto y JSON redactados no contienen canarios de paths, IDs, labels,
  constantes, fórmulas o nombres de variables.
- [x] Dos ejecuciones con la misma entrada producen bytes idénticos para la
  salida determinista; el JSON no contiene tiempo de pared.
- [x] `decidir` usa un orden canónico; si agota el presupuesto antes de la
  primera explicación, devuelve error de recurso.
- [x] `enumerar` devuelve las primeras 100 solo tras detectar la 101.ª, con
  `truncated: true, reason: result_limit`; no las presenta como completas.
- [x] Si el presupuesto de enumeración se agota antes de saber si terminó,
  devuelve error de recurso sin lista de explicaciones.
- [x] Casos de vacuidad, guardas, cuantificadores, `TC[Parent]`, expansión de
  esquemas y mezcla de presencia/ausencia tienen regresores.
- [x] Los conflictos funcionales de un delta quedan fuera de esta fase y están
  asignados explícitamente a la aceptación de fase 04; esto no afirma que fase
  04 ya esté implementada.

## Casos de prueba requeridos

- Violación por presencia en una fórmula del fragmento soportado.
- Fallo de `Existe` y de `ParaTodo`-`Existe` dentro de la gramática positiva,
  con gap como componente del rechazo y sin reparación automática.
- Obligaciones con negación, implicación o cuantificador universal interno:
  `gap: null`, sin perder el `counterexample`.
- Guarda universal verdadera y falsa; la falsa es vacua. Varios bindings
  falsificantes seleccionan solo el primero en orden canónico.
- Rechazo que contenga tanto una explicación por presencia como un gap.
- Fórmula falsa fuera del fragmento de `gap`, con `counterexample` y `gap`
  ausente, sin omitir el objeto.
- Varios cores incomparables, orden determinista e irreducibilidad por
  eliminación de cada átomo.
- Dominios vacíos, guardas falsas, bindings anidados y clausura transitiva.
- Un objeto por instancia fallida, también fuera del fragmento de `gap`, sin
  saltar fallos en `decidir`; selección canónica de un core
  entre varios incomparables.
- Detección de la instancia fallida 101, marca `truncated` y preservación del
  orden de los primeros 100 objetos; agotamiento presupuestario devuelve
  recurso sin lista.
- Canarios privados en paths, labels, IDs de aristas, atributos y fuente de
  invariantes; ninguna cadena original aparece en salida redacted.

## Verificación prevista

```powershell
cargo test -p universo-core
cargo test -p universo-cli
cargo clippy -p universo-core --all-targets -- -D warnings
cargo clippy -p universo-cli --all-targets -- -D warnings
cargo fmt --all -- --check
python tools/check_public_boundary.py
```

## Registro de evidencia

| Commit / estado | Hito | Verificación / alcance |
|---|---|---|
| `1a923bc` | Conteos medidos de evaluación/evidencia para trazas. | Base para el presupuesto del pase explicativo; no prueba integración CLI. |
| `5f28991` | Agregación de contraejemplos y selección `decide`/`enumerate`. | API de núcleo; no añade salida de explicaciones a `check`. |
| `cf0f752` | Extracción de `presence_core` subconjunto-minimal en su fragmento operacional. | Commit confirmado; accessor sensible explícito y `Debug` redactado. |
| `df06f11` | Extracción de `gap` positivo e integración en el agregado. | Commit confirmado; casos positivos, vacuidad, orden, presupuesto y privacidad están en la suite de core. |
| `4d8b238` | CLI `--explain decide|enumerate`, JSON v2/texto redactado. | Commit confirmado; base para los regresores de compatibilidad v1, privacidad y límites. |
| `fd637ce5e82e17f6d75713cd0ce59f5a9e83af2d` | Pruebas core de casos límite y presencia. | Replay de suficiencia/irreducibilidad sobre snapshots reducidos, cores incomparables con trazas reales, orden canónico y forma positiva recursiva con guarda. |
| `ccd54b5ac1fd68178faca48389d3de2313054c6d` | Pruebas de límites de CLI y evidencia de procedencia. | Agotamiento exacto sin parcial para ambos modos, modo `decide` que salta una fuente admitida y coordenadas fuente/instancia exactas. |

Verificación final del gate en HEAD `ccd54b5`: `cargo test --workspace` pasó
251 pruebas y 0 fallos. En detalle, `cargo test -p universo-core` pasó con
151 pruebas unitarias y 7 complementarias; `cargo test -p universo-cli` pasó
con 1 prueba unitaria, 8 de `check`, 14 de CLI y 12 de explicación. Clippy,
`cargo fmt --all -- --check`, `python tools/check_public_boundary.py` y
`git diff --check` también pasan. No se afirma que la fase 04 esté implementada:
el criterio 17 solo la designa como propietaria del gate de aceptación de
conflictos funcionales al combinar deltas.

## Siguiente fase

[04 — Operación transaccional](04-operacion-transaccional.md).
