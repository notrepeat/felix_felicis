# Lifecycle del contexto privado de admisión

**Estado:** están implementados el lease de lectura canónica y la creación
exclusiva del temporal; el flujo de transacción sigue incompleto. Ya existe
`DuenioTransaccionAnclado::desde_lock`: retiene el lock y el parent, captura
identidad, scope y autoridad, y exige que el lock sibling visible corresponda
al handle del guard. El dueño emite leases secuenciales del journal y del
contexto desde ese parent retenido. El lease de lectura abre el sibling
canónico sin seguir enlaces, comprueba el handle y decodifica el frame.

El lease también codifica el envelope con la cuota combinada antes de crear o
escribir `.<basename>.felix.admctx.tmp`; revalida parent, scope y la identidad
del lock visible, crea el temporal en exclusiva y sincroniza el archivo. El
recibo opaco retiene el lease y los datos de identidad, longitud y digest. Si
una escritura o sincronización falla después de crear el archivo, no se devuelve
recibo ni se elimina el temporal; `create_new` hace que un retry choque con ese
huérfano. Aún no se publica el canonical, no hay cleanup ni preflight global de
huérfanos, y ningún caller ordena contexto → journal `Prepared` → rename. El
flujo propuesto publicaría y confirmaría ambos antes de cualquier rename
destructivo, y retiraría el journal antes que el contexto.

El scope capturado usa el probe disponible en cada plataforma; no acredita una
frontera general de mounts ni cubre hardlinks o carreras de procesos externos.
La exclusión sigue limitada a escritores Felix cooperantes.

Los journals `felix-journal-v1` ya existentes siguen el recovery actual sin
auxiliar. Este diseño no los reinterpreta, no exige contexto para ellos ni
cambia la tabla de recovery vigente. Como v1 no identifica qué flujo creó un
journal, el dispatch que distinga transacciones nuevas de las legacy es un gate
previo a cualquier integración: si no puede demostrarse que un auxiliar
pertenece al nuevo flujo, se preservan los artefactos y no se cambia ni se
puentea el recovery existente.

Este contrato gobierna únicamente el lifecycle del contexto. No autentica el
journal, no concede autoridad de recovery y no cambia el contrato
`felix-journal-v1`, las filas vigentes ni la política del backup. Fase 04 y U7
siguen parciales.

## Ruta normal

1. `DuenioTransaccionAnclado::desde_lock` recibe `&mut LockEscritura`, abre una
   sola vez el parent de `lock.ubicacion()` y conserva el handle, la identidad,
   el scope y la autoridad. No abre ni inspecciona root, ni vuelve a resolver
   una ruta caller-provided. Los fallos de identidad, scope, parent o lock
   visible se devuelven como un error fijo y redactado.
2. Tras capturar y admitir la propuesta, calcula `Old` y `New` y construye el
   `Journal` inicial en `Prepared`. Crea en memoria el envelope que liga el
   contexto con `transaction_id`, parent y fingerprints completos de `Old` y
   `New`.
3. **Implementado parcialmente:** el codec exterior produce un único buffer
   dentro de la cuota combinada de 512 MiB antes de crear o escribir el
   temporal. El lease revalida el parent y el lock visible, crea el sibling en
   exclusiva y sincroniza sus bytes; el recibo opaco conserva el lease y sus
   datos. La publicación sin reemplazo y la reapertura del canonical para
   confirmar identidad, tipo y bytes siguen pendientes.
4. **Pendiente:** solo después de publicar y confirmar el contexto, el flujo
   futuro obtiene un lease secuencial del journal y publica `Prepared`. Ese
   lease desde el parent retenido ya está implementado como primitiva de
   storage, pero no hay caller que conecte el orden contexto → journal → rename.
   Solo después de confirmar el journal puede empezar el primer cambio
   destructivo (`root` → backup).
5. El mismo contexto se conservaría sin modificaciones a través de las fases
   del journal de ese nuevo flujo y de cualquier retry. Un futuro recovery
   context-aware leería ambos siblings bajo un nuevo `&mut LockEscritura`,
   mediante leases secuenciales. Los leases de lectura están disponibles como
   primitivas; todavía falta el caller que coordine ambas lecturas, contraste el
   parent y compare el binding del envelope con el journal. El binding
   serializado solo no acredita esas identidades. Luego, en un flujo futuro,
   reproduce la admisión y aplica únicamente una fila permitida por la tabla
   vigente.
6. **Pendiente:** tras observar y validar el estado final autorizado, el dueño
   futuro obtiene la capability del journal, la retira por identidad y
   contenido esperados, y suelta ese lease. Solo después de confirmar la
   ausencia del journal obtiene el lease del contexto y lo retira por la
   identidad exacta observada.

