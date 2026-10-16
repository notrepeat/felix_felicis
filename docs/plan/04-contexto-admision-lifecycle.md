# Lifecycle del contexto privado de admisión

**Estado:** están implementados los leases de lectura canónica, la creación
exclusiva y verificación del temporal, y la publicación canónica confirmada del
contexto; el flujo de transacción sigue incompleto. Ya existe un DTO privado,
versionado y acotado para serializar campos de identidad de archivo observados
en un handle; su codec no reconstruye una identidad física ni entrega autoridad.
Este dato no es un intent de retiro. El codec puro `FELIX-RETIRE` v1 del commit
local `4bf47893870be24bce8c35ec692aa402cbe4e6fe` representa un Journal
`Committed`, tres identidades como DTO, el scope del parent y la longitud/SHA-256
del contexto, con un máximo total de 1536 bytes. La base pasó 3/3 pruebas focales
en Windows y Linux; la cobertura complementaria de
`6e1349246b71ed5acb82cab7927bcde1cc84b738` llevó el filtro a 7/7 por sistema.
Formato, clippy, frontera, diffcheck y limpieza Linux también terminaron verdes.
Existe un writer temporal privado de intent y un verificador que reabre su nombre
exacto desde el parent retenido y contrasta tipo, scope, ID, longitud, bytes,
SHA-256 y EOF en el mismo handle. P1 añade un publicador privado que revalida la
observación completa justo antes del syscall y confirma por postcondiciones ID,
scope, frame, EOF y ausencia del temporal, incluso ante error reportado por el
syscall. Hace un solo `NOREPLACE`, sin retry, y entrega un recibo ligado al owner;
no limpia ni aplica. P2 prueba la revalidación secuencial: si aparece un
canónico tras observar, el publicador se invoca pero no llega al
callback/syscall. Las mutaciones de Journal, contexto o root invalidan antes
del syscall; en Linux, mover el parent no invalida y publica desde el parent
retenido con decoys intactos. Windows no ejercita ese move porque sharing lo
bloquea. El rename `NOREPLACE` es una operación atómica de namespace; las
comprobaciones secuenciales alrededor de él no forman un snapshot ni cierran
las carreras de escritores externos.
El lector file-bound del intent canónico ya está implementado y exige que el
temporal del intent y los temporales de Journal y contexto estén ausentes. Retiene
el owner y el handle final validado, pero no congela los bytes ni el nombre; su
resultado es una observación puntual, no una prueba de replay. Puede observar el
intent aunque falte el Journal canónico o el contexto; esa ausencia no explica su
causa ni autoriza cleanup. La reconciliación tras reiniciar, el retiro del
Journal, del intent y del contexto, además del caller productivo, siguen
pendientes. Un `intent.tmp` residual bloquea y se preserva, sin republicación ni
retry. El detalle se describe al final de esta nota.

La observación local A agrega una lectura coordinada de Intent, Journal y
Context para un estado cerrado, pero no completa la reconciliación ni cambia el
estado global 4/9. Su filtro owner pasó 111/111 en Windows y 116/116 en Linux;
formato, clippy de workspace con todos los targets y `-D warnings`, frontera
pública y diffcheck pasaron en ambos sistemas, y la limpieza Linux terminó con
exit 0. La unidad A está en el commit local
`04edb1095cfa499502a9f511407b3d497110d4ca` (370 líneas de churn), y su cobertura
adicional en `5041550131e4f1399cf1e7f6a54b4fdeb9f12f6e` (252). Esta evidencia
se obtuvo localmente antes de publicar el lote.

También existe `DuenioTransaccionAnclado::desde_lock`: retiene el lock y el
parent, captura identidad, scope y autoridad, y exige que el lock sibling
visible corresponda al handle del guard. El dueño emite leases secuenciales
del journal y del contexto desde ese parent retenido. El lease de lectura abre
el sibling canónico sin seguir enlaces, comprueba el handle y decodifica el
frame. El dueño
también abre el root relativo al mismo parent en modo de lectura y devuelve su
huella raw opcional como dato; no declara que `New` esté instalado ni concede
autoridad de recovery o cleanup. El método privado `comparar_root_replay` ya
reúne esas lecturas secuenciales y devuelve la relación entre `NewEsperado` y el
root raw como datos, sin elegir una fila de recovery ni modificar archivos. Aún
no existe un caller productivo que consuma ese resultado para apply o recovery.
La unidad local `b3b16c6` añade una prueba opaca limitada a replay admitido y
root coincidente en `Installed` o `Committed`; tampoco habilita filas ni se
conecta al recovery. Retiene un préstamo mutable del mismo owner y el Journal
completo (con su fingerprint `New`), no el lease ni el envelope/`NewEsperado`
completos: los leases secuenciales de journal y contexto se sueltan antes de
devolverla. El binding del envelope liga metadatos de la transacción, no su
fase; los fixtures escriben el Journal en fases sintéticas y no demuestran una
transición real de apply ni que un caller haya instalado `New`. La unidad local
`69f3842e9b208aeadaf7721d84a9fc0d07b541af` añade
`PruebaReplayAdmitidoRootCoincidente::revalidar(self)`,
que consume la prueba y solo la reemite después de revalidar el mismo owner,
guard, parent y scope; comprobar los cuatro slots de journal/contexto; y
recargar el ID y el Journal completo, incluida su fase. Relee identidad,
longitud y SHA-256 del
contexto, comprueba binding y replay soportados de nuevo y exige que root siga
coincidiendo. Sus tests cubren reemisión sin cambios e invalidación ante avance sintético de
`Installed` a `Committed`, frame cambiado de igual longitud con el mismo ID y
reemplazo canónico de bytes iguales con ID distinto. La igualdad del Journal
es semántica del registro completo, no una comparación de hash de bytes; el
SHA-256 se calcula sobre el frame crudo del contexto. Los filtros focales
pasaron Windows: owner 51/51, contexto 24/24 y envelope 24/24; Linux: owner
54/54, contexto 25/25 y envelope 24/24. Formato, clippy de workspace con todos
los targets y warnings denegados, frontera pública y diffcheck pasaron en ambos
sistemas; la limpieza del `TMPDIR` Linux terminó con exit 0. El CI de
`7a7273f42f61a391e0eb28d0d9226820c2d3956d` es anterior y no cubre esta unidad.
Esta revalidación no observa backup ni stage, no escoge una fila y no da
autoridad. La unidad `b94ef7026de087539ed2a1ef39f3b6d5c16e1260` añade
`observar_fila_conservar_new`: revalida la prueba, inspecciona el inventario
bajo el mismo owner y devuelve datos de la decisión y los pasos existentes
para `Installed` o `Committed` con root `New`, stage ausente y backup `Absent`
u `Old`. No ejecuta esos pasos ni concede capability de recovery, cleanup,
apply o dispatch. Los filtros focales pasaron Windows owner 55/55, contexto
24/24 y envelope 24/24; Linux owner 58/58, contexto 25/25 y envelope 24/24.
Formato, clippy de workspace con todos los targets y warnings denegados,
frontera pública y diffcheck pasaron en ambos sistemas; la limpieza del
`TMPDIR` Linux terminó con exit 0. La compilación inicial detectó E0277 en dos
payloads y un import sin uso; se corrigieron los slices y el import sin quitar
cobertura. El CI `22c9869915367e5d32ea8d9899c410c57dc0a810` (run
`38870400830`) es anterior y no cubre este commit. La verificación local está
completa, pero esta unidad todavía no tiene CI propio. El caller productivo sigue
pendiente. El rollback retira el consumidor y sus tests/wiring, conservando la
prueba original, su revalidación, el observer y la tabla legacy. El rollback de
`69f3842` permanece aparte: primero se retira este consumidor porque depende de
`revalidar`; luego se revierten el método, wiring y tests de revalidación,
conservando la prueba original, el observer y la tabla legacy. Véase
`04-contexto-admision.md` para condiciones completas y límites.

