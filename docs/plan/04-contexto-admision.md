# Contexto privado para reproducir una admisión

**Estado:** decisión de diseño autorizada; `ContextoAdmision` y su envelope son
valores de datos en memoria con codecs privados. El lifecycle documenta las
primitivas de lectura, creación temporal, verificación y publicación canónica ya
implementadas. El dueño observa el root raw desde el parent retenido y devuelve
la huella como dato. `capturar_snapshot_soportado` y su variante CAS capturan
desde ese root abierto bajo el lock retenido y devuelven un `Snapshot` de datos.
El CAS compara el token antes de validar procedencia o parsear, pero no ejecuta
recovery ni se conecta a apply. Los tags fijos del snapshot no avalan la
propuesta: su admisión debe verificarse por separado. Ya existe un coordinador
privado que carga journal y contexto con leases secuenciales, valida el replay y
compara esa huella con `NewEsperado`; devuelve datos de comparación, no una
decisión de recovery. Falta el caller de producción que aplique la tabla
vigente, además de cleanup y preflight global de huérfanos. El valor no es
prueba ni autoridad de admisión. La unidad A añade una prueba opaca que solo
confirma replay admitido y huella root coincidente en `Installed` o `Committed`;
no prueba que un caller instaló `New` ni elige una fila de recovery. Una
recuperación futura debe revalidar el root antes de volver a aceptar `New`
después de perder el backup. El estado global sigue en 4/9 fases; U7 y fase 04
permanecen parciales. Replay y decoder aún no tienen caller desde
apply/producción ni cierran esas fases.

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
JSON ni modifica el comportamiento de v1. El journal y su codec son privados:
esta decisión tampoco define una API pública, un formato wire ni un contrato
de compatibilidad para un futuro archivo auxiliar.

El contexto, replay, `NewEsperado` y framing `FELIX-ADMCTX` v1 siguen siendo
crate-private y experimentales; el encoder actual calcula una cuota de 512 MiB
que incluye su propio framing. El envelope exterior combinado, que también
incluye journal y encuadre, sigue experimental. Su encoder puro en memoria está
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

Los journals v1 existentes no contienen contexto de replay y no se reinterpretan
como si lo tuvieran. En particular, la ausencia o pérdida del contexto no
autoriza a omitir la validación de `New` ni a aceptar `Committed`. Si una
recuperación futura requiere replay y falta evidencia, debe fallar cerrada y
conservar los artefactos para decisión manual. Cambiar esa conducta requiere
una decisión de contrato separada.

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
journal `Prepared` → primer rename siguen pendientes. El framing exterior y sus
codecs ya están implementados; el diseño también fija cortes, preservación de
huérfanos y límites de durabilidad. Las primitivas de storage no son un caller
de producción ni una decisión de recovery.

El encoder y decoder exterior son codecs puros, mantienen `felix-journal-v1`
sin cambios y no abren rutas. El reader genérico exige EOF y no se integra a un
caller. La orquestación y el recovery context-aware requieren otra revisión.
En particular, antes de integrarlos se debe demostrar el
dispatch entre transacciones nuevas y journals v1 legacy: los journals legacy
sin auxiliar siguen exactamente el recovery actual. El codec y el binding no
son autoridad para aceptar `New`, habilitar `KeepNew`/`Committed` o limpiar el
backup. Sigue bloqueada la frontera real de mount, bind mount o FUSE para
cleanup de `PartialOld`; no se cambia el contrato ni la capability del backup.

`Committed` mantiene los requisitos de la tabla vigente. No se salta su
validación por retirar un auxiliar futuro. Fase 04 y U7 continúan parciales;
el progreso global permanece en 4/9.

## Evidencia de verificación

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

Antes de usar una prueba futura, el mismo owner debe revalidar guard, parent y
scope, observar los cuatro siblings y recargar el ID y registro completo/fase
del journal. También debe releer el ID, longitud y SHA-256 del contexto,
comprobar binding y replay soportados, y observar root, backup y stage antes
de elegir cualquier fila. Esas revalidaciones y la decisión siguen pendientes.

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

Los filtros focales reproducibles, ejecutados por separado en Windows y Linux, son:

```text
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::duenio_transaccion
cargo test --locked --offline -p universo-okf --lib transaccion::anclado::contexto_admision
cargo test --locked --offline -p universo-okf --lib transaccion::admision::envelope
```

Son fixtures sintéticas del filesystem bajo el lock cooperante; no ejecutan apply
ni recovery. El rollback de B revierte su commit y quita solo la extensión de
pruebas auxiliares/wiring, conservando A. El rollback de A retira su productor,
test y wiring/reexport; no altera el journal v1 ni las rutas legacy de recovery.

Los registros siguientes son históricos de rondas anteriores. Se conservan
sin borrar sus resultados; sus conteos no reemplazan la verificación actual ni
identifican por sí solos la causa de los fallos observados entonces.

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
