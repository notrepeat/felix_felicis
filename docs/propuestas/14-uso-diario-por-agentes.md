# Propuesta 14 — Uso diario por agentes: localizar, aguantar la deuda y escalar

Estado: **borrador** (2026-11-24). Nada implementado. Cada sección es el
borrador de un issue; las reproducciones están en
[docs/reproducciones](../reproducciones/README.md) y se corren con
`python docs/reproducciones/reproducir.py`. Complementa las brechas
[01](../brechas/01-representacion-y-reglas.md),
[02](../brechas/02-rechazo-testigos-y-reparacion.md) y
[04](../brechas/04-agentes-y-control-obligatorio.md).

## Por qué

El motor se usó por primera vez fuera de su biblioteca de origen: un agente
modeló el diseño escrito de un sistema de software como un universo privado
(unas 230 notas, 290 aristas, 14 tipos y 32 reglas) y `check` encontró 14
reglas incumplidas en 40 nodos, sin falsos positivos. Como verificador de
consistencia por censo, funcionó.

El paso siguiente es que todo cambio a ese universo pase obligatoriamente por
el motor (`propose` y `why` por MCP, más hooks y CI). Para eso no basta con que
el veredicto sea correcto: el agente tiene que poder ubicar y reparar cada
falla, el universo tiene que poder vivir con deuda conocida mientras se paga, y
la puerta no puede fallar por presupuesto ni por concurrencia. El
[estudio de cuatro brazos](../../evaluation/four_arms_report.md) ya mostró que
lo que funciona es el rechazo obligatorio que dice qué falla y dónde; hoy, a
esta escala, el motor dice qué falla pero casi nunca dónde.

## Resumen

| Issue | Tema | Prioridad | Costo | Caso |
|---|---|---|---|---|
| I8 | Deuda conocida: línea base o excepciones; `propose` sólo rechaza lo nuevo | P0 | M | R07 |
| I2 | Todas las fallas con su ubicación (CLI y MCP con opt-in) | P0 | M | R02 |
| I1 | Explicación con restricción de rango, sólo de lo que falla y sin anular el veredicto | P1 | M | R01 |
| I5 | Lint de cuantificadores, `exists` atómico como guarda y presupuesto coherente | P1 | M | R05 |
| I7 | Camino de arranque: sellar, constantes de nodo y carga masiva | P1 | S | R07, R08 |
| I10 | Concurrencia del MCP: lecturas en paralelo y ocupado reintentable | P1 | S–M | R10 |
| I11 | CI: códigos de salida distintos, anotaciones y receta reutilizable | P1 | S–M | R11 |
| I12 | `escritura_agentes` también en la CLI | P1 | S–M (decisión) | R12 |
| I4 | Clausura transitiva sobre `Edge` | P2 | M | R04 |
| I3 | Comparar atributos ordenados entre dos nodos | P2 | S–M | R03 |
| I9 | Vistas markdown generadas desde el bundle y su verificación | P2 | M | — |
| I13 | Mensajes de error con causa | P2 | S | R04, R07 |
| I14 | Tipos de origen y destino de cada arista en el manifiesto | P2 | S | — |
| I16 | Apply por MCP para universos sueltos (opt-in) | P2 | M (decisión) | — |
| I6 | `mcp-config` para un universo suelto | P3 | S | R06 |
| I15 | Cardinalidad exacta y máxima | P3 | S | — |
| I17 | Detalles de la CLI | P3 | S | — |

Costo: S, hasta 2 días; M, de 3 a 7 días; L, más.

---

## I1 — La explicación se agota en universos pequeños

**Síntoma.** `check --explain decide|enumerate` y `why` del MCP devuelven
`explanation_budget_exceeded` para cualquier regla que recorra aristas, aun
trivial y aun cuando se cumple.

**Reproducción (R01).** La regla
`(forall ((e E) (a N) (b N)) :guarda (Edge e a b k) (Type b x))`
con 20 notas y 30 aristas consume 528 400 unidades de 1 000 000; con 30 notas y
45 aristas se agota. En el universo real, 20 de 32 reglas se agotan, incluidas
8 de las 14 que fallan. `check` sin explicación tarda 1 a 4 ms.

