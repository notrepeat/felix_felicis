# Contexto privado para reproducir una admisión

**Estado:** Journal v1 permanece intacto y los codecs y lectores versionados
siguen privados. U7 y la fase 04 continúan parciales; el progreso global
permanece en 4/9. Este flujo aún no tiene caller productivo desde apply ni recovery.

<a name="corte-actual-observacion-normal-versionada"></a>
## Corte actual: observación normal versionada

C2 (3e3ec9e, 261 líneas) persiste el envelope tipado v2 en almacenamiento privado.
El owner codifica el envelope al crear el temporal; el recibo comprueba ID nativo,
longitud y SHA-256, y el lector versionado valida parent y codec.
La publicación usa NOREPLACE. Las pruebas comparan los bytes del temporal y del
canónico; después contrastan el binding estructural leído del canónico con un
JournalV2 de fixture.

J2a (b0cb5d0, 161 líneas) incorpora el lease tipado para JournalV2. El filtro
transaccion::anclado::journal_temporal:: pasó 85/0/0 en Windows y 87/0/0
en Linux con Rust 1.88 locked/offline. El filtro local C2
contexto_admision::tests:: pasó 51/0/0 en Windows y 54/0/0 en Linux;
formato, clippy, frontera pública y diff-check también pasaron.

El CI exacto 01e8b53377ca767061f867b855310624c42b88fa (run 39105202986)
terminó SUCCESS: Linux 1326/0/9, Windows 1305/0/9; cada job ejecutó
38 suites Cargo, siete pruebas Python y 14 pasos. Cubre C2 y J2a, pero no los
cortes locales posteriores J2b, J2c ni los cortes del publicador.

J2b (d17f5a2, 373 líneas) crea el temporal J2 desde el owner tipado; sus
pruebas focales del owner pasaron 293/0/0 en Windows y 299/0/0 en Linux.
J2c (2fe41fd, 172 líneas) añade cobertura de scope y lock: el filtro
duenio_transaccion::journal_v2_ pasó 10/0/0 en ambos sistemas, con cinco
pruebas de J2b y cinco de J2c. El rename nativo en Windows queda limitado por
sharing del sistema; el resultado no afirma una renombrada efectiva allí.

El motor compartido A (b61475a, 78 líneas) conserva la ruta v1; el filtro
journal_temporal:: pasó 85/0/0 en Windows y 87/0/0 en Linux, incluidas
las regresiones v1. B (7ceb915, 394 líneas) añade el lector temporal
versionado: journal_temporal:: pasó 90/0/0 en Windows y 92/0/0 en Linux,
incluidas cinco pruebas nuevas y las regresiones existentes. Ambos cortes
pasaron formato, clippy del workspace con todos los targets y -D warnings,
frontera pública y diff-check; las compilaciones fueron frescas y Linux limpió
sus temporales con exit 0. El CI anterior no cubre J2b, J2c, A ni B.

El flujo sigue siendo de datos privados: no acredita admisión ni habilita
apply o recovery. El lector no constituye un publicador canónico J2 ni un
caller productivo; Journal v1 permanece intacto, U7 y fase 04 siguen parciales,
y el progreso global permanece en 4/9.

| Corte | Commit | Cambio medido | Pruebas focales Windows / Linux |
|---|---|---:|---|
| RA0 — guard de slots de retiro | 140cebf | 75 líneas | 11/0/0; 14/0/0 |
| RA1 — retención de retiro A | 7cbcc55 | 386 líneas | 28/0/0; 28/0/0 |
| RA2 — identidades de retiro A | 8bf3051 | 296 líneas | 32/0/0; 32/0/0 |

Los tres cortes pasaron sus gates focales con Rust 1.88 locked/offline y
paquete limpio. También pasaron formato, clippy del workspace con todos los
targets y -D warnings, frontera pública y diff-check; las revisiones
confirmaron artefactos frescos y limpieza al finalizar.

En RA1, el primer intento se detuvo al compilar con E0689, antes de ejecutar
pruebas, por el tipo inferido del rango de siete casos. Se anotó el inicio
como 0_usize..7, conservando los siete casos; el reintento pasó 28/0/0 en
Windows y Linux.

NormalV2 exige Journal y Context canónicos presentes, e Intent y los tres
temporales ausentes; sus sondas finales comparan secuencialmente sólo las
identidades de Journal y Context. Retiro A conserva los wrappers originales
de Journal, Context e Intent. Su guard clasifica de forma pura las formas
A/B/C; RA2 sondea secuencialmente las tres identidades finales y su postcheck
exterior domina éxito y error.

El clasificador RAW fija root_new_validated=false. En Prepared, BackupReady e
Installed, el coordinador usa la misma muestra raw de root para comparar la
huella completa y clasificar; deriva root_new_validated=true internamente
sólo cuando root es New y la huella Coincide. El caller no aporta ese boolean
y no hay proyección productiva de JournalV2 a Journal v1.

La secuencia sigue siendo observacional, no un snapshot atómico ni garantía
de estabilidad posterior. No concede autoridad filesystem ni caller
productivo de apply, recovery o cleanup. Journal v1 permanece intacto; U7 y
la fase 04 siguen parciales y el progreso global permanece en 4/9.

<a name="corte-actual-observacion-legacy-o-ausencia-estricta"></a>
### Corte actual: observación legacy o ausencia estricta

La ausencia significa que el nombre canónico de Journal falta; no selecciona
una “ausencia de J1”. El contrato legacy comprueba la ausencia de los auxiliares
canónicos y temporales de Context e Intent, vuelve a sondear la ausencia de
Journal y falla cerrado ante reaparición, tipo inesperado o error de scope,
identidad o probe. journal.tmp queda fuera. Un Journal presente distinto de J1
se rechaza.

La lectura legacy conserva el File observado y revalida el owner antes y
después. El postcheck exterior domina Some, None y Err, y el probe se cierra
antes de ese chequeo. La sonda visible_identity_probe no hace ese postcheck por sí sola.
Para límites y evidencia previa, véase la historia que sigue.

La estrategia adoptada conserva Journal v1 y añade Journal v2 paralelo con
`schema=felix-journal-v2`, kind cerrado `context_required` y cuota propia de
1024 bytes. El modelo, codec estricto y selector de schema están en el árbol
local; los vectores dorados v1 no alteran sus bytes. El encoder v2 midió 722
bytes en Windows y 665 en Linux.

También está implementado localmente el codec exterior `FELIX-ADMENV` v2:
framing estricto con magic compartido y versión exterior 2, `JournalV2` en
`Prepared` con kind preservado, y el mismo frame interior `FELIX-ADMCTX` v1.
Frame y contexto tienen cuotas propias de 512 MiB. El selector exterior limita
antes de clasificar con el máximo de las cuotas v1/v2, selecciona una sola rama
y no hace fallback; cada decoder aplica su límite independiente. Estos límites
no son una cota de RSS.

Las unidades A y B1–B4 cerraron sus gates locales en ambos sistemas. B4
(`3020f6cf5b16ce5deb07786c901b76a2a17c4faa`, sobre `733c8db`) pasó 53/0/0
pruebas focales; formato, clippy, frontera y diff semántico terminaron en 0.
El CI `ec973eeee769fce7fa56947f06d352ee00212d9c` (run `38993176466`) cubrió
el lote publicado hasta RETIRE v2 D3B. El CI exacto posterior `27fc2ff` (run `38995183362`) terminó SUCCESS:
Windows `1148/0/9` y Linux `1167/0/9`, con 38 suites Cargo y 7 pruebas Python
por job; cubre R0–R2, pero precede a J0/J1. El CI `cad51cc` (run
`38999370612`) terminó SUCCESS con Windows `1169/0/9` y Linux `1189/0/9`;
incluye 38 suites Cargo y 7 pruebas Python por job y cubre Journal v2 y sus
lectores J0/J1, pero precede a las unidades Context Ctx1–Ctx4.
C1/C2, D1/D2A y D2B corregido tienen gates locales cerrados; D3A
(`b67714f2`) y D3B (`984ef824`) también. R0 (`655cc804`) extrae el cálculo raw
compartido y R1 (`055da2b3`) conserva JournalV2 completo en la expectativa
tipada; sus gates locales cerraron en ambos sistemas. R2 agrega binding
estructural ADMENV v2 antes del cálculo tipado. El filtro local
`transaccion::admision::envelope::` pasó 56/0/0 en Windows y Linux, incluidas
seis pruebas v2 observadas por nombre; formato, clippy, frontera y diff-check
pasaron en ambos sistemas, los artefactos fueron frescos y cleanup Linux terminó
en 0. No se ejecutó el filtro más amplio `transaccion::admision::`. Estos pasos
operan sobre datos en memoria y no activan un caller productivo de apply o
recovery. J0/J1 añaden el lector file-bound versionado del Journal. El lector de
Context se resume en «Lector versionado de Context». El lector versionado de
Intent añade I1A/B1/B2, I2C e I3. I2C valida la lectura presente bajo owner y
temporales; I3 añade ausencia exacta tras dos sondas y el postcheck final. Sus
gates locales están cerrados; la tabla de evidencia separa los conteos de cada
corte. El CI publicado `be97f03` precede a ambos. Ninguno añade aún el gate de
familia persistida ni un caller productivo.

También existen el writer privado del intent temporal y su verificador
de solo archivo: este reabre el nombre exacto desde el parent retenido y contrasta
tipo, scope, ID, longitud, bytes, SHA-256 y EOF en el mismo handle.
La publicación privada del intent revalida la observación completa justo antes
del syscall y confirma el resultado por postcondiciones. En Windows reabre la fuente con acceso `DELETE` y valida ID, scope y frame en el handle que usa para el rename;
en Linux reabre el nombre de fuente justo antes del rename, con la carrera externa residual documentada.
Hace un solo intento `NOREPLACE`, sin retry; cierra la fuente antes de confirmar
ID canónico, scope, frame, EOF y ausencia del temporal incluso si el syscall
reporta error. Su recibo conserva owner y proof, no es clonable y redacciona su
`Debug`. Esta unidad no limpia ni aplica.
El dueño observa el root raw desde el parent retenido y devuelve la huella como
dato. `capturar_snapshot_soportado` y su variante CAS capturan desde ese root
abierto bajo el lock retenido y devuelven un `Snapshot` de datos. El CAS compara
el token antes de validar procedencia o parsear, pero no ejecuta recovery ni se
conecta a apply. Los tags fijos del snapshot no avalan la propuesta: su admisión debe verificarse por separado.
Ya existe un coordinador privado que carga journal y contexto
con leases secuenciales, valida el replay y compara esa huella con `NewEsperado`;
devuelve datos de comparación, no una decisión de recovery. El observer del
inventario carga el journal canónico y clasifica root, backup y stage con
`root_new_validated=false`; devuelve solo datos. Por separado, el observer de
siblings comprueba seis nombres desde el parent retenido. La emisión y
revalidación de la prueba opaca exigen que el intent canónico y su temporal estén
ausentes; ese observer no lee su contenido. La ruta especial vuelve a comprobar
Journal, contexto, replay, root e inventario mientras el intent temporal existe;
P1 hace un único intento `NOREPLACE`: el canary se introduce tras la
revalidación completa y antes del syscall; el syscall real se ejecuta una vez,
rechaza el destino ocupado y conserva ambos archivos. P2 prueba que
un canónico aparecido después de observar deja cero llamadas al callback/syscall,
aunque el publicador sí se invoca. La mutación de Journal, contexto o root
invalida antes del syscall; en Linux, mover el parent no invalida y permite
publicar desde el parent retenido con contador 1 y decoys intactos. Windows no
ejercita ese move porque sharing lo bloquea. El syscall `NOREPLACE` hace un
cambio atómico del namespace sin reemplazo; las comprobaciones secuenciales
alrededor de él no forman un snapshot ni cierran carreras externas.

El lector file-bound del intent canónico exige el intent presente y los
temporales de intent, Journal y contexto ausentes. Abre un archivo regular y
no-follow, limita la lectura a 1537 bytes para rechazar frames mayores de 1536,
usa el decoder existente y liga ID, scope y parent al handle y al parent retenido.
Antes de devolver una observación, reabre el nombre canónico y vuelve a validar
ID y scope. Retiene el owner y el `File` final: conserva el objeto observado,
pero no congela sus bytes ni garantiza que el nombre siga ligado a él.

El lector puede observar el intent aunque falte un nombre canónico de Journal o
Context; no explica la causa. La prueba normal de replay sigue exigiendo que el
Intent canónico y su temporal estén ausentes. El helper privado de inventario
recibe el Journal completo solo como dato, deriva backup y stage desde ubicación
del owner y `Journal.transaction_id`, revalida parent/guard/scope/binding y
observa el root por identidad nativa y digest raw. Clasifica backup y stage como
datos y mantiene `root_new_validated=false`; no atribuye autoridad al registro.

La secuencia privada final parte de una observación `Croot` fresca, intenta
retirar Intent una sola vez y, solo con `Eliminado` o `Ausente`, produce una
observación final ligada al owner. La observación final vuelve a validar owner
antes y después, exige los seis siblings ausentes antes y después, y reconstruye
un inventario fresco: Journal `Committed` ligado al parent, root `New` con
huella nativa completa, backup y stage ausentes, y
`root_new_validated=false`. No reutiliza un inventario anterior.

Esta cadena entrega datos privados; no concede autoridad de admisión, apply o
recovery ni tiene caller productivo. `Conflicto` e `Indeterminado` no prueban
que no hubo efecto; no se reintenta ni se presenta el resultado como `NoEffect`.
Las comprobaciones son secuenciales, no una operación atómica frente a writers
externos. No demuestran dirsync, resistencia a power-loss ni cobertura de
fronteras de mounts.

La observación local A solo acepta Journal, contexto e intent canónicos presentes,
con los tres temporales ausentes. Vincula el Journal completo y la identidad
nativa, longitud y SHA-256 del Context con el intent, valida el replay soportado
y exige root `New` con backup/stage ausentes. Devuelve datos y mantiene
`root_new_validated=false`; no emite una prueba normal ni decide o ejecuta
recovery. El filtro owner pasó 111/111 en Windows y 116/116 en Linux; formato,
clippy de workspace con todos los targets y `-D warnings`, frontera pública y
diffcheck pasaron en ambos sistemas, y la limpieza Linux terminó con exit 0.
La unidad A está en el commit local `04edb1095cfa499502a9f511407b3d497110d4ca`
(370 líneas de churn); su cobertura adicional está en
`5041550131e4f1399cf1e7f6a54b4fdeb9f12f6e` (252 líneas). Esta evidencia se
obtuvo localmente antes de publicar el lote.

La identidad persistida de archivo tiene ahora un DTO
versionado y acotado que solo convierte desde una identidad observada en un
handle; sigue siendo dato, no prueba de retiro ni autoridad. La unidad A añade
una prueba opaca que solo confirma replay admitido y huella root coincidente en
`Installed` o `Committed`; no prueba que un caller instaló `New` ni elige una
fila de recovery. La unidad local
`69f3842e9b208aeadaf7721d84a9fc0d07b541af` añade
`PruebaReplayAdmitidoRootCoincidente::revalidar(self)`: consume la prueba y
solo la reemite tras revalidar el mismo owner, lock, parent y scope, comprobar
los cuatro slots de journal/contexto, recargar el ID y el Journal completo,
releer identidad,
longitud y SHA-256 del contexto, repetir el replay soportado y observar que
root aún coincide. Sus pruebas cubren reemisión sin cambios e invalidación
ante avance sintético `Installed`→`Committed`, un frame distinto de igual
longitud y reemplazo del canónico por bytes iguales con ID distinto. La
igualdad del Journal compara el registro completo semánticamente, no hashea sus
bytes; la longitud y SHA-256 corresponden al frame crudo del contexto. Aún no
observa backup ni stage, ni selecciona una fila. El estado
global sigue en 4/9 fases; U7 y fase 04 permanecen parciales. Replay y decoder
aún no tienen caller desde apply/producción ni cierran esas fases. Una unidad
consumidora local ahora revalida la prueba y observa el inventario para devolver
la decisión y los pasos existentes de `Keep New` como datos privados. Sus
filtros focales locales pasaron en Windows y Linux. El CI `5a73329` (run
`38983901592`) cubre este commit y sus ancestros.
La unidad no ejecuta pasos ni concede autoridad o capability. La factory local
`656c9b97b94c9c68454742be205c9d7512e1aa08` consume esa prueba y vuelve a exigir
`Committed`, igualdad semántica del Journal completo, root `New` y backup/stage
ausentes. Retiene la misma prueba y el préstamo del owner; devuelve una prueba
privada de inicio ligada al owner para una futura unidad. No crea un intent,
escribe, retira ni ofrece autoridad de cleanup.

## Decisión

La evidencia privada de replay de una admisión debe conservar, como una unidad,
el árbol raw completo `Old`, la configuración exacta usada al admitir, la
representación canónica v1 del `Delta` tipado y la identidad del evaluador
(`version_motor` y `policy_id`). No basta con guardar la configuración o una
huella del resultado: sin el estado previo y la propuesta exacta no se puede
reproducir por qué se derivó `New`.