El diseño reserva al dueño la coordinación del flujo futuro. Mantendría el
préstamo mutable del lock, el handle del parent y los recibos de identidad/bytes
durante la operación; adquiriría y soltaría las sub-capabilities del journal y
contexto en secuencia, nunca dos leases mutables simultáneos. Los leases
implementados no aceptan paths ni nombres del caller. El recibo temporal actual
retiene el lock, el parent y sus datos; no habilita cleanup. La capability del
backup sigue siendo independiente. No se usa `Stage` para guardar contexto,
`Old` ni backup.

## Siblings y framing

El lease deriva internamente los nombres sibling desde el basename de
`UbicacionRaiz`: el nombre canónico de contexto se construye desde ese basename
y el temporal añade `.tmp`. No acepta rutas ni nombres del caller:

| Artefacto | Nombre sibling propuesto | Uso |
|---|---|---|
| Contexto publicado | `.<basename>.felix.admctx` | Único registro auxiliar de la transacción. |
| Temporal de contexto | `.<basename>.felix.admctx.tmp` | Creación exclusiva; futuro origen de publicación. |
| Journal | `.<basename>.felix.journal` | Se conserva exactamente `felix-journal-v1`. |

El contexto puede contener los bytes privados completos de `Old`, la
configuración y la propuesta. El temporal usa modo `0600` en Unix y ACL heredada
del parent en Windows. El gate actual rechaza prefijos UNC explícitos en
Windows; no detecta unidades mapeadas ni todos los filesystems remotos. Un probe
de parent no verificable falla cerrado, pero el scope no prueba una frontera
general de mounts.

El wire exterior propuesto es experimental y canónico:

```text
magic[12] = "FELIX-ADMENV"
version:u16be = 1
total_bytes:u64be
journal_bytes:u32be + journal_prepared[]
context_bytes:u64be + FELIX-ADMCTX_v1[]
```

`total_bytes` incluye el header, ambos campos de longitud, el journal canónico y
todo el framing y payload de `FELIX-ADMCTX`. El total inclusivo no puede superar
512 MiB. El encoder deriva un `Journal::new` desde los metadatos inmutables del
envelope, que siempre codifica `Prepared`, aunque el envelope se haya ligado a
un journal de fase posterior. `Journal::parse` debe validar canonicalidad y el
decoder exterior exige `Prepared`; la fase mutable no se guarda en el auxiliar.
El journal v1, sus bytes y su límite de 1024 bytes no cambian.

Antes de reservar o copiar payloads, el parser exterior comprueba el límite
total, las longitudes con aritmética checked, el tamaño máximo de journal, la
versión y la ausencia de bytes sobrantes. El encoder interior recibe como cuota
el espacio restante después del framing exterior y el journal mediante una
variante interna `codificar_con_limite`; `codificar` conserva su wrapper y cuota
actual. Se mantienen todas las comprobaciones existentes. El decoder interior conserva su preflight prestado,
sus límites por componente, la validación de procedencia y el recálculo de
token y digest. `validar_replay` sigue comprobando la admisión contra las
versiones de motor y policy soportadas; el wire no convierte datos en
autoridad. La cuota limita bytes serializados, no RSS ni fallos OOM recuperables.

## Cortes y conservación

La tabla describe solo una transacción iniciada por el futuro dueño. Un journal
v1 preexistente sin contexto sigue por el recovery legacy actual; esta tabla no
se usa para reinterpretarlo. Resolver el dispatch y demostrarlo con fixtures
sintéticas es requisito antes de conectar un nuevo caller.