**Causa.** La captura de evidencia no usa la restricción de rango por aristas
de `v0.2.1` (`evaluador.rs`, `if !capturando && let Some(rango)`): con
evidencia se recorre el producto completo de nodos y aristas. Además el pase de
explicación vuelve a evaluar con traza todas las oraciones, también las que se
cumplen, con un único presupuesto global.

**Efectos secundarios.**
- En `check`, la explicación es todo o nada: si se agota, la salida es
  `status: error` con `summary: null`, y se pierde el veredicto ya calculado.
  `propose --explain` ya lo hace bien (conserva el veredicto y pone
  `explanation_error`).
- Cuando sí cabe, la explicación no dice qué nodo falla: `instances` es una
  lista posicional (156 entradas, 155 verdaderas por guarda falsa, en un caso
  real), sin forma de mapear la posición a una nota.

**Propuesta.**
1. Usar el rango por aristas también al capturar evidencia (la evidencia de
   una asignación excluida por la guarda es la guarda falsa: se puede resumir
   como «N asignaciones fuera de la guarda»).
2. Explicar sólo las instancias que el pase base marcó falsas.
3. Presupuesto de explicación por regla, no global, y error por regla: las
   demás se siguen explicando.
4. `check --explain` conserva `status` y `summary` y reporta
   `explanation_error`, como `propose`.
5. Omitir de `instances` las asignaciones con guarda falsa (o contarlas).

**Criterio de aceptación.** R01 explica la regla con 200 notas y 300 aristas
consumiendo menos del 10 % del presupuesto; un agotamiento en una regla no
anula el veredicto ni la explicación de las otras.

## I2 — Sólo se ve la primera falla, y por MCP ninguna ubicación

**Síntoma.** Para ubicar 40 nodos que incumplen 14 reglas, el agente tuvo que
reescribir cada regla sustituyendo la variable exterior por `@ruta` y correr
`check` una vez por nodo: 428 invocaciones (2,9 s). Por MCP no hay testigo,
así que un agente ve los `rule_id` pero ningún nodo.

**Reproducción (R02).** Con 5 nodos sin `marca: si` y 3 sin `orden`:
- `check` (texto y JSON) sólo da conteos: `failed: 2`, sin ids de regla;
- `propose --witness` da una asignación: `marca-si: v=n/x0001`;
- `--explain enumerate --rules` da un objeto por regla, sin nodo.

El MCP sí lista todas las reglas falsas en `provenance`, y `reglas aplicar`
también: `check` es la única salida que no.

**Propuesta.**
1. `check` lista las reglas falsas (id, mensaje, línea, número de instancias
   falsas) en texto y JSON. Es un cambio aditivo al JSON v1.
2. `--witness all [--limite N]` en `check` y `propose`: todas las asignaciones
   falsas de cada regla, por defecto hasta 100 en total, con `truncated`. Para
   cuantificadores anidados bajo `=>`, `and` o `exists`, el testigo baja hasta
   el `forall` exterior que se pueda ligar (hoy sólo baja si el cuerpo es
   directamente otro `forall`).
3. MCP: `witness` en `check`, `propose` y `why` con `include_values:true`, el
   mismo opt-in que ya usan `context` y `search` para revelar rutas. Sin
   opt-in, todo sigue redactado.

**Alternativa.** Una herramienta MCP `locate` aparte, sólo con opt-in. Más
superficie, mismo efecto.

**Criterio de aceptación.** R02 devuelve los 8 nodos en una llamada de CLI y en
una llamada MCP con opt-in.

## I3 — No se compara un atributo ordenado entre dos nodos

**Síntoma.** `(< Orden x c)` acepta una constante o una metavariable de
`esquema`, no otra variable. «La arista va hacia un orden mayor» sólo se
escribe con un `esquema` sobre el dominio cerrado.

**Reproducción (R03).** `(>= Orden b a)` es `invariant_syntax`. El rodeo con
`esquema ((c Dom.Orden))` funciona pero crece con el dominio: 20, 100 y 400
instancias, 6, 20 y 80 ms con 200 notas. El conteo de fallas cuenta instancias
del esquema (7 de 20, 1 de 400), no aristas, y cada valor nuevo exige cambiar
el manifiesto.

**Propuesta.** Permitir un término de nodo a la derecha en `Compara` cuando el
predicado es `orden` o `fecha`: `(< Orden a b)`. Semántica: falso si a alguno
le falta el valor (y documentarlo, porque `(not (< …))` entonces no equivale a
`>=`).