| Entrada del replay | Contenido que se debe preservar | Motivo |
|---|---|---|
| `Old` | Inventario raw completo: rutas relativas, tipos y bytes de todas las entradas. | Reconstruye el estado previo a la propuesta, incluso si el backup ya no existe. |
| Configuración | Bytes exactos de manifiesto e invariantes capturados para esa admisión. | Evita sustituir la configuración original por la configuración vigente o volver a leer rutas externas. |
| Delta | Representación canónica v1 del valor `Delta` tipado que se conservó para replay. | Permite repetir la propuesta tipada; no afirma conservar byte por byte el JSON original que recibió el caller. |
| Evaluador | `version_motor` y `policy_id` exactos que se usaron. | Evita atribuir el resultado a reglas o a una versión distinta. |

El replay puro reconstruye `Old`, vuelve a evaluar la propuesta y comprueba
que el token y el digest raw recalculados coincidan con los valores de base
capturados; también requiere que `version_motor` y `policy_id` sigan
soportados. `calcular_new_esperado` comprueba la huella raw de `Old` contra
`journal.old()` antes de reevaluar. Si la propuesta se admite, serializa el
candidato, deriva el inventario raw que produciría staging —incluidos
auxiliares, overrides de parent y directorios vacíos— y coteja su digest con
`journal.new_fingerprint()`. Devuelve `NewEsperado` con entradas y digest raw
esperados, más `transaction_id`, parent e identidades copiados del journal.

`NewEsperado` no comprueba identidad ni estado de filesystem, no autentica el
journal y no da una capability de recovery. `observar_root_raw` abre el root
relativo al parent retenido en modo de lectura y devuelve una huella opcional
como dato; no demuestra que `New` esté instalado. El coordinador privado bajo
lock, `DuenioTransaccionAnclado::comparar_root_replay`,
carga journal y contexto con leases secuenciales, reproduce el replay con los
parámetros explícitos de motor y policy y compara identidad y huella completas
con el root raw observado. Su relación (`Ausente`, `Coincide` o `Diferente`)
sigue siendo solo un dato; no aplica la tabla vigente ni habilita conservar
`New` o avanzar `Committed`. El caller productivo que conecte ese resultado con
apply o recovery sigue pendiente.
El token, los digests y cualquier attestation guardados son datos para
comprobar consistencia, no prueba ni autoridad de admisión; si un actor puede
editar los datos, también puede recalcular digests sin clave. Una huella del
árbol `New` o una attestation aislada no sustituye los bytes y el contexto
necesarios para repetir el cálculo.

## Confianza y compatibilidad

Este contexto es privado y local. El modelo de confianza sigue siendo el de
writers Felix cooperantes; una huella o attestation sin clave no autentica el
journal frente a un proceso local que pueda editar tanto los datos como sus
digests. No se trata como recibo firmado ni como defensa contra ese adversario.

El codec `felix-journal-v1` conserva sus bytes canónicos, el rechazo de campos
desconocidos y su límite actual de 1024 bytes. El contexto no se añade a ese
JSON ni modifica el comportamiento de v1. Que el lector o el codec Rust sean
`private` o `pub(crate)` no elimina la compatibilidad de los bytes ya
persistidos: los consumidores deben seguir interpretando v1 sin cambios. Esta
decisión no define una API pública ni el contrato de un archivo auxiliar futuro.

El contexto, replay, `NewEsperado` y framing interior `FELIX-ADMCTX` v1 siguen
siendo crate-private y experimentales; el encoder actual calcula una cuota de
512 MiB que incluye su propio framing. La envolvente exterior `FELIX-ADMENV` v1,
que combina contexto, Journal v1 y encuadre, sigue experimental. Su encoder puro en memoria está
implementado en `admision/envelope/codec.rs` y pasa al codec interior solo la
cuota restante antes de reservar. El preflight exterior devuelve vistas
prestadas y valida el framing sin decodificar payloads. El decoder exterior
owned en memoria parsea el journal v1 canónico en `Prepared`, reconstruye el
contexto interior y liga sus metadatos inmutables. El lector genérico sobre
`Read` valida el tamaño antes de reservar y exige EOF tras el frame; no abre
rutas ni admite streams persistentes. El dueño emite un lease de lectura que
abre el sibling canónico desde el parent retenido, comprueba tipo, scope e
identidad del handle y devuelve el envelope decodificado con esa identidad. Solo
la ausencia observada produce `None`; la lectura no reproduce admisión ni liga
por sí sola el envelope al journal observado. El lease también puede codificar
el envelope antes de crear o escribir el sibling temporal exclusivo
`.<basename>.felix.admctx.tmp`, tras revalidar parent, scope y la identidad del
lock visible. El helper crea sin seguir enlaces, valida el mismo handle y su
scope, escribe y sincroniza; devuelve un recibo opaco con el lease retenido y
datos de identidad, longitud y digest. Un error tras crear el archivo conserva
el temporal y no devuelve recibo; un retry choca con `create_new`. La
verificación reabre el temporal y comprueba identidad y contenido. La
publicación hace un único rename sin reemplazo y devuelve un recibo sin borrow
del lock solo si confirma el canonical, su identidad, scope y contenido, y la
ausencia del temporal. Un error de publicación no demuestra que el rename no
tuvo efecto; no hay retry ni cleanup automático. El recibo publicado liga datos
al owner original, pero no autoriza recovery ni cleanup. El preflight global de
huérfanos sigue pendiente. El gate remoto rechaza solo prefijos UNC
explícitos en Windows; no detecta unidades mapeadas ni todos los mounts. El
wire y wrapper interior no cambian. El filtro focal local del codec exterior
pasó 10/10 pruebas en Windows. `bc38412` incorpora un preflight
lector con vistas
prestadas, cubierto con pruebas de robustez en `ef79ea7`. El decoder owned ya
está implementado en memoria: reconstruye las entradas raw por ruta, tipo y bytes;
revalida los marcadores de configuración contra las entradas; parsea y
recanoniza el delta; y vuelve a calcular token y digest. El decoder no consulta
el filesystem, no prueba que la propuesta original fuera admitida ni liga el
contexto a `transaction_id` o parent de un journal. Esos marcadores se
interpretan semánticamente al decodificar, no autentican el contexto. El
encoder de `fb45b09` no reconstruía ni reevaluaba el contexto al leerlo; esa
función llegó en `3e9b5cd`. El framing y el encoder no establecen formato
público estable ni compatibilidad futura.

`IdentidadArchivoPersistida` es un DTO privado separado del journal y del
envelope. Su frame `FELIX-FILEID` es versionado, canónico y acotado; conserva
campos de una identidad observada en un handle y no convierte bytes serializados
de vuelta a identidad física. El DTO no basta para autorizar el retiro de un
sibling. El commit local `4bf47893870be24bce8c35ec692aa402cbe4e6fe` añade un
codec binario puro `FELIX-RETIRE` v1 para un registro experimental: incorpora un
Journal canónico en fase `Committed`, tres DTO de identidad, el scope del parent
y la longitud y SHA-256 del frame de contexto. El límite total es 1536 bytes;
el decoder rechaza magic o versiones desconocidas, secciones malformadas o no
canónicas y bytes extra con errores redactados; el constructor exige Journal
`Committed` y longitud de contexto entre 1 y la cuota del envelope. Solo
serializa datos: no crea ni
abre el archivo de intent, no reconstruye identidades físicas y no autoriza
retiro, recovery o cleanup. Este frame experimental es un codec de datos, no una
versión de Journal ni `FELIX-RETIRE Journal v2`.

Los journals v1 existentes no contienen contexto de replay y no se
reinterpretan como si lo tuvieran. En particular, la ausencia o pérdida del
contexto no autoriza a omitir la validación de `New` ni a aceptar `Committed`. Si una recuperación
futura requiere replay y falta evidencia, debe fallar cerrada y conservar los
artefactos para decisión manual. La estrategia adoptada es un formato paralelo de Journal v2 para flujos nuevos,
con `schema=felix-journal-v2`, `kind=context_required` y cuota propia de 1024
bytes. El modelo valida transaction ID de 32 hex minúsculas, `old != new`,
transiciones y sucesores; los errores DTO usan un mapeo fijo y redactado. El
codec estricto usa JSON canónico, orden fijo y campos cerrados; no hay fallback
entre schemas ni proyección de `JournalV2` a v1.

La Unidad 0 añade vectores literales de compatibilidad para Journal v1 sin
modificar sus bytes. Las unidades 1A/1B y Unidad 2 tienen gates locales cerrados;
Unidad 2 midió un máximo de 722 bytes en Windows y 665 en Linux. Unidad 3
(`e83fd1f695f5af20474c42f00fcc704029cdf44c`) cerró sus filtros locales: selector
6/0/0 y Journal v2 12/0/0 en ambos sistemas; codec v1 10/0/0 en Windows y
9/0/0 en Linux. Formato, clippy, frontera y diff semántico terminaron en 0 en
ambos sistemas; cleanup Linux terminó en 0. El selector privado selecciona por
schema una sola vez y conserva la variante v1/v2; solo clasifica datos y no
acredita el recovery legacy ni una operación de filesystem.

Los frames exteriores `FELIX-ADMENV` v1 y `FELIX-RETIRE` v1 permanecen estrictos
y nunca aceptan Journal v2 bajo framing v1. `FELIX-ADMCTX` v1 es un frame
interior distinto y se conserva dentro del envelope ADMENV v2. El codec y
selector exterior `FELIX-ADMENV` v2 están implementados en el árbol local; B4
(`3020f6cf5b16ce5deb07786c901b76a2a17c4faa`) cerró sus gates locales y CI.

