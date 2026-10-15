# Contexto privado para reproducir una admisión

**Estado:** decisión de diseño autorizada; `ContextoAdmision` es hoy un valor
de datos en memoria y no se persiste. Este documento no define almacenamiento
ni implementa I/O. El valor no es prueba ni autoridad de admisión: una
recuperación futura debe revalidarlo antes de volver a aceptar `New` después de
perder el backup. El estado global sigue en 4/9 fases; U7 y fase 04 permanecen
parciales. Replay y decoder aún no tienen caller desde apply/producción ni
cierran esas fases.

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
journal y no da una capability de recovery. La capa bajo lock debe contrastar
el root observado con la identidad y la huella esperadas y aplicar la fila
admitida por la tabla actual antes de conservar `New` o avanzar `Committed`.
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
crate-private y experimentales; el encoder calcula una cuota total de 512 MiB
que incluye framing. `bc38412` incorpora un preflight lector con vistas
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

## Vínculo puro a journal (sin persistencia)

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
consulta el filesystem, no conserva rutas o handles, no es capability y no
tiene codec ni lifecycle. El decoder de `FELIX-ADMCTX` sigue sin ligarlo por sí
mismo a un journal. `FELIX-ADMCTX` v1 y `felix-journal-v1` permanecen sin
cambios; no hay auxiliar persistido ni caller de publicación o recovery.

## Pendiente antes de cualquier I/O

- La próxima unidad debe decidir el lifecycle antes de escribir o leer el
  envelope. El tipo en memoria no elige dueño, ubicación, nombre, formato
  auxiliar ni orden de operaciones. Debe cerrar estas decisiones sin cambiar
  el contrato de `felix-journal-v1`:

  | Decisión | Criterio de cierre |
  |---|---|
  | Dueño y lock | Un dueño de transacción identifica la capability que publica, lee y retira la evidencia; se especifica su relación con parent, journal, backup y stage. El envelope no posee esa capability. |
  | Cortes de publicación | Una tabla cubre cada corte entre escritura temporal, sincronización, publicación del auxiliar y journal, y primer cambio destructivo. Define qué conserva cada corte y quién puede retirar un auxiliar huérfano; no permite recolección por nombre o namespace solamente. |
  | Cuotas y lectura | Se fija un límite total inclusivo para envelope y contexto, y el lector limita bytes antes de materializar payloads. El encuadre extra debe caber en la cuota sin inflar la lectura sobre el límite. La cuota wire no promete RSS limitado ni errores OOM recuperables. |
  | Recovery y compatibilidad | Bajo lock se compara el vínculo completo con journal, se reproduce el contexto y se contrasta identidad/digest del root observado antes de aplicar solo filas permitidas. Contexto ausente, truncado, grande, no canónico, incompatible o incoherente falla cerrado y conserva la evidencia. Los journals v1 sin auxiliar nunca se tratan como si contuvieran replay. |
  | Cleanup | Se determina el orden de retiro de journal, auxiliar y backup. El contexto requerido se conserva mientras exista journal, incluso en `Committed`; se define el destino de un corte que deje auxiliar huérfano y la frontera pendiente de `PartialOld` frente a mounts. |
  | Harness Windows | Los filtros actuales registran `ERROR_SHARING_VIOLATION` seguido por `STATUS_INVALID_HANDLE` en el retry y dos `PermissionDenied` al preparar junctions; antes de cualquier I/O queda aislar los fallos de recovery aún no explicados. No se atribuyen al sandbox ni a una regresión. |

  La ubicación de almacenamiento, el orden de publicación, la política de
  huérfanos y la capability que autoriza cleanup siguen abiertos; esta tabla
  define el alcance del siguiente diseño, no los resuelve por afirmación.

- Definir el dueño y la ubicación del auxiliar privado, su escritura atómica,
  sincronización, identidad y relación de lock con journal, backup y stage.
  `EnvelopeContextoAdmision` ya liga esos datos en memoria. El siguiente
  diseño debe decidir cómo persistir el vínculo y qué capability gobierna su
  lifecycle y cleanup. El tipo no elige ubicación ni ownership; esa decisión
  queda pendiente y no es arquitectura pública.
- Fijar cuotas inclusivas por componente y para el total serializado. El valor
  en memoria ya limita delta y metadata, y el encoder experimental impone un
  máximo total de 512 MiB con framing; eso no habilita lectura o persistencia.
  El límite de 1024 bytes corresponde al journal y no sirve como cuota del
  contexto; los límites actuales de árbol y configuración son precedentes,
  no una garantía pública para el auxiliar. El límite wire tampoco es una
  promesa de RSS acotado o de fallo OOM recuperable: el decoder usa
  asignaciones `Vec`, `to_owned` y `Arc` que no devuelven errores de reserva.
- Especificar comportamiento fail-closed ante contexto ausente, truncado,
  demasiado grande, no canónico, incompatible o incoherente con el `New`
  observado; conservar journal y evidencia en esos casos.
- Integrar y probar bajo lock la comparación de `NewEsperado` con identidad y
  digest del root observado, aplicando solo filas permitidas por la tabla. El
  cálculo puro no autoriza conservar `New` ni aceptar `Committed` por sí solo.
- Determinar cuándo puede retirarse el contexto sin eliminar evidencia que
  todavía necesita una fila de recovery. Mientras exista el journal, incluso
  en `Committed`, el contexto sigue siendo requerido. La opción segura es
  retirarlo después de eliminar el journal por su capability; un corte entre
  ambas limpiezas deja un auxiliar huérfano que no se puede recolectar por
  nombre o namespace solamente. La autoridad para cleanup de ese huérfano y el
  orden de cleanup de backup/contexto quedan abiertos. También sigue pendiente
  la frontera segura de cleanup de `PartialOld` frente a mount, bind mount o
  FUSE.

`Committed` sigue requiriendo `New` revalidado según la tabla actual. No se
autoriza saltarse esa validación cuando el auxiliar se borre. No se debe
implementar persistencia del contexto hasta resolver lifecycle, cuotas y
fallos de setup del entorno de pruebas.

## Evidencia de esta sesión

El acceso remoto de Git y `gh` sigue bloqueado: el último `git ls-remote` falló
por `getaddrinfo` y la consulta GET de CI con `gh` no conectó. No se obtuvo
SHA, estado ni conclusión del run y no se publicó push; no hay un CI remoto
nuevo que registrar. Toda la evidencia siguiente es local, no un run de CI:

- Sobre la base `5935608` más el árbol de trabajo, el filtro
  `cargo test --locked --offline -p universo-okf --lib admision::envelope`
  terminó 6/6. Los mismos bytes de esta unidad quedaron registrados en
  `74f7afa` (envelope, 345 líneas) y el seam de pruebas en `b37635b`
  (237 líneas). La prueba y los commits corresponden a los mismos bytes; no hay
  CI remoto asociado.

- El filtro focal de diagnóstico Windows registró 1 passed/1 failed. La
  suite de recovery también volvió a ejecutarse en la ronda actual; su
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

### Verificación actual del candidato local

Los resultados siguientes son locales, sobre la base `5935608` y los mismos
bytes de código finalmente registrados en `74f7afa` (envelope) y `b37635b`
(seam). Los commits no cambiaron los bytes verificados y no hay CI remoto
asociado.

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
- Se reportó el CI `38630887294`; esta sesión no pudo reconfirmarlo porque el
  acceso remoto quedó bloqueado. El resultado se registra como informado, no
  verificado aquí.