## I4 — Sin clausura transitiva sobre `Edge`

**Síntoma.** `tc` sólo existe sobre `Parent`. No se puede expresar
alcanzabilidad ni ausencia de ciclos en un grafo de dependencias; la regla se
despliega a mano a uno o dos pasos.

**Reproducción (R04).** `(tc Edge k a a)` es `invariant_syntax` (sin decir
por qué). La regla desplegada a dos pasos admite el ciclo
`x0001 → x0002 → x0003 → x0001`.

**Propuesta.** `(tc Edge <clave> a b)`: clausura estricta sobre las aristas de
una clave, calculada una vez por evaluación (BFS desde cada origen pedido, con
memo) y cargada al presupuesto como los pasos de `TcParent`. Con eso, «sin
ciclos» es `(forall ((a N)) (not (tc Edge k a a)))` y «nunca depende, ni a N
pasos» es una regla.

**Alternativa.** Un predicado derivado declarado en el manifiesto
(`[[derivados]] nombre = "Alcanza", clausura = "k"`). Más explícito, menos
general.

## I5 — `check` admite y `propose` agota: falta un lint de cuantificadores

**Síntoma.** Una regla que pasa en `check` hace que `propose` falle por
presupuesto, sin decir qué regla ni cómo reescribirla.

**Reproducción (R05).** Ocho reglas con `(not (exists ((g E)) (Edge g b a j)))`
en el cuerpo: `check` admite en 31 ms; `propose` con un delta vacío da
`evaluation_budget_exhausted`. Ninguna se agota sola: `check` da 1 000 000 por
instancia y `propose` 1 000 000 para todo el programa. Escrita como
`(exists ((g E)) :guarda (Edge g b a j) (= a a))`, admite en 3 ms en los dos.

**Propuesta.**
1. Tratar como guarda un `exists` (o `forall`) cuyo cuerpo es un único átomo
   `Edge` sobre la variable cuantificada: es la forma natural de decir «hay una
   arista k de b a a» y hoy recorre todas las aristas por cada asignación
   exterior.
2. Aceptar `:guarda` sin cuerpo (`(exists ((g E)) :guarda (Edge g b a j))`),
   para no necesitar un relleno como `(= a a)`.
3. Un análisis estático al cargar el programa: avisa
   (`unguarded_quantifier`, con la línea) de un cuantificador cuyo dominio no
   restringe una guarda indexable, de una guarda `Edge` que no va primera en
   el `and`, y estima el costo con los tamaños del bundle. `check --lint` y
   salida en `validate`.
4. Un mismo criterio de presupuesto en `check`, `propose`, `seal` y `guard`
   (por regla con tope global, o global en todos), y un mismo código:
   hoy `check` dice `evaluation_budget_exceeded` y `propose`
   `evaluation_budget_exhausted`. El error dice qué regla lo agotó.

## I6 — `mcp-config` exige `felix.toml`

**Reproducción (R06).** En un universo suelto, `universo mcp-config` da
`criterio_not_found`, aunque `universo-mcp --root --manifiesto --invariantes`
funciona. **Propuesta:** aceptar `--root`, `--manifiesto` e `--invariantes` y
emitir esa línea de servidor.

## I7 — Arranque de un universo modelado de golpe

**Síntoma.** No hay un camino documentado desde «tengo el modelo» hasta
«`guard` en `pass`»: `init` sella un ejemplo, y escribir el bundle a mano deja
`seal: mismatch`.

**Reproducción (R07 y R08).**
- Tras `init`, agregar una nota da `mismatch` hasta un `seal` manual.
- Una regla con una constante de nodo (`@n/x0001`) no se puede sellar sobre un
  bundle vacío: `seal` devuelve sólo `invalid_invariant_statement`, sin línea
  (`check` sí dice `node_constant_missing` en `r.sx:1:1`). Las reglas con
  constantes sólo pueden entrar después de las notas que nombran.
- El camino por `delta` sí escala (R08): 200 notas y 300 aristas en un delta
  v2 de 53 KB, apply en 0,6 s, `guard: pass`. Con un universo real de 229
  notas y 990 hechos: 1,5 s.

**Propuesta.**
1. Documentar la receta de arranque: `init` vacío (o `seal` de un bundle
   vacío con reglas sin constantes), borrador, `delta --desde`,
   `propose --emit-snapshot-token`, `propose --apply`, y luego
   `reglas aplicar` con las reglas que nombran notas.