También está implementado localmente el codec de datos `FELIX-RETIRE` v2. C1/C2
añaden el modelo `Committed`/`context_required` y comparadores de DTO; D1 codifica
el frame, D2A valida su estructura, D2B decodifica JournalV2 y FILEID de forma
estricta, y D3A/B seleccionan una sola versión sin fallback. El modelo limita el
Context a 1..=512 MiB; el frame tiene una cuota derivada de 1536 bytes
(`71 + 1024 + 3 × 147`), no una medición de registro. C1/C2 y D1/D2A tienen
recibos locales; D2B corregido pasó 21/0/0 pruebas RETIRE v2 y 17/0/0 de
compatibilidad v1 en Windows y Linux, con formato, clippy, frontera y diff `0`.
Su primer intento de gates falló en compilación con `E0432` y `E0425` antes de
iniciar tests; esos errores se corrigieron y no se atribuyen conteos a ese
intento. D3A (`b67714f2`) está integrado y cerró sus gates locales corregidos;
D3B (`984ef824`) también cerró sus gates locales. No se decodifica JournalV2
como v1. El replay R0/R1 y el binding estructural R2 se resumen en
[Replay tipado de Journal v2](#replay-tipado-de-journal-v2). J1A/J1B/J1C1/C2/C3 añaden
lectura file-bound del Journal persistido v1/v2 bajo lease estricto y su
postcheck owner-bound; no implementan replay owner-bound v2 para `&JournalV2` ni
caller productivo. Un futuro consumidor deberá conservar y verificar `kind`, sin
proyección a v1 ni booleano de caller, antes de un dispatcher. Ninguno de estos
codecs demuestra recovery ni comportamiento del filesystem para Context o Intent.

El selector es solo clasificación de datos. El Journal, su kind, el Context y
los resultados de binding no conceden autoridad; no se activan apply ni recovery.
Los journals legacy v1 se conservan sin migrarlos ni inferirles contexto, y la
ruta v1 sin auxiliares mantiene su comportamiento actual.

## Vínculo de replay y leases de datos

`EnvelopeContextoAdmision` es un valor crate-private en memoria que conserva un
`ContextoAdmision` junto a una copia de `transaction_id`, parent y huellas
completas `old`/`new` del journal. El vínculo omite `JournalState`, que avanza
durante la transacción. `ligar` solo copia esos campos; `validar_replay` exige
que coincidan con el journal recibido y vuelve a llamar
`calcular_new_esperado`, que revalida `Old`, admisión y `New` antes de devolver
su expectativa.

Esto añade consistencia entre el contexto en memoria y los metadatos del
journal, no autenticidad ni autoridad: `NewEsperado` sigue copiando los IDs del
journal y ningún valor prueba por sí solo el estado instalado. El envelope no
consulta el filesystem, no conserva rutas o handles y no es capability. El
codec y el decoder no ligan el resultado a un journal observado. El coordinador
privado carga primero el journal canónico y después el contexto, reproduce el
replay con los parámetros de motor y policy que recibe, y compara el fingerprint
completo con la observación raw del root. Esos parámetros son entradas explícitas
del cálculo, no una identidad de evaluador soportada por un caller productivo.
El coordinador no elige una fila de recovery, no altera archivos ni habilita
cleanup. `FELIX-ADMCTX` v1 y `felix-journal-v1` permanecen sin cambios.

La unidad local `8c6893a93d9fff8b544db6a5d677a7f24a3448f6` añade
`EnvelopeContextoAdmision::validar_replay_soportado`
para un contexto ya materializado en memoria. Selecciona un par cerrado de tags:
`felix_felicis:proposal-semantics/v1` cubre materialización/captura, Delta,
candidato y conflictos, evaluación/cuotas, serialización, staging e inventario
raw/digest; un cambio observable del replay requiere otro tag. El framing wire
se versiona por separado. `felix_felicis:admission-policy/v1` deriva
`NewEsperado` solo si la propuesta resulta `Admitida`; rechazos y errores fallan
cerrados. Los tags son datos de consistencia, no autenticación ni autoridad.
No es un preflight antes del decode wire ni un caller productivo. El helper
genérico y el coordinador del owner que reciben motor/policy explícitos siguen
siendo cálculos de datos, no una identidad confiable elegida por un caller.

## Lifecycle y gates para publicación y recovery

El documento [04-contexto-admision-lifecycle.md](04-contexto-admision-lifecycle.md)
define el lifecycle propuesto y las primitivas implementadas. El dueño conserva
`&mut LockEscritura`, valida la identidad del lock sibling y retiene únicamente
el parent como handle de filesystem. Emite leases secuenciales del journal y
del contexto desde ese parent, revalidando identidad, scope y lock visible. El
lease de contexto lee el sibling canónico y crea/sincroniza un temporal
exclusivo; su recibo temporal conserva el lease y los datos del archivo. La
verificación reabre el temporal y la publicación intenta un único rename sin
reemplazo, luego confirma el canonical desde el parent retenido. El recibo
publicado no conserva el borrow del lock y solo liga identidad, scope y datos;
no da autoridad de recovery ni cleanup. Si la confirmación falla, el error no
demuestra que el rename no tuvo efecto y la operación no reintenta ni limpia.
El cleanup, el preflight global de huérfanos y el caller que ordene contexto →
journal `Prepared` → primer rename siguen pendientes. El framing exterior v1 y
sus codecs permanecen intactos. El modelo y codec de Journal v2 y su selector
cerraron gates locales y están cubiertos por CI. El CI `e5bcf331` (run
`38988788938`) cubre también ADMENV2 A–B4; sus resultados están en la tabla de
evidencia. Ese CI es anterior a RETIRE v2 y no lo acredita.

RETIRE v2 ya cuenta con modelo/binding C1/C2, encoder D1, preflight D2A,
decoder canónico D2B y selectores D3A/B en el árbol local. Son codecs y
comparadores de datos; no abren archivos ni implementan replay owner-bound v2.
C1/C2, D1/D2A y el D2B corregido tienen gates locales cerrados. El primer
intento D2B falló al compilar (`E0432` por el import de cuota y `E0425` por un
nombre de helper), sin iniciar tests; la corrección cambió el import de la cuota
al módulo padre y arregló el typo, y después pasó los gates en ambos sistemas.
D3A (`b67714f2`) cerró gates locales corregidos: selector 5/0/0 y formato,
clippy, frontera y diff en 0 para Windows y Linux. El intento inicial de clippy
falló por un import `JournalKind` no usado; el fix se limitó a quitarlo. Las
pruebas v2 `21/0/0` y compatibilidad v1 `17/0/0` proceden del snapshot anterior,
con fuentes v2/v1 idénticas; no se repitieron tras el cambio del selector.
D3B (`984ef824`) añade dos pruebas y cerró gates locales: selector 7/0/0 en
Windows y Linux; formato, clippy y frontera en 0; diff-check del selector
rastreado en 0 y cleanup Linux en 0. El chequeo `--no-index` del archivo nuevo
salió 1 por comparar contra vacío y no reportó whitespace; los artefactos fueron
frescos. El CI `ec973eeee769fce7fa56947f06d352ee00212d9c` (run `38993176466`)
cubre el lote publicado hasta D3B. El CI `27fc2ff` precede a J0/J1; el CI
posterior `cad51cc` cubre Journal v2 hasta J1 y sus ancestros, pero precede a
Ctx1–Ctx4. Al preparar este lote, los gates focales de Context se documentan
abajo; el lote no habilita caller de producción. Las primitivas de storage no
son una decisión de recovery.

El encoder y decoder exteriores v1 son codecs puros, mantienen
`felix-journal-v1` sin cambios y no abren rutas. El reader genérico exige EOF y
no se integra a un caller. La orquestación y el recovery context-aware requieren
otra revisión. La secuencia privada `Croot→Intent→observación final` sigue
siendo un flujo observacional con operaciones v1; no consume los nuevos frames
ADMENV2 o RETIRE2 ni conecta esos codecs a apply/recovery. R0/R1/R2 añaden un
cálculo tipado de expectativa sobre datos y binding estructural previo al
perfil, no una comprobación file-bound del owner. Los selectores clasifican
framing y preservan la rama elegida; no validan el filesystem ni prueban el
recovery legacy. Falta el replay owner-bound v2 antes de un futuro dispatcher.
Los journals legacy v1 sin auxiliares mantienen el recovery actual; no se
infiere contexto ni se convierte v1 a v2. Un Journal v1 junto a un auxiliar
nuevo es ambiguo por falta de kind. El futuro preflight/caller deberá rechazar
esa combinación antes del dispatcher o CAS; esa compuerta aún no está
implementada. Una terna v1 coherente puede seguir produciendo las observaciones
privadas actuales, pero no autoriza recovery global futuro. Solo una familia v2
coherente con owner fresco y replay validado podrá preparar una decisión futura;
no se habilitan ahora filas de caller. El codec y el binding no son autoridad
para aceptar `New`, habilitar `KeepNew`/`Committed` o limpiar backup.
Sigue bloqueada la frontera real de mount, bind mount o FUSE para cleanup de
`PartialOld`; no se cambia el contrato ni la capability del backup.

`Committed` mantiene los requisitos de la tabla vigente. No se salta su
validación por retirar un auxiliar futuro. El reader file-bound v2 de Context
se resume en la sección Ctx1–Ctx4. Ctx4 cerró sus gates locales y quedó
en el commit local `190d046` (312 líneas de churn).
I1A y B1 ya añaden el reader privado file-bound de Intent. B2 cerró gates
locales en Windows y Linux y quedó en el commit `08ddb0b8`; el CI registrado de
Context precede a estos cortes. No hay todavía preflight global de familia
persistida ni caller productivo. Fase 04 y U7 continúan parciales; el progreso
global permanece en 4/9.

## Clasificador privado de familia (U1/U2)

U1 conserva como datos las combinaciones vacío, Journal v1 sin auxiliares y
Journal v2 con ADMENV2; rechaza Intent y las mezclas. U2 añade las fases de
retiro A/B/C: A exige igualdad completa de Journal v2, B permite el binding
estructural ENV2 de `Prepared` con RETIRE2 `Committed`, y C clasifica Intent
solo como dato sin replay ni cleanup. La matriz U2 acepta seis y rechaza 21 de
27 combinaciones. Son clasificadores puros privados: no son un preflight global,
no certifican owner/filesystem y no habilitan caller.

U1 cerró sus gates locales: el filtro focal registró `3/0/0` en Windows y Linux.
Formato, clippy, frontera y diff-check terminaron en `0`; cleanup Linux también
terminó en `0`. Quedó en el commit local
`e2e8feb20eab3d55f1d69014e2dd7f08616cde00`. U2 cerró su filtro focal con
`6/0/0` en Windows y Linux; formato, clippy, frontera y diff-check terminaron
en `0`, y cleanup Linux en `0`. Quedó en el commit local
`17e01ca0b388d012d308d5a95b87429d44cf423b` (101 líneas de churn sobre U1).
El CI publicado `b8786e3` (run `39011763572`) terminó SUCCESS con Windows
`1218/0/9` y Linux `1238/0/9`, 38 suites Cargo y 7 pruebas Python por job,
y demos/controles verdes. Ese CI precede a U1 y U2; no los cubre.

### Binding puro de fases de retiro ABC

Este helper privado valida datos DTO para las fases de retiro. En A, el
JournalV2 asociado a la fase debe ser igual al Journal observado; Intent,
Journal y Context deben compartir el scope observado. También compara las
identidades persistidas, el binding estructural ADMENV, la longitud y el
digest del Context con sus registros. Los errores son fijos y redactados.

La extensión B/C conserva las fases existentes: B permite el binding
estructural ENV2 de `Prepared` con RETIRE2 `Committed`; C describe Intent
solo como dato, sin replay, cleanup o éxito. Ambos cortes usan valores ya
observados y no abren archivos, reconstruyen identidades nativas desde DTO
ni conceden frescura, autoridad o permiso de acción. No añaden un preflight
global ni un caller.

A quedó en el commit local
`e7e4ab53f7b6c292d545700c784caf720395da93` (299 líneas de churn). Su filtro
pasó `9/0/0` en Windows y Linux: seis pruebas heredadas y tres nuevas;
formato, clippy, frontera y diff-check terminaron en 0, y cleanup Linux
también terminó en 0. B/C quedó en el commit local
`3e96637ad0138671813e8f9080a29c8ba2cef986` (219 líneas de churn). Su filtro
pasó `11/0/0` por sistema: nueve pruebas heredadas y dos nuevas; los mismos
controles terminaron en 0 y cleanup Linux en 0. Los conteos de filtros son
acumulados por corte, no una suma de pruebas nuevas.

El CI publicado `4cb9945684486fb7515bf3acabfccd3a08cbc88c` (run
`39014089327`) terminó SUCCESS: Windows `1224/0/9` y Linux `1244/0/9`, con
38 suites Cargo y 7 pruebas Python por job, demos y controles verdes. Ese CI
precede a A y B/C, así que no cubre estos commits locales.

El helper no sustituye la observación owner-bound de archivos. La puerta
global de familia, la coordinación de filesystem, replay y cualquier caller
de apply/recovery siguen pendientes; 4/9, U7 y fase 04 permanecen parciales.

## Evidencia de verificación

Los dieciséis CI publicados terminaron `SUCCESS` en Linux y Windows. Cada fila
comparte 38 suites Cargo y 7 pruebas Python por sistema, con 0 fallidos y 9
ignorados por sistema. Consulta cualquier ejecución con `gh run view <run-id>`.

| SHA abreviado | Run ID | Windows (pass/fail/ignored) | Linux (pass/fail/ignored) |
|---|---:|---:|---:|
| `f8954d44` | `38932852247` | `1025/0/9` | `1044/0/9` |
| `c05e670` | `38964216495` | `1030/0/9` | `1049/0/9` |
| `c30c704` | `38967692704` | `1037/0/9` | `1056/0/9` |
| `f4bdfcd` | `38971406899` | `1049/0/9` | `1068/0/9` |
| `1b4b25e` | `38973980033` | `1057/0/9` | `1076/0/9` |
| `203bb6b` | `38978377109` | `1064/0/9` | `1083/0/9` |
| `5a73329` | `38983901592` | `1083/0/9` | `1102/0/9` |
| `e5bcf33` | `38988788938` | `1112/0/9` | `1131/0/9` |
| `ec973ee` | `38993176466` | `1140/0/9` | `1159/0/9` |
| `27fc2ff` | `38995183362` | `1148/0/9` | `1167/0/9` |
| `cad51cc` | `38999370612` | `1169/0/9` | `1189/0/9` |
| `6b26b9a` | `39004162338` | `1194/0/9` | `1214/0/9` |
| `e79099f` | `39008050182` | `1206/0/9` | `1226/0/9` |
| `be97f03` | `39009583297` | `1212/0/9` | `1232/0/9` |
| `b8786e3` | `39011763572` | `1218/0/9` | `1238/0/9` |
| `4cb994` | `39014089327` | `1224/0/9` | `1244/0/9` |

El CI `39004162338` corresponde al lote publicado
`6b26b9a8620f9aceb183b5d32e853a52944931ac`: Windows `1194/0/9` y Linux
`1214/0/9`; ambos jobs incluyeron 38 suites Cargo, 7 pruebas Python, demos y
controles verdes. Es la evidencia remota del lote Context; precede a I1A, B1 y
B2. Sus gates locales se documentan por separado. Ctx4 cerró gates locales y
quedó en el commit local `190d046` (312 líneas de churn).

Los CI `e79099f1fe1a43d4d4cc1440f630312ce20e1afb` (run `39008050182`) y
`be97f039c358f84a7f7383c94aafd70a0f977381` (run `39009583297`) también
terminaron `SUCCESS`: respectivamente Windows/Linux `1206/0/9` y `1226/0/9`,
y `1212/0/9` y `1232/0/9`. Cada job incluyó 38 suites Cargo, 7 pruebas
Python, demos y controles verdes. El primero precede a I2A/B; el segundo cubre
I2A/B y precede a I2C/I3. Al preparar esta actualización, I2C/I3 tienen gates
locales pero aún no están cubiertos por un CI publicado.

La prueba integrada de Unit6 (`5a270ca1686bb66169de6916b8a1d824da27ad09`, +86 líneas Rust) recorre con
fixture sintética la cadena privada A → retiro Journal/B → retiro Context →
Croot → retiro Intent → observación final. Comprueba la observación coherente y
redactada, la ausencia de los seis nombres reservados y, por separado, que las
entradas backup y stage están ausentes en el inventario; preserva la identidad y
el árbol del root, y confirma que el replay normal sigue rechazando tras soltar
la observación final. Es una prueba privada integrada, sin caller productivo ni
recuperación real.

Los gates focales locales se registran por separado del CI remoto. Los conteos
son pass/fail/ignored del filtro owner, no el total de la suite:

| Corte local | Windows (pass/fail/ignored) | Linux (pass/fail/ignored) | Controles locales |
|---|---:|---:|---|
| Unit4, `9c7755e` | `161/0/0` | `166/0/0` | fmt/clippy/frontera/diffcheck `0`; cleanup Linux `0` |
| Unit5-A, `aa0db56` | `164/0/0` | `169/0/0` | fmt/clippy/frontera/diffcheck `0`; cleanup Linux `0` |
| Unit5-B, `1b4b25e` | `166/0/0` | `171/0/0` | fmt/clippy/frontera/diffcheck `0`; cleanup Linux `0` |
| Unit5-C, `000c7b7` | `173/0/0` | `178/0/0` | fmt/clippy workspace con `-D warnings`/frontera/diff semántico `0`; cleanup Linux `0` |
| Unit6, `5a270ca` | `167/0/0` | `172/0/0` | fmt/clippy/frontera/diffcheck `0`; cleanup Linux `0` |

Unit5-C (`000c7b724545d2bb92f6e1c922122df4d72dda80`, parent `5a270ca`) añade +336 líneas
Rust, seis funciones nuevas y siete fixtures. Sus gates locales cerraron Windows
173/0/0 y Linux 178/0/0; formato, clippy de workspace con `-D warnings`, frontera y diff
semántico terminaron en 0. Cleanup Linux terminó en 0; la verificación usó nombres
y artefactos frescos.

### Estado local de Journal v2

| Corte | Estado verificado |
|---|---|
| Unidad 0, commit local `3872b28` | Vectores literales sintéticos Unix/Windows añadidos para Journal v1; no se cambió su codec ni bytes. Incluida en CI `5a73329`. |
| Unidad 1A, `3cb5a9f`; Unidad 1B, `eb9282d` | Gates locales aprobados en Windows y Linux según recibos; ambos commits incluidos en CI `5a73329`. |
| Unidad 2, `ccb390a`, modelo y codec | Filtros v2 `12/0/0` en ambos sistemas; compatibilidad v1 `10/0/0` Windows / `9/0/0` Linux. fmt, clippy, frontera y diff `0`; cleanup Linux `0`. Encoder máximo observado: 722 bytes Windows y 665 Linux, ambos bajo la cuota propia de 1024. |
| Unidad 3, `e83fd1f695f5af20474c42f00fcc704029cdf44c`, selector puro | Selector `6/0/0` y Journal v2 `12/0/0` en Windows y Linux; compatibilidad v1 `10/0/0` Windows / `9/0/0` Linux. fmt, clippy, frontera y diff semántico `0` en ambos sistemas; cleanup Linux `0`. Solo clasifica datos v1/v2. |
| J0, `6af0fb90b513f85e622662c47c39bfd3a05eaeb3` | Selector `7/0/0` en ambos sistemas y controles `0`; deriva la cuota común máxima desde los límites v1/v2, sin cambiar parseo ni semántica. Cubierto por CI `cad51cc`. |

El CI `e5bcf331` cubre Journal v2 y sus ancestros, incluidas las unidades 0–3,
y ADMENV2 A–B4. El CI `cad51cc` cubre además J0/J1, aunque precede a Ctx1–Ctx4.
El CI posterior del lote Context está registrado arriba y precede a I1A/B1/B2.
El selector solo clasifica datos; no activa recovery ni un caller.
J1A/J1B/J1C1/C2/C3 son una frontera de lectura file-bound distinta, cubierta
por `cad51cc`; las unidades Context posteriores tienen gates focales locales
documentados abajo. Los gates locales de Journal v2 y ADMENV2 A–B4 cerraron.

### Estado local del envelope ADMENV2

| Corte local | Windows (pass/fail/ignored) | Linux (pass/fail/ignored) | Controles locales |
|---|---:|---:|---|
| A, `ddd42bb` | `27/0/0` | `27/0/0` | fmt/clippy/frontera/diff `0` |
| B1, `8741dcb` | `32/0/0` | `32/0/0` | fmt/clippy/frontera/diff `0` |
| B2, `1dc22a6` | `38/0/0` | `38/0/0` | fmt/clippy/frontera/diff `0` |
| B3, `733c8db` | `46/0/0` | `46/0/0` | fmt/clippy/frontera/diff `0` |
| B4, `3020f6cf5b16ce5deb07786c901b76a2a17c4faa` | `53/0/0` | `53/0/0` | fmt/clippy/frontera/diff semántico `0` |

A–B4 agregan el envelope, framing, decoder y selector `FELIX-ADMENV` v2 con
kind `context_required` preservado y contexto interior v1. Cada cuota es 512
MiB; no se afirma límite de RSS. El selector usa el máximo de las cuotas v1/v2
antes de elegir una única rama, y cada codec valida su cuota propia. A–B4
cerraron sus controles locales y están cubiertas por CI `e5bcf331`. No hay
consumidor file-bound para retiro v2 ni integración con recuperación o apply.

### Replay tipado de Journal v2

R0 extrae el cálculo raw compartido y deja las comparaciones del wrapper v1 en
su orden original. R1 devuelve `NewEsperadoV2` con JournalV2 completo, kind y
estado conservados. R2 compara el vínculo estructural completo de ADMENV v2
antes del cálculo de perfil cerrado. Son transformaciones de datos en memoria;
no validan un owner ni admiten un caller de recovery.

| Corte | Evidencia local | Estado |
|---|---|---|
| R0, `655cc804` | Helper raw compartido; 8/0/0 tests focales en Windows y Linux. | Gates locales cerrados; formato, clippy `-D warnings`, frontera y diff en 0; cleanup Linux en 0. |
| R1, `055da2b3` | `NewEsperadoV2` conserva Journal completo; 13/0/0 tests focales en Windows y Linux. | Gates locales cerrados; formato, clippy `-D warnings`, frontera y diff en 0; cleanup Linux en 0. |
| R2, `5477b103` | Binding ADMENV v2 antes del cálculo tipado; filtro `transaccion::admision::envelope::` 56/0/0 en Windows y Linux, con seis pruebas v2 observadas por nombre. | Gates locales cerrados; formato, clippy, frontera y diff-check `0`; artefactos frescos y cleanup Linux `0`. |

El CI `ec973eeee769fce7fa56947f06d352ee00212d9c` (run `38993176466`) precede
a R0/R1/R2. El CI `27fc2ff` cubre esos tres cortes; el posterior `cad51cc`
incluye J0/J1 y sus ancestros, pero precede a Ctx1–Ctx4. Sigue pendiente el
replay owner-bound v2 para `&JournalV2`.

### Estado local de RETIRE v2

RETIRE v2 añade un modelo tipado, codecs puros y selección estricta de versión.
El frame conserva JournalV2 completo con `Committed` y `context_required`, tres
DTO FILEID, scope y metadata del Context. No convierte DTO a identidad nativa,
no hace I/O de filesystem, replay ni autorización. El frame de 1536 bytes es
una cuota derivada (`71 + 1024 + 3 × 147`), no un tamaño de registro medido; el
modelo limita el Context a `1..=512 MiB`.

| Corte | Evidencia local | Estado al preparar este borrador |
|---|---|---|
| C1, `86de718` | Modelo `Committed`/kind y límites Context; 3/0/0 Windows y Linux según los resúmenes de Cargo. | Gates locales cerrados. El wrapper Windows no devolvió un exit recuperable aunque registró el resumen exitoso; clippy, formato, frontera y diff pasaron. |
| C2, `205d94f` | Comparadores puros de DTO y Journal completo; 6/0/0 en Windows y Linux. | Gates locales cerrados; controles de formato, clippy, frontera y diff `0`. |
| D1, `ccc898a` | Encoder RETIRE v2, 9/0/0; compatibilidad RETIRE v1, 17/0/0 en ambos sistemas. | Gates locales cerrados; formato, clippy, frontera y diff `0`. |
| D2A, `9409fd9` | Preflight de framing, límites y cursor; 14/0/0 RETIRE v2 y 17/0/0 compatibilidad v1 en ambos sistemas. | Gates locales cerrados; formato, clippy, frontera y diff `0`; cleanup Linux `0`. |
| D2B, `406694efce983b9b129040022d899a0d13649128` | Modelo `1AA24E2A…6602E943`, codec `E6631637…CE49660`; 21/0/0 RETIRE v2 y 17/0/0 compatibilidad v1 en Windows y Linux. | Gates locales cerrados; formato, clippy `-D warnings`, frontera y diff `0`; cleanup Linux `0`. El intento previo falló en compilación con `E0432`/`E0425` antes de tests y no aporta conteos. |
| D3A, selector (`b67714f2`) | Parent `4CBD45947BB9E06E6176DEA43885D7349F9C48F2581FEDA74DE88F364D8CEE2B`, selector `0489365037F4E7109A5399848C030BEA5701795DAB1A6322031ACD504306632A`; +396/−0 líneas Rust, cinco tests. Selector 5/0/0 en ambos sistemas; v2 21/0/0 y v1 17/0/0 en ambos del snapshot anterior, con esas fuentes idénticas. | Gates corregidos cerrados en ambos sistemas: formato, clippy, frontera y diff `0`. El primer clippy falló por un import `JournalKind` no usado, luego eliminado. Cubierta por CI `ec973`. |
| D3B, límites de selector (`984ef824`) | Selector `66B05DA93706F5A50D19ECEEF284229B0D8D5D32508404F8967BEA996ACC1EF6`, test `FDCAF8EF12C150EF482871FDED55A246C8BD70F86C0F31C3A1787899F5442400`; +67/−0 líneas Rust, dos casos nuevos y siete pruebas en total. | Gates locales cerrados: selector 7/0/0, formato, clippy, frontera y diff-check rastreado en 0; cleanup Linux 0 y artefactos frescos. El test fija aceptación exacta en 16 bytes y rechazo a 17 con cuotas en ambos órdenes. Cubierta por CI `ec973`. |

La corrección D2B movió el import de la cuota Context al módulo padre y arregló
el typo del helper existente; el gate receipt corregido cierra esa unidad. El CI
anterior `e5bcf331` antecede a C1/D2A; el posterior `ec973` cubre RETIRE v2
hasta D3B, pero no los cortes R0/R1/R2 que llegaron después.

### Registro histórico detallado

En la ronda de `aa814e3c28231bb83934a1275328594fa8853649`, ese candidato estaba
publicado en `origin/codex/plan-demostracion` y el CI exacto `38764635021`
terminó SUCCESS en ambos jobs, Linux y Windows, y en todos sus pasos. Ese
resultado corresponde a esa base y no cubre unidades posteriores. No se
atribuye una causa a fallos históricos por el resultado actual.

En esa ronda, la verificación local reportada sobre ese SHA registró en Windows
783 passed/0 failed/9 ignored para workspace, recovery 78/0/3, diagnósticos
2/2 y filtros focales 37/37. `cargo fmt`, clippy con warnings denegados y el
chequeo de frontera pública terminaron verdes. La repetición de runtime nativo
en WSL se canceló ante la orden de detener; no cuenta como resultado ni como
runtime Linux exitoso.

El CI remoto exacto de `713cec7f629c794c635a6ae8a89b1c1d8a619559` (run
`38833928734`) terminó SUCCESS en Linux y Windows, en ambos jobs y todos sus
pasos. La suite del workspace en WSL terminó con exit 0, aunque su salida no
conservó un conteo agregado. Esa evidencia precede y no cubre la lectura ni la
escritura temporal ancladas.

El commit local `7ff9e0f` añade la creación exclusiva temporal al lease de
lectura. Sus verificaciones locales focales terminaron Windows: anclado 127/127
y admisión 40/40; Linux con `TMPDIR` nativo asertado: anclado 131/131 y
admisión 40/40. La lectura reabre el frame sintético después de soltar y
readquirir el lock en el mismo proceso; no simula reinicio ni recuperación tras
una caída. `cargo clippy --workspace --all-targets --locked --offline -- -D
warnings`, formato, frontera pública y diffcheck también terminaron verdes. Son
filtros focales y checks locales, no una suite de tests del workspace ni CI para
este commit. El CI remoto verificado de `4460ba0` (run `38845533059`) terminó
verde en Linux y Windows, pero cubre la base anterior y no incluye `7ff9e0f`.

El commit local `679c2c2` añade una prueba de pipeline de storage con fixture
sintética: publica contexto y journal `Prepared`, suelta y readquiere el lock
en el mismo proceso, reabre ambos por leases secuenciales y llama al replay
puro. Comprueba identidades y bytes, y que un journal cambiado o motor/policy
incompatibles no alteren los archivos. No observa `New` instalado ni demuestra
recovery, reinicio o coordinación de un caller. El filtro reproducible es:

```text
cargo test --locked -p universo-okf --lib publica_y_reabre_contexto_y_journal_prepared_para_replay_puro
```

En ese commit, los filtros locales fueron Windows: contexto 21/21 y admisión
40/40; Linux: contexto 22/22 y admisión 40/40. Clippy de workspace con
warnings denegados terminó verde en ambos sistemas; formato, chequeo de frontera
y diffcheck terminaron verdes en Windows. El CI remoto de esa ronda corresponde
a `8c12e197dd51150905a5871559c52f53472b12ee` (run `38849833617`):
sus suites de workspace reportaron Linux 834/0/9 y Windows 822/0/9 en 38
targets, además de 7 pruebas Python por sistema. Ese CI cubre la base anterior;
no incluye `33c7d52`, `86569b3` ni `679c2c2`.

El CI remoto exacto de `fad5736030ac86798df75ae16d420c1682a5e915` (run
`38852225652`) terminó SUCCESS en Linux y Windows. Reportó Linux 845 pass/0 fail/9 ignored y
Windows 833 pass/0 fail/9 ignored en 38 targets, además de 7 pruebas Python
por sistema. Este resultado es de `fad5736`; no cubre el observer local
`3daced9` ni la fixture de pipeline.

La unidad local `3daced9` añadió observación read-only del root anclada al
parent retenido. Sus filtros focales registraron Windows owner 12/12,
anclado 18/18 e inventario 17/17; Linux owner 13/13, anclado 17/17 e
inventario 23/23. Clippy de workspace con warnings denegados, formato,
frontera pública y diffcheck terminaron verdes en las verificaciones locales.
La huella observada es un dato; no autoriza admisión, recovery ni cleanup.

La unidad local de fixture prepara un root con el `New` raw esperado, liga el
journal a su identidad real, publica y reabre ambos siblings bajo un lock
readquirido en el mismo proceso, y compara el replay con la observación raw.
El filtro reproducible es:

```text
cargo test --locked -p universo-okf --lib fixture_prepara_y_reabre_replay_con_root_new_real
```

Sus filtros focales registraron Windows owner 13/13, contexto 21/21, journal
59/59 y envelope 20/20; Linux owner 14/14, contexto 22/22, journal 60/60 y
envelope 20/20. Clippy de workspace con todos los targets y `-D warnings`,
formato, frontera pública y diffcheck terminaron verdes en ambos sistemas; en
Linux se fijó un `TMPDIR` nativo corto. La prueba no simula reinicio, pérdida de
energía, recovery ni un caller de producción, y no prueba una instalación real
de `New`. La primera compilación se detuvo por visibilidad del método de
publicación desde la fixture; un seam `cfg(test)` lo resolvió sin ampliar API.
La revisión también corrigió el lifetime de la ruta, la comparación de la
identidad opaca y el orden de `rustfmt` antes de los gates verdes. El cleanup del
harness se registró aparte y no fue un fallo de comportamiento del producto.
Esta unidad quedó registrada en
`1f5507ed90ebccd2e4a2946fd48d1d0c985dd08b`.

El commit `c3853e03819386d4c4364d57be615f26b2b2766c`, sobre la base
`1f5507ed90ebccd2e4a2946fd48d1d0c985dd08b`, añade un coordinador privado de
solo datos que carga journal y contexto por leases secuenciales, valida su
binding y replay, y compara la huella completa del root raw. Sus pruebas cubren
las relaciones `Coincide` y `Ausente`, más dos casos de `Diferente`: mismo ID
con digest distinto e ID distinto con contenido igual. También comprueban que
incompatibilidades de motor/policy, binding, parent o siblings faltantes fallan
cerradas y preservan los archivos observados. Los parámetros de motor y policy
siguen siendo datos explícitos, sin identidad soportada por un caller de
producción.

Los filtros focales de esta unidad registraron Windows owner 16/16, contexto
21/21, journal 59/59 y envelope 20/20; Linux owner 17/17, contexto 22/22,
journal 60/60 y envelope 20/20. Clippy de workspace con todos los targets y
`-D warnings` terminó verde en ambos sistemas; formato y diffcheck pasaron en
ambos, y el chequeo de frontera pública pasó en Linux y Windows. El CI remoto
consultado durante esa revisión inicial correspondía a `fad5736` y no cubría
esta unidad.

El CI remoto exacto de `650c2555d6087cdea6fd3df22f70dc3d129578d4` (run
`38855248822`) terminó SUCCESS en Linux y Windows. Las suites de workspace
reportaron Linux 855 pass/0 fail/9 ignored y Windows 842 pass/0 fail/9 ignored
en 38 targets, además de 7 pruebas Python por sistema. Es evidencia de esa
base publicada; no cubre la primitive local `4f7d3286cc63c4b181029acc75ae1044c1e4c963`
ni su integración de cuatro nombres.

La primitive local `4f7d3286cc63c4b181029acc75ae1044c1e4c963` prueba la
observación de un sibling no-follow: solo una ausencia observada produce
`Ausente`; un archivo regular se valida por scope e identidad del mismo handle,
sin leer payload, y los errores fallan cerrados. Sus filtros focales registraron
Windows owner 20/20 y Linux owner 22/22; formato, clippy con warnings
denegados, frontera pública y diffcheck pasaron en ambos sistemas.

La integración `bddaea4c9285dd20fc6ea39bd163b00d7c55ca5d` combina los cuatro
resultados de presencia bajo el mismo owner. Es observación secuencial de datos:
no es permiso para iniciar una operación, caller, dispatch o cleanup, ni
garantiza un snapshot frente a writers externos. Sus filtros focales registraron
Windows owner 8/8 y contexto 21/21, Linux owner 9/9 y contexto 22/22. Formato,
clippy de workspace con todos los targets y `-D warnings`, frontera pública y
diffcheck pasaron en ambos sistemas; Linux usó un `TMPDIR` corto nativo y el
cleanup del harness terminó correctamente. Linux ejercitó el parent movido y
preservó los decoys; en Windows el sharing del guard bloqueó el rename, por lo
que la rama explícita preservó el estado sin afirmar que ejercitó ese escenario.
El CI remoto citado arriba no cubre esta integración local.

La unidad local `8c6893a93d9fff8b544db6a5d677a7f24a3448f6` pasó los filtros
focales Envelope 10/10 y replay 8/8 en Windows y Linux. Clippy de workspace
con todos los targets y `-D warnings`, formato, frontera pública y diffcheck
pasaron en ambos sistemas; Linux usó un `TMPDIR` válido y lo retiró al
terminar. El CI remoto exacto de la base anterior que se detalla a continuación
no cubría esta unidad. En aquella revisión todavía no se había registrado un
CI remoto específico para el selector; el CI posterior de `e8ac035` lo verificó,
en esta sección.

El CI remoto exacto de `66633e407d44aaf9c106ac5bb5f71ba2f5b27cbd` (run
`38857168736`) terminó SUCCESS en Linux y Windows. Las suites de workspace en
38 targets reportaron Linux 864 pass/0 fail/9 ignored y Windows 850 pass/0
fail/9 ignored, además de 7 pruebas Python por sistema. Esta es la base previa
al selector cerrado de tags; el run no cubre la unidad `8c6893a`.

El CI remoto exacto de `e8ac0353d353e6fafb8d520c540efb223adbde21` (run
`38858628837`) terminó SUCCESS en Linux y Windows. Las suites de workspace en
38 targets reportaron Linux 868 pass/0 fail/9 ignored y Windows 854 pass/0
fail/9 ignored, además de 7 pruebas Python por sistema. Ese CI precede y no
cubre la captura anclada a través del owner descrita a continuación.

La captura fija `f2e76f8ccafae2b0be422be6cf1600e42b150f01` pasó sus filtros
locales de captura en Windows 37/0/1 y Linux 36/0/1; formato, clippy de
workspace, frontera pública y diffcheck terminaron verdes en ambos sistemas.
Esos filtros prueban la captura desde un `DirectorioAnclado`, sin integración
con el owner ni con un caller.

El refactor `3cf8b605a643fb11949eb35785e1bbbdaef33864` conserva el parent ya
retenido y comparte la apertura read-only del root, revalidando lock, identidad
y scope antes de abrirlo. Sus filtros focales del owner pasaron Windows 24/24
y Linux 26/26; fmt, clippy, frontera pública y diffcheck individuales también
terminaron verdes.

La unidad `66e8419f63ffd409eaf687fc7aa54183613ac19c` integra la captura fija
desde el owner. Los filtros locales pasaron owner 28/28 y captura 37/0/1
(pass/fail/ignored) en Windows, y owner 30/30 y captura 36/0/1 en Linux. Fmt,
clippy de workspace, frontera pública y diffcheck terminaron verdes en ambos
sistemas. Linux movió el parent y capturó el root original, distinto del decoy;
Windows bloqueó el rename por sharing y la prueba informó que no ejercitó el
decoy. El cleanup del harness se registró por separado. Una primera salida
agregada de Linux terminó con exit 1 sin diagnóstico; la repetición controlada
por etapas y cleanup terminó con exit 0. La salida inicial se conserva aparte,
sin atribuirla al producto ni inventar una causa. En esa revisión, el CI remoto
consultado era `e8ac0353d353e6fafb8d520c540efb223adbde21` (run
`38858628837`); no cubría esta unidad.

La unidad local `43b2599548a33d41c4534dd70283a107f7ca3582` añade la
comparación soportada bajo el owner: carga journal y contexto por leases
secuenciales, resuelve los tags cerrados sin strings esperados de un caller,
valida binding/replay y compara la huella raw completa. Sus filtros focales
registraron owner 30/30 en Windows y 32/32 en Linux, y envelope 24/24 en ambos;
fmt, clippy de workspace, frontera pública y diffcheck también terminaron
verdes en ambos sistemas. El coordinador sigue devolviendo datos, no autoridad
de recovery.

La unidad `8ea2193fbccd6cb20892b420707311145160be3d` añade evidencia de lectura
canónica (identidad de archivo, longitud y SHA-256) calculada durante la misma
lectura acotada del handle. Sus filtros registraron contexto 24/24 en Windows,
25/25 en Linux y envelope 24/24 en ambos; fmt, clippy de workspace, frontera
pública, diffcheck y cleanup terminaron verdes. La evidencia es dato: no es
autenticación, permiso ni autoridad de recovery.

El CI remoto exacto del commit publicado
`b69941b7206bc27873d17487ab34e0193ca34123` (run `38861852735`) terminó SUCCESS
en Linux y Windows. Las suites de workspace en 38 targets reportaron Linux
874 pass/0 fail/9 ignored y Windows 860 pass/0 fail/9 ignored, además de 7
pruebas Python por sistema. En ese punto, el CI cubre la captura anclada del
owner `66e8419` y sus antecedentes. Es anterior y no cubre el comparador local
`43b2599`, la evidencia de lectura `8ea2193`, la unidad A `b3b16c6` ni la
unidad B local `0f2187c2759573f1d86747efbcc61589e31ea6de`.

La unidad A solo emite su prueba opaca cuando el replay soportado resulta
`Admitida`, la fase del Journal es `Installed` o `Committed` y el root raw
coincide con `NewEsperado`. Conserva el `Journal` completo, incluido su
fingerprint `New`, y un préstamo mutable del mismo owner; los leases de
journal/contexto se sueltan antes de devolverla. No retiene el envelope ni el
`NewEsperado` completo. Los tests construyen un envelope ligado y luego fijan
una fase sintética en el Journal; no simulan una transición real de apply ni
demuestran que un caller instaló `New`.

El método local `PruebaReplayAdmitidoRootCoincidente::revalidar(self)`
consume y reemite la prueba solo después de revalidar guard, parent y scope,
observar los cuatro slots de journal/contexto, y recargar el ID y Journal
completo, incluida su
fase. También compara la identidad, longitud y SHA-256 del contexto, vuelve a
comprobar binding y replay soportados, y exige que la huella del root siga
coincidiendo. La igualdad del Journal es semántica del registro completo; el
SHA-256 se calcula sobre el frame crudo del contexto. Los tests usan fixtures
sintéticas y preservan el storage. El commit local
`69f3842e9b208aeadaf7721d84a9fc0d07b541af` pasó filtros focales Windows
(owner 51/51, contexto 24/24, envelope 24/24) y Linux (owner 54/54, contexto
25/25, envelope 24/24). Formato, clippy de workspace con todos los targets y
warnings denegados, frontera pública y diffcheck pasaron en ambos sistemas; la
limpieza del `TMPDIR` Linux terminó con exit 0. El CI de
`7a7273f42f61a391e0eb28d0d9226820c2d3956d` registrado abajo es anterior y no
cubre este commit. El CI posterior `ec973` cubre el estado publicado hasta D3B,
pero no R0/R1/R2. Esta revalidación no observa backup ni stage, no decide una
fila y no concede autoridad. El consumidor descrito a continuación pasó sus
filtros locales en Windows y Linux y quedó dentro de la base publicada anterior
a R0/R1/R2.

La unidad consumidora `b94ef7026de087539ed2a1ef39f3b6d5c16e1260` añade
`observar_fila_conservar_new(self)` sobre la prueba revalidada y contiene 396
altas. Vuelve a observar el journal y el inventario bajo el mismo owner, exige
que el registro completo no haya cambiado, y considera solo `Installed` o
`Committed` con root `New`, stage ausente y backup `Absent` u `Old`. Marca
`root_new_validated` únicamente en una copia local del inventario para
consultar `decide_recovery`; compara la decisión y la secuencia exacta de pasos
esperados para cada fase. Devuelve una observación privada con la prueba y esos
datos; no ejecuta pasos ni ofrece una capability de mutación, cleanup, apply o
dispatch. Sus fixtures cubren ambos estados elegibles y backup ausente/Old,
además del rechazo con preservación para stage presente, `PartialOld` y backup
`Unknown`. Una primera compilación reportó E0277 en dos payloads del fixture y
un import sin uso; se corrigieron los argumentos como slices y se retiró el
import sin quitar cobertura. Los filtros focales pasaron Windows owner 55/55,
contexto 24/24 y envelope 24/24; Linux owner 58/58, contexto 25/25 y envelope
24/24. Formato, clippy de workspace con todos los targets y warnings
denegados, frontera pública y diffcheck pasaron en ambos sistemas; la limpieza
del `TMPDIR` Linux terminó con exit 0. El CI base
`22c9869915367e5d32ea8d9899c410c57dc0a810` (run `38870400830`, Linux 901
pass/0 fail/9 ignored, Windows 886/0/9 en 38 targets y 7 pruebas Python por
sistema) es anterior y no cubre esta unidad. El rollback retira el tipo y
método consumidor, imports, wiring y tests nuevos, conservando el productor,
la prueba/revalidación, el observer y la tabla legacy. El rollback de
`69f3842` se conserva como límite separado: como el consumidor depende de
`revalidar`, primero se revierte el consumidor; después se revierte el método,
wiring y tests de revalidación, conservando la prueba original, el observer y
la tabla legacy.

La unidad A `b3b16c646d64b4ad7629af2494e84bd8dcbecd7d`
pasó localmente owner 34/34, contexto 24/24 y envelope 24/24 en Windows; Linux
owner 36/36, contexto 25/25 y envelope 24/24. Formato, clippy de workspace con
todos los targets y warnings denegados, frontera pública y diffcheck pasaron;
la limpieza del harness terminó correctamente. El CI remoto registrado arriba
precede A y no la cubre.
La unidad B de pruebas `0f2187c2759573f1d86747efbcc61589e31ea6de` tiene 161
líneas de churn real y pasó sus gates focales: Windows owner 39/39, contexto
24/24 y envelope 24/24; Linux owner 41/41, contexto 25/25 y envelope 24/24.
Formato, clippy de workspace con todos los targets y warnings denegados,
frontera pública y diffcheck terminaron verdes en ambos sistemas; la limpieza
del harness Linux terminó con exit 0. Cubre temporales presentes, canónicos
ausentes, tags no soportados, root ausente reteniendo el original y contenido
alterado con el mismo ID. Una compilación inicial detectó un movimiento de
`PathBuf` en dos aserciones; se corrigió tomando la referencia antes de los
gates verdes. El CI remoto b699 es anterior y no cubre B. Ninguna de estas
pruebas habilita una fila, cleanup, apply ni recovery.

El refactor local `dc75aecc9177ee39376fc4ff191055d78cadc1b0` extrae una apertura
relativa de siblings desde el parent retenido y conserva el orden de
revalidación y las validaciones del root. Sus filtros registraron owner 39/39
en Windows y 41/41 en Linux; el filtro de captura soportada fue 4/4 en ambos,
incluido dentro del conteo del owner y sin sumarlo aparte.
Formato, clippy de workspace, frontera pública y diffcheck terminaron verdes;
la limpieza del harness Linux terminó con exit 0.

El commit local `b4d2b88563527fcc3d17124153b0be1472fde87f` añade un observador
privado que carga el journal canónico y observa root, backup y stage desde el
parent retenido. Devuelve `Journal` y `RecoveryInventory` como datos y mantiene
`root_new_validated=false`; no selecciona fila ni ejecuta recovery o cleanup.
El core pasó filtros locales Windows owner 43/43, contexto 24/24 y envelope
24/24; Linux owner 46/46, contexto 25/25 y envelope 24/24. Formato, clippy de
workspace, frontera pública y diffcheck pasaron; la limpieza del harness Linux
terminó con exit 0.

La extensión combinada de pruebas de storage y namespace quedó integrada en
`996ec93f91da61a33e6130b4f5a858de70ef79a1` con 342 líneas de churn real. Los
filtros locales pasaron Windows owner 47/47, contexto 24/24 y envelope 24/24;
Linux owner 50/50, contexto 25/25 y envelope 24/24. Formato, clippy de
workspace con todos los targets y `-D warnings`, frontera pública y diffcheck
terminaron verdes en ambos sistemas; la limpieza del harness Linux pasó.
Las fixtures cubren root ausente con backup Old y stage New, journal ausente,
parent del journal incompatible y parent movido con siblings decoy. Los
errores de journal ausente o parent incompatible fallan cerrados; las
observaciones conservan IDs y bytes. Linux ejercitó el parent movido y los
decoys; la prueba falla si el rename no ocurre. En Windows, sharing con
códigos 5/32/33 bloqueó el movimiento y la rama explícita informó que no
ejercitó el decoy. Son fixtures sintéticas de observación, no admisión,
instalación por un caller ni recovery.

El CI remoto exacto de la base `ce5337ed5b63ae1910a6778e41fea3df50043a8a`
(run `38866490446`) terminó SUCCESS en Linux y Windows. En 38 targets reportó
Linux 888 pass/0 fail/9 ignored y Windows 874 pass/0 fail/9 ignored, además de
7 pruebas Python por sistema. Ese run precede y no cubre el refactor
`dc75aecc`, el observer `b4d2b885` ni la cobertura `996ec93f`.

Los filtros focales reproducibles, ejecutados por separado en Windows y Linux, son:

```text
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::contexto_admision
cargo test --locked --offline -p universo-okf --lib transaccion::admision::envelope
```

Son fixtures sintéticas del filesystem bajo el lock cooperante; no ejecutan apply
ni recovery. El rollback de `996ec93f` retira solo la cobertura test-only y su
wiring, conservando el observer core. Para retirar el observer, primero se
revierte esa cobertura dependiente; después se retiran el módulo, las pruebas
core y el wiring de `b4d2b885`. Solo entonces se revierte el helper compartido
`dc75aecc`, que el observer utiliza para abrir siblings desde el parent
retenido. Ese helper refactor no modifica el recovery legacy; ningún paso de
esta secuencia cambia el journal v1 ni sus rutas legacy.

La nota histórica de rollback de las unidades de prueba de la attestation sigue
aplicando por separado: revertir B (`0f2187c`) quita solo su cobertura y wiring,
conservando A (`b3b16c6`); revertir A retira su productor, prueba y
wiring/reexport, sin alterar el journal v1 ni el recovery legacy.

El CI remoto exacto de la base publicada `7a7273f42f61a391e0eb28d0d9226820c2d3956d`
(run `38868788132`) terminó SUCCESS en Linux y Windows. En 38 targets reportó
Linux 897 pass/0 fail/9 ignored y Windows 882 pass/0 fail/9 ignored, además de
7 pruebas Python por sistema. Es una base previa: no incluye
`69f3842e9b208aeadaf7721d84a9fc0d07b541af`.

La unidad `4598a3a0c7dc1f4665e6493f8a513e355f313505` añade el DTO versionado
de identidad persistida y su conversión unidireccional desde una identidad de
archivo observada en un handle. Tiene 386 líneas de churn real. El filtro
focal `cargo test --locked --offline -p universo-okf --lib transaccion::anclado::identidad_archivo`
pasó 8/8 en Windows y Linux para la unidad base, y 11/11 en ambos con las
pruebas complementarias; formato, clippy de workspace con todos los
targets y `-D warnings`, frontera pública y diffcheck terminaron verdes.
El commit `afa924c19534cc3b10059c47d256d514b538521b` añade 109 líneas de pruebas
para valores máximos, lecturas cortas, `Interrupted` y errores de I/O en header,
payload y EOF. El filtro focal pasó 11/11 en ambos sistemas; formato, clippy,
frontera pública y diffcheck pasaron en ambos, y la limpieza del harness Linux
terminó con exit 0. El CI remoto exacto de `8e87735` (run `38872303821`) reportó
Linux 905 pass/0 fail/9 ignored y Windows 890/0/9 en 38 targets, además de
7 pruebas Python por sistema. Es anterior y no cubre el DTO ni las pruebas
adicionales. Revertir las pruebas `afa924c` vuelve al DTO de `4598a3a`; retirar
también ese DTO y su wiring no cambia los codecs del journal o del contexto ni
el recovery legacy. La unidad no cambia el estado global: 4/9 sigue siendo el
progreso y U7/fase 04 siguen parciales.

El codec `FELIX-RETIRE` v1 de `4bf47893870be24bce8c35ec692aa402cbe4e6fe`
tuvo 396 líneas de churn y pasó 3/3 pruebas focales en Windows y Linux. Las
pruebas complementarias de `6e1349246b71ed5acb82cab7927bcde1cc84b738` añaden
157 líneas y elevan el filtro focal a 7/7 en ambos sistemas. También pasaron
formato, clippy de workspace con todos los targets y `-D warnings`, chequeo de
frontera y diffcheck en ambos; la limpieza del harness Linux terminó con exit
0. El comando reproducible es
`cargo test --locked --offline -p universo-okf --lib transaccion::anclado::intent_persistido`.
Las pruebas abarcan roundtrip canónico, versión/fase/cuotas, truncamiento en
cada frontera, corrupción de magic/identidades y journal desconocido o no
canónico. El CI remoto exacto `0fac7be` (run `38875241340`) es anterior: Linux
914/0/9 y Windows 899/0/9 en 38 resúmenes, más 7 pruebas Python por sistema; no
cubre estas unidades. Revertir `6e134924` conserva el codec base; retirar luego
`4bf4789` elimina el DTO y su wiring sin cambiar los codecs de journal/contexto
ni el recovery legacy. Esas unidades de codec y namespace no añadieron escritura ni lectura o parseo
del contenido del intent desde el filesystem, ni reconciliación; el writer
privado se describe más adelante. El observer solo comprueba la presencia de
sus nombres canónico y temporal; si uno está presente o no se puede verificar,
la prueba nueva se bloquea y el archivo se preserva.

La unidad de namespace `36bd34a55cddcb53bb6c4154f83474635c49ebd5` añade al
observer dos slots derivados internamente del basename: el intent canónico
`.<basename>.felix.admctx-retire` y su temporal
`.<basename>.felix.admctx-retire.tmp`. Junto con el journal y el contexto, el
owner observa así seis nombres relativos al parent retenido. Para cada entrada,
solo una ausencia observada se informa como `Ausente`; las presentes se
validan por tipo, scope e identidad del mismo handle, sin leer payload. Un
error de tipo, reparse, scope, identidad o componente demasiado largo falla
cerrado. Es una observación secuencial, no un snapshot frente a writers externos
ni un preflight global. La presencia de cualquiera de los dos slots del intent
impide emitir o revalidar la prueba de replay admitido; no se intenta
interpretar, publicar, reconciliar o limpiar el archivo. Las pruebas de ambos
slots verifican el bloqueo de emisión y la invalidación de una prueba ya emitida,
conservando los canarios observados.

El commit tiene 256 líneas de churn real (235 altas y 21 bajas, cinco paths).
El filtro focal reproducible, ejecutado en Windows y Linux, es:

```text
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion
```

Pasó Windows 58/58 y Linux 63/63. Formato, clippy de workspace con todos
los targets y `-D warnings`, chequeo de frontera pública y diffcheck terminaron
verdes en ambos; la limpieza del harness Linux terminó con exit 0. Linux
ejercitó el nombre que supera el límite del componente y el parent movido con
decoys. En Windows, el sharing bloqueó el movimiento del parent y la rama
explícita no ejercitó los decoys; la fixture de longitud límite está condicionada
a Linux y no se ejecuta en Windows.
El CI exacto de `e8cd77e` (run `38877305256`) es anterior y no cubre este
commit: en 38 targets reportó Linux 921 pass/0 fail/9 ignored y Windows 906
pass/0 fail/9 ignored, más 7 pruebas Python por sistema. El rollback de
`36bd34a` retira la extensión de seis nombres y sus
pruebas/wiring, restaurando el preflight previo de cuatro; el codec de datos
`FELIX-RETIRE` y el recovery legacy permanecen intactos. Esta unidad no cambia
4/9: U7 y fase 04 siguen parciales.

La factory `PruebaReplayAdmitidoRootCoincidente::preparar_retiro_contexto`
del commit local `656c9b97b94c9c68454742be205c9d7512e1aa08` consume y revalida
la prueba; después exige Journal `Committed` sin cambios, root `New` y backup y
stage `Absent`. Su wrapper conserva la prueba no clonable y el préstamo del
owner. Las fixtures bajo lock prueban el caso válido, el rechazo de `Installed`,
backup `Old` coherente aunque la tabla de conservación lo admita, stage presente
y contexto modificado; los rechazos preservan los IDs y bytes observados. Son
fixtures sintéticas de filesystem, no ejecución de apply o retiro.

El commit añadió 188 líneas en cuatro paths. El filtro focal
`cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion`
pasó Windows 63/63 y Linux 68/68. Formato, clippy de workspace con todos los
targets y `-D warnings`, frontera pública y diffcheck pasaron en ambos sistemas;
la limpieza del harness Linux terminó con exit 0. El CI exacto de la base
`968b70c9bea07c8203152e4180ae4d91d942920e` (run `38878921722`) es anterior y
no cubre esta factory: en 38 targets reportó Linux 926 pass/0 fail/9 ignored y
Windows 909 pass/0 fail/9 ignored, además de 7 pruebas Python por sistema. El
rollback retira solo la factory y sus tests/wiring, conservando el consumidor,
la prueba y revalidación, el observer y la tabla legacy. No cambia 4/9; U7 y
fase 04 siguen parciales.

La unidad local `3e2760d0baaa9ff7faed7e89f9790d8d234782a7` extrae la apertura y
validación común del temporal y añade `crear_vacio` más una escritura concreta
con `write_all` y `sync_all`. La capacidad `ArchivoTemporalVacio` conserva el
handle del archivo, su identidad medida desde ese handle y el parent retenido
con su identidad y scope. No adquiere ni retiene por sí misma el lock ni al
owner; una operación futura debe mantener su propia prueba ligada al owner.
`crear_con` conserva su orden y comportamiento previos. La nueva capacidad
rechaza una escritura si el archivo dejó de estar vacío; los errores posteriores
a su creación preservan el archivo y no ejecutan cleanup. Esta primitiva no
escribe un intent, no publica ni elimina nombres y no demuestra durabilidad del
directorio o resistencia a pérdida de energía.

El cambio tuvo 318 líneas de churn real en dos paths. Estos filtros focales
pasaron en Windows y Linux:

```text
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::archivo_temporal
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::contexto_admision
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::journal_temporal
```

Los filtros reportaron respectivamente Windows 12/12, 24/24 y 59/59, y Linux
13/13, 25/25 y 60/60. Formato, clippy de workspace con todos los targets y
`-D warnings`, frontera pública y diffcheck pasaron en ambos sistemas; la
limpieza del harness Linux terminó con exit 0. Linux ejercitó el parent movido
y el decoy; en Windows el sharing bloqueó el rename y esa rama indicó que el
decoy no se ejercitó. El rollback revierte la extracción y la capacidad nueva
junto con sus pruebas, conservando el comportamiento previo de `crear_con`,
el codec de datos y el recovery legacy. El CI exacto de la base
`a0093f898bb8f5c71540e879703d6449ba120113` (run `38880090980`) terminó SUCCESS
en ambos jobs: Linux 931 pass/0 fail/9 ignored y Windows 914/0/9 en 38
resúmenes, además de 7 pruebas Python por sistema. Ese CI precede y no cubre
esta unidad.

El commit local `e3c7ebd455fd6edf36663a23a08e1bca28a202a4` añade el writer
privado `PruebaInicioRetiroContexto::escribir_intent_temporal`. Rechaza las
rutas UNC sintácticas antes de las lecturas de revalidación de esta entrada;
después revalida el estado fresco,
deriva los nombres sibling internos y crea el temporal de intent en exclusiva.
Mide la identidad propia desde el handle vacío antes de codificar `FELIX-RETIRE`
v1, y escribe y sincroniza el frame acotado. `IntentTemporalEscrito` retiene la
prueba y el owner, además del ID, longitud, digest y bytes esperados; no vuelve
a abrir el nombre visible, no confirma su contenido, no publica ni renombra y
no elimina. Los errores posteriores a `create_new` preservan el archivo y no
devuelven recibo. Las pruebas independientes de los tres IDs detectan cruces
de campos en el codec; otras comprueban que mutar el contexto o el journal tras
la prueba bloquea antes de crear el intent y conserva los cambios. El gate UNC
es sintáctico: no hubo fixture UNC de runtime ni se afirma detectar unidades
mapeadas o todos los mounts.

El writer añadió 334 líneas de churn real en seis paths (329 altas y 5 bajas).
Los filtros focales fueron:

```text
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::intent_persistido
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::journal_temporal
```

En Windows reportaron 66/66, 8/8 y 59/59; en Linux, 71/71, 8/8 y 60/60.
Formato, clippy de workspace con todos los targets y `-D warnings`, frontera
pública y diffcheck terminaron verdes en ambos sistemas; la limpieza del
harness Linux terminó con exit 0. El CI exacto de la base publicada
`c5fe6d28f3c78b94fa07315c8fcedcd35393bb4b` (run `38881213774`) terminó SUCCESS
con Linux 938 pass/0 fail/9 ignored y Windows 921/0/9 en 38 resúmenes, además
de 7 pruebas Python por sistema. Ese CI precede y no cubre el writer. El
rollback retira el writer y sus pruebas/seams, preservando la factory y el
helper de temporal vacío, los codecs v1 y el recovery legacy. En ese punto aún
faltaba la verificación completa previa a publicar; también estaban pendientes
la publicación, cleanup, preflight global y caller productivo. U7 y fase 04
siguen parciales; el progreso global permanece en 4/9.

El commit local `fa96f27e7fc684726727d548c26073eafdd5076d` añade
`IntentTemporalEscrito::verificar_temporal`. Consume el estado escrito y abre
el nombre temporal exacto desde el parent retenido. La prueba opaca conserva el
handle de lectura y el proof; el verificador comprueba en ese mismo handle que
el archivo sea regular y no reparse, que su scope coincida, que su ID sea el
medido al crear, y que longitud, bytes, SHA-256 y EOF coincidan con el frame
esperado acotado. Esto acredita solo el nombre y archivo observados en esa
operación: no revalida de nuevo el Journal/contexto/replay/root, el inventario
backup/stage ni el namespace completo, y no concede autoridad de publicación
o cleanup. La prueba normal sigue exigiendo ambos nombres de intent ausentes;
esta operación especial no la relaja. La publicación y una revalidación
completa inmediatamente anterior siguen pendientes.

La unidad tuvo 360 líneas de churn real (358 altas y 2 bajas). El filtro focal
`cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion`
pasó Windows 72/72 y Linux 77/77. Formato, clippy de workspace con todos los
targets y `-D warnings`, frontera pública, diffcheck y cleanup terminaron
verdes. Linux ejercitó parent movido y decoys; en Windows sharing bloqueó el
rename y la rama indicó que el escenario no se ejercitó. El CI exacto de la
base `e359b02` (run `38884144988`) terminó SUCCESS en ambos sistemas, pero es
anterior y no cubre `fa96f27`: reportó Linux 942 pass/0 fail/9 ignored y
Windows 925/0/9 en 38 resúmenes por sistema, además de 7 pruebas Python por
sistema y el demo. El
rollback retira la verificación y sus pruebas sin cambiar el writer temporal,
el verificador de contexto ni el recovery legacy. U7 y fase 04 siguen
parciales; el progreso global permanece en 4/9.

La unidad A `cbb06d2bb40702bdf55d2a16f50d67220a0acc1e` extrae la comprobación
común de artefactos y la guardia de owner para reutilizarlas sin relajar la
prueba normal. La unidad B1 `b92ec27f6972ac239a0edb4b9bb0e6107a333f70` añade
una observación especial consumible del intent temporal presente. Después de
revisar el vínculo owner/lock/parent/scope y los seis nombres, reabre y verifica
el intent exacto; vuelve a cargar Journal y contexto, repite binding/replay/root
y exige Journal `Committed`, root `New`, backup y stage ausentes. El resultado
retiene el proof y el owner y es solo una observación puntual: no publica, no
renombra y no concede cleanup. La unidad B2
`85b7cb92f7bdb3251ccd7c16f465664b360d45f8` añade pruebas de mutación del intent
con mismo ID, truncamiento y byte extra, ausencia de Journal/contexto y contexto
válido del mismo ID y longitud con SHA distinta; las pruebas preservan los bytes
e identidades observados. El diagnóstico Windows `--nocapture` pasó 8/8 y
confirmó el rechazo efectivo de mutaciones, el reemplazo de intent y el rechazo
ante backup `Old` reaparecido en la fixture. Linux ejercitó esos caminos en el
filtro owner 86/86, que falla si las operaciones no se realizan. La cobertura
previa de parent movido sigue siendo
la única que ejercita Linux con decoys; Windows conserva su rama explícita
porque sharing impide el rename.

Las tres unidades añadieron 714 líneas de churn real: A, 103 (53 altas y 50
bajas); B1, 395 altas; B2, 216 altas. El filtro focal para cada unidad fue
`cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion`.
A pasó 72/72 en Windows y 77/77 en Linux; B1, 78/78 y 83/83; B2, 81/81 y
86/86. Formato, clippy de workspace con todos los targets y `-D warnings`,
frontera pública y diffcheck terminaron verdes en ambos sistemas; la limpieza
del harness Linux también terminó correctamente.
El CI exacto de la base publicada
`92986a21cbba38af2af8557f326c9aaf04fcce3d` (run `38885487075`) terminó SUCCESS:
Linux 948 pass/0 fail/9 ignored y Windows 931/0/9 en 38 resúmenes por sistema,
más 7 pruebas Python por sistema y el demo. Ese CI es anterior y excluye A+B.
El rollback de aquella ronda retiraba B2 antes de B1 y luego la extracción A,
preservando el writer/verificador de archivo y el recovery legacy. En esa unidad,
la observación completa especial revalidaba Journal, contexto, replay, root e
inventario con el intent temporal presente, pero el publicador aún estaba
pendiente. P1 y P2 implementan y verifican esa publicación privada, sin cleanup,
reconciliación ni caller productivo. U7 y fase 04 siguen parciales; el progreso
global permanece en 4/9.

## Publicación privada del intent (ed5, P1 y P2; commits locales)

| Unidad | SHA | churn | Filtro focal owner Windows/Linux |
|---|---|---:|---:|
| `ed5`, revalidación del mismo archivo | `ed5be1e1f62d615c391e1f66a8d167dfc1e14d68` | 36 (29+/7-) | 81/86 |
| P1, publicador privado | `ee6a1fdb6f88801c983a26dfeb27038427e57740` | 329 (328+/1-) | 84/89; contexto 24/25 |
| P2, pruebas de carrera | `790011750d4d45d5f8d910dae524b04e0c3132d5` | 348 altas | 88/93 |

P1 cubre publicación exitosa; en el caso canary, el nombre canónico se introduce
tras el gate completo y antes del syscall real `NOREPLACE`, que se invoca una vez,
rechaza el rename por ocupación y conserva ambos archivos; y un rename real seguido por error inyectado
cuyo resultado se confirma mediante postcondiciones. P2 confirma que si aparece
un canónico después de observar, la llamada al publicador revalida y retorna
antes del callback/syscall (cero llamadas al callback/syscall, no cero llamadas
al publicador). El error de syscall inyectado sin efecto de rename hace una sola
llamada al callback; es una inyección, no un fallo real del SO. Las mutaciones de
Journal, contexto o root invalidan antes del syscall. El move real del parent no
invalida: publica desde el parent retenido con contador 1 y decoys intactos. Linux
ejercita ese move con decoys; en Windows el sharing impide mover el parent y los
decoys no se ejercitan.

En las tres unidades pasaron formato, clippy de workspace con todos los targets
y `-D warnings`, chequeo de frontera y diffcheck en Windows y Linux; la limpieza
Linux también pasó. El CI publicado anterior, base
`af7d318417b0cdd35b6bdf7710be3d3a1288bf89`, run `38888277606`, terminó SUCCESS
con Linux 957/0/9 y Windows 940/0/9, 38 resúmenes por sistema, 7 pruebas Python
por sistema y demo. Ese CI no incluye `ed5`, P1 ni P2. El rollback local es P2,
luego P1 y luego `ed5`, preservando el lector de contexto, el writer del intent
y el recovery legacy. Al cerrar P2, el progreso global permanecía en 4/9; el
lector file-bound y el helper de inventario descritos en el bloque siguiente no
completan una fase. En ese punto seguían pendientes la reconciliación y el retiro
del Journal, del intent y del contexto, cleanup, caller productivo y recovery
completo; fase
04/U7 siguen parciales. No hay garantía de dirsync, pérdida de energía ni
cobertura general de montajes.

## Lector file-bound e inventario con Journal almacenado

La API pura `FELIX-RETIRE` de
`2bfbca587565c310b633904a013ec25ed251e821` expone internamente el decoder y
la validación del vínculo entre la identidad nativa medida y el DTO persistido,
además del scope; el Journal completo queda disponible solo como dato de
lectura. Tuvo 84 líneas de churn y pasó 10/10 pruebas focales en Windows y
Linux, además de formato, clippy de workspace con todos los targets y
`-D warnings`, frontera pública, diffcheck y limpieza del harness Linux.

El lector de `734871f998a05a3bea017d012eb7d2b5a7ed7451` tuvo 348 líneas de
churn y pasó el filtro owner 94/94 en Windows y 99/99 en Linux. El lector
decodifica el primer handle y luego retiene el handle de la reapertura final,
validado contra el mismo ID y scope. No vuelve a leer ni a comparar bytes en
ese segundo handle; la retención no inmoviliza el contenido ni el namespace.
Su prueba normal de replay sigue bloqueando ante un intent presente. La cobertura
adicional del reader `3be9ee7d9cca9fab324ba13c08a1a1f834ab3660`
tuvo 148 líneas de churn; el helper de inventario
`83e5533d776b33bf12c23c7bfdb3e33f1260bd2f` tuvo 167. El gate combinado pasó
owner 99/99 en Windows y 104/104 en Linux; formato, clippy workspace con todos
los targets y `-D warnings`, frontera y diffcheck terminaron con exit 0 en ambos
sistemas. El resumen de productos Linux y la limpieza del harness también
terminaron con exit 0.

El CI exacto de la base publicada `e579` (run `38903272871`) terminó SUCCESS:
Linux 964/0/9 y Windows 947/0/9 en 38 resúmenes Cargo por sistema, además de 7
pruebas Python y el demo por sistema. Es anterior y no cubre la API pura
`2bfbca587565c310b633904a013ec25ed251e821`, el lector
`734871f998a05a3bea017d012eb7d2b5a7ed7451` ni las unidades locales posteriores.
El estado global permanece en 4/9; U7 y fase 04 siguen parciales. No hay garantía de
dirsync, pérdida de energía, cobertura general de montajes o recovery completo.

## Comparadores, revalidación y observación coordinada A

La comparación de identidad nativa y scope del propio intent ya estaba en la
API pura de `2bfbca587565c310b633904a013ec25ed251e821`. El commit local
`ca0b726bab54c99169903b6a1520ddc95bbc64ba`, con 182 líneas de churn, añade los
comparadores del Journal (ID nativo y registro semántico completo) y del Context
(ID nativo, longitud y SHA-256). Solo aceptan datos observados por el caller: no
abren archivos, reconstruyen identidades nativas ni convierten el Journal
almacenado en permiso. El filtro focal pasó 17/17 en Windows y Linux.

El commit local `1632fef5ae7df5d1a7f3f75b6292de5b527da92e` tuvo 157 líneas de
churn y añade `ObservacionIntentCanonico::revalidar(self)`. Consume una
observación, comprueba owner/Arc, parent y scope antes de volver a leer el
nombre canónico, mantiene el `File` original durante la lectura nueva y solo
devuelve la observación fresca si coinciden ID, registro completo, parent,
scope y Arc. Compara datos secuenciales; no congela contenido ni namespace.
El filtro owner pasó 102/102 en Windows y 107/107 en Linux.

La unidad coordinada A es el commit local
`04edb1095cfa499502a9f511407b3d497110d4ca` (370 líneas de churn: 132 de
lector, 235 de pruebas y 3 de wiring). La cobertura adicional es
`5041550131e4f1399cf1e7f6a54b4fdeb9f12f6e` (252 líneas: 250 de pruebas y 2 de
wiring). Esta evidencia se obtuvo localmente antes de publicar el lote.

La unidad A integrada en el árbol local compone esa lectura con el Journal y el
Context canónicos. Vuelve a validar Intent y los seis nombres sibling,
comprueba el ID y el Journal semántico completo, vincula el Context por ID,
longitud y SHA-256, ejecuta el replay soportado y compara la huella completa de
`New` con el Journal. El inventario debe observar root `New`, backup/stage
`Absent` y conservar `root_new_validated=false`. El resultado privado retiene
la observación final de Intent y los datos de Journal, Context, huella e
inventario; no es clonable, redacciona `Debug` y no autoriza cleanup, apply ni
una decisión de recovery.

La observación independiente del Intent puede emitirse con Journal o Context
ausente, pero A exige ambos canónicos. Sus pruebas integradas hacen que A falle
cerrado en esos estados sin recrearlos; también cubren Journal reemplazado por
bytes iguales con ID distinto, Context de mismo ID y longitud con SHA distinta,
Context válido con binding actualizado, root `Unknown`, stage `Unknown`, backup
`Old`, tags de replay no soportados y el rechazo de la prueba normal mientras el
Intent está presente. El filtro owner de A pasó 111/111 en Windows y 116/116
en Linux. Formato, clippy de workspace con todos los targets y `-D warnings`,
frontera pública y diffcheck pasaron en ambos sistemas; la limpieza del harness
Linux terminó con exit 0. El detalle de commits y churn está registrado arriba.

La ausencia de Context no demuestra que un Journal v1 corresponda a una
transacción legacy ni que el sidecar sea huérfano: v1 no distingue si la
admisión requería Context, y el runner legacy puede retirar Journal y dejar
Context. A no intenta inferir ese historial. Un dispatch durable necesitará una
versión nueva o un discriminador explícito ligado a la identidad; hasta entonces
no hay caller productivo de la observación.

Las primeras ejecuciones de clippy en Windows y Linux encontraron el mismo
import sin uso en una prueba de API; se quitó sin cambiar conducta. Los filtros
17/17 de API y owner 102/102 Windows, 107/107 Linux de revalidación pasaron
sobre sus freezes y no se repitieron por ese ajuste de import. Los filtros de A pasaron
111/111 en Windows y 116/116 en Linux. Formato, clippy de workspace con todos
los targets y `-D warnings`, frontera pública y diffcheck quedaron verdes en
ambos sistemas; limpieza del harness Linux terminó con exit 0. El CI exacto
de la base publicada `2c54c7a15071958a51679075d7a3de8d1cb7c519` (run
`38907971410`) terminó SUCCESS con Linux 977/0/9 y Windows 960/0/9 en 38
resúmenes Cargo por sistema, además de 7 pruebas Python y el demo por sistema.
Ese CI precede y no cubre los commits `ca0b` y `1632`, ni A y su cobertura.
El rollback previsto para el lote sigue dependencias: cobertura extra, unidad A,
revalidación, comparadores. El progreso global permanece en 4/9; fase 04 y U7
siguen parciales. No se declara snapshot frente a escritores externos, dirsync,
tolerancia a pérdida de energía, cobertura general de montajes ni recovery
completo.

## Observaciones locales B y C

Las unidades B core (`10c34f80e4e5a4d30a948d35f35e02a9e5d64c7e`, 232 líneas de
churn), B cobertura (`41d74b4b74a4be4614e0cfdfd8f995975245c3c7`, 223 líneas)
y C (`c3da8173cbd26c17af81fb5909f90735d6c046fb`, 196 líneas) forman commits
separados sobre la base publicada `37d29a1e4a8382584b884d127c889228ce139b80`.
El root y la revisión independiente aprobaron los freezes. Los gates
combinados pasaron: owner 119/119 en Windows y 124/124 en Linux; formato,
clippy de workspace con todos los targets y `-D warnings`, frontera pública y
diffcheck terminaron con exit 0 en ambos sistemas, y la limpieza Linux terminó
con exit 0. Esta evidencia se obtuvo localmente antes de publicar el lote.

B observa el estado cerrado con intent y Context canónicos presentes, Journal
canónico ausente y los tres temporales ausentes. Toma el Journal `Committed`
completo del intent solo como dato, exige que su parent coincida con el parent
retenido, compara Context por ID nativo, longitud y SHA-256 observados en su
handle, valida el replay soportado y exige que `New` coincida con el Journal.
El inventario clasifica root `New`, backup/stage `Absent` y conserva
`root_new_validated=false`. B no observa un ID canónico de Journal ni demuestra
por qué falta ese archivo. Su resultado privado distinto conserva la observación
del intent y los datos validados; no es una prueba normal, decisión o permiso.

C observa intent presente con Journal y Context canónicos ausentes y los tres
temporales ausentes. A y B rechazan cuando faltan ambos sidecars; C devuelve una
observación privada distinta que retiene solo el intent. No lee payload de
Journal o Context, no conserva identidades observadas de esos sidecars, no hace
replay ni observa el root/inventario. El observer del namespace sí puede abrir
una entrada presente y medir su tipo, scope e identidad para clasificarla antes
de rechazar el gate; esa medición no se retiene como evidencia en C. La ausencia
de cualquiera de los sidecars no demuestra su causa ni autoriza cleanup.

Las pruebas incorporadas cubren B con Journal ausente/presente, Context con
mismo ID y longitud pero SHA distinta, root con árbol igual e identidad nueva,
backup `Old` y stage `Unknown`; C cubre ambos sidecars ausentes y el rechazo si
solo Journal o solo Context está presente. En los rechazos se comprueba la
preservación de los artefactos. El CI exacto de la base publicada
`37d29a1e4a8382584b884d127c889228ce139b80` (run
`38912187461`) terminó SUCCESS: Linux 996/0/9 y Windows 979/0/9, con 38
resúmenes Cargo, 7 pruebas Python y el demo por sistema. Ese CI precede y no
cubre B ni C; los gates descritos arriba son locales y no sustituyen un CI del
lote publicado.

La prueba normal sigue exigiendo que el intent canónico y su temporal estén
ausentes. B y C solo observan estados cerrados bajo el owner y el parent
retenido; sus comprobaciones son secuenciales y no forman un snapshot ante
escritores externos. No retiran sidecars o intent, no seleccionan una fila ni
invocan recovery, cleanup, dispatch o apply. El rollback seguirá dependencias:
primero C, luego la cobertura extra de B y finalmente B core, preservando A,
los comparadores, el lector y el flujo legacy. El progreso global permanece en
4/9; fase 04 y U7 siguen parciales. No se declaran garantías de dirsync,
pérdida de energía, mounts generales ni recovery completo.

## Reapertura interproceso y consistencia observacional C

El commit `602700b` añade un harness de reapertura normal en otro proceso (296
líneas de churn). El padre prepara fixtures sintéticas para A, B y C, suelta
owner y lock antes de lanzar un hijo y exige PID distinto. Cada hijo readquiere
lock y owner, observa el estado asignado y emite su marcador; el padre comprueba
status, que se ejecutó un test, el marcador esperado y la preservación de bytes,
IDs, ausencias y root. El filtro focal pasó 1/1 en Windows y Linux. Es
reapertura normal entre procesos, no terminación abrupta, reboot, power loss ni
recuperación durable.

La coordinación posterior de los tests está en
`b3aba292f246927ebe68f0847b9ff3279cd5f6c7` (3 archivos, 16 altas y 3 bajas;
19 líneas de churn). `preparar_con_tags` adquiere el mutex test-only antes de
abrir el lock del fixture y conserva su guard hasta que se descarta
`FixturePublicada`. Dos tests de contexto y el test de identidad de restart que
readquieren directamente el lock conservan una guard local durante ese ciclo.
No se cambia el lock de producción. El fallo previo del filtro owner de Linux
`writer_lock_busy` al readquirir en
`revalidar_tests::mismo_frame_con_id_canonico_nuevo_falla_y_preserva_ambos_archivos`
(128 pasaron, 1 falló) es compatible con la hipótesis de interferencia durante
una reapertura; la causa no quedó demostrada. El pase completo posterior
tampoco demuestra esa causa.

La extensión C es un tipo separado que consume y revalida la observación C base.
Con Journal y Context ausentes y los tres temporales ausentes, toma el Journal
completo almacenado en el intent solo como descriptor de datos y usa el helper de
inventario para comparar la huella nativa completa del root con
`journal.new_fingerprint()`. Exige root `New`, backup y stage `Absent`, conserva
`root_new_validated=false` y vuelve a validar C antes de devolver el dato. No lee
payload de Context, no ejecuta replay ni infiere quién retiró los sidecars. No
retira ni recrea archivos y no ofrece permiso de cleanup o apply. Sus fixtures
cubren estado base, root recreado con el mismo árbol y otro ID, backup `Old` y
stage `Unknown`, con preservación en los rechazos. El commit
`6c02f721484ab484a6d76cfed33a647d708e8f83` tiene 3 paths y 332 líneas de churn
(330 altas y 2 bajas).

Para revertir este lote, retirar primero la extensión C de `6c02f72` (método,
tipo, tests y wiring) y después la reapertura interproceso de `602700b` (test y
wiring), conservando los observers A/B/C base, los comparadores y el lector.
La coordinación de tests `b3aba29` es una corrección independiente: también
protege fixtures frente a forks ya existentes y no depende de C ni del harness
nuevo. Si luego se revierten los observers base, retirar primero el harness
nuevo; para el orden restante, seguir el registro histórico B/C anterior sin
reescribirlo.

La primera compilación del filtro C en Windows encontró `E0425` por una
referencia a `root` sin binding; se corrigió para usar
`fixture.publicada.root` antes de la ejecución siguiente. La suite completa de
la biblioteca de `universo-okf`, ejecutada con
`cargo test --locked --offline -p universo-okf --lib` y paralelismo
predeterminado, terminó Windows 632/0/5 (637 tests) y Linux 652/0/5
(657 tests). Formato, clippy del workspace con todos los
targets y `-D warnings`, frontera pública y diffcheck terminaron con exit 0 en
ambos sistemas; el cleanup y la salida del runner Linux también fueron 0. Esta
evidencia se obtuvo localmente antes de publicar el lote.

El CI exacto de la base publicada `a69d6c877a297053991eb0f0ee06806808f7221a`
(run `38914774277`) terminó SUCCESS: Linux 1004/0/9 y Windows 987/0/9, con 38
resúmenes Cargo, 7 pruebas Python, demos, 4 estados y 16 salidas por sistema.
Esa base precede y no cubre `602700b`, el harness de coordinación ni la
extensión C; los gates locales corresponden a los freezes indicados arriba y
no sustituyen un CI del lote publicado. El estado global permanece en 4/9;
fase 04 y U7 siguen parciales. No se afirman snapshot externo, dirsync, power
loss, cobertura general de mounts ni recovery completo.

## Lease estricto del Journal y secuencia privada A → Journal → B

U1 (`1d5302b70de0f57db18baad1346fa0c5e9aba639`, 259 líneas de churn) añade
la emisión del lease estricto del Journal y su lectura con scope. Los
constructores legacy mantienen su comportamiento y no consultan el probe. El
camino estricto valida ID y scope del parent, y la lectura comprueba el scope
del archivo en el mismo handle abierto.

U2 (`dee43d028ec3cb42a5a406885733141ba000d645`, 324 líneas) extiende el helper
de borrado existente: valida el scope de la fuente en el handle final que
comprueba su identidad y el Journal, y confirma con una lectura estricta. Añade
el seam por llamada y pruebas de éxito, rechazo y preservación. Los scopes
negativos son datos sintéticos; no requieren ni representan mounts montados.

Los gates de U1/U2 pasaron la suite completa de la biblioteca con
`cargo test --locked --offline -p universo-okf --lib`: Windows 640 passed, 0
failed, 5 ignored; Linux 660 passed, 0 failed, 5 ignored. Formato, clippy de
workspace con todos los targets y `-D warnings`, frontera pública y diffcheck
terminaron con exit 0 en ambos sistemas; producto y cleanup Linux también
terminaron con exit 0. El wrapper exterior Linux reportó exit 2 por el CR final
de `exit 0\r`; un smoke separado con LF terminó 0 y no cambia el resultado de
esa invocación anterior. Estos resultados corresponden al freeze U1/U2 y no se
atribuyen al consumidor A añadido después.

El consumidor privado A→Journal→B usa la observación A completa recién
revalidada, conserva owner, parent y scope, y pasa al lease estricto solo la
identidad nativa medida y el Journal completo. Intenta la eliminación una vez.
Ante `Eliminado` o `Ausente`, vuelve a observar B y solo devuelve B si supera su
propio gate; ante `Conflicto` o `Indeterminado`, retorna `NoVerificable` sin
reintentar ni tratar el error como prueba de que no hubo efecto. No retira
Context ni Intent, no aplica una fila de recovery y todavía no tiene caller
productivo. La comprobación es secuencial: no afirma snapshot frente a escritores
externos, durabilidad ante power loss ni recuperación completa.

El consumidor A quedó en el commit `05daf482006570fa01e51a2dae2e31c4fb809845`,
con 317 líneas de churn: 74 de fuente, 240 de pruebas y 3 de wiring. El filtro
owner pasó Windows 129/0/0 y Linux 134/0/0. Formato, clippy workspace,
frontera pública y diffcheck de archivos rastreados y nuevos terminaron con
exit 0 en ambos sistemas; producto, cleanup, runner reutilizable y wrapper Linux
terminaron con exit 0 usando archivos UTF-8 sin BOM y LF. Los logs son
`strict-a-remove-20261016-windows.log` y
`strict-a-remove-20261016-linux.log`. Esta evidencia local corresponde al commit
indicado; el resultado previo de U1/U2 con wrapper exit 2 por CR final permanece
separado y no se reinterpreta con el wrapper limpio de A.

El CI exacto de la base publicada
`f5845df6962d1e8e7a902d1975daab6a6a92e85b` (run `38920132266`) terminó SUCCESS:
Linux 1009/0/9 y Windows 992/0/9, con 38 resúmenes Cargo, 7 pruebas Python,
demos, 4 estados y 16 salidas por sistema. Esa base precede y excluye U1, U2 y
A. El rollback del lote nuevo sigue las dependencias: retirar primero A,
después U2 y por último U1; los constructores legacy y el recovery existente
se conservan. El progreso global permanece en 4/9; fase 04 y U7 siguen
parciales.

### Lector versionado del Journal (J0/J1)

J0 (`6af0fb90`) deriva la cuota común del selector como el máximo de los límites
propios de Journal v1 y v2. Sus siete pruebas focales y controles locales
pasaron en Windows y Linux; el CI `27fc2ff` precede a este commit.

J1A (`ff1d08ca`) añade lectura tipada del Journal canónico mediante el lease
estricto y el mismo parent retenido. Exige scope verificado, archivo regular y
no-follow; mide metadata antes de reservar, vuelve a `seek(0)` sobre el mismo
handle, limita la lectura con centinela, usa el selector existente y valida el
parent del registro. Reabre el nombre visible y exige identidad y scope iguales
antes de devolver la observación. `None` representa solo ausencia confirmada dos
veces. El resultado conserva el `File`, su identidad y el Journal completo;
mantener el handle abierto no congela sus bytes ni el nombre del namespace, y
las comprobaciones no forman un snapshot frente a escritores externos. El filtro
`journal_temporal` más los seis casos nuevos pasó 73/0/0 en Windows y 74/0/0
en Linux; formato, clippy, frontera y diff terminaron en 0 en ambos sistemas.


J1B añade el wrapper owner-bound: libera el lease, vuelve a validar parent,
scope y lock visible, y solo entonces expone el resultado file-bound. Un fallo
del postcheck descarta tanto Some como None. Sus cinco tests incluyen tres
casos de filesystem real y dos inyecciones sintéticas que ejercitan la misma
rama de descarte sin afirmar que se sustituyó el lock. Windows puede bloquear
el rename del lock abierto, por lo que esos dos tests sintéticos no sustituyen
la evidencia de una sustitución real. El primer gate local de Windows, sobre el
fixture anterior, registró 176 pasadas y 2 fallidas: ambas fallaron en el setup
al leer el lock abierto (OS error 33), antes del rename; no fue un fallo del
postcheck productivo. El fixture corregido siembra el canario antes de adquirir
el guard y libera owner y guard antes de comprobar los bytes. Los gates locales
corregidos cerraron Windows 178/0/0 y Linux 183/0/0. El commit
1d9d8a64623957cb1825bdf43f5c0b09bad90a07 contiene 254 líneas de churn; todavía
no tiene CI propio.

J1C1 (1d80f37c563c1f3c7594e45012e83ea4d08e2b5a, 113 líneas de churn) añade dos
casos deterministas: un reemplazo del nombre con bytes iguales e identidad
distinta se rechaza y conserva ambos objetos; el crecimiento tras metadata se
detecta con el centinela TooLarge y conserva los bytes nuevos. El filtro dejó
ocho pruebas del lector: Windows 75/0/0 y Linux 76/0/0, con controles locales
en 0. Este resultado no demuestra una frontera general frente a escritores
externos.

J1C2 (`c706721b5cf7268ae24fda4922754adefae487e7`) amplía el probe de scope
solo para tests y por llamada. Cuatro pruebas ejercitan mismatch y error
inyectado en parent inicial, source inicial, sonda visible final y parent tras
ausencia; verifican preservación de los archivos o de la ausencia. El lector
suma doce funciones de test. Sus gates locales cerraron con Windows 79/0/0 y
Linux 80/0/0; las doce pruebas lectoras y los controles de formato, clippy,
frontera y diff semántico pasaron. J1C2 no modifica la entrada productiva ni
omite la validación de scope existente.

J1C3 (`ccbed6c023e03abda0641319d246b46d0b08641c`, 116 líneas de churn) cerró
sus gates locales. El filtro lector pasó 82/0/0 en Windows, con 15 pruebas del
lector, y 84/0/0 en Linux, con 16. Formato, clippy con `-D warnings`, frontera
pública y diff semántico terminaron en 0; cleanup Linux también terminó en 0.
Añade tres pruebas comunes y una Unix: schema desconocido, malformación v1 sin
fallback, directorio en el nombre canónico y symlink. Windows no reclama
cobertura de symlink. Las pruebas sintéticas no equivalen a una sustitución
real del lock ni a una frontera general frente a escritores externos.

Este apartado cubre solo el reader del Journal. El lector de Context y el estado
del reader de Intent se describen en las secciones siguientes. El preflight de
familia persistida y el caller de apply/recovery siguen pendientes.

### Lector versionado de Context (Ctx1–Ctx4)

Ctx1 añadió un recolector incremental acotado con centinela `cap+1`, aritmética
comprobada, reserva fallible, reintento ante `Interrupted`, detección de EOF y
SHA-256 de los bytes reunidos. No preasigna la cuota de 512 MiB ni afirma una
cota de RSS. Ctx2 deriva la cuota común de las cuotas v1/v2 y añade comparadores
puros del parent. Ctx3A y Ctx3B leen el archivo presente bajo lease estricto:
validan owner, parent y scope; exigen archivo regular/no-follow, miden metadata
antes de leer, vuelven a `seek(0)` en el mismo handle y aplican selector y cap.
La sonda final verifica identidad y scope del nombre visible. La ausencia solo
se devuelve tras dos sondas exactas, con comprobación del parent entre ellas.

Ctx4 añade el wrapper owner-bound: adquiere el lease, obtiene el resultado de
lectura y suelta el lease antes de revalidar parent, scope y lock visible. Un
fallo de owner posterior descarta cualquier resultado de lectura `Some`, `None`
o `Err`; un fallo al adquirir el lease puede cerrar antes de la lectura. La
observación conserva el `File` original como dato privado, sin `Clone` ni getter
de archivo. No es snapshot de bytes/nombre ni autoridad. Sus siete pruebas están
en el árbol activo. Sus gates focales cerraron con `7/0/0` en Windows y Linux;
formato, clippy, frontera y diff terminaron en `0`, los artefactos fueron
frescos y cleanup Linux terminó en `0`. El recibo
`C7A1E470CAE76CD842C73A93F10237F07E8C90E7B50433E5F175C6D5B6D33192` registra
esos resultados. El commit local es `190d04628f69fd59c564af79eabf3e0142de322e` (312 líneas de churn).

| Corte | Filtro local Windows | Filtro local Linux | Estado |
|---|---:|---:|---|
| Ctx1, `8fa02f4` | `7/0/0` | `7/0/0` | Gates locales cerrados |
| Ctx2, `90af67b` | `8/0/0` | `8/0/0` | Gates locales cerrados |
| Ctx3A, `ef2e693` | `3/0/0` | `3/0/0` | Gates locales cerrados |
| Ctx3A2, `07fb3e0` | `2/0/0` | `2/0/0` | Gates locales cerrados |
| Ctx3B, `8bb5270` | `5/0/0` | `5/0/0` | Gates locales cerrados |
| Ctx4, `190d046` | `7/0/0` | `7/0/0` | Gates locales cerrados; 312 líneas de churn |

Los resultados de Ctx1–Ctx4 son gates focales locales. El lote Context
publicado pasó el CI `39004162338`, registrado arriba; este CI precede a
I1A/B1/B2.

Ctx5 aún debe ampliar la matriz de errores de scope. El lector v1 de Context
conserva sus rutas y semántica. I1A/B1/B2 no añaden el gate de familia
persistida, replay owner-bound v2 ni caller productivo de apply o recovery. El
progreso global permanece en 4/9, con U7 y fase 04 parciales.

### Lector versionado de Intent

I1A (`68d55bf56ec563e2133778751bfc7ff13ed83b94`) añade un recolector privado
para un `File` ya abierto. Valida metadata, tipo regular, scope e identidad del
mismo handle, vuelve a `seek(0)` y reúne bytes acotados con centinela, reserva
fallible. Sus seis pruebas y controles locales cerraron en Windows y
Linux. B1 (`a391741405212e5f4322951581242bcb701d954d`) aplica el selector
estricto v1/v2 y la cuota familiar derivada, y devuelve un candidato de datos
versionados sin vínculo. Conserva solo el `File` y el enum versionado en campos
privados, sin `Clone` ni getter del archivo. El filtro acumulado pasó `9/0/0`
(tres casos añadidos sobre I1A) y sus controles locales cerraron en ambos
sistemas. El frame válido de longitud exacta se acepta y el lector limita la
cuota solicitada a la cuota familiar.

B2 añade al candidato la identidad nativa y el scope capturados por I1A y
compara, en ambas variantes, ese ID con el DTO persistido, el scope del archivo
con el persistido y el parent del Journal. La conversión va de identidad
nativa a DTO; no reconstruye identidad nativa desde bytes. El resultado sigue
siendo dato privado, no una prueba de owner,
autorización o snapshot. El filtro acumulado pasó `12/0/0` (tres casos añadidos
sobre B1), con doce funciones nuevas de prueba en el lector en total. Formato,
clippy, frontera y diff terminaron en `0` en ambos sistemas; los artefactos
fueron frescos y cleanup Linux se reportó `PASS`. El commit local es
`08ddb0b8d671b9a6d468ddc94bd756b0750c3b1c` (286 líneas de churn); el recibo
postcommit es `3AF525CCA621DBB401BC436ADF5902BB4C361C7EC517FEEADB5E25EC1E2CB6CE`.
I2C (`7770ff593fbb6c1be4e35e01da6dd5bbc5843efa`, 393 líneas de churn) añade la
lectura presente owner-bound: valida parent/scope/lock y temporales antes y
después, lee el mismo `File` y aplica un postcheck común tras cerrar el préstamo.
Un fallo de postcheck prevalece sobre un resultado presente o un error de
lectura. Sus tres pruebas owner y controles locales cerraron `3/0/0` en Windows
y Linux; formato, clippy, frontera y diff semántico terminaron en `0`. El CI
`be97f03` precede a I2C. I3 (`14ff62f2a73c38ca28c99617962c65cc0a898615`,
209 líneas de churn) añade la doble sonda de ausencia exacta: la segunda
apertura debe confirmar ausencia y las comprobaciones de owner, scope y los tres
temporales se repiten entre sondas y tras la lectura. Reaparición, I/O o tipo
no admitido devuelven error; el postcheck final domina también una ausencia
candidata.

| Corte | Filtro local Windows | Filtro local Linux | Controles |
|---|---:|---:|---|
| I2C, `7770ff5` | `3/0/0` | `3/0/0` | formato, clippy, frontera y diff `0` |
| I3, `14ff62f` | `6/0/0` | `6/0/0` | formato, clippy, frontera y diff `0`; cleanup Linux `0` |

El recibo de I3 es `68C172468A3B42FC521AF4EEF6A6F146BE2D4C6F6C5772622812C90A3F456D4F`.
Estos son gates locales; el CI `be97f03` es el último CI publicado citado aquí
y precede a I2C/I3. El gate de familia por fases, replay owner-bound v2, la
matriz general de scope/tipo (I4) y cualquier caller de recovery/apply siguen
pendientes. El estado global permanece en 4/9; U7 y fase 04 continúan parciales.

Los registros siguientes son históricos de rondas anteriores. Se conservan
sin borrar sus resultados; sus conteos no reemplazan la verificación actual ni
identifican por sí solos la causa de los fallos observados entonces.

## Verificación raw y retiro estricto de Context

U1 (`21716d1d7161ab894bd0e664f4c4caff701b6576`, 249 líneas de churn)
añade la verificación raw del Context canónico bajo el owner retenido. Revalida
parent, lock y scope antes y después; abre el archivo no-follow y regular, y
contrasta en el mismo objeto la identidad, longitud y SHA-256 con la
EvidenciaLecturaContexto recibida, comprueba EOF y respeta la cuota compartida
de 512 MiB. Devuelve el File verificado, no reconstruye un DTO ni concede
autoridad. Su filtro focal
pasó Windows 28/0/0 y Linux 29/0/0. Formato, clippy workspace, frontera y
diffcheck terminaron con exit 0 en ambos sistemas; cleanup Linux terminó 0.
Los primeros lanzamientos fallaron en setup y stderr de PowerShell; las
corridas corregidas, registradas como `context-u1-*-corrected` y capturas de
sesión, son las acreditadas.

U2a (`bc1dcdab3f217fe7c13dba282092988b4d4d77c4`, 167 líneas) distingue una
ausencia real del error de lectura o discrepancia: solo NotFound/EntradaAusente
produce None; el wrapper estricto lo mapea a CanonicalUnverified y la lectura
temporal a TemporaryUnverified. Los lectores legacy conservan su semántica.
El filtro pasó Windows 30/0/0 y Linux 31/0/0; formato, clippy workspace,
frontera, diffcheck y cleanup Linux terminaron con exit 0.

U2b (`6858014ec2d3dfd1d083ef02b2f87c4c198843e7`, 368 líneas) añade una
operación privada para retirar Context. Reusa la lectura raw U1 y la
distinción de ausencia U2a. Revalida owner, parent, lock y scope antes y después
de una sola llamada. Comprueba el scope en el handle final usado para el intento
y confirma con una lectura strict fresca. En Windows valida con DELETE ese mismo
handle final; en Unix queda una carrera residual entre la validación y unlinkat.
Si el owner no puede verificarse al confirmar, el resultado queda indeterminado;
las discrepancias observadas son conflictos. No hay retry ni se infiere ausencia
desde un error genérico.
El filtro pasó Windows 33/0/0 y Linux 35/0/0; formato, clippy workspace,
frontera, diffcheck y cleanup Linux terminaron con exit 0.

U2c (`ad5b2f8c170c1a748d58af1a97ada7a8f495adc0`, 251 líneas) amplía solo las
pruebas de resultados del intento: ausencia previa, callback con error o éxito
sin retirar, ausencia tras error, contenido cambiado con el mismo ID, reemplazo
con el mismo frame y otro ID, y pérdida de verificación del lock. Los casos
inyectados prueban la clasificación del callback, no los syscalls productivos.
Tras invalidar las caches de paquete, la suite focal nueva pasó Windows 39/0/0
(6 casos aplicables) y Linux 42/0/0 (7 casos); se verificaron los nombres de
test y el dep-info del módulo. Clippy workspace con todos los targets y
`-D warnings` terminó 0 en ambos sistemas; producto, cleanup y runner Linux
terminaron 0. Formato, frontera y diffcheck ya habían quedado en 0 en el pase
inicial; no se repitieron. Los primeros conteos 33/35 usaron un binario
reutilizado sin los nombres nuevos y no se acreditan como prueba de U2c.

El CI exacto de la base publicada `e4fe35d6ee687ad57ccbd12b20f2cf6003a3eac5`
(run `38924787482`) terminó SUCCESS: Linux 1022/0/9 y Windows 1005/0/9, con
38 resúmenes Cargo, 7 pruebas Python y demos por sistema. Precede y excluye
U1, U2a, U2b, U2c y el puente. Los gates locales descritos para U1, U2a, U2b
y U2c se completaron antes de publicar este lote. El rollback del lote
integrado retira primero el puente, luego U2c, U2b, U2a y U1. Los leases y
lectores legacy conservan su comportamiento; la compatibilidad v1 no cambia.

El puente privado B → Context → C está en el commit local
`4918a2016e58a4ad24ad477caf10323d4e5d6c6f` (333 líneas: 71 de lector, 3 de
wiring y 259 de pruebas). Obtiene una B fresca, usa su evidencia Context y hace
un único intento con el lease strict; solo `Eliminado` o `Ausente` permite
observar C. C base comprueba el namespace, no el root. La prueba que muta el
root después de B confirma que la extensión separada de consistencia root
rechaza esa mutación, aunque C base pueda observarse. El puente retiene el
intent, no reintenta y no interpreta errores como ausencia de efecto.
El filtro owner pasó Windows 134/0/0 y Linux 139/0/0; se verificaron cinco
nombres nuevos y el dep-info del módulo. Formato, clippy workspace con todos
los targets y `-D warnings`, frontera y diffcheck terminaron con exit 0 en
ambos sistemas. Producto, cleanup y launcher Linux terminaron con exit 0.
Las corridas usaron runtime Rust 1.88 con `locked` y `offline`. No completa
recovery, no limpia el intent y no tiene caller productivo. Las comprobaciones
no forman un snapshot frente a escritores externos. El estado global permanece
en 4/9, con fase 04 y U7 parciales; no se afirma dirsync, power loss, cobertura
general de mounts ni recovery completo.

### Evidencia histórica de avances anteriores

- Sobre la base `5935608` más el árbol de trabajo, el filtro
  `cargo test --locked --offline -p universo-okf --lib admision::envelope`
  terminó 6/6. Los mismos bytes de esta unidad quedaron registrados en
  `74f7afa` (envelope, 345 líneas) y el seam de pruebas en `b37635b`
  (237 líneas). La prueba y los commits corresponden a los mismos bytes; en
  esa ronda no había CI remoto asociado.

- El filtro focal de diagnóstico Windows registró 1 passed/1 failed. La
  suite de recovery también volvió a ejecutarse en aquella ronda; su
  resultado está separado abajo. `share_delete_denial_records_source_open_and_success_clears_the_previous_diagnostic`
  reprodujo un bloqueo real al abrir la fuente (`AperturaFuente`,
  `raw_os_error=32`, `ERROR_SHARING_VIOLATION`). Tras cerrar el handle que lo
  bloqueaba, el retry alcanzó `NtSetInformationFile` y falló con
  `NTSTATUS=0xC0000008` (`STATUS_INVALID_HANDLE`) e
  `IO_STATUS_BLOCK.Status=0`; esa prueba falló en la expectativa de retry. La
  prueba de diagnóstico sintético del mismo filtro pasó. Esto no identifica la
  causa de las fallas restantes de recovery ni permite atribuirlas al sandbox
  o a una regresión.

- Los 10 commits locales de código/pruebas desde la base esperada `116a0d8`
  son `5d04927` (materialización raw), `bfe13b7` (contexto), `8803966`
  (replay), `dcdbd83` (identidad), `fb45b09` (encoder), `d91f946` (cursor),
  `e6b1130` (`NewEsperado`), `bc38412` (preflight lector), `ef79ea7`
  (pruebas de robustez del lector) y `3e9b5cd` (decoder). Los filtros de las
  unidades anteriores reportaron materialización raw 2/2, admisión 6/6, replay 4/4,
  identidad 1/1, encoder 2/2, cursor 3/3, `NewEsperado` 4/4, preflight lector
  2/2, codec 7/7 y admisión 11/11; para `ef79ea7`, lector 7/7 y admisión
  16/16. La revisión pasó. La prueba de identidad solo suelta y readquiere
  `LockEscritura` en el mismo proceso; no usa proceso hijo ni demuestra reboot
  o recuperación tras caída del SO. Es distinta de los tres cortes
  interproceso documentados para `b990c72`, `b5e4da8` y `26eb9c4`. La fixture
  de admisión se corrigió para declarar `sorts` y usar sintaxis de átomos
  aceptada por el parser.
- La unidad decoder pasó revisión principal y secundaria. Sus filtros focales
  nuevos terminaron 31/31: admisión 20/20, replay 8/8, materialización raw
  2/2 e identidad 1/1; este último solo suelta y readquiere el lock en el mismo
  proceso, no simula reboot ni caída del SO. El filtro `decodificar` se volvió
  a ejecutar tras aclarar un comentario de cabecera sin cambios semánticos y
  pasó 4/4; formato y diff check también terminaron con exit 0. El total 31/31
  cubre esos cuatro filtros focales, no la suite completa de recovery.
- Con decoder, formato, clippy offline del workspace con todos los targets y
  `-D warnings`, la comprobación de frontera pública y la compilación cruzada
  Linux terminaron con exit 0. El cross-check compila, pero no ejecuta runtime
  Linux.
- `cargo check -p universo-okf --all-targets --target
  x86_64-unknown-linux-gnu` terminó con exit 0 con el preflight lector
  y el decoder registrados; es compilación cruzada, no ejecuta tests ni runtime.

### Historial de ejecuciones de la suite de recovery

- En una ejecución previa, la suite completa se ejecutó con
  `cargo test --locked --offline -p universo-okf --lib transaccion::journal::recovery::`.
  Con `TEMP` predeterminado dio 46 passed, 31 failed y 3 ignored; el resultado
  fue fallido, pero no se conservó su código de salida. Al redirigir
  `TEMP`/`TMP` al directorio de verificación permitido en el workspace dio 61
  passed, 17 failed y 3 ignored; el resultado fue fallido y la herramienta
  reportó exit 1, pero no se conservó el código bruto de Cargo. Quedaron 16
  `anchored_rename_io` en pruebas de rename y un `PermissionDenied` explícito
  al preparar una junction. Con la redirección se observó una reducción de
  fallos reportados, pero no se aisló la causa y la suite no quedó verde. En aquella ejecución no se conservó
  NTSTATUS crudo de los renames y su causa raíz no quedó identificada; no se
  atribuye al sandbox ni se afirma regresión.

### Ensayos locales históricos previos al CI exacto

Los resultados siguientes son locales de una ronda anterior, sobre la base
`5935608` y los mismos bytes de código finalmente registrados en `74f7afa`
(envelope) y `b37635b` (seam). Estos conteos quedan preservados como historia;
no son el estado actual del candidato remoto `aa814e3` y no hay inferencia de
causa a partir de su diferencia con la verificación actual.

- `cargo test --locked --offline -p universo-okf windows_diagnostics_tests -- --nocapture`:
  1 passed/1 failed. El fallo observado es el retry de rename tras la
  denegación real de apertura descrita arriba (`AperturaFuente`, error 32),
  seguido por `STATUS_INVALID_HANDLE` en `NtSetInformationFile` con
  `IO_STATUS_BLOCK.Status=0`; la prueba sintética pasó.
- `cargo test --locked --offline -p universo-okf transaccion::anclado::renombrar -- --skip windows_diagnostics_tests`:
  12 passed/6 failed. En esos fallos se observaron cuatro
  `STATUS_INVALID_HANDLE` y dos `PermissionDenied` al preparar junctions. No
  se aisló la causa de todos los fallos de rename ni se atribuye al sandbox o
  a una regresión; este filtro no explica por sí solo los 17 fallos de
  recovery.
- Un probe de código temporal, fuera de la suite y no reproducible/publicado,
  observó `STATUS_INVALID_HANDLE` con handles consultables y duplicados,
  y `RootDirectory` apuntando al parent; con `RootDirectory=NULL` y basename
  tuvo éxito. Es solo diagnóstico: no demuestra causa ni es solución de producción.
- `cargo test --locked --offline -p universo-okf --lib transaccion::journal::recovery::`
  con `TEMP` y `TMP` del proceso apuntando a `target/codex-verification-tmp`:
  61 passed, 17 failed, 3 ignored y 347 filtered; exit 1. La suite se ejecutó
  de nuevo y sigue roja. Los detalles de los filtros focales no identifican la
  causa de las 17 fallas en conjunto.
- `cargo test --locked --offline -p universo-okf --lib admision::envelope`:
  6/6.
- `cargo fmt --all -- --check`,
  `cargo clippy --locked --offline --workspace --all-targets -- -D warnings`
  y `python tools/check_public_boundary.py`: todos terminaron con exit 0.
- `cargo check --locked --offline --target x86_64-unknown-linux-gnu -p universo-okf --all-targets`:
  exit 0; es compilación cruzada y no ejecuta pruebas ni runtime Linux.
- Un intento de ejecutar runtime en WSL devolvió `E_ACCESSDENIED`; no se
  registra como runtime Linux exitoso ni se atribuye causa al entorno.

- La revisión de privacidad fue manual y de solo lectura sobre los diffs de
  este avance y las fixtures sintéticas del repositorio público. No revisó
  bundles personales ni fue un escáner; no garantiza ausencia de datos
  sensibles fuera del material inspeccionado.
- Se reportó el CI histórico `38630887294`; en aquella sesión no pudo
  reconfirmarse porque el acceso remoto quedó bloqueado. El resultado permanece
  registrado como informado, no verificado; en aquella ronda el CI exacto
  `38764635021` verificó el candidato `aa814e3` con estado SUCCESS.