| Corte observado bajo el lock | Estado que se conserva | Conducta siguiente |
|---|---|---|
| Antes de crear el temporal | Este intento aún no ha creado un temporal ni un journal; root no se modifica. Eso no demuestra ausencia global de artefactos. | El lease codifica antes de crear/escribir; si falla el codec, no toca el sibling. El caller y sus preflight aún no existen. |
| Durante escritura/sync del temporal | Puede quedar un `.tmp`; no se devuelve recibo. Este intento no escribe journal ni modifica root, pero no demuestra ausencia global de esos nombres. | El error conserva el archivo y no ejecuta cleanup. Un retry choca con `create_new`; aún no hay recolección ni cleanup por identidad. |
| Temporal escrito y sincronizado, antes de publicarlo | El recibo opaco describe el temporal creado por este intento. El método no publica canonical, no escribe journal ni renombra root; no demuestra ausencia global de esos nombres. | No hay operación de publicación disponible. Si el proceso termina, el temporal queda huérfano; el preflight global que lo detectaría sigue pendiente. |
| Contexto publicado, antes del journal | Contexto canónico presente; journal ausente; ningún rename destructivo ocurrió. | Si el dueño sigue vivo, solo puede retirar el contexto con su identidad retenida y una observación inequívoca de journal ausente. Tras terminar ese proceso, el siguiente escritor conserva el huérfano y bloquea nuevas publicaciones. |
| Publicación del journal indeterminada | Se conservan contexto, temporal y cualquier journal visible; root aún no debe cambiar. | Reobservar los nombres con el parent retenido. Si no puede probar el journal canónico esperado o su ausencia, parar y preservar todo. Si se confirma journal ausente, el contexto queda huérfano y bloquea. |
| Journal `Prepared` de este nuevo flujo confirmado, antes de `root` → backup | Journal y contexto canónicos; root debe seguir siendo `Old`. | El futuro recovery context-aware carga contexto y comprueba el vínculo. Si falta, es inválido o no coincide, conserva journal y artefactos; no habilita el recovery nuevo. |
| Cualquier fase de este nuevo flujo con journal presente | Journal y contexto se conservan juntos, incluso en `Committed`. | La falta, truncamiento, exceso de cuota, wire no canónico, versión no soportada o mismatch falla cerrado y preserva la evidencia. Solo la tabla vigente decide recovery. |
| Estado final autorizado, antes de retirar journal | Journal y contexto siguen presentes. | Validar estado final antes de retirar evidencia. No se retira contexto primero. |
| Journal retirado, antes de retirar contexto | El estado final ya fue validado; puede quedar el auxiliar huérfano. | El dueño vivo intenta cleanup únicamente con la identidad retenida y verifica ausencia. Si el proceso termina o la verificación es ambigua, el auxiliar queda preservado y bloquea la próxima publicación. |
| Contexto retirado y ausencia confirmada | No hay journal ni contexto de esta transacción. | Se aplican los preflight normales de root, backup y stage antes de iniciar otra transacción. |

La presencia de contexto con journal ausente nunca constituye recibo de una
admisión ni prueba de que el auxiliar sea huérfano recuperable. Todavía no hay
preflight global del dueño: el helper solo rechaza la colisión del nombre `.tmp`
que intenta crear y no inspecciona el canonical ni el journal. La integración
futura deberá preservar y bloquear ante artefactos inesperados, sin añadir una
comprobación al recovery legacy actual. No hay barrido por prefijo, namespace o
`transaction_id`; resolver un huérfano requiere una decisión/capability
específica. Un error posterior a un unlink puede dejar cleanup incompleto; el
dueño futuro solo reintentará o confirmará la identidad que posee.

## Alcance y gates

- El encoder exterior es una unidad pura en memoria, implementada en
  `admision/envelope/codec.rs`; sus pruebas cubren determinismo entre fases,
  cuota inclusiva y errores redactados. El decoder owned puro también está
  implementado; exige journal canónico `Prepared`, reconstruye y liga el
  contexto, y sus pruebas cubren roundtrip/replay, fases, binding, journal
  inválido y corrupción inner. El reader genérico sobre `Read` exige EOF y no
  abre rutas ni se conecta a un caller. `FELIX-ADMCTX` v1 conserva su wire; las
  funciones siguen `pub(crate)`.
- Si encoder y decoder con sus pruebas superan 400 líneas de churn, se separan
  en dos commits por comportamiento: primero codificación canónica con límite
  combinado; después preflight/decode y pruebas de corrupción/binding. Cada
  commit lleva sus pruebas focales, resultado exacto y rollback propio; no se
  reduce legibilidad o cobertura para alcanzar el límite.
- La lectura canónica y la creación exclusiva del temporal ya están
  implementadas como primitivas bajo lease; el recibo solo conserva el lease y
  datos del archivo. No existen publicación del canonical, cleanup, preflight
  global, ordenación caller contexto → journal → rename ni recovery
  context-aware. El lease de lectura no liga el envelope al journal observado.
  Ningún resultado del codec habilita por sí mismo `KeepNew`, `Committed`,
  aceptación de un `New` observado ni limpieza del backup.
- Antes de conectar el recovery nuevo, debe diseñarse y probarse el dispatch
  entre transacciones nuevas y journals v1 legacy. Un v1 sin contexto mantiene
  su camino actual; la ausencia de sidecar no se usa para reinterpretarlo como
  transacción nueva. Si no se puede distinguir con evidencia verificable,
  permanece bloqueada la integración context-aware.
- La capability del backup y las recetas existentes siguen su contrato
  vigente. En particular, este diseño no resuelve la frontera de mount, bind
  mount o FUSE para cleanup de `PartialOld`; ese bloqueo permanece explícito.
- La escritura temporal rechaza UNC explícito en Windows; no detecta drives
  mapeados ni afirma identificar cualquier filesystem remoto.
- `sync_all` del archivo y cortes de terminación de proceso no demuestran
  durabilidad del directorio, pérdida de energía, caída del SO ni garantías de
  `fsync` del filesystem. El diseño no declara esas propiedades.

El lifecycle documentado no cierra fase 04 ni U7, no agrega callers de
producción y no cambia el progreso global de 4/9.