2. `init --vacio` (sin nota de ejemplo) e `init --desde <borrador>
   --manifiesto <m> --invariantes <i>`, que hace esos pasos en una
   transacción.
3. Validar las constantes de nodo contra el candidato, no contra la base, para
   que un apply pueda traer notas y reglas a la vez (o un apply conjunto de
   delta y configuración).
4. `seal` reporta las mismas ubicaciones que `check`.

## I8 — Un universo con deuda conocida no puede usar la puerta (P0)

**Síntoma.** Si la base ya incumple reglas, `seal` se niega, `guard` falla
siempre y `propose` rechaza cualquier delta, también uno correcto, con un
testigo que apunta a la deuda vieja. Un hook o un CI obligatorio queda en rojo
permanente, y el agente no distingue si su cambio empeoró algo.

**Reproducción (R07).** Con dos notas que ya fallan `marca-si`, un delta que
crea una nota correcta se rechaza con `witness: marca-si: v=n/x0001`.

**Propuesta.** Dos piezas, la primera suficiente para desbloquear:
1. **Modo regresión.** `propose` compara el conjunto de asignaciones falsas de
   la base con el del candidato y admite si no aparecen nuevas
   (`--regresion`, o por defecto cuando hay línea base). Informa
   `fixed`, `new` y `remaining` por regla. Depende de I2 (asignaciones
   completas).
2. **Excepciones declaradas.** Un archivo sellado junto a las invariantes:
   `(excepcion :regla "marca-si" :nodo @n/x0001 :motivo "…" :hasta 2027-01-31)`.
   `check` y `guard` las cuentan aparte, una excepción vencida vuelve a fallar
   y una que ya no hace falta se reporta para borrarla. `seal` acepta un estado
   cuyas únicas fallas están exceptuadas.

**Alternativas.** Severidad por regla (`:nivel aviso|bloqueo`): más simple,
pero tolera la regla entera y no el caso, y la deuda deja de verse. Ignorar la
deuda en CI con `|| true`: descartada, apaga la puerta.

## I9 — Vistas markdown generadas desde el bundle

**Síntoma.** Cuando el universo modela un documento escrito, hay dos verdades:
el documento y el bundle. El motor sólo vigila el bundle, y nada avisa cuando
el documento cambia sin el universo o al revés.

**Propuesta.** `universo vista <raiz> --manifiesto <m> <documento.md>`
reemplaza el contenido de bloques marcados, por ejemplo
`<!-- felix:vista tipo=x columnas=title,marca,k -->` … `<!-- /felix:vista -->`,
con una tabla determinista generada desde el bundle. Con `--verificar` no
escribe y sale con 1 si algún bloque difiere (para CI y pre-commit). El texto
libre del documento no se toca; las tablas pasan a ser una proyección del
universo.

**Alternativas.** Extraer el universo desde el documento (parsear tablas):
mantiene la verdad en el texto, pero ata el motor a formatos ajenos y no
resuelve la prosa. Sólo enlaces de nota a sección del documento: barato, pero
no detecta divergencias.

## I10 — Concurrencia del MCP

**Síntoma.** Un worker por servidor; una segunda llamada mientras corre la
primera vuelve como error JSON-RPC `-32603 server_busy`. Un cliente que lanza
herramientas en paralelo (habitual en agentes) lo ve como fallo interno, no
como «reintenta».

**Reproducción (R10).** Dos `tools/call` seguidas: la segunda recibe el error
en menos de 1 ms.

**Propuesta.**
1. Cola corta: esperar el semáforo hasta un plazo (por ejemplo 2 s) antes de
   rechazar.
2. Lecturas en paralelo: `context`, `check`, `why` y `search` no escriben;
   pueden correr en hasta N workers. Las escrituras siguen exclusivas.
3. El rechazo como resultado de herramienta (`isError: true`,
   `code: server_busy`, `retry_after_ms`) en vez de error de protocolo.

## I11 — Integración con CI

**Síntoma.** `check` y `guard` salen con 1 tanto por un rechazo como por un
error (sintaxis, presupuesto, bundle inválido); no hay anotaciones ni una
receta reutilizable.

**Reproducción (R11).** Un error de sintaxis y un rechazo dan los dos código 1.

