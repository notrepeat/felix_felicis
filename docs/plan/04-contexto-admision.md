# Contexto privado para reproducir una admisión

**Estado:** decisión de diseño autorizada; `ContextoAdmision` es hoy un valor
de datos en memoria y no se persiste. El lifecycle propuesto para un futuro
dueño está separado en [04-contexto-admision-lifecycle.md](04-contexto-admision-lifecycle.md);
ninguno de los dos documentos implementa I/O. El valor no es prueba ni
autoridad de admisión: una recuperación futura debe revalidarlo antes de volver
a aceptar `New` después de perder el backup. El estado global sigue en 4/9
fases; U7 y fase 04 permanecen parciales. Replay y decoder aún no tienen caller
desde apply/producción ni cierran esas fases.

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
crate-private y experimentales; el encoder actual calcula una cuota de 512 MiB
que incluye su propio framing. El envelope exterior combinado, que también
incluye journal y encuadre, sigue experimental. Su encoder puro en memoria está
implementado en `admision/envelope/codec.rs` y pasa al codec interior solo la
cuota restante antes de reservar. El decoder exterior, I/O y callers siguen
pendientes; el wire y wrapper interior no cambian. `bc38412` incorpora un
preflight lector con vistas
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

## Lifecycle y gates antes de persistencia

El documento [04-contexto-admision-lifecycle.md](04-contexto-admision-lifecycle.md)
define como propuesta el dueño con `&mut LockEscritura`, el parent anclado, los
leases secuenciales, el orden contexto → journal `Prepared` → primer rename,
el framing exterior con cuota combinada inclusiva de 512 MiB y el cleanup
journal → contexto. También fija cortes, preservación de huérfanos y límites de
durabilidad. Es diseño para un futuro flujo; no autoriza I/O ni un caller de
producción.

El encoder exterior es una unidad pura en memoria, mantiene `felix-journal-v1`
sin cambios y no tiene I/O ni caller. El decoder, persistencia y recovery
context-aware requieren otra revisión. En particular, antes de integrarlos se debe demostrar el
dispatch entre transacciones nuevas y journals v1 legacy: los journals legacy
sin auxiliar siguen exactamente el recovery actual. El codec y el binding no
son autoridad para aceptar `New`, habilitar `KeepNew`/`Committed` o limpiar el
backup. Sigue bloqueada la frontera real de mount, bind mount o FUSE para
cleanup de `PartialOld`; no se cambia el contrato ni la capability del backup.

`Committed` mantiene los requisitos de la tabla vigente. No se salta su
validación por retirar un auxiliar futuro. Fase 04 y U7 continúan parciales;
el progreso global permanece en 4/9.

## Evidencia de verificación

El candidato exacto `aa814e3c28231bb83934a1275328594fa8853649` está publicado en
`origin/codex/plan-demostracion`. El CI exacto `38764635021` terminó SUCCESS
en ambos jobs, Linux y Windows, y en todos sus pasos; el operador verificó los
logs. Corresponde al SHA aa814e3 y no cubre el encoder local posterior sin
commit. No se atribuye una causa a fallos históricos por el resultado actual.

La verificación local más reciente, reportada sobre ese SHA, registró en
Windows 783 passed/0 failed/9 ignored para workspace, recovery 78/0/3,
diagnósticos 2/2 y filtros focales 37/37. `cargo fmt`, clippy con warnings
denegados y el chequeo de frontera pública terminaron verdes. La repetición de
runtime nativo en WSL se canceló ante la orden de detener; no cuenta como
resultado ni como runtime Linux exitoso.

En este worktree posterior, sin SHA ni CI propios, el filtro del encoder
exterior terminó 3/3, el filtro del codec interior 2/2 y
`cargo fmt --all -- --check` terminó con exit 0. Son pruebas focales en memoria;
no ejercitan persistencia, I/O ni recovery context-aware.

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
  registrado como informado, no verificado; el CI `38764635021` es la
  verificación exacta más reciente del candidato y tiene estado SUCCESS.