La extensión de namespace `36bd34a55cddcb53bb6c4154f83474635c49ebd5` hace que
el observer del owner compruebe seis nombres sibling desde el parent retenido.
Los dos slots nuevos son el intent canónico y su temporal; solo su presencia se
observa, sin leer ni parsear su contenido. La emisión y revalidación de la prueba
opaca requieren que ambos estén ausentes; cualquier presencia o error de
verificación bloquea y preserva. Esa unidad de namespace no añadió writer ni
reconciliador, cleanup o preflight global; el writer privado se describe más
abajo, mientras los otros pasos siguen pendientes.

El commit tuvo 256 líneas de churn real (235 altas, 21 bajas y cinco paths).
El filtro `cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion`
pasó Windows 58/58 y Linux 63/63. Formato, clippy de workspace con todos los
targets y `-D warnings`, frontera pública y diffcheck terminaron verdes en ambos
sistemas; el cleanup del harness Linux pasó. Linux ejercitó el componente
demasiado largo y el parent movido con decoys. En Windows, el sharing bloqueó
el movimiento y el test informó que no ejercitó los decoys; la fixture larga no
pudo prepararse. El CI exacto de `e8cd77e` (run `38877305256`) es anterior y no
cubre este commit: en 38 targets reportó Linux 921 pass/0 fail/9 ignored y
Windows 906 pass/0 fail/9 ignored, más 7 pruebas Python por sistema. El rollback
de `36bd34a` retira solo la extensión de seis
slots y sus pruebas/wiring, devolviendo al preflight anterior de cuatro; el
codec de datos y el recovery legacy quedan intactos. El detalle de límites y
fixtures está en [04-contexto-admision.md](04-contexto-admision.md#evidencia-de-verificación).
La unidad `66e8419f63ffd409eaf687fc7aa54183613ac19c` añade métodos de captura
fija sobre el root abierto desde el parent retenido y bajo el lock. Los gates
locales de esa unidad pasaron; el detalle y sus límites están en la evidencia
de `04-contexto-admision.md`. El CAS compara el token antes de validar
procedencia o parsear, sin ejecutar ni sustituir un recovery previo. El
`Snapshot` es dato: sus tags no avalan la propuesta, no es admisión y no habilita
apply o recovery. En el replay posterior, la policy valida la propuesta y solo
deriva `NewEsperado` si resulta `Admitida`.

El lease codifica el envelope con la cuota combinada antes de crear o escribir
`.<basename>.felix.admctx.tmp`; revalida parent, scope y la identidad del lock
visible, crea el temporal en exclusiva y sincroniza el archivo. El recibo opaco
retiene el lease y los datos de identidad, longitud y digest. Si una escritura
o sincronización falla después de crear el archivo, no se devuelve recibo ni
se elimina el temporal; `create_new` hace que un retry choque con ese huérfano.
La verificación reabre el mismo basename y comprueba tipo, scope, identidad y
bytes. La publicación intenta un solo rename sin reemplazo y confirma el
canonical, su identidad, scope y contenido, y la ausencia del temporal. Solo
entonces devuelve un recibo opaco sin borrow del lock; ese recibo liga los
datos al owner original, pero no autoriza recovery ni cleanup. Un error después
del syscall no demuestra que el rename no tuvo efecto: no hay retry ni cleanup
automático. Aún faltan cleanup, preflight global de huérfanos y el caller que
ordene contexto → journal `Prepared` → rename. Un caller futuro debe confirmar
ambos antes de cualquier rename destructivo y retirar el journal antes que el
contexto.

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
3. **Primitivas implementadas:** el codec exterior produce un único buffer
   dentro de la cuota combinada de 512 MiB antes de crear o escribir el
   temporal. El lease revalida el parent y el lock visible, crea el sibling en
   exclusiva y sincroniza sus bytes. La verificación reabre el basename; la
   publicación hace un solo rename sin reemplazo y reabre el canonical para
   confirmar identidad, tipo, scope y bytes, además de comprobar que el
   temporal está ausente. El recibo publicado no retiene el borrow del lock.
   Aún falta el caller que coordine estas primitivas con el journal.
4. **Pendiente:** solo después de publicar y confirmar el contexto, el flujo
   futuro obtiene un lease secuencial del journal y publica `Prepared`. Ese
   lease desde el parent retenido ya está implementado como primitiva de
   storage, pero no hay caller que conecte el orden contexto → journal → rename.
   Solo después de confirmar el journal puede empezar el primer cambio
   destructivo (`root` → backup).
5. El mismo contexto se conservaría sin modificaciones a través de las fases
   del journal de ese nuevo flujo y de cualquier retry. Un futuro recovery
   context-aware leería ambos siblings bajo un nuevo `&mut LockEscritura`,
   mediante leases secuenciales. `cargar_canonico` ya vincula el parent del
   journal al parent retenido; `validar_replay` compara el binding del envelope
   con el journal. El coordinador privado `comparar_root_replay` carga journal y
   contexto en leases secuenciales, valida el replay con los parámetros de
   motor y policy recibidos y compara la huella completa del root observado con
   `NewEsperado`. Devuelve `Ausente`, `Coincide` o `Diferente` junto a los datos
   comparados; no decide recovery ni apply. La observación consumidora local
   consulta después la tabla vigente y devuelve la decisión y los pasos como
   datos privados, sin ejecutarlos. Los valores de motor y policy son
   parámetros explícitos de esa comparación, no una identidad de evaluador
   soportada por un caller productivo. El caller que consuma esa observación y
   aplique únicamente una fila permitida por la tabla vigente sigue pendiente.
   El binding serializado solo no acredita identidades ni estado instalado.
6. **Pendiente:** tras observar y validar el estado final autorizado, el dueño
   futuro obtiene la capability del journal, la retira por identidad y
   contenido esperados, y suelta ese lease. Solo después de confirmar la
   ausencia del journal obtiene el lease del contexto y lo retira por la
   identidad exacta observada.

El diseño reserva al dueño la coordinación del flujo futuro. El recibo temporal
retiene el lock, el parent y sus datos; después de publicar, el recibo no
conserva el borrow y permite adquirir un lease secuencial del journal mientras
el guard sigue vivo. Ambos recibos solo ligan datos al owner; no habilitan
cleanup. Los leases implementados no aceptan paths ni nombres del caller. La
capability del backup sigue siendo independiente. No se usa `Stage` para
guardar contexto, `Old` ni backup.

## Siblings y framing

El lease deriva internamente los nombres sibling desde el basename de
`UbicacionRaiz`: el nombre canónico de contexto se construye desde ese basename
y el temporal añade `.tmp`. No acepta rutas ni nombres del caller:

| Artefacto | Nombre sibling | Uso |
|---|---|---|
| Journal | `.<basename>.felix.journal` | El wrapper normal del inventario carga y valida el nombre canónico; el lector del intent no exige que exista. El helper de inventario recibe `&Journal` como dato. |
| Temporal de journal | `.<basename>.felix.journal.tmp` | El observer comprueba presencia; no interpreta este slot. |
| Contexto publicado | `.<basename>.felix.admctx` | Sibling que la primitiva publica y confirma; el lease lector decodifica el frame. |
| Temporal de contexto | `.<basename>.felix.admctx.tmp` | Creación exclusiva y fuente que se verifica antes de publicar. |
| Intent de retiro | `.<basename>.felix.admctx-retire` | El observer comprueba presencia, sin leer ni parsear el contenido. |
| Temporal del intent | `.<basename>.felix.admctx-retire.tmp` | El writer lo crea bajo una prueba de inicio y P1 lo publica tras revalidar. El lector file-bound actual bloquea y preserva un residual; no lo publica ni reintenta. La reconciliación tras reiniciar sigue pendiente. |

El observer informa hechos de presencia secuenciales bajo el lock. Solo la
ausencia observada se clasifica como `Ausente`; los errores de tipo, reparse,
scope, identidad o nombre demasiado largo fallan cerrados. La lista no es un
snapshot frente a escritores externos y no habilita una operación.

Las pruebas ejercitan presencia en ambos slots nuevos: la emisión se bloquea
y una prueba previamente emitida deja de revalidar, preservando los canarios.
El filtro focal reproducible, ejecutado en Windows y Linux, es:

```text
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion
```

El contexto puede contener los bytes privados completos de `Old`, la
configuración y la propuesta. El temporal usa modo `0600` en Unix y ACL heredada
del parent en Windows. El gate actual rechaza prefijos UNC explícitos en
Windows; no detecta unidades mapeadas ni todos los filesystems remotos. Un probe
de parent no verificable falla cerrado, pero el scope no prueba una frontera
general de mounts.

El wire exterior implementado es experimental y canónico:

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
un journal de fase posterior. `Journal::parse` valida canonicalidad y el
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
| Temporal escrito y sincronizado, antes de publicarlo | El recibo opaco describe el temporal creado por este intento. La operación no escribe journal ni renombra root; no demuestra ausencia global de esos nombres. | La primitiva puede verificar y publicar el contexto. Si el proceso termina antes, el temporal queda huérfano; el preflight global sigue pendiente. |
| Error al publicar el contexto | El error no demuestra que el rename no tuvo efecto; la operación no reintenta ni limpia. | No se emite recibo publicado. Un caller futuro debe reobservar bajo el lock y preservar cualquier estado ambiguo; esa coordinación aún no existe. |
| Contexto publicado, antes del journal | El canonical fue confirmado por la primitiva. Esta operación no lee ni escribe el journal ni renombra root, por lo que no demuestra ausencia global de esos nombres. | El recibo publicado libera el borrow y permite pedir el lease secuencial del journal; ningún caller ordena aún ese paso. No hay cleanup si el proceso termina. |
| Publicación del journal indeterminada | Se conservan contexto y cualquier journal visible; root aún no debe cambiar. | Reobservar los nombres con el parent retenido. Si no puede probar el journal canónico esperado o su ausencia, parar y preservar todo. Si se confirma journal ausente, el contexto queda huérfano y el dueño futuro debe preservarlo y bloquear el flujo nuevo. |
| Journal `Prepared` de este nuevo flujo confirmado, antes de `root` → backup | Journal y contexto canónicos; root debe seguir siendo `Old`. | El futuro recovery context-aware carga contexto y comprueba el vínculo. Si falta, es inválido o no coincide, conserva journal y artefactos; no habilita el recovery nuevo. |
| Cualquier fase de este nuevo flujo con journal presente | Journal y contexto se conservan juntos, incluso en `Committed`. | La falta, truncamiento, exceso de cuota, wire no canónico, versión no soportada o mismatch falla cerrado y preserva la evidencia. Solo la tabla vigente decide recovery. |
| Estado final autorizado, antes de retirar journal | Journal y contexto siguen presentes. | Validar estado final antes de retirar evidencia. No se retira contexto primero. |
| Journal retirado, antes de retirar contexto | El estado final ya fue validado; puede quedar el auxiliar huérfano. | El dueño vivo intenta cleanup únicamente con la identidad retenida y verifica ausencia. Si el proceso termina o la verificación es ambigua, el auxiliar queda preservado y bloquea la próxima publicación. |
| Contexto retirado y ausencia confirmada | No hay journal ni contexto de esta transacción. | Se aplican los preflight normales de root, backup y stage antes de iniciar otra transacción. |

La presencia de contexto con journal ausente nunca constituye recibo de una
admisión ni prueba de que el auxiliar sea huérfano recuperable. Todavía no hay
preflight global del dueño: la creación solo impide reutilizar el nombre `.tmp`
y la publicación confirma el canonical después del intento, pero ninguna de
las dos operaciones inspecciona el journal ni bloquea por inventario global. La
integración futura deberá preservar y bloquear ante artefactos inesperados, sin
añadir una comprobación al recovery legacy actual. No hay barrido por prefijo,
namespace o `transaction_id`; resolver un huérfano requiere una
decisión/capability específica. Un error posterior a un unlink puede dejar
cleanup incompleto; el dueño futuro solo reintentará o confirmará la identidad
que posee.

## Publicación privada del intent (ed5, P1 y P2; commits locales)

`ed5` (`ed5be1e1f62d615c391e1f66a8d167dfc1e14d68`, 36 de churn) extrae la
revalidación del mismo archivo. P1 (`ee6a1fdb6f88801c983a26dfeb27038427e57740`,
329 de churn) revalida toda la observación antes de publicar. En Windows reabre la fuente con acceso `DELETE` y valida ID, scope y frame en el handle que usa para el rename;
en Linux reabre el nombre exacto de la fuente inmediatamente antes del rename, con una carrera externa residual. Ambos caminos ejecutan un único rename `NOREPLACE`,
sin retry. Se cierra la fuente antes de confirmar ID canónico, scope, frame,
EOF y ausencia del temporal, también cuando el syscall devuelve error. El recibo
retiene owner y proof, no implementa `Clone` y su `Debug` redacciona; esta unidad
no hace cleanup ni apply.

P1 pasó 3 pruebas: publicación exitosa; en el caso canary, el nombre canónico se
introduce tras el gate completo y antes del syscall real `NOREPLACE`, que se
invoca una vez, rechaza por destino ocupado y conserva ambos archivos; y rename
real seguido por error inyectado, confirmado
mediante postcondiciones. P2 (`790011750d4d45d5f8d910dae524b04e0c3132d5`, 348
altas) pasó 4 pruebas: canónico aparecido después de observar hace que la llamada
al publicador retorne antes del callback/syscall (cero llamadas al callback/syscall,
no cero llamadas al publicador); error inyectado sin efecto de rename hace una
llamada al callback y no simula un error real del SO; mutaciones de Journal,
contexto o root invalidan antes del syscall; y el move del parent publica desde
el parent retenido con contador 1 y decoys intactos, sin invalidar. El filtro
owner fue ed5 81/86, P1 84/89 y P2 88/93 en Windows/Linux; P1 contexto pasó
24/25. Linux ejerce el move real con decoys; en Windows sharing impide mover el
parent y los decoys no se ejercitan.

Formato, clippy workspace con todos los targets y `-D warnings`, chequeo de
frontera y diffcheck pasaron en ambos sistemas para las tres unidades; limpieza
Linux pasó. El CI publicado anterior, base
`af7d318417b0cdd35b6bdf7710be3d3a1288bf89`, run `38888277606`, terminó SUCCESS:
Linux 957/0/9 y Windows 940/0/9 en 38 resúmenes por sistema, más 7 pruebas
Python por sistema y demo. Excluye los tres commits locales. Rollback: P2, P1,
`ed5`, manteniendo el lector de contexto, writer de intent y recovery legacy.
Al cerrar P2, aún estaba pendiente el lector file-bound. El lector y el helper
de inventario descritos a continuación son posteriores a ese checkpoint; no
completan una fase. También siguen pendientes reconciliación, retiro del Journal,
del intent y del contexto, cleanup y caller productivo. U7 y fase 04 siguen
parciales; no se afirma dirsync, tolerancia a pérdida de energía, cobertura
general de montajes ni recovery completo.

## Lector file-bound e inventario con Journal almacenado

La API pura `FELIX-RETIRE` de
`2bfbca587565c310b633904a013ec25ed251e821` ofrece al parent un decoder y la
validación estrecha del vínculo de identidad nativa y scope del intent. También
expone una referencia de solo lectura al Journal completo como dato; los campos
del registro siguen privados y el DTO no convierte datos de vuelta a una
identidad física ni entrega autoridad. El cambio tuvo 84 líneas de churn y pasó
10/10 pruebas focales en Windows y Linux, además de formato, clippy de workspace
con todos los targets y `-D warnings`, frontera pública, diffcheck y limpieza
Linux.

El lector de `734871f998a05a3bea017d012eb7d2b5a7ed7451` tuvo 348 líneas de
churn y pasó el filtro owner 94/94 en Windows y 99/99 en Linux. Bajo el owner
retiene lock, parent, scope, Arc y el intent canónico: exige temporales de
intent/Journal/contexto ausentes, abre el intent regular y no-follow, lee como
máximo 1537 bytes para rechazar más de 1536, decodifica con el codec existente,
valida el ID nativo/scope medidos y exige que el parent del Journal almacenado
coincida con el parent retenido. Reabre el nombre y valida ID/scope otra vez
antes de devolver la observación. El registro se decodifica desde el primer
handle; el `File` final solo retiene el objeto reabierto, no congela sus bytes o
el nombre del sibling. Por ello el valor es una observación secuencial, con
carreras externas posibles, y no autoriza una acción posterior sin revalidar.

La ausencia del Journal o del contexto canónico no impide esta lectura ni
demuestra por qué falta cada archivo. La prueba normal de replay continúa
requiriendo que intent canónico y temporal estén ausentes. Las pruebas adicionales locales incluyen
la observación con Journal ausente y Context presente o ausente, preservación
de los objetos observados, y el rechazo de la prueba normal antes y después de
la observación. El helper de inventario recibe `&Journal` completo como dato; un
futuro consumidor puede pasarlo desde el intent, pero el helper no infiere su
origen ni le da autoridad. Revalida parent/guard/scope, comprueba el binding
`Journal.parent()` y deriva el backup desde la ubicación interna del owner y el
stage desde `Journal.transaction_id`. Observa el root con su huella nativa
completa (identidad y digest raw) y clasifica presencia o ausencia de backup y
stage solo como datos; mantiene `root_new_validated=false`.
El wrapper normal conserva la carga del Journal canónico antes de delegar al
helper. Ninguno de estos métodos hace replay, decide recovery, reconcilia,
renombra o elimina artefactos.

La cobertura extra del lector `3be9ee7d9cca9fab324ba13c08a1a1f834ab3660` tuvo
148 líneas de churn; el helper de inventario
`83e5533d776b33bf12c23c7bfdb3e33f1260bd2f` tuvo 167. El gate combinado pasó
owner 99/99 en Windows y 104/104 en Linux; formato, clippy workspace con todos
los targets y `-D warnings`, frontera y diffcheck terminaron con exit 0 en ambos
sistemas. El resumen de productos Linux y la limpieza del harness también
terminaron con exit 0. El CI exacto de la base publicada `e579` (run
`38903272871`) terminó SUCCESS: Linux 964/0/9 y Windows
947/0/9 en 38 resúmenes Cargo por sistema, más 7 pruebas Python y el demo por
sistema. Ese CI es anterior y no cubre la API pura
`2bfbca587565c310b633904a013ec25ed251e821`, el lector
`734871f998a05a3bea017d012eb7d2b5a7ed7451` ni los cambios posteriores. El
progreso global permanece en 4/9; fase 04 y U7 siguen parciales. No hay garantías de dirsync, pérdida de energía,
cobertura general de montajes o recovery completo.

## Observación coordinada A

La comparación de identidad nativa y scope del propio intent ya estaba en la
API pura `2bfbca587565c310b633904a013ec25ed251e821`. El commit
`ca0b726bab54c99169903b6a1520ddc95bbc64ba` tuvo 182 líneas de churn y añade
comparadores del Journal (ID nativo y registro semántico completo) y Context
(ID nativo, longitud y SHA-256). No hace I/O ni reconstruye una identidad
nativa. El filtro focal pasó 17/17 en Windows y Linux.

El commit `1632fef5ae7df5d1a7f3f75b6292de5b527da92e` (157 líneas de churn)
añade `ObservacionIntentCanonico::revalidar(self)`. Mantiene vivo el handle
inicial mientras obtiene una observación nueva bajo el mismo owner, parent,
scope y Arc; solo devuelve la nueva observación si ID nativo, registro completo,
parent, scope y Arc coinciden. Retiene el handle final como dato del objeto
observado; no congela los bytes ni el namespace. Pasó owner 102/102 en Windows
y 107/107 en Linux.

| Unidad | Gates locales y commits verificados |
|---|---|
| Comparadores puros `ca0b` | Filtro 17/17 por sistema; formato, clippy workspace con todos los targets y `-D warnings`, frontera y diffcheck pasaron en ambos; limpieza Linux pasó. |
| Revalidación `1632` | Filtro owner 102/102 Windows y 107/107 Linux; mismos controles completos por sistema. |
| A y cobertura extra | Core A `04edb1095cfa499502a9f511407b3d497110d4ca`, 370 de churn; cobertura `5041550131e4f1399cf1e7f6a54b4fdeb9f12f6e`, 252. Owner 111/111 Windows, 116/116 Linux; controles verdes en ambos y cleanup Linux PASS. |

Las primeras ejecuciones de clippy en Windows y Linux encontraron el mismo
import sin uso en una prueba de API; se eliminó sin cambiar conducta. Los
filtros de API 17/17 y de revalidación 102/102 Windows, 107/107 Linux ya habían
pasado sobre sus freezes
y no se repitieron por ese ajuste. Los controles posteriores quedaron verdes
en ambos sistemas.

A consume una observación canónica del Intent y exige los seis slots con
Journal, Context e Intent presentes, y sus tres temporales ausentes. Revalida
Intent al inicio y al final, carga Journal y Context mediante los leases del
owner y compara los vínculos guardados con las identidades nativas medidas, el
Journal completo y la longitud/SHA-256 del frame crudo de Context. El replay
debe estar soportado y su huella completa `New` debe coincidir con el Journal.
El observer de inventario recibido con ese Journal como dato debe clasificar
root `New`, backup `Absent` y stage `Absent`; mantiene
`root_new_validated=false`. Devuelve una observación privada, no clonable y con
`Debug` redactado, que conserva el owner, el Intent file-bound, el Journal,
evidencia de Context, huella de replay e inventario. No crea una prueba normal,
selecciona una decisión ni ofrece cleanup, apply o recovery.

El lector de Intent por sí solo puede observar el canónico cuando falta Journal
o Context; A rechaza esos dos estados sin recrear los nombres ni explicar la
ausencia. Su prueba normal sigue bloqueada mientras Intent esté presente.
Las pruebas A cubren éxito y preservación, Journal con mismo contenido y otro
ID, Context con mismo ID/longitud pero digest distinto, actualización válida
del binding, root `Unknown`, stage `Unknown`, backup `Old`, tags no soportados,
y ausencias de Journal o Context. El filtro owner pasó 111/111 en Windows y
116/116 en Linux. Formato, clippy de workspace con todos los targets y
`-D warnings`, frontera pública y diffcheck pasaron en ambos sistemas; la
limpieza del harness Linux terminó con exit 0. El core A tiene 370 líneas de
churn (132 lector, 235 pruebas, 3 wiring); su cobertura adicional tiene 252
(250 pruebas, 2 wiring).

El CI exacto de la base publicada
`2c54c7a15071958a51679075d7a3de8d1cb7c519` (run `38907971410`) terminó SUCCESS:
Linux 977/0/9 y Windows 960/0/9 en 38 resúmenes Cargo por sistema, más 7
pruebas Python y el demo por sistema. Precede y no cubre `ca0b`, `1632`, A ni
sus pruebas. El rollback previsto retira primero la cobertura adicional de A,
después la unidad A, la revalidación y los comparadores, preservando
writer/publicador y recovery legacy. La cifra de avance continúa en
4/9; U7 y fase 04 siguen parciales. No se afirman snapshot frente a writers
externos, dirsync, tolerancia a pérdida de energía, cobertura general de
montajes ni recovery completo.

## Observaciones file-bound B y C

B core (`10c34f80e4e5a4d30a948d35f35e02a9e5d64c7e`, 232 líneas de churn), B
cobertura (`41d74b4b74a4be4614e0cfdfd8f995975245c3c7`, 223 líneas) y C
(`c3da8173cbd26c17af81fb5909f90735d6c046fb`, 196 líneas) forman commits
separados sobre la base publicada `37d29a1e4a8382584b884d127c889228ce139b80`;
root y revisión independiente aprobaron los freezes. Los gates combinados
pasaron: owner 119/119 en Windows y 124/124 en Linux; formato, clippy de
workspace con todos los targets y `-D warnings`, frontera pública y diffcheck
terminaron con exit 0 en ambos sistemas, y la limpieza Linux terminó con exit
0. Esta evidencia se obtuvo localmente antes de publicar el lote. El CI exacto
de esa base (run `38912187461`) terminó SUCCESS con Linux 996/0/9 y Windows
979/0/9 en 38 resúmenes Cargo, más 7 pruebas Python y el demo por sistema. No
cubre B ni C, y los gates locales no sustituyen un CI del lote publicado.

B requiere intent y Context canónicos presentes, Journal canónico ausente y
los tres temporales ausentes. Usa el Journal `Committed` completo del registro
del intent como dato; valida su vínculo con el parent retenido, el Context por
identidad nativa/longitud/SHA-256 medidas, el replay soportado y la igualdad de
la huella `New`. El helper observa root `New`, backup/stage `Absent` y mantiene
`root_new_validated=false`. B no observa identidad de un Journal canónico ni
infiere la causa de su ausencia. Revalida intent y gate antes de devolver su
tipo privado, que no es prueba normal ni autoridad.

C requiere intent presente, Journal y Context canónicos ausentes y los tres
temporales ausentes. A y B rechazan ese par de ausencias; C emite una observación
privada diferente que conserva únicamente el intent. No lee payload de los
sidecars, no retiene sus identidades medidas, no hace replay ni observa el root
o inventario. Para clasificar nombres presentes, el observer de namespace puede
abrirlos y medir tipo, scope e identidad antes de rechazar; esos datos no forman
parte de la observación C. C no infiere historia ni autoriza borrar o recrear
archivos.

Las pruebas añadidas cubren los estados cerrados B y C y preservación en los
rechazos. El rollback por
dependencia retira C, después las pruebas extra B y por último B core. Siguen
pendientes reconciliación durable, dispatch, cleanup, retiro del Journal, del
intent y del Context, y caller productivo. El estado permanece en 4/9, con fase
04 y U7 parciales; no se afirma snapshot externo, garantía de dirsync, tolerancia
a pérdida de energía, cobertura general de mounts ni recovery completo.

## Alcance y gates

- El encoder exterior es una unidad pura en memoria, implementada en
  `admision/envelope/codec.rs`; sus pruebas cubren determinismo entre fases,
  cuota inclusiva y errores redactados. El decoder owned puro también está
  implementado; exige journal canónico `Prepared`, reconstruye y liga el
  contexto, y sus pruebas cubren roundtrip/replay, fases, binding, journal
  inválido y corrupción inner. El reader genérico sobre `Read` exige EOF y no
  abre rutas ni se conecta a un caller. `FELIX-ADMCTX` v1 conserva su wire; las
  funciones siguen `pub(crate)`.
- `IdentidadArchivoPersistida` usa un frame `FELIX-FILEID` privado, canónico y
  acotado. La conversión hacia el DTO parte de una identidad observada en un
  handle; el codec no convierte bytes de vuelta a identidad física, ni produce
  recibo, autenticación o autoridad. La unidad `4598a3a0c7dc1f4665e6493f8a513e355f313505`
  tuvo 386 líneas de churn y pasó el filtro focal 8/8 en Windows y Linux, además
  de formato, clippy de workspace con todos los targets y `-D warnings`, frontera
  pública y diffcheck. Las pruebas complementarias `afa924c19534cc3b10059c47d256d514b538521b`
  añadieron 109 líneas y pasaron 11/11 en ambos sistemas; cubren máximos,
  lecturas cortas, `Interrupted` y errores de I/O en header, payload y EOF. Sus
  checks de formato, clippy, frontera pública y diffcheck pasaron en ambos; la
  limpieza del harness Linux terminó con exit 0. El CI exacto `8e87735`
  (run `38872303821`) es anterior y no cubre estas unidades. Revertir las pruebas
  deja el DTO base; retirar también el DTO y su wiring no altera los codecs de
  journal/contexto ni recovery legacy. El detalle está en
  [04-contexto-admision.md](04-contexto-admision.md#evidencia-de-verificación).
- El codec puro `FELIX-RETIRE` v1 de `4bf47893870be24bce8c35ec692aa402cbe4e6fe`
  tuvo churn 396; la cobertura `6e1349246b71ed5acb82cab7927bcde1cc84b738`
  añadió 157 líneas. El filtro focal exacto
  `cargo test --locked --offline -p universo-okf --lib transaccion::anclado::intent_persistido`
  pasó 7/7 en Windows y Linux; formato, clippy de workspace con todos los
  targets y `-D warnings`, frontera y diffcheck pasaron en ambos, y limpieza
  Linux terminó con exit 0. El CI `0fac7be` (run `38875241340`) es anterior y
  no cubre el codec; sus cifras históricas son Linux 914/0/9, Windows 899/0/9
  en 38 resúmenes y 7 pruebas Python por sistema. Revertir las pruebas deja el
  codec base; revertir también la base solo retira el DTO experimental y wiring,
  sin alterar journal/contexto v1 ni recovery legacy. El registro es aún un
  codec de datos: no se persiste ni concede permiso de cleanup.
- Si encoder y decoder con sus pruebas superan 400 líneas de churn, se separan
  en dos commits por comportamiento: primero codificación canónica con límite
  combinado; después preflight/decode y pruebas de corrupción/binding. Cada
  commit lleva sus pruebas focales, resultado exacto y rollback propio; no se
  reduce legibilidad o cobertura para alcanzar el límite.
- La lectura canónica, creación exclusiva, verificación y publicación sin
  reemplazo están implementadas como primitivas bajo lease. El recibo publicado
  conserva datos y vínculo al owner sin retener el borrow del lock; no concede
  recovery ni cleanup. Siguen pendientes cleanup, preflight global, ordenación
  caller contexto → journal → rename y recovery context-aware. El lease de
  lectura no liga el envelope al journal observado. Ningún resultado del codec
  habilita por sí mismo `KeepNew`, `Committed`, aceptación de un `New`
  observado ni limpieza del backup.
- La unidad A `b3b16c646d64b4ad7629af2494e84bd8dcbecd7d` pasó localmente sus
  filtros: owner 34/34, contexto 24/24 y envelope 24/24 en Windows; owner
  36/36, contexto 25/25 y envelope 24/24 en Linux. Formato, clippy de workspace
  con todos los targets y warnings denegados, frontera pública, diffcheck y
  limpieza del harness terminaron verdes. El CI remoto b69941b es anterior y
  en ese punto cubría la captura anclada del owner `66e8419` y sus antecedentes.
  No cubre el comparador local `43b2599`, la evidencia de lectura `8ea2193`, A
  ni B.
  La prueba solo representa replay admitido en `Installed` o `Committed` junto
  con la huella observada; no activa una fila de recovery. La cobertura
  complementaria de siblings `0f2187c2759573f1d86747efbcc61589e31ea6de` tiene
  161 líneas de churn real y pasó filtros focales: Windows owner 39/39,
  contexto 24/24 y envelope 24/24; Linux owner 41/41, contexto 25/25 y envelope
  24/24. Formato, clippy de workspace con todos los targets y warnings
  denegados, frontera pública y diffcheck terminaron verdes en ambos sistemas;
  la limpieza del harness Linux terminó con exit 0. Cubre temporales presentes,
  canónicos ausentes, tags no soportados, root ausente reteniendo el original y
  contenido root distinto con el mismo ID. Una compilación inicial detectó un
  movimiento de `PathBuf` en dos aserciones; se corrigió tomando la referencia.
  El CI b69941b es anterior y no incluye esta unidad.
- El refactor privado del opener sibling `dc75aecc9177ee39376fc4ff191055d78cadc1b0`
  pasó filtros focales owner 39/39 en Windows y 41/41 en Linux, además del
  filtro de captura soportada 4/4 en ambos; ese filtro está incluido en el
  conteo del owner y no se suma aparte. Formato, clippy de workspace,
  frontera pública, diffcheck y limpieza Linux terminaron verdes. Mantiene la
  revalidación de parent y scope antes de abrir el sibling.
- El observer core local `b4d2b88563527fcc3d17124153b0be1472fde87f` pasó owner
  43/43, contexto 24/24 y envelope 24/24 en Windows; Linux owner 46/46,
  contexto 25/25 y envelope 24/24. Formato, clippy de workspace, frontera
  pública, diffcheck y limpieza Linux terminaron verdes. Devuelve únicamente
  `Journal` y datos `RecoveryInventory`, con `root_new_validated=false`.
  La extensión combinada test-only `996ec93f91da61a33e6130b4f5a858de70ef79a1`
  añadió 342 líneas de churn y pasó filtros locales Windows owner 47/47,
  contexto 24/24 y envelope 24/24; Linux owner 50/50, contexto 25/25 y envelope
  24/24. Formato, clippy de workspace con todos los targets y `-D warnings`,
  frontera pública y diffcheck terminaron verdes en ambos sistemas; la limpieza
  del harness Linux pasó. Las pruebas cubren root ausente con backup Old y
  stage New, journal ausente o ligado a otro parent y parent movido con decoys.
  Los errores de journal ausente y parent incompatible fallan cerrados y se
  preservan IDs y bytes. Linux ejercitó el parent movido; la prueba falla si el
  rename no ocurre. En Windows, sharing con códigos 5/32/33 bloqueó el rename y
  la rama declaró explícitamente que no ejercitó el decoy. Son datos de
  observación sintéticos: no ejecutan admisión, apply, recovery o cleanup.
  El rollback de `996ec93f` retira solo las pruebas test-only y su wiring,
  conservando el observer core. Para quitar el observer, primero se retira esa
  cobertura dependiente; después se revierten su módulo, sus pruebas core y su
  wiring de `b4d2b885`. Solo entonces se retira el helper compartido
  `dc75aecc`, del que depende el observer para abrir siblings desde el parent
  retenido. El helper no modifica el recovery legacy; la secuencia no cambia
  `felix-journal-v1` ni sus rutas legacy.
- El CI remoto exacto de la base `ce5337ed5b63ae1910a6778e41fea3df50043a8a`
  (run `38866490446`) terminó SUCCESS. Reportó Linux 888 pass/0 fail/9 ignored
  y Windows 874 pass/0 fail/9 ignored en 38 targets, además de 7 pruebas
  Python por sistema. Es anterior y no cubre el refactor, el observer core ni
  la cobertura `996ec93f`.
- El observer raw del root y `comparar_root_replay` devuelven datos bajo el lock,
  no autoridad. La fixture local prepara un `New` raw real, reabre journal y
  contexto tras readquirir el lock en el mismo proceso y compara el replay con
  esa observación. Sus filtros focales pasaron; el detalle está en
  [04-contexto-admision.md](04-contexto-admision.md#evidencia-de-verificación).
  La unidad de comparación `c3853e03819386d4c4364d57be615f26b2b2766c` también
  pasó sus filtros focales, clippy de workspace, formato, frontera pública y
  diffcheck locales; el detalle está en la evidencia anterior. El CI remoto
  exacto registrado para `650c2555d6087cdea6fd3df22f70dc3d129578d4` (run
  `38855248822`) terminó SUCCESS en Linux y Windows; sus conteos están en la
  evidencia enlazada arriba. Ese run no cubre los cambios locales posteriores.
  El probe individual de siblings `4f7d3286cc63c4b181029acc75ae1044c1e4c963`
  pasó filtros focales; sus conteos están en la evidencia enlazada arriba. La
  integración local `bddaea4c9285dd20fc6ea39bd163b00d7c55ca5d` pasó sus filtros
  focales, clippy de workspace, formato, frontera pública y diffcheck en ambos
  sistemas; sus conteos y el límite del caso de parent movido están en la
  evidencia enlazada arriba. La integración solo reúne datos de observación
  secuencial: no autoriza iniciar una operación, caller, dispatch o cleanup, ni
  garantiza un snapshot frente a escritores externos. Ninguna de estas pruebas
  demuestra instalación de `New` por un caller productivo, recovery ni
  resistencia a pérdida de energía.
- La unidad local `8c6893a93d9fff8b544db6a5d677a7f24a3448f6` añade
  `EnvelopeContextoAdmision::validar_replay_soportado`
  para un contexto ya materializado y un par cerrado de tags semánticos. La
  policy deriva `NewEsperado` solo para `Admitida`; los tags son datos, no
  autenticación. El helper genérico y el coordinador que reciben strings
  explícitos siguen siendo cálculos de datos. Los filtros locales pasaron
  Envelope 10/10 y replay 8/8 en Windows y Linux; clippy de workspace con todos
  los targets y `-D warnings`, formato, frontera pública y diffcheck pasaron en
  ambos sistemas. Linux usó y retiró un `TMPDIR` válido. Esta entrada no
  anticipa el decode wire. La captura fija del owner produce un `Snapshot` de
  datos y no convierte los tags en aval de admisión. No hay caller productivo,
  apply, dispatch o recovery. El CI exacto de `e8ac035` (run `38858628837`)
  cubre este selector y está descrito en
  [04-contexto-admision.md](04-contexto-admision.md); no cubre la captura fija
  del owner `66e8419`. El detalle de los filtros
  locales, los tags y la evidencia del CI están en esa nota.
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

- El CI remoto exacto de la base publicada `7a7273f42f61a391e0eb28d0d9226820c2d3956d`
  (run `38868788132`) terminó SUCCESS: Linux 897 pass/0 fail/9 ignored y
  Windows 882 pass/0 fail/9 ignored en 38 targets, más 7 pruebas Python por
  sistema. Ese CI precede y no cubre la unidad local
  `69f3842e9b208aeadaf7721d84a9fc0d07b541af`.

La factory local `656c9b97b94c9c68454742be205c9d7512e1aa08`
`PruebaReplayAdmitidoRootCoincidente::preparar_retiro_contexto` consume y
revalida la prueba, exige Journal `Committed` sin cambios, root `New` y backup
y stage `Absent`, y conserva el mismo préstamo del owner. Devuelve una prueba
privada de inicio ligada al owner; no crea el intent ni escribe, retira o
habilita cleanup. Sus fixtures
de filesystem bajo lock cubren éxito `Committed`, rechazo de `Installed`,
backup `Old`, stage presente y contexto modificado. El filtro focal pasó
Windows 63/63 y Linux 68/68; formato, clippy de workspace, frontera pública,
diffcheck y cleanup Linux terminaron verdes. El CI de la base
`968b70c9bea07c8203152e4180ae4d91d942920e` (run `38878921722`) precede y no
cubre esta unidad. El rollback retira la factory y sus tests/wiring, dejando
intacto el consumidor, la prueba previa y la tabla legacy.

La unidad local `3e2760d0baaa9ff7faed7e89f9790d8d234782a7` extrae el helper
compartido para abrir y validar un temporal, y ofrece `crear_vacio` seguido de
`ArchivoTemporalVacio::escribir`, que usa `write_all` y `sync_all`. La capacidad
retiene el handle del temporal, su ID del mismo handle y el préstamo del parent
con identidad y scope capturados. No adquiere ni conserva `LockEscritura` ni
el owner. Una escritura rechaza un archivo que ya no esté vacío; si falla
después de crear el nombre, lo conserva sin cleanup. La ruta existente
`crear_con` mantiene su orden y comportamiento. Esto prepara una futura
integración, pero no escribe ni publica un intent y no confirma su reapertura.

El commit tuvo 318 líneas de churn real en dos paths. Los filtros
`transaccion::anclado::archivo_temporal`,
`transaccion::anclado::contexto_admision` y
`transaccion::anclado::journal_temporal`, ejecutados con
`cargo test --locked --offline -p universo-okf --lib <filtro>`, pasaron en
Windows 12/12, 24/24 y 59/59, y en Linux 13/13, 25/25 y 60/60. Formato,
clippy de workspace con todos los targets y `-D warnings`, frontera pública y
diffcheck pasaron en ambos; la limpieza del harness Linux terminó con exit 0.
Linux ejercitó el parent movido con decoy. En Windows el sharing impidió el
rename y el test dejó explícito que no ejercitó ese escenario. El rollback
retira el helper/capacidad y sus pruebas, preservando `crear_con`, el codec y
el recovery legacy. El CI exacto de la base
`a0093f898bb8f5c71540e879703d6449ba120113` (run `38880090980`) terminó SUCCESS
con Linux 931 pass/0 fail/9 ignored y Windows 914/0/9 en 38 resúmenes, más
7 pruebas Python por sistema. Es anterior y no cubre la unidad temporal vacía.

El commit local `e3c7ebd455fd6edf36663a23a08e1bca28a202a4` añade el writer
`PruebaInicioRetiroContexto::escribir_intent_temporal`: gatea las rutas UNC
sintácticas antes de las lecturas de revalidación de esta entrada y antes de
crear. Deriva el nombre temporal internamente, crea en exclusiva y mide su ID
desde el handle vacío antes de
codificar, escribir y sincronizar `FELIX-RETIRE` v1. Retiene la prueba/owner y
los datos y frame esperados. Un error después de crear preserva el archivo. La
unidad no reabre el nombre visible ni confirma ID, scope o bytes desde ese
nombre; tampoco publica, renombra o elimina. Los filtros focales
`transaccion::anclado::duenio_transaccion`,
`transaccion::anclado::intent_persistido` y
`transaccion::anclado::journal_temporal` pasaron con
`cargo test --locked --offline -p universo-okf --lib <filtro>`: Windows 66/66,
8/8 y 59/59; Linux 71/71, 8/8 y 60/60. Formato, clippy de workspace con todos
los targets y `-D warnings`, frontera pública y diffcheck pasaron en ambos; la
limpieza del harness Linux terminó con exit 0. El CI exacto de la base
`c5fe6d28f3c78b94fa07315c8fcedcd35393bb4b` (run `38881213774`) terminó SUCCESS
con Linux 938 pass/0 fail/9 ignored y Windows 921/0/9 en 38 resúmenes, más
7 pruebas Python por sistema; no cubre este commit. En esa unidad todavía
quedaba pendiente reabrir el nombre temporal desde el parent retenido, además
de la publicación, cleanup y caller productivo. No hubo fixture UNC de runtime. El
rollback retira el writer y sus seams/tests, dejando la factory, el helper de
temporal vacío y el recovery legacy. U7 y fase 04 siguen parciales; el progreso
global permanece en 4/9.

El commit local `fa96f27e7fc684726727d548c26073eafdd5076d` añade
`IntentTemporalEscrito::verificar_temporal`. Consume el writer y reabre el
nombre temporal exacto mediante el parent retenido. `IntentTemporalVerificado`
conserva el proof y el `File` de lectura; valida en ese mismo handle que el
archivo sea regular, no reparse y esté en el scope esperado, que su identidad
coincida con la medida al crear, y que longitud, bytes, SHA-256 y EOF coincidan
con el frame esperado acotado. Linux ejercitó el parent movido y decoys; Windows
no pudo mover el parent porque sharing bloqueó el rename, y la rama lo declara
sin afirmar que ejercitó el decoy. La operación no vuelve a validar el Journal,
el contexto, el replay, el root ni backup/stage, y no da autoridad de
publicación o cleanup. La prueba normal sigue rechazando la presencia de
cualquiera de los nombres de intent; este verificador no relaja ese gate.

El cambio tuvo 360 líneas de churn real (358 altas y 2 bajas). El filtro focal
`cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion`
pasó 72/72 en Windows y 77/77 en Linux. Formato, clippy de workspace con todos
los targets y `-D warnings`, frontera pública, diffcheck y cleanup terminaron
verdes. El CI exacto de la base `e359b02` (run `38884144988`) terminó SUCCESS
en ambos sistemas; reportó Linux 942 pass/0 fail/9 ignored y Windows 925/0/9
en 38 resúmenes, más 7 pruebas Python por sistema y el demo. Es anterior y no
cubre `fa96f27`. El rollback retira el verificador y sus pruebas conservando el
writer temporal y el recovery legacy. Esa unidad todavía dejaba pendiente la
revalidación completa inmediatamente anterior a la publicación; no hubo fixture
UNC de runtime. U7 y fase 04 siguen parciales; el progreso global permanece en
4/9.

Las unidades A y B completan ahora esa observación especial mientras el intent
temporal está presente. A (`cbb06d2bb40702bdf55d2a16f50d67220a0acc1e`, 103 de
churn) extrae la comprobación común de artefactos. B1
(`b92ec27f6972ac239a0edb4b9bb0e6107a333f70`, 395 altas) consume el token
verificado, vuelve a comprobar owner/lock/parent/scope y los seis nombres,
revalida Journal, contexto, replay y root, y exige `Committed`, root `New`,
backup/stage ausentes. Devuelve solo una observación que retiene el proof y el
owner. B2 (`85b7cb92f7bdb3251ccd7c16f465664b360d45f8`, 216 altas) cubre intent
mutado con el mismo ID, truncado, con bytes extra, Journal/contexto ausentes y
contexto válido de mismo ID y longitud pero SHA distinta; las pruebas conservan
bytes e identidades. El diagnóstico Windows `--nocapture` pasó 8/8 y confirmó
mutaciones, reemplazo de intent y el rechazo ante backup `Old` reaparecido en la
fixture. Linux ejercitó esos caminos en el filtro owner 86/86, que falla si las
operaciones no se realizan. El escenario de parent movido con decoys sigue
ejercitado en Linux; Windows conserva la rama de sharing que indica que no lo
ejercitó.

El filtro focal de estas unidades fue
`cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion`:
A pasó 72/72 Windows y 77/77 Linux, B1 78/78 y 83/83, B2 81/81 y 86/86.
Formato, clippy workspace con todos los targets y `-D warnings`, frontera
pública y diffcheck terminaron verdes en ambos sistemas; la limpieza del harness
Linux también terminó correctamente. El CI exacto
de la base `92986a21cbba38af2af8557f326c9aaf04fcce3d` (run `38885487075`)
terminó SUCCESS con Linux 948 pass/0 fail/9 ignored y Windows 931/0/9 en 38
resúmenes, además de 7 pruebas Python por sistema y el demo; precede y excluye
A+B. El rollback de aquella ronda retiraba B2, luego B1 y finalmente A, preservando el
writer y verificador del archivo, la tabla legacy y el recovery. En esa unidad,
la observación especial no publicaba ni reconciliaba el intent y el publicador
seguía pendiente. P1 implementa la revalidación inmediatamente antes del syscall
y la publicación confirmada; P2 cubre las carreras observadas. Reconciliación,
cleanup y caller productivo siguen pendientes. U7 y fase 04 siguen parciales;
el progreso global permanece en 4/9.

El lifecycle documentado no cierra fase 04 ni U7, no agrega callers de
producción y no cambia el progreso global de 4/9.