**Propuesta.**
1. Códigos distintos: 0 admite, 1 rechaza, 2 uso, 3 error del motor o de las
   entradas. Detrás de `--codigos-ci` para no romper scripts actuales, y por
   defecto en la próxima versión mayor.
2. `--format github` (anotaciones `::error file=…,line=…`) y SARIF, con la
   ubicación de la nota del testigo (requiere I2).
3. Una acción compuesta de ejemplo que descarga el binario de la release y
   corre `guard`, sin toolchain de Rust.

## I12 — `escritura_agentes` no se cumple en la CLI

**Síntoma.** `escritura_agentes = false` sólo lo hace cumplir el MCP. Un
agente con terminal cambia un universo protegido por la CLI.

**Reproducción (R12).** En un repositorio `init --git`, `promover` a `vigente`
se rechaza por la regla de confirmación; `reglas aplicar` con un archivo vacío,
autor `agente`, se aplica sobre `vigente`; el mismo `promover` pasa.

**Propuesta (requiere decisión).** La CLI no puede saber quién la llama. Las
opciones:
- (a) En universos con `escritura_agentes = false`, `reglas aplicar`,
  `manifiesto aplicar`, `relacion`, `atributo`, `nota editar` y `seal` piden
  confirmación en una TTY o `--dueño`; sin TTY, fallan. Barato; un agente
  puede pasar `--dueño`, pero queda escrito en el historial.
- (b) El hook de pre-commit rechaza cambios a un universo protegido cuya
  última entrada del log no venga de `promover` o de un autor declarado como
  dueño.
- (c) Documentar que la barrera es del arnés: negar la CLI a los agentes y
  dejarles sólo el MCP.

Recomendación: (a) más (b).

## I13 — Mensajes de error sin causa

- `invariant_syntax` dice «no cumple la sintaxis» y una columna. El parser ya
  tiene la causa en varios casos («`tc` sólo se aplica sobre `Parent`»,
  comparación con una variable) y no llega a la salida. Un agente que escribe
  reglas necesita «esperaba una constante o metavariable; encontré la
  variable `a`».
- `seal` devuelve `invalid_invariant_statement` sin línea ni código interno.
- Los dos códigos de presupuesto (I5).

## I14 — Tipos de origen y destino de las aristas

Hoy «la arista `k` va de un tipo a otro» se escribe como una regla por clave
(22 cláusulas en el universo real, más una regla que enumera los 14 tipos). El
manifiesto puede declararlo: `[[aristas]] clave = "k", origen = ["x"],
destino = ["x"]`, y `validate` lo comprueba como comprueba los dominios de
atributos. Lo mismo para «toda nota es de un tipo declarado».

## I15 — Cardinalidad exacta y máxima

«Exactamente uno» se escribe `(and (al-menos 1 …) (not (al-menos 2 …)))`, que
repite la guarda y el cuerpo (6 reglas del universo real). Propuesta:
`(exactamente n …)` y `(a-lo-sumo n …)`, con el mismo corte temprano.

## I16 — Apply por MCP para universos sueltos

**Síntoma.** El MCP simula (`propose`, `why`) pero nunca publica; un flujo
«todo cambio pasa por el MCP» tiene que salir a la CLI para aplicar, y ahí
deja de ser obligatorio. En un repositorio de criterio hay escritura
(`capture`, `reinforce`, `object`); en un universo suelto, no.

**Propuesta (requiere decisión).** Herramienta `apply` con
`expect_snapshot`, sólo si el servidor arranca con `--permitir-apply`, con
autor y motivo obligatorios y el mismo plazo largo de las escrituras.
**Alternativa:** dejar el apply en la CLI y que el hook de pre-commit exija
que la última entrada del log coincida con el sello (ya lo hace `guard`).

## I17 — Detalles de la CLI

- La ayuda general no muestra la forma suelta de `reglas aplicar`
  (`universo reglas <raiz> aplicar … --manifiesto --invariantes`); sólo
  aparece tras un error de uso.
- `universo conectar --verificar` fuera del repositorio de criterio da
  `criterio_not_found`, mientras `universo metricas` lo encuentra desde
  cualquier carpeta.
- Una variable con el mismo nombre que un tipo (`(forall ((x N)) :guarda
  (Type x x) …)`) da `invariant_syntax` sin explicación.
