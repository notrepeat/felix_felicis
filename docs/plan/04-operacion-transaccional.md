# 04 — Evaluar propuestas sin corromper el universo vigente

El objetivo es recibir un delta, construir un candidato aislado y publicar el
cambio únicamente cuando el check lo admita. Todo rechazo debe dejar estado y
bytes intactos.

## Estado

| Campo | Valor |
|---|---|
| Estado | en curso |
| Depende de | 03 |
| Desbloquea | 05, 08 |

## Objetivo

Definir y entregar una transacción v1 que evalúe un delta contra un snapshot
identificado, construya un candidato aislado y publique solo tras aceptación
explícita. Un rechazo o error previo al commit conserva intactos el estado
vigente y sus bytes.

La fase 03 asigna a esta fase la aceptación de conflictos funcionales de un
delta (tipo, etiqueta, padre e incidencia). Esta asignación de alcance cierra
el criterio 17 de fase 03; no afirma que la detección o diagnóstico ya estén
implementados.

Estado de entrega: las Unidades 0–4 están cerradas dentro de sus límites; U5,
U7 y U8 conservan avance parcial, y U6 está cerrada para root anclado y
staging. U7 incluye filas exactas `KeepOld`, `RestoreOld` y
`DiscardNewRestoreOld`. `KeepOld` limpia stage `New`/`PartialNew` con root
`Old`, backup `Absent` y `root_new_validated=false` en `Prepared`,
`BackupReady` e `Installed`. `RestoreOld` acepta root `Absent`, backup `Old`,
`root_new_validated=false` y stage `Absent`, `New` o `PartialNew` en esas tres
fases; reabre el stage por la identidad durable del journal (y exige digest
completo cuando es `New`), lo limpia y reobserva antes de restaurar el backup
y retirar el journal. `DiscardNewRestoreOld` acepta solo `BackupReady` con
root `New`, backup `Old`, stage `Absent` y validación false/true, o `Installed`
con esa misma fila y validación false. Renombra root `New` al stage identificado
por el journal, reobserva para reconciliar el intento, limpia y reobserva el
stage, restaura el backup verificado y retira el journal al final. Root `New`
con cualquier stage sibling presente falla cerrado. Estas funciones siguen
crate-private y no tienen caller desde apply/producción.
Desde `5c11f76`, el observer marca `PartialOld` solo en backup cuando conserva
la identidad durable de `old`, su digest raw difiere y las identidades de
`old` y `new` son distintas. Si la identidad de backup es otra, o las
identidades de `old` y `new` coinciden, queda `Unknown`; root y stage conservan
sus clasificadores más estrechos. `PartialOld` en cualquier slot produce
`FailClosed` con `NO_STEPS`: es observación y rechazo, sin capacidad nueva de
recovery, cleanup, paso del runner ni orden de limpieza. La limpieza del
backup `PartialOld` se pospone porque no hay una frontera segura contra cruce
de mount, bind mount o FUSE. Es un límite/bloqueo de diseño, no un
comportamiento probado por el commit.

Desde `bb06df3`, el cleanup explícito de `StageAnclado` con capability tiene
guardia parcial de scope. Captura el scope del parent; verifica la raíz, los
handles de archivo/directorio y las reaperturas durante el preflight; y vuelve
a comprobar parent y objetivo antes de cada unlink/rmdir, reapertura y rmdir
final. Error, ausencia o mismatch del probe fallan cerrado como
`stage_cleanup_io`. Si cleanup falla, `ErrorLimpiezaStage` conserva el
`StageAnclado` para retry; un error posterior a unlink puede dejar borrado
parcial. Las carreras externas entre check y operación siguen fuera del
contrato. CI `38576523094`
pasó para este commit en Linux y Windows. Esto no cubre cleanup de sibling
huérfano ni una frontera general de mounts. Los pasos de cleanup de los runners
de recovery existentes ya invocan `StageAnclado::limpiar` y heredan el probe
real; siguen pendientes la cobertura end-to-end de recovery que ejercite esa
frontera y un caller productivo de los runners.
Desde `a4583c2`, `inventariar_root_anclado` compara el scope del parent con el
del root antes de enumerar y valida por handle cada directorio y archivo antes
de descender o leer; error, ausencia o mismatch fallan como `snapshot_io`.
Captura anclada, `StageAnclado::fingerprint_raw`/recaptura y el observer de
recovery heredan esta comprobación mediante los wrappers de inventario. Es un
slice de lectura, no una frontera general: las pruebas son sintéticas (sin
bind mount real ni mounts anidados), el probe Linux usa `STATX_MNT_ID` y el de
Windows `VolumeSerialNumber` (volumen, no instancia de montaje), el backend por
ruta/bundle sigue pendiente y las carreras externas TOCTOU quedan fuera del
contrato.
Desde `3e8ec0a` y `22494c8`, `ErrorEscrituraStage` recupera ownership de la
capability como `Result<StageAnclado, Self>`; si no hay capability, devuelve el
error intacto para conservar código, estado parcial e indicador diagnóstico.
Desde `75d223e`, la capability writable guarda el scope inicial y lo propaga a
descendientes. Las escrituras comprueban el parent antes de abrir/crear
directorios o archivos, el child handle tras abrir y el file handle justo antes
de `write_all`; parent y stage deben coincidir con el scope inicial, así que un
cambio conjunto a otro scope también falla cerrado. Error, ausencia o
mismatch producen `stage_io`. Si la comprobación falla después de crear el
archivo puede quedar vacío, mientras la capability recuperable permite cleanup
explícito y retry. El token de scope no se guarda en el journal. No se habilita
cleanup automático de sibling huérfano sin capability; rename, integración a
apply y publicación siguen pendientes.
U8 añade captura por ruta acotada y privada para simulación y pruebas y, en
`63e59d1`, preflight consumidor de la suma de
configuración antes de abrir el root bajo lock y antes de clonar/inventariar
el root anclado. El exceso devuelve `snapshot_resource_limit` redactado,
conserva root/canary y retiene el guard; dentro de cuota, el CAS sigue antes
de provenance y parseo. Desde `4b94261`, `ConfigRawCapturada::nueva` también
es fallible y hace cumplir la misma cuota combinada como invariante: la
comprobación ocurre después de convertir ambas entradas a `Arc<[u8]>` y cobra
cada campo lógico, aunque comparta asignación. Por tanto, no limita lectura,
asignación/materialización previa, RSS ni I/O antes del lock; el preflight del
consumidor sigue siendo una defensa necesaria. Esta garantía es distinta de
la captura privada por ruta, que limita la lectura en curso y permite como
máximo el remanente más un byte centinela en fuentes externas. Siguen
pendientes el proveedor con lectura acotada antes del lock, los callers de
apply/CLI y las cotas para `version_motor` y `policy_id`. Para `RestoreOld` y
`DiscardNewRestoreOld`, un error ambiguo de cleanup provoca reobservación: si
el stage ya desapareció se continúa; si persiste parcial, se conserva el
journal y un siguiente intento puede reutilizar la identidad durable. Las
pruebas de retry sueltan y readquieren el lock en el mismo proceso; no simulan
kill/restart ni prueban crash durability. El recovery
sigue incompleto. Fase 04 sigue en curso: la guardia parcial de scope para
escrituras de staging, el lock y la captura anclada aún no están conectados a
apply ni publican candidatos.

En `609936a`, las regresiones del CAS privado por ruta verifican que un match
con configuración externa conserva los nombres, tipos y bytes del árbol y los
bytes externos, sin crear lockfile sibling. La captura también funciona con
el guard de escritura que adquirió el caller retenido; un segundo escritor
sigue recibiendo `writer_lock_busy`. Si cambia la configuración externa por
bytes inválidos, el CAS devuelve `stale` redactado antes de parsearla y sin
cambios de contenido o namespace. La ruta no hace escrituras intencionales de
contenido o namespace, pero estas pruebas no cubren atime, permisos,
metadatos de entradas ni identidades individuales; la comparación añadida en
`a3cc3c9` sí comprueba el `ArtifactFingerprint` del root, que combina identidad
del directorio raíz y digest raw. Se conserva TOCTOU y no se garantiza
atomicidad ante cambios hostiles del filesystem. El helper es crate-private,
sin caller desde apply/CLI ni simulación pública.

En `eef28f5`, la regresión existente del dispatcher cubre seis combinaciones
de `KeepOld`/`RestoreOld` en `Prepared`, `BackupReady` e `Installed`, con una
observación inicial por ejecución. `KeepOld` conserva el fingerprint old y
comprueba el inventario del parent salvo el journal retirado; `RestoreOld`
recupera el fingerprint old y deja journal, backup y stage ausentes. `51cd5c4`
extiende ambas aserciones al inventario recursivo: compara rutas, tipos y bytes
de archivos normales; para el lock abierto solo presencia, tipo y longitud.
En `RestoreOld`, el inventario esperado modela el movimiento del subárbol de
backup a root y la retirada del journal. Es evidencia de filas privadas, no
de un caller productivo ni de atomicidad frente a cambios hostiles. El
inventario no observa atime, permisos ni otros metadatos, y no elimina TOCTOU.

Desde `09aee6e`, `proponer` comparte 1.000.000 de unidades deterministas de
evaluación entre todas las fórmulas expandidas de una propuesta. El contador
empieza después de construir el candidato, detectar conflictos y completar la
expansión, cuya cuota separada sigue en 100.000 instancias. Al agotarse,
devuelve `evaluation_budget_exhausted` sin candidato ni resultado parcial.
`evaluar` conserva su presupuesto por llamada y el `check` CLI baseline lo
reinicia por fórmula. La cuota incluye el preflight de cada fórmula, pero no
cubre la preparación previa al contador: construcción del candidato, detección
de conflictos y expansión; tampoco acota memoria ni tiempo total.

## Auditoría de prerequisitos de U7 (2026-10-14)

Este registro separa la evidencia reciente de identidad Windows de las
condiciones de diseño aún pendientes para la frontera general de scope,
validación y recovery. U7 y la fase 04 siguen en curso; el progreso maestro
permanece en 4/9.

Los commits `752f56e` y `9f48304` añaden una guardia parcial para crear el
sibling de staging: comprueba el parent del root antes de `mkdir` y el parent
del stage tras reabrirlo por handle. Linux compara `STATX_MNT_ID`; Windows,
`VolumeSerialNumber`. Error, dato ausente o mismatch fallan cerrado. Cualquier
error desde el intento de `mkdir` puede dejar un sibling vacío huérfano
posible; el error interno conserva el código estable `stage_io` y propaga el
indicador diagnóstico sin ampliar la API pública; no se devuelve capability
parcial y no hay cleanup automático del posible sibling. El indicador aún no
tiene consumidor productivo. Esto no completa la frontera de mounts/volumes ni
modifica journal, recovery o walkers; el cleanup explícito de un stage con
capability se aborda parcialmente en `bb06df3`.

Los commits `401a4be` y `316bdb8` hacen que un `FILE_ID_INFO` exitoso con un
`FileId` de 16 bytes todo cero o todo `0xFF` falle como identidad requerida por
transacciones y journal. Solo el marcador exacto de sentinel puede degradarse a
identidad ausente en `identidad_directorio_para_recorrido`, donde el walker
bundle de solo lectura continúa con fallback por ruta canónica; un
`Unsupported` genérico no activa esa ruta. Los errores Windows 1/50/87
conservan el fallback de lectura ya existente. `cedc11c` hace que los
preflights de pruebas omitan ese sentinel exacto, sin cambiar producción.
CI `38517718310`
pasó para `cedc11c` en Linux (`verify`) y Windows (`verify-windows`).

La guardia parcial de `752f56e`/`9f48304` compara por handle el parent del root
antes de crear el sibling y el parent del stage reabierto después; en Linux
usa `STATX_MNT_ID` y en Windows `VolumeSerialNumber`. `bb06df3` reutiliza esa
probe en el cleanup explícito de `StageAnclado`: toma scope del parent y
preflight/revalida por handle la raíz, archivos, directorios y reaperturas
antes de borrar. No persiste el mount ID. Error, dato ausente o mismatch fallan
cerrado, pero estas guardias parciales no son una frontera completa de
mount/volume. Desde `a4583c2`, el inventario anclado comprueba parent/root
antes de enumerar y el scope de cada handle de directorio/archivo antes de
descender o leer; errores, ausencia o mismatch producen `snapshot_io`. Captura
anclada, `StageAnclado::fingerprint_raw`/recaptura y observer de recovery
heredan la comprobación mediante los wrappers. Sigue siendo un slice parcial:
el backend por ruta/bundle queda fuera, mientras las escrituras de staging
tienen solo la guardia parcial descrita abajo. Las pruebas son sintéticas, sin
bind mount real ni mounts anidados; en Windows,
`VolumeSerialNumber` identifica volumen pero no instancia de montaje; TOCTOU
externo sigue fuera del contrato. Los pasos de cleanup de los runners de
recovery existentes ya invocan `StageAnclado::limpiar` y heredan el probe real;
faltan cobertura end-to-end de recovery que ejercite esa frontera y un caller
productivo de los runners. Tampoco se autoriza cleanup del sibling huérfano sin
capability ni existe consumidor productivo del indicador. No hay fallback por
ruta ni por `st_dev`; la ausencia o error al comprobar scope debe fallar
cerrado. El walker de rutas del bundle pertenece a otra familia y no queda
cubierto. La cobertura determinista del sentinel `FileId` de Windows ya está
registrada arriba. Siguen pendientes la prueba de bind mount Linux que comparte
`st_dev` y los mounts anidados. Ninguna prueba aislada demostraría pertenencia
general al mismo volume/mount.

Todo error que se produzca desde el intento de `mkdir` puede dejar un sibling
vacío huérfano posible, incluso si falla la apertura posterior por handle. Se
mantiene el código estable `stage_io` y no cambia la API pública; el indicador
aditivo es diagnóstico interno, no capability ni autorización de cleanup. No
hay cleanup automático, y todavía falta un consumidor productivo del indicador.

Los commits `3e8ec0a`/`22494c8` hacen que la recuperación de capability de
`ErrorEscrituraStage` sea owned (`Result<StageAnclado, Self>`): `Ok` transfiere
el stage y `Err` devuelve el error intacto. `75d223e` captura el scope inicial
en la capability writable y lo propaga a descendientes; verifica parent antes
de open/mkdir/create, el child después de abrir y el file handle antes de
`write_all`. Ambos deben coincidir con el scope inicial, así que se rechaza
también un cambio conjunto de parent y stage a otro scope. Un probe incierto
falla con `stage_io`; una comprobación posterior a create puede dejar un archivo
vacío, pero mantiene capability para cleanup explícito/retry. El token de scope
no va al journal. Esto no cubre rename ni integra staging a apply/publicación;
no hay caller productivo de los runners. Pruebas sintéticas: no demuestran bind
mount real ni mounts anidados. En Windows, `VolumeSerialNumber` no identifica
instancia de montaje; TOCTOU externo sigue fuera del contrato.

`RootNewValidado` también necesita procedencia de admisión: un `Candidato`
público o un comparador de representación no demuestra qué base, delta e
invariantes se evaluaron ni que la propuesta fuera admitida. El root debe
respaldarse con una prueba de admisión sellada y no forjable, o recovery debe
volver a ejecutar `proponer` con la base, el delta y las invariantes y comprobar
el resultado admitido. Journal v1 no conserva esas entradas; por tanto, hoy un
restart no puede revalidar `New`. Hasta que exista esa capacidad opaca, el
observer mantiene `root_new_validated=false`. El enum, la tabla pura y las
pruebas modelan `Committed`, pero el preflight, el dispatcher y el caller
productivo no lo ejecutan; por eso, la transición/flujo productivo hacia
`Committed` es inalcanzable actualmente.

La siguiente secuencia se divide en unidades pequeñas, en este orden:

1. Completar y probar la frontera de scope, incluidos bind mount real y mounts
   anidados.
2. Completar los consumidores aún fuera del camino anclado: backend por
   ruta/bundle, integración a apply de la guardia parcial de escritura y los
   caminos restantes de rename/publicación; añadir cobertura end-to-end de
   recovery para los pasos que ya delegan cleanup en `StageAnclado::limpiar`,
   habilitar un caller productivo de los runners, completar los demás caminos
   de cleanup y añadir un consumidor productivo del indicador de sibling
   huérfano posible.
3. Prueba de admisión sellada o revalidación con sus entradas.
4. Validación de identidad y contenido del root `New`.
5. Habilitar `Committed` solo después de esa validación.

## Decisiones de contrato v1

- `add`/`remove` es un parche transaccional explícito sobre un snapshot; es
  distinto de AGM/R y no afirma minimalidad ni prioridad. No se infieren
  operaciones a partir de un resultado deseado.
- El JSON v1 tiene la forma exacta
  `{"schema":"felix-delta-v1","add":[...],"remove":[...]}`. La API typed
  canonicaliza y deduplica átomos repetidos; el parser JSON rechaza átomos
  repetidos en cada lista, claves desconocidas o ausentes y claves duplicadas.
  No se aceptan colisiones `Ext` con los predicados reservados `Type`, `Edge`,
  `Parent` y `HasLab`.
- Canonicalizar, deduplicar y ordenar las operaciones antes de evaluar. Una
  misma identidad canónica en `add` y `remove` es solapamiento y se rechaza.
  Un `Edge` idéntico en ambos lados queda sujeto a la misma regla; el par se
  rechaza como solapamiento, no se interpreta como una operación no-op.
  `apply` requiere el snapshot esperado; una identidad distinta es `stale`.
- Antes de operar sobre `<raiz>`, inspeccionar sin seguir links y rechazar si
  la raíz proporcionada es symlink, junction o reparse point. Canonizar esa
  raíz una sola vez; derivar lock, staging, backup y journal del mismo parent
  y basename canónicos para que aliases converjan al mismo destino.
- Resolver primero los retiros `Edge` y `HasLab` contra el snapshot a handles
  internos. Agruparlos por `(origen, tipo, destino)`, retirar en lote y
  compactar supervivientes preservando el orden del snapshot. Aplicar adds
  después, ordenados numéricamente por `(origen, tipo, destino, ordinal)`, no
  por el texto de `Atomo`. Reconstruir cada grupo por posiciones finales y
  reconstruir `HasLab` desde handles; los ordinales declarados deben coincidir
  con posiciones contiguas. No depender del orden de llamadas de
  `quitar_arista` ni usar ordinales previos a los retiros.
- `HasLab` es funcional solo por sujeto; igual etiqueta en sujetos distintos
  se permite salvo invariante. Para cambiar la etiqueta del mismo sujeto,
  retirar y añadir explícitamente. En aristas, el retiro usa su identidad del
  snapshot y el alta de etiqueta usa la identidad final después de compactar
  ordinales.
- `Ext` requiere predicado declarado, sujeto nodo existente y constante de
  valor perteneciente al dominio declarado. Para el mismo par
  `(predicado, nodo)`, reemplazar un valor exige `remove` del valor vigente y
  `add` del nuevo; `add` no sobrescribe y `remove` identifica el valor exacto.
- El token de snapshot usa framing etiquetado con longitudes y separación de
  dominio/version (`FELIX-SNAPSHOT-v1`). La política
  `files-and-directories-v1` incluye archivos (también vacíos) y entradas de
  directorio (también vacíos), con rutas UTF-8 exactas, separadores `/`, orden
  por bytes y sin normalización Unicode. También incluye manifiesto e
  invariantes raw, versión de motor y política. `SnapshotToken` es público y
  opaco: `Debug`/`Display` redactan el digest; `to_hex()` solo lo revela cuando
  se invoca explícitamente. La captura y el CAS de captura permanecen
  `pub(crate)`. Cada archivo se lee una vez a buffers inmutables; el token se
  calcula desde esos bytes, el token esperado se compara antes del parseo y la
  materialización usa los mismos buffers. Una diferencia produce `stale`
  fijo/redactado antes de parsear manifiesto, invariantes o bundle. Esta
  primitiva no escribe ni publica; la captura por ruta aún tiene límite TOCTOU.
  U6 aporta acceso anclado para el root, pero el CAS sigue sin integrarse a
  apply.
- Cargar y serializar `Parent` requiere opt-in por `layout.parent_key`. La
  Unidad 3 cierra la carga exacta, raíz explícita, fallback legacy y
  serialización determinista del candidato a documentos en memoria. Esto no
  escribe el bundle ni publica cambios; ver el alcance y los límites abajo.
- La API pura de `universo-core` es
  `proponer(estructura, delta, invariantes) -> Result<ResultadoPropuesta, ErrorPropuesta>`;
  construye/evalúa candidato y separa errores técnicos del veredicto
  funcional, sin filesystem ni publicación. La API objetivo de
  `universo-okf`/CLI `aplicar(..., expected_snapshot, ...)` adquiere lock,
  hace CAS del snapshot, reevalúa con core y coordina staging/publicación solo
  para `admit`; esa integración todavía no está implementada.
- `propose` simula por defecto y omite el token de snapshot. Solo
  `--emit-snapshot-token` lo incluye. Aplicar exige el comando concreto:

  ```text
  universo propose <raiz> --manifiesto <m> --invariantes <i> --delta <d.json> --format json --apply --expect-snapshot <token>
  ```

  Obtener el token, de forma opt-in y sin aplicar:

  ```text
  universo propose <raiz> --manifiesto <m> --invariantes <i> --delta <d.json> --format json --emit-snapshot-token
  ```

- Un lock advisory/exclusivo de sibling
  (`<parent-canónico>/.<basename-canónico>.felix.lock`) usa un file handle
  sobre un lockfile reutilizable mediante `fs4`. No borrar ni reemplazar el lockfile. El OS libera
  el lock al morir el proceso. Tras adquirirlo, siempre correr recovery antes
  del stale recheck; retenerlo hasta terminar publicación y cleanup. Con lock
  ocupado, el segundo writer responde `busy`; si lo adquiere luego, compara
  contra la raíz nueva y responde `stale` cuando cambió. Nunca last-write-wins.
- El staging es sibling de la raíz
  (`<parent-canónico>/.<basename-canónico>.felix.stage.<txn-id>`):
  para MVP apply, rechazar cualquier symlink, junction o reparse point dentro
  de root antes de clonar. Copiar auxiliares solo si son archivos regulares o
  directorios, conservar bytes de archivos, actualizar rutas administradas y
  recargar el bundle staged para compararlo con el candidato. Simulación
  legacy puede leer links; no publica.
- Commit con journal sibling
  (`<parent-canónico>/.<basename-canónico>.felix.journal`) y backup sibling
  (`<parent-canónico>/.<basename-canónico>.felix.backup`): escribir `prepared`, renombrar
  `root -> backup`, escribir `backup_ready`, renombrar `stage -> root` y
  escribir `installed`. Recovery bajo lock es determinista: `prepared`
  conserva old (si root falta y backup existe, restaura backup); después de
  asegurar old, elimina staging residual y cualquier backup incoherente bajo
  lock antes de borrar el journal. `backup_ready` restaura old (si root ya
  contiene new, lo descarta antes de restaurar backup); después elimina
  staging residual y cualquier backup incoherente bajo lock antes de borrar
  el journal. `installed` conserva new si root recarga y valida contra el
  candidato; si no valida y hay backup, restaura old; sin backup y sin
  candidato válido, falla cerrado. Durante cleanup se elimina backup antes
  de marcar `committed`; por eso `installed` sin backup es válido solo si root
  valida el candidato. `committed` conserva new y recovery limpia staging y
  cualquier backup residual. El journal se elimina al final, después de
  limpiar restos. Un estado o archivo requerido incoherente falla cerrado
  para recuperación manual. El protocolo no promete visibilidad continua
  durante los renombres ni durabilidad ante pérdida de energía.
- Snapshot token, hashes, rutas, etiquetas y valores de datos son sensibles.
  La salida los omite o redacta por defecto; solo el token solicitado con
  `--emit-snapshot-token` se expone para el flujo explícito de apply.

## Guardas de revisión

Decision needed before apply: No
Chained PRs recommended: Yes
Chain strategy: feature-branch-chain
400-line budget risk: High

## Unidades de trabajo

- [x] 0 — Contrato typed/canonicalizador core y parser JSON v1 OKF:
  `crates/universo-core/src/delta.rs`, `crates/universo-core/src/lib.rs`,
  `crates/universo-okf/src/delta.rs`.
  Test `crates/universo-core/tests/delta_transaccional.rs`
  (`cargo test -p universo-core --test delta_transaccional`): asserts para
  el modelo/canonicalizador typed, `add`, `remove`, solapamiento y orden.
  `crates/universo-okf/tests/delta_json_v1.rs`
  (`cargo test -p universo-okf --test delta_json_v1`) prueba parseo/errores
  del JSON v1. Los 6 casos de core cubren orden/canonicalización, deduplicación,
  solapamiento, redacción y predicados reservados; los 5 de OKF cubren forma y
  claves JSON, duplicados, roundtrip, orden canónico y cargas de 1/100/10.000
  operaciones. Esa carga valida roundtrip, cantidad y bytes serializados; no
  registra µs/op. El parser/canonicalizador rechaza el delta completo antes de
  construir el candidato, que corresponde a Unidad 1. Esta Unidad 0 quedó
  completada en dos commits revisables: `1074902ec4687af435f2772de77febd161a59e68`
  (240 líneas añadidas) y
  `a54688ca274fa46f1a6a438df7988c13ef1176de` (247 líneas añadidas), dos
  commits revisables de menos de 400 líneas cada uno y sin cambio de alcance.
- [x] 1 — Candidato, Edge y Ext: `crates/universo-core/src/transaccion/candidato.rs`,
  `crates/universo-core/src/transaccion/aristas.rs`,
  `crates/universo-core/src/transaccion/ext.rs`.
  Cerrada como unidad de construcción de candidatos; la fase 04 sigue en curso.
  El core clona la estructura en un candidato aislado y admite retiros y altas
  `Edge`/`HasLab` de arista, además de `Ext` de nodo. Resuelve los retiros de
  arista contra el snapshot a handles; luego retira en lote, compacta cada
  grupo preservando el orden de supervivientes y conserva sus labels por
  handle. Las altas `Edge` requieren un `HasLab` correspondiente y usan
  ordinales finales contiguos en orden numérico; los grupos
  `(origen, tipo, destino)` se procesan de forma independiente. Las mezclas
  remove/add también usan las posiciones finales.

  Un `HasLab` retirado identifica la etiqueta vigente por handle en el
  snapshot. Si el handle sobrevive, la etiqueta nueva se añade con su identidad
  final, ya compactada. Para reemplazar una etiqueta se requiere remove+add de
  `HasLab`; no se retira y reañade el `Edge`. Un `Edge` vigente puede añadirse
  idempotentemente con su `HasLab` exacto, y el delta vacío también es no-op.
  En cambio, el mismo `Edge` canónico en add y remove se rechaza por
  solapamiento en `Delta::nuevo`; no es necesario representar un no-op con ese
  par de operaciones. `Ext` requiere predicado declarado, sujeto nodo existente
  y valor dentro del dominio declarado; reemplazar el valor del mismo
  `(predicado, nodo)` exige retirar el valor vigente y añadir el nuevo. Las
  altas idénticas son no-op y las bajas exigen la identidad exacta. `Type`,
  `Parent` y `HasLab` de nodo siguen rechazados fail-closed como
  `unsupported_operation` al cierre de Unidad 1. Unidad 2 añadió construcción
  core para `Parent` y `HasLab` de nodo y su evaluación mediante `proponer`;
  `Type` sigue rechazado técnicamente. Unidad 3 incorpora la carga optativa y
  serialización en memoria de `Parent`; la escritura y publicación filesystem
  continúan pendientes.

  `crates/universo-core/tests/candidato_delta.rs`
  (`cargo test -p universo-core --test candidato_delta`) pasa 3/3 casos de
  aislamiento, preservación de entrada y rechazo redactado de operaciones no
  soportadas. `crates/universo-core/tests/delta_aristas.rs`
  (`cargo test -p universo-core --test delta_aristas`) pasa 14/14 casos de
  retiros/altas, ordinales numéricos, ordinales finales en mezcla remove/add,
  grupos independientes, label vigente requerida, no-op exacto, reemplazo por
  handle snapshot→identidad final, fallo sin mutación y redacción. Las pruebas
  también distinguen ordinales `#2`/`#10` con 11 aristas paralelas.
  `crates/universo-core/tests/ext_transaccional.rs`
  (`cargo test -p universo-core --test ext_transaccional`) pasa 10/10 casos de
  validación, reemplazo, idempotencia, rechazo redactado y composición; y
  `crates/universo-okf/tests/ext_roundtrip.rs`
  (`cargo test -p universo-okf --test ext_roundtrip`) pasa 3/3 casos de JSON,
  serialización y recarga.

  Runtime release de `candidato_runtime` (`cargo test -p universo-core
  --release --test candidato_runtime -- --ignored --nocapture`): dos muestras
  únicas independientes; ms, átomos y bytes canónicos UTF-8 de la estructura
  candidata:

  | Escenario | Muestra 1 | Muestra 2 | Átomos candidato | Bytes canónicos |
  |---|---:|---:|---:|---:|
  | `Ext`: 10.004 átomos base, 1.000 cambios, 2.000 átomos delta | 2,900 ms | 3,113 ms | 10.004 | 429.358 |
  | `Edge`: grupo de 1, 1 cambio, 2 átomos delta | 0,014 ms | 0,016 ms | 4 | 78 |
  | `Edge`: grupo de 100, 1 cambio, 2 átomos delta | 0,032 ms | 0,027 ms | 202 | 9.859 |
  | `Edge`: grupo de 10.000, 1 cambio, 2 átomos delta | 1,912 ms | 1,736 ms | 20.002 | 1.027.755 |

  Cada tiempo es una muestra sin warmup ni distribución estadística: la
  variación observada confirma que estos valores son orientativos y no definen
  SLA. Los bytes son la suma de strings canónicos UTF-8 de los átomos, no heap
  ni tamaño del bundle. La medición de un retiro por grupo no descarta ni
  optimiza el posible costo O(R×D) con muchos retiros. Rollback: descartar el
  candidato. Los commits de la unidad se mantuvieron bajo 400 líneas authored.
- [x] 2 — Conflictos funcionales y propuesta core: `nodos.rs`,
  `conflictos.rs` y `propuesta.rs` en
  `crates/universo-core/src/transaccion/`.
  `detectar_conflictos_funcionales` acumula el estado final
  (`snapshot - remove + add`) para claves `Type`, `HasLab` y `Parent`, y
  normaliza labels de arista por handle/topología final. La misma etiqueta en
  sujetos distintos se permite; una clave con más de un valor final distinto
  se cuenta como conflicto. Los diagnósticos incluyen solo clase y cantidad.

  El candidato core ahora aplica `Parent` y `HasLab` de nodo. `proponer` separa
  errores técnicos (`Err(ErrorPropuesta)`) de veredictos funcionales:
  `Admitida`, `RechazadaConflictos` y `RechazadaInvariantes`. Expande y evalúa
  las invariantes sobre el candidato; la procedencia de instancias falsas se
  redacta y omite fórmula y sustituciones. Errores técnicos de integridad
  (como labels ausentes/huérfanos, ciclo o retiro inválido) permanecen técnicos
  incluso si el plan contiene conflictos funcionales. `Type` siempre sigue
  siendo `unsupported_operation` técnico en `proponer`; sus conflictos solo
  están disponibles mediante el detector standalone. Los conflictos `Ext`
  siguen siendo técnicos según Unidad 1. El rechazo no muta el snapshot en
  memoria; esta ruta core no lee ni modifica archivos.

  La incidencia no es representable como conflicto v1: `IdArista` ya codifica
  origen, tipo, destino y ordinal, y la canonicalización rechaza una identidad
  inconsistente antes del detector. El detector standalone no valida
  `HasLab` huérfanos ni exige etiqueta para cada arista nueva; la construcción
  del candidato y `proponer` clasifican etiquetas ausentes/huérfanas como
  errores técnicos. `proponer` informa conflictos antes de expandir/evaluar
  invariantes; el detector no aplica ni publica cambios. La persistencia
  La carga y serialización en memoria de `Parent` se completan en Unidad 3;
  escribir documentos al bundle y publicarlo sigue pendiente.

  Las pruebas finales pasan: `nodo_parent_transaccional` 13/13,
  `conflictos_delta` 13/13, `proponer_preflight` 4/4 y
  `proponer_transaccional` 10/10. Cubren retiros/altas de nodo, reemplazos,
  ciclos y precedencia técnica, conflictos frente a invariantes, privacidad,
  orden estable, igualdad de etiqueta entre sujetos y repetición determinista
  del mismo delta sobre el mismo snapshot.

  Runtime release del detector (`cargo test -p universo-core --release --test
  conflictos_runtime -- --ignored --nocapture`): exactamente 1.000 conflictos
  (334 `Type`, 333 `HasLab`, 333 `Parent`) y 3 clases resumen. Dos ejecuciones
  disponibles:

  | Muestra | Tiempo | Conflictos/s | Nota |
  |---|---:|---:|---|
  | Antes del rename del contador a `conflictos_s` | 10,512 ms | 95.132 | Semántica equivalente. |
  | HEAD `dfc359f` | 13,315 ms | 75.101 | Harness final. |

  Cada valor procede de una muestra única, sin warmup ni distribución
  estadística; son orientativos y no constituyen SLA. El resumen indica
  `snapshot_unchanged=true`; la salida no expone hashes, identidades ni valores.
  Rollback: no publicar cambios. El harness del detector no mide el costo de
  evaluación. Desde `09aee6e`, `proponer` comparte una cuota de 1.000.000 entre
  todas las fórmulas expandidas, después de completar candidato, conflictos y
  expansión; la preparación, las copias, memoria y tiempo total quedan fuera.
  Los commits del slice se mantuvieron bajo 400 líneas authored/churn.
- [x] 3 — Carga y serialización en memoria de `Parent`. Código: core
  `crates/universo-core/src/manifiesto.rs` y `crates/universo-core/src/estructura.rs`;
  OKF `crates/universo-okf/src/manifiesto_toml.rs`, `bundle.rs`, `serializar.rs`
  y `errores.rs`. Pruebas: `crates/universo-okf/tests/padre_persistible.rs`.
  `layout.parent_key` es optativa y valida una clave de frontmatter. Sin ella
  continúa la inferencia legacy y un cambio de `Parent` falla con
  `parent_not_persistable`. Con ella, frontmatter ausente usa inferencia legacy,
  una cadena no vacía declara el `BundlePath` lógico exacto y una cadena vacía
  declara raíz explícita. El loader conserva si el `Parent` era explícito o
  inferido. `serializar_candidato` es puro en memoria: devuelve documentos,
  exige un override exacto solo por cada hijo cuyo `Parent` cambió, valida que
  cada valor coincida con el `Parent` final (incluida la raíz `None`) y conserva
  para los demás hijos la distinción explícito/inferido. Sus errores redactan
  paths; roundtrips repetidos producen los mismos documentos. Esto no escribe
  archivos, mide hashes de filesystem ni publica el candidato.

  `cargo test --locked -p universo-okf --test padre_persistible`: 16 pasaron,
  1 harness runtime quedó ignorado en la corrida focal. En el harness release,
  1.000 cambios y 1.000 overrides dieron 1.791 ms en Windows
  `x86_64-pc-windows-msvc`, Rust/Cargo 1.88.0. El intervalo mide solo
  `serializar_candidato`; setup, segundo pase determinista y recarga ocurren
  fuera. Es una muestra orientativa, sin SLA; no mide bytes escritos ni costo
  de publicación. Los siete commits y la verificación completa de este cierre
  figuran en el registro de evidencia. U4 ya cubre snapshot/token y CAS interno;
  siguen pendientes `Type`, integrar captura/CAS en apply, el publicador
  filesystem, hashes antes/después de una propuesta rechazada, staging,
  journal/recovery y CLI. La cuota global de evaluación ya está implementada
  en la API core `proponer`; su evidencia se registra en `09aee6e`.

  Comando reproducible del harness ignorado:

  ```text
  cargo test --release -p universo-okf --test padre_persistible runtime_parent_mide_serializacion_de_1000_cambios -- --ignored --nocapture
  ```
- [x] 4 — Captura snapshot raw y CAS antes del parseo: OKF
  `transaccion/{snapshot,inventario,captura}.rs`.
  `snapshot_transaccional` pasa 2/2 casos públicos; la captura pasa 8/8 pruebas
  unitarias y deja ignorado el harness runtime. El contrato usa framing
  etiquetado `FELIX-SNAPSHOT-v1` y política `files-and-directories-v1`, que
  incluye archivos y directorios vacíos, manifiesto/invariantes raw, versión y
  política de motor. `SnapshotToken` es público y opaco; `to_hex()` es una
  revelación explícita, mientras `Debug`/`Display` redactan. Captura y CAS son
  `pub(crate)`: leen una vez a buffers inmutables, comparan la identidad antes
  de parsear y materializan desde esos mismos buffers. La diferencia devuelve
  `stale` fijo/redactado; no se escribe ni publica nada. La captura por ruta
  conserva límite TOCTOU; U6 aporta acceso anclado para root y staging, pero
  la captura/CAS siguen sin integrarse a apply. La CLI todavía no invoca
  `to_hex()`; su opt-in será `--emit-snapshot-token`.

  Harness release verificado en Windows `x86_64-pc-windows-msvc`, Rust/Cargo
  1.88.0:

  | Bundle | Captura | MB/s | Framing | Replay |
  |---|---:|---:|---:|---:|
  | 1 MiB | 6,124 ms | 171,23 | 0,616 ms (10,1%) | 2,552 ms |
  | 10 MiB | 20,274 ms | 517,21 | 6,515 ms (32,1%) | 16,407 ms |
  | 100 MiB | 168,517 ms | 622,24 | 66,216 ms (39,3%) | 147,430 ms |

  Son muestras orientativas, sin SLA. Comando exacto:

  ```text
  cargo test --release -p universo-okf runtime_snapshot_mide_bundles_de_1_10_y_100_mib -- --ignored --nocapture
  ```
- [ ] 5 — Lock single-writer: `crates/universo-okf/src/transaccion/lock.rs` y
  validación de raíz en `raiz.rs`. El lockfile sibling reutilizable mantiene
  un handle `fs4`, responde `busy`, y el OS lo libera al cerrar el handle o el
  proceso. La prueba de contención del registro confirma una adquisición por
  proceso en escenarios de 2 y 8 escritores. Es avance parcial: apply aún no
  integra lock, recovery-before-CAS ni retención hasta cleanup; la inspección
  de raíz por ruta no cierra la sustitución concurrente de ancestros. U6 ya
  aporta una capability anclada para las operaciones de staging; falta integrar
  estas primitivas al flujo de apply.

  Muestra release multiproceso más reciente en este checkout:

  | Writers | Total | Espera total / media / máxima | Intentos | Busy | Máximo de intervalos marcados |
  |---:|---:|---:|---:|---:|---:|
  | 2 | 103,390 ms | 55,635 / 27,817 / 54,842 ms | 7 | 5 (71,4%) | 1 |
  | 8 | 413,403 ms | 1.423,391 / 177,924 / 363,207 ms | 100 | 92 (92,0%) | 1 |

  Una muestra, orientativa y sin SLA. El máximo solo cubre la sección marcada
  de 40 ms; pausas antes/después de los marcadores pueden quedar fuera aunque
  el proceso conserve el guard, así que no mide el tiempo de vida completo del
  lock. Comando exacto:

  ```text
  cargo test --locked --release -p universo-okf --test writer_lock_transaccional runtime_lock_mide_contencion_multiproceso -- --ignored --nocapture
  ```
- [x] 6 — Root anclado y builder de staging:
  `crates/universo-okf/src/transaccion/{anclado,stage}.rs`. El harness principal
  es una prueba unitaria privada en
  `crates/universo-okf/src/transaccion/stage/stage_bundle_transaccional.rs`,
  registrada por `#[cfg(test)] mod stage_bundle_transaccional` en `stage.rs`;
  no existe `tests/stage_bundle_transaccional.rs`. La prueba focal verifica el
  sibling anclado, recibo ligado a identidad y snapshot, recarga, auxiliares
  byte a byte, directorios vacíos, cleanup explícito y conservación del
  token/identidad de root.

  La evidencia complementaria está distribuida entre las pruebas de `raiz.rs`,
  `anclado/tests.rs`, `stage.rs`, `inventario.rs` y
  `tests/writer_lock_transaccional.rs`: aliases convergen a parent+basename
  canónicos; root symlink se rechaza; en Windows,
  `junction_root_se_rechaza_sin_seguirla` confirma el reparse point y conserva
  el canary externo al fallar `LockEscritura::adquirir`, antes de iniciar
  staging. También comprueba como guarda de efectos laterales que no aparezca
  un sibling stage; no demuestra que el materializador haya sido llamado. La
  cobertura del builder está separada en el harness focal. Las pruebas de
  stage/inventario rechazan symlink/junction dentro del árbol, entradas no
  regulares, corrupción, omisiones y extras; la recarga compara inventario y
  candidato.

  `StageAnclado` crea y limpia el sibling mediante handles y capacidad
  explícita; el caller debe retener `LockEscritura` durante cleanup. El lock
  serializa escritores Felix cooperantes y no excluye un proceso externo
  hostil que modifique el filesystem. U6 no conecta captura, staging ni lock a
  `apply`.

  Comando focal correcto:

  ```text
  cargo test --locked -p universo-okf --lib transaccion::stage::stage_bundle_transaccional
  ```

  Rechazo Windows del root junction en la precondición del lock:

  ```text
  cargo test --locked -p universo-okf --test writer_lock_transaccional junction_root_se_rechaza_sin_seguirla
  ```

  Harness release ignorado, medido en Windows: 10.000 auxiliares,
  104.857.600 bytes, 51.158,840 ms, 2,050 MB/s y 1,955 MiB/s. Es una muestra
  orientativa, sin warmup ni distribución estadística; no define SLA. El
  temporizador cubre el pipeline completo de `materializar_staging_anclado`:
  crear y escribir staging, recargar e inventariar el bundle, validar el
  candidato recargado y emitir el recibo. Captura, planificación y construcción
  del candidato ocurren antes del intervalo medido. El harness elimina staging
  y confirma que token e identidad del root siguen intactos.

  ```text
  cargo test --locked --release -p universo-okf --lib transaccion::stage::stage_bundle_transaccional::runtime_staging_anclado_mide_10000_auxiliares_100_mib_exactos -- --ignored --nocapture
  ```

  Evidencia de implementación: anclado `8125508`/`2f55f5e`, staging y recibo
  `f2739bd`/`c144d06`, cleanup `4786235`, harness `43c4f98`, fixes
  `20ebc26`/`e31829d`/`6b4d98d` y test directo Windows `cddc3e3`. El commit de
  cleanup suma 758 inserciones, por encima del objetivo de partición de
  `<400 líneas authored`; se registra aquí sin afirmar que ese objetivo se
  cumplió.
- [ ] 7 — Journal, commit y recovery. La eliminación canónica verificada está
  en `crates/universo-okf/src/transaccion/anclado/journal_temporal/eliminar.rs`
  (`6cf6cfc`), con pruebas en
  `crates/universo-okf/src/transaccion/anclado/journal_temporal/eliminar_tests.rs`
  (`603ae4d`). El runner en
  `crates/universo-okf/src/transaccion/journal/recovery/runner.rs` (`d1b42ec`)
  ejecuta la fila `KeepOld` para `Prepared`/`BackupReady`/`Installed` con root
  `Old`, backup/stage `Absent` y `root_new_validated=false`, y `RemoveJournal`.
  La preparación del backup verificado reabre el sibling anclado en
  `anclado/renombrar.rs` (`9fa50f5`), con pruebas `76b898a` y ajustes de
  portabilidad `701de13`.

  El executor `ejecutar_fila_journal_restore_old` (`7bdbc07`, cobertura de
  conflictos `57a46f0`) restaura solo la fila `root=Absent`, `backup=Old`,
  `stage=Absent`, `root_new_validated=false` → `RestoreOld`; aplica en
  `Prepared`/`BackupReady`/`Installed` y reconcilia el renombrado antes de
  retirar el journal. `2138c1e` lo reexporta como API crate-private desde
  `recovery.rs`. `d1b0ef6` añade cobertura de reconciliación/stage y
  `23e7718` completa la matriz pura. En `1dddce1`/`a4d01e5` se comparte un
  único preflight inicial; `85f7e89`/`7656d15` añaden el dispatcher
  crate-private y sus pruebas, con ajuste de estilo en `0e8f0d4`. En ese corte,
  solo enruta `KeepOld`/`RestoreOld`; ningún caller desde apply/producción
  existe todavía.

  `34b7e0a` clasifica `PartialNew` solo si coincide la identidad durable del
  fingerprint `New` del journal y difiere el digest raw; identidad distinta
  sigue siendo `Unknown`. `428d432` amplía `KeepOld` en `Prepared`,
  `BackupReady` e `Installed` a root `Old`, backup `Absent`, stage `New` o
  `PartialNew` y `root_new_validated=false`. El executor reabre el sibling
  anclado con la identidad del journal (y exige el digest esperado para `New`),
  limpia el stage, reobserva el inventario y solo entonces retira el journal.
  Si la limpieza devuelve un error ambiguo, reobserva: si el stage ya no está,
  puede retirar el journal; si queda un stage parcial, conserva el journal y
  reporta el error. `PartialNew` permite que una nueva ejecución reintente por
  identidad durable aunque el digest ya sea menor. Un txn inválido o una
  identidad distinta fallan antes de mutar los siblings.

  `980a3f0` cubre cleanup ambiguo y reobservación, retry de limpieza parcial
  tras readquirir el lock, identidad distinta y txn inválido; `86bb79b` corrige
  dos préstamos señalados por clippy. CI run `38504988001` pasó tests y fmt, pero
  clippy falló por esos dos préstamos. CI final `38505451293`, sobre
  `86bb79b809625847ed4835385a31068fc97e363e`, pasó en Linux y Windows: Linux
  716 passed/0 failed/6 ignored y Windows 708/0/6, con 38 resúmenes de suites;
  `python -m unittest discover -s tools -p "test_run_software_architecture_demo.py" -v`
  pasó 7 pruebas por sistema operativo. Se vieron las cuatro regresiones
  `stage_presente_falla_cerrado_y_conserva_journal_y_root`,
  `cleanup_ambiguo_reobserva_y_retira_journal_solo_despues_del_stage`,
  `txn_invalido_falla_antes_de_mutar_siblings_y_journal` y
  `keep_old_reintenta_limpieza_parcial_del_stage_tras_reinicio`; fmt, clippy y
  `check_public_boundary.py` también pasaron. Esta evidencia CI no afirma una
  verificación focal final en WSL. El test de retry suelta y readquiere el lock
  en el mismo proceso: no mata ni relanza el proceso ni demuestra crash
  durability.

  Al cierre de `86bb79b`, todavía no existía executor
  `DiscardNewRestoreOld`; `RestoreOld` requería stage `Absent`. Esa función
  seguía crate-private, sin caller desde apply/producción ni publicación
  completa. No se afirmaba atomicidad ni durabilidad ante pérdida de energía:
  el lock coordina escritores Felix cooperantes y TOCTOU por mutaciones
  externas queda fuera del contrato.
  CI histórico `38240021138` pasó solo para `701de13`; `38242930608` pasó para
  SHA `db71e3c` con `RestoreOld` previo al dispatcher; `38246129604` aplica
  solo a `a4d01e5`. `a082467` conserva el estado histórico anterior. Objetivo
  por slice: <400 líneas authored.

  `c888347` amplía `RestoreOld` en `Prepared`, `BackupReady` e `Installed` a
  root `Absent`, backup `Old`, `root_new_validated=false` y stage `Absent`,
  `New` o `PartialNew`. El stage debe corresponder a la identidad durable del
  journal; `New` además exige el digest completo esperado, mientras
  `PartialNew` permite reintentar por identidad aunque el digest haya cambiado
  durante una limpieza previa. Primero limpia el stage y reobserva; solo con la
  fila exacta (root ausente, backup old y stage ausente) restaura el backup y
  retira el journal al final. Si cleanup informa error, reobserva: si el stage
  ya desapareció continúa; si sigue presente, conserva el journal y devuelve
  el error para un nuevo intento después de readquirir el lock. Un estado
  distinto falla cerrado.

  `c763f85` añade `DiscardNewRestoreOld` únicamente para `BackupReady` con
  root `New`, backup `Old`, stage `Absent` y `root_new_validated` false o true,
  y para `Installed` con esa fila y validación false. Renombra root `New` al
  stage del txn verificado, reobserva y reconcilia el rename incluso si reportó
  error; solo acepta el postestado exacto. Luego limpia/reobserva el stage,
  restaura el backup old y retira el journal al final. Si el rename quedó
  aplicado pese al error, continúa; tras cleanup parcial, el journal permite
  reintentar `RestoreOld` por la identidad durable. Root `New` con cualquier
  stage sibling presente y las demás filas no autorizadas fallan cerrado.
  Ocho pruebas/casos de recovery nuevos visibles cubren las filas, cleanup
  ambiguo, retry con una nueva adquisición del lock y el rechazo con canarios
  intactos. Los tests de retry sueltan y readquieren el lock en el mismo
  proceso; no simulan kill/restart ni prueban crash durability. Las pruebas
  focales `cargo test -p universo-okf recovery::` registran 60 passed en
  `c888347` y 65 passed en `c763f85`; runtime harness: N/A, porque las pruebas
  focales ejercitan filesystem real y el rename ambiguo inyectado.

  CI run `38509652854` terminó con éxito para `c763f8527d412778e99eb26c81194f96fdc0c545`:
  Linux 723 passed/0 failed/6 ignored y Windows 715/0/6, con 38 resúmenes de
  suites; las 8 pruebas de recovery nuevas son visibles. El unittest de
  herramientas pasó 7 pruebas por sistema operativo; `fmt`, `clippy` y
  `check_public_boundary.py` también pasaron. U7 sigue parcial: no hay caller
  desde apply/producción ni publicación completa; quedan `KeepValidated`,
  `Committed` y el recovery completo. No se afirma atomicidad ni durabilidad;
  el lock coordina escritores Felix cooperantes y TOCTOU por mutaciones
  externas queda fuera del contrato.

  Contrato de recursos compartido por los inventarios de snapshot/stage/recovery
  y por la carga desde disco con `bundle::cargar_bundle`: 128 MiB raw, 100.000
  entradas, profundidad máxima 128 y 16 MiB acumulados de rutas lógicas. Los
  límites son inclusivos. El inventario cuenta bytes raw de todos los archivos
  regulares; `cargar_bundle` solo suma documentos Markdown efectivamente leídos.
  `calcular_token_snapshot` y `calcular_digest_raw_tree` validan estas cuotas
  sobre el árbol raw antes de ordenar; exceder alguna devuelve
  `snapshot_resource_limit`. El token aplica además una cuota inclusiva e
  independiente de 128 MiB a la suma de `manifiesto_raw` e
  `invariantes_raw`, antes de validar u ordenar el árbol y de iniciar el
  hashing; no se suma al presupuesto separado del árbol raw. `version_motor` y
  `policy_id` siguen sin cota. El framing y los bytes de los tokens v1 que
  permanecen dentro de cuota no cambian. El chequeo mide slices raw ya
  recibidas y no garantiza memoria total del proceso ni del parser.
  `cargar_bundle` aplica además un presupuesto físico independiente de 16 MiB
  para todas las rutas canónicas relativas que retiene: raíces declaradas en el
  índice y directorios únicos en `procesados`. Los aliases de diamante omitidos
  no se cobran dos veces; cada copia retenida sí cuenta. Este costo no sustituye
  ni se suma al presupuesto de rutas lógicas.
  El preflight de limpieza de `StageAnclado` reutiliza solo los límites
  estructurales compartidos (100.000 entradas, profundidad 128 y 16 MiB de
  rutas); no lee ni presupuesta bytes raw de contenido. Si se exceden, devuelve
  `stage_cleanup_resource_limit` antes del primer unlink y conserva la
  capability recuperable del stage para que pueda corregirse y reintentarse.
  Ese preflight evita borrar parcialmente por exceso de cuota, pero no es una
  instantánea atómica frente a procesos externos: el lock coordina escritores
  Felix cooperantes y las comprobaciones de identidad reducen, sin eliminar, la
  carrera de mutación hostil entre comprobación y unlink.
  En inventarios, el root tiene profundidad 0; en un bundle, la profundidad
  cuenta componentes desde la raíz del bundle y cada directorio declarado se
  cuenta como entrada. Las entradas incluyen directorios y archivos bajo las
  raíces declaradas, con presupuesto global para todas ellas. El primer byte o
  elemento adicional devuelve
  `ErrorSnapshot::LimiteInventarioExcedido` o `Regla::LimiteBundleExcedido`,
  respectivamente.

  Ambos lectores de contenido leen como máximo el remanente más un byte, también
  si un archivo crece después de validarse. En bundles, el presupuesto raw solo
  suma documentos Markdown efectivamente leídos; los nombres ignorados no
  consumen bytes. `parsear_bundle`, que recibe documentos ya materializados en
  memoria, queda fuera de este límite. Los nombres por directorio se pueden
  retener para preservar el orden lexical: los límites restringen el input
  lógico, no garantizan un RSS exacto ni un timeout duro. `LimiteBundleExcedido`
  amplía el enum público y exhaustivo `Regla`; los consumidores con `match`
  exhaustivo deben manejarlo.
- [ ] 8 — CLI simulate: `crates/universo-cli/src/propose.rs`.
  Test `crates/universo-cli/tests/propose_transaccional.rs`
  (`cargo test -p universo-cli --test propose_transaccional`) ejecuta el binario,
  comprueba token omitido por defecto/opt-in, hashes intactos y RED canaries en
  JSON/texto: path, label, valor Ext, edge id, fórmula, delta y token no salen.
  Runtime: 1/10/100 MiB, latencia; rollback: simulate no escribe. Objetivo por
  slice: <400 líneas authored.
- [ ] 9 — CLI apply: `crates/universo-okf/src/transaccion/aplicar.rs` y
  `crates/universo-cli/src/propose.rs`.
  Test `crates/universo-cli/tests/apply_transaccional.rs`
  (`cargo test -p universo-cli --test apply_transaccional`) verifica comando
  completo, recovery-before-CAS, re-evaluación, stale, rechazo sin cambios y
  RED canaries en JSON/texto sin path, label, valor Ext, edge id, fórmula,
  delta ni token de entrada en output. Runtime: dos writers/10 MiB, ms y
  busy/stale; rollback:
  restaurar old si no llega a installed. Objetivo por slice: <400 líneas authored.
- [ ] 10 — Docs/ejemplo: `README.md`, `docs/04-recuperacion.md` y
  `docs/plan/04-operacion-transaccional.md`.
  Test `crates/universo-cli/tests/docs_transaccional.rs`
  (`cargo test -p universo-cli --test docs_transaccional`) contrasta comandos
  y recovery documentados. Runtime: ejemplo temporal, ms/bytes; rollback:
  borrar bundle temporal y verificar root original. Objetivo por slice:
  <400 líneas authored.

## Entregables

1. Modelo canónico de delta: átomos a añadir y retirar.
2. Detección y diagnóstico de conflictos funcionales de tipo, etiqueta,
   padre e incidencia al combinar el delta con el estado vigente.
3. Aplicación sobre staging sibling y commit `root -> backup`, `stage -> root`
   con recovery por journal.
4. API core `proponer(...) -> Result<ResultadoPropuesta, ErrorPropuesta>` y
   API OKF/CLI `aplicar(..., expected_snapshot, ...)` con CAS/publicación.
5. Lock single-writer, recovery-before-CAS, revalidación stale y publicación
   solo después de `admit`.
6. CLI `propose` de solo simulación por defecto y apply con token opt-in.
7. Registro local del snapshot evaluado sin exposición por defecto.

## Criterios de aceptación

- [x] Para las operaciones soportadas por Unidad 0/1, un delta válido produce
  el candidato esperado y no modifica el snapshot de entrada.
- [ ] La transacción implementa `Type`; sigue siendo una operación técnica no
  soportada por `proponer`.
- [x] El candidato core construye cambios de `Parent` y `HasLab` de nodo; la
  serialización en memoria de `Parent` queda limitada a `layout.parent_key` y
  `serializar_candidato`.
- [x] Root symlink/junction/reparse se rechaza; una raíz válida se canoniza
  una vez y todos los artefactos sibling se derivan del parent+basename
  canónicos, de modo que aliases convergen.
- [x] `add`/`remove` se comportan como parche transaccional explícito, sin
  atribuir al delta minimalidad o prioridad de AGM/R.
- [x] `add`/`remove` se canonicalizan con orden determinista y el solapamiento
  se rechaza.
- [x] El CAS interno compara la identidad raw esperada antes del parseo y un
  cambio devuelve `stale` fijo/redactado, sin revelar los tokens.
- [x] Los retiros Edge/HasLab se resuelven a handles del snapshot antes de
  mutar; las removals por `(origen, tipo, destino)` compactan supervivientes
  en orden snapshot. Adds canónicos exigen ordinales finales contiguos.
- [x] `Ext` valida predicado declarado/dominio; cambiar valor para
  `(predicado, nodo)` exige remove+add.
- [x] `HasLab` es funcional para todo tipo de sujeto; etiquetas iguales en
  sujetos distintos se permiten salvo invariante.
- [x] Cambiar `HasLab` de una arista exige remove+add y mantiene la asociación
  por handle durante la compactación.
- [x] Cambiar `HasLab` de nodo exige remove+add.
- [x] La serialización en memoria de un cambio `Parent` requiere `parent_key`
  y un override cuyo `BundlePath` coincida exactamente con el candidato; `None`
  representa raíz explícita. Sin la clave, el cambio devuelve
  `parent_not_persistable`.
- [x] La salida separa error técnico de veredicto funcional y redacta valores
  privados por defecto.
- [ ] `universo-core::proponer(...)` no hace I/O; `universo-okf`/CLI
  `aplicar(..., expected_snapshot, ...)` conecta recovery, CAS, re-evaluación
  y publicación bajo lock. La primitiva U4 aún no está integrada en `apply`.
- [x] Los conflictos funcionales se distinguen de las violaciones de
  invariantes y se informan sin publicar valores privados por defecto.
- [x] Un error al construir candidato no muta la estructura snapshot en memoria.
- [ ] Un delta rechazado por la transacción completa no cambia los archivos.
- [ ] Simulación no escribe y omite el token; `--emit-snapshot-token` lo
  entrega solo bajo solicitud. Apply usa el comando completo de arriba y
  reevalúa/revalida bajo lock.
- [ ] El lock advisory/exclusivo usa un file handle del lockfile sibling
  reutilizable, se libera al morir el proceso y recovery corre siempre después
  de adquirir lock y antes del stale recheck. Testea crash/restart con lock.
- [ ] Un writer mantiene el lock desde recovery hasta cleanup; un concurrente
  resulta `busy` o `stale`, nunca last-write-wins.
- [ ] Staging de apply rechaza cualquier symlink/junction/reparse dentro de
  root; preserva archivos/directorios auxiliares regulares byte a byte y
  recarga/compara el candidato.
- [ ] CLI privacy tests usan RED canaries de path, label, valor Ext, edge id,
  fórmula, delta y snapshot token; los secretos no aparecen por defecto ni
  se refleja el token de entrada en apply. El token solo sale con opt-in.
- [ ] Recovery conserva old para `prepared`/`backup_ready`; `installed` sin
  backup conserva new solo si root valida el candidato; `committed` limpia
  restos. Backup se elimina antes que journal y el journal al final. Testea
  crash/restart durante cleanup. No se afirma visibilidad continua ni
  recuperación garantizada ante pérdida de energía.
- [x] El mismo delta y snapshot producen el mismo candidato para las
  operaciones soportadas; los retiros de Edge son independientes del orden de
  entrada.
- [x] El mismo delta contra el mismo estado produce el mismo veredicto
  funcional.
- [ ] Un delta contra un estado diferente se detecta y no se aplica a ciegas
  en el flujo completo de apply; U4 solo entrega el CAS interno.
- [ ] Decidir optimización incremental con mediciones reproducibles y
  suficientes; las muestras actuales son orientativas y no definen SLA.

## Prueba de seguridad central

1. Calcular hashes del bundle.
2. Proponer una contradicción conocida.
3. Confirmar `reject`.
4. Volver a calcular hashes.
5. Exigir igualdad byte a byte.

## Verificación prevista

```powershell
cargo test -p universo-core --test delta_transaccional
cargo test -p universo-okf --test delta_json_v1
cargo test -p universo-core --test candidato_delta
cargo test -p universo-core --test delta_aristas
cargo test -p universo-core --test ext_transaccional
cargo test -p universo-okf --test ext_roundtrip
cargo test -p universo-core --test conflictos_delta
cargo test -p universo-okf --test padre_persistible
cargo test -p universo-okf --test snapshot_transaccional
cargo test -p universo-okf --test writer_lock_transaccional
cargo test --locked -p universo-okf --lib transaccion::stage::stage_bundle_transaccional
cargo test --locked -p universo-okf --test writer_lock_transaccional junction_root_se_rechaza_sin_seguirla
cargo test --locked -p universo-okf --lib elimina_coincidencia_y_confirma_ausencia_idempotente
cargo test --locked -p universo-okf --lib prepared_y_backup_ready_eliminan_solo_el_journal_y_conservan_root
cargo test --locked -p universo-okf --lib installed_root_old_sin_vecinos_elimina_journal_y_conserva_root
cargo test --locked -p universo-okf --lib committed_root_old_sin_vecinos_falla_y_conserva_estado
cargo test --locked -p universo-okf --lib restore_old_restaura_backup_en_las_tres_fases_y_conserva_identidad_y_bytes
cargo test -p universo-cli --test propose_transaccional
cargo test -p universo-cli --test apply_transaccional
cargo test -p universo-cli --test docs_transaccional
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-10-11 | `1074902ec4687af435f2772de77febd161a59e68`, `a54688ca274fa46f1a6a438df7988c13ef1176de` | Unidad 0: 6 casos core y 5 OKF; workspace/clippy pasan en HEAD de implementación; fmt/frontera, enlaces locales y diff pasan tras editar los planes. | Aprobada; la prueba de carga no mide µs/op. |
| 2026-10-11 | `2a49aed5c0c957e87abced7d0db0a2cd6c3d1677` | Candidato aislado y retiro de aristas en core; 364 añadidas/1 borrada (365 authored); `candidato_delta` 3/3. | Aporte parcial a Unidad 1. |
| 2026-10-11 | `19d60627d0b2483a2dc5f266ceb360554f2781be` | Pruebas de retiros en lote; 203 líneas añadidas; `delta_aristas` 5/5. | Aporte parcial a Unidad 1. |
| 2026-10-11 | `f42cdb929d45ffac95350cbefbc59d5d8ff042fc` | Aislamiento y rechazo fail-closed; 170 añadidas/18 borradas (188 authored). | Aporte parcial a Unidad 1. |
| 2026-10-11 | `9afb46405d8285259bfa621d570ae4c58cc893db` | Altas `Edge`/`HasLab` de arista; 166 añadidas/8 borradas (174 authored). | Aporte parcial a Unidad 1. |
| 2026-10-11 | `fa710d474ab8abc22f0779437d283b6ad12331c6` | Casos de altas, no-op, grupos y mezcla remove/add; 253 líneas añadidas. | Aporte parcial a Unidad 1. |
| 2026-10-11 | `826b3816f5b257707664567ebbee39ffe3be9d35` | Reemplazo de etiqueta por identidad final; 274 añadidas/24 borradas (298 authored). | Aporte parcial a Unidad 1. |
| 2026-10-11 | `00c6b9690570de33e4853a78998127eb9b10ac38` | Aplicación transaccional de `Ext`; 342 añadidas/18 borradas (360 authored); `ext_transaccional` 10/10. | Unidad 1 cerrada en conjunto. |
| 2026-10-11 | `1e7aa65aa641c80a0ad0ab9fed2eb7ccd4cb81d4` | Roundtrips transaccionales de `Ext`; 165 añadidas/2 borradas (167 authored); `ext_roundtrip` 3/3. | Unidad 1 cerrada en conjunto. |
| 2026-10-11 | `3b148913dee4af1d3cbfcbc7ae5dbf7769361833` | Clasificación de retiros `Ext`; 182 añadidas/24 borradas (206 authored). | Unidad 1 cerrada en conjunto. |
| 2026-10-11 | `59dc0dd23fca6da6ced96f06de709a7b66489e49` | Prueba release de runtime para `Ext` y grupos `Edge`; 213 líneas añadidas. | Dos muestras únicas orientativas; no constituyen SLA. |
| 2026-10-11 | `52234caf2dd8811094a379fc8184d4e228b43ee3` | Detector de conflictos funcionales sobre estado final; 149 añadidas/1 borrada (150 authored/churn). | Aporte parcial a Unidad 2. |
| 2026-10-11 | `b5c027a16006133fc5dec7dec583928b192b9005` | Cobertura del detector; 342 líneas añadidas (342 authored/churn). | Aporte parcial a Unidad 2. |
| 2026-10-11 | `86f16d3a95d3f9622eebc9eabf4ce9aa8bcb096c` | Normalización de etiquetas Edge; 229 añadidas/42 borradas (271 authored/churn). | Aporte parcial a Unidad 2. |
| 2026-10-11 | `ec29ea6985907fcc6992e47753325e7fec8d5d45` | Agregación por identidad final; 211 añadidas/74 borradas (285 authored/churn). | Aporte parcial a Unidad 2. |
| 2026-10-11 | `875915ca82fbc1449c731734711f0203a0521e83` | Casos límite y redacción; 321 añadidas/1 borrada (322 authored/churn). | Aporte parcial a Unidad 2. |
| 2026-10-11 | `dfc359f9a2339b14cedd6c16c35a550446746461` | Ajuste de métricas runtime; 3 añadidas/3 borradas (6 authored/churn). | Runtime final: 1.000 conflictos, 3 clases resumen; muestra única orientativa. |
| 2026-10-11 | `64206882d7cee4c5c9262c056e9edb54270cbdba` | Construcción core de `Parent` y `HasLab` nodo; 310 añadidas/50 borradas (360 authored/churn). | Aporte a Unidad 2; sin persistencia de `Parent`. |
| 2026-10-11 | `9ccf8b3d51a018dacf6034c5dc87d884d091a3d7` | Pruebas de `Parent`/`HasLab` nodo; 374 líneas añadidas (374 authored/churn). | Aporte a Unidad 2. |
| 2026-10-11 | `d382fd5b4037b899ba6969aaae2bca5aa185a9b2` | Simplificación de validación; 1 añadida/3 borradas (4 authored/churn). | Aporte a Unidad 2. |
| 2026-10-11 | `3d40561fe717ba9f0e10349eead1e1639457dbf7` | Aserciones de transacción `Parent`/nodo; 112 añadidas/23 borradas (135 authored/churn). | Aporte a Unidad 2. |
| 2026-10-11 | `7d50be550bd093e24b89be710c6d79f05171da9a` | Orden de comprobación de ciclos `Parent`; 7 añadidas/12 borradas (19 authored/churn). | Aporte a Unidad 2. |
| 2026-10-11 | `595f0e2bf6fece6db319305890804f6245b03d43` | Clasificación de errores y preflight de propuesta; 243 añadidas/44 borradas (287 authored/churn); `proponer_preflight` 4/4. | Aporte a Unidad 2. |
| 2026-10-11 | `e6b4f60f2d623d60445396969d9150d1f61e311b` | API core `proponer`; 223 añadidas/1 borrada (224 authored/churn). | Aporte a Unidad 2. |
| 2026-10-11 | `15ed4b83af1c25883c83910cd60333f47c2c6748` | Pruebas de propuesta y privacidad; 373 líneas añadidas (373 authored/churn). | Aporte a Unidad 2. |
| 2026-10-11 | `9607a9a8191bbba3ae334178f5a61147359dc48e` | Clasificación técnica de `HasLab` incompleto; 49 añadidas/22 borradas (71 authored/churn). | Aporte a Unidad 2. |
| 2026-10-11 | `12bcc314918a4b4738b582475a3e0b1d33ed5469` | Precedencia técnica y casos `HasLab`; 185 añadidas/37 borradas (222 authored/churn). | Aporte a Unidad 2. |
| 2026-10-11 | `eb497cc2e60db1dd9e2df7397797855d142ce663` | Privacidad/agregación de `proponer`; 91 añadidas/7 borradas (98 authored/churn). | Unidad 2 cerrada en core. |
| 2026-10-11 | `010ffaf976e8366f9f724989201b5f9cff488bec` | Añade opt-in `layout.parent_key` y carga configurada de `Parent`. | Inicio de Unidad 3. |
| 2026-10-11 | `a79075172ce1a48e000eadfddde7a86cf56705ca` | Cubre semánticas de carga de `Parent` explícito e inferido. | Evidencia focal de Unidad 3. |
| 2026-10-11 | `9b6243172ea591e7577e56dcff31535917030eb5` | Valida la gramática de `parent_key` en frontmatter. | Hardening del opt-in. |
| 2026-10-11 | `b6dbf17467b97175c4380ad3f05208c7883f2b08` | Añade roundtrip del frontmatter `Parent` explícito. | Serialización de nodo y recarga. |
| 2026-10-11 | `0ac65d8a7f48d1f7f1376c777c3e3ae2db785ed5` | Valida serialización en memoria de candidatos y overrides `Parent`. | Núcleo funcional de Unidad 3. |
| 2026-10-11 | `7dc774726fe1ef89e06110af067cc2d2c37bff6e` | Cubre casos límite del serializador de candidatos `Parent`. | Complementa roundtrip, errores y redacción. |
| 2026-10-11 | `27289a9a14d5fba8bc0e7b61726e58069bf5bd39` | Añade harness release de serialización `Parent`. | 1.791 ms para 1.000 cambios; solo mide `serializar_candidato`. |
| 2026-10-11 | `36e130f` | Framing etiquetado SHA-256 `FELIX-SNAPSHOT-v1` y token opaco; 362 líneas añadidas. | Aporte U4; bajo 400 líneas authored/churn. |
| 2026-10-11 | `3601ac4` | Inventario raw de archivos y directorios; 234 líneas añadidas. | Aporte U4; bajo 400 líneas authored/churn. |
| 2026-10-11 | `dbb0d27` | Digest de referencia para el framing v1; 4 líneas añadidas. | Aporte U4; bajo 400 líneas authored/churn. |
| 2026-10-11 | `cfa7a2a` | Captura inmutable y parseo desde los buffers capturados; 367 añadidas/5 borradas (372 authored/churn). | Aporte U4; bajo 400 líneas authored/churn. |
| 2026-10-11 | `540fba9` | Contrato público determinista, opaco y redactado del token; 87 líneas añadidas. | Aporte U4; bajo 400 líneas authored/churn. |
| 2026-10-11 | `dc87001` | Reutiliza buffers para aliases de case; 271 añadidas/15 borradas (286 authored/churn). | Aporte U4; bajo 400 líneas authored/churn. |
| 2026-10-11 | `bf39602` | Harness release de captura, framing y replay; 116 líneas añadidas. | Aporte U4; bajo 400 líneas authored/churn. |
| 2026-10-11 | `1c3df9d` | CAS del snapshot raw antes de parsear; pruebas `stale` y redacción; 360 añadidas/10 borradas (370 authored/churn). | Cierra U4 como primitiva interna; bajo 400 líneas authored/churn. |
| 2026-10-11 | `fdfdb0b` | Valida raíces de transacción y deriva siblings canónicos; 162 líneas añadidas. | Avance parcial U5; la carrera de reemplazo concurrente queda para U6. |
| 2026-10-11 | `de514b9` | Lock single-writer con file handle y pruebas; 322 añadidas/1 borrada (323 authored/churn). | Avance parcial U5; el lock no está integrado a apply/recovery. |
| 2026-10-11 | `de86ad6` | Resuelve aliases de raíz según semántica del filesystem; 247 añadidas/12 borradas (259 authored/churn). | Avance parcial U5; inspección por ruta aún no ancla ancestros. |
| 2026-10-11 | `506224c` | Aclara el límite de carrera en validación de raíz; 2 añadidas/2 borradas (4 authored/churn). | Avance parcial U5; el root anclado se diseña en U6. |
| 2026-10-11 | `61b22fd` | Harness de contención de procesos escritores 2/8; 317 añadidas/1 borrada (318 authored/churn). | Aporte parcial U5; una muestra orientativa. |
| 2026-10-11 | `2475dc3` | Califica la métrica como intervalo marcado; 11 añadidas/5 borradas (16 authored/churn). | Aporte parcial U5; no mide la vida completa del guard. |
| 2026-10-11 | Verificación en HEAD `f42cdb9` | Workspace, clippy, fmt y frontera pública pasan en el HEAD de código; enlaces, frontera, fmt y diff pasan tras editar los planes. | Aprobada; no incluye medición de rendimiento. |
| 2026-10-11 | Verificación en HEAD `826b381` | `delta_aristas` 14/14, `candidato_delta` 3/3, workspace, clippy, fmt y frontera pública pasan; diff y enlaces pasan tras editar los planes. | Aprobada; runtime sigue pendiente. |
| 2026-10-11 | Verificación en HEAD `59dc0dd` | `candidato_delta` 3/3, `delta_aristas` 14/14, `ext_transaccional` 10/10, `ext_roundtrip` 3/3; workspace, clippy, fmt y frontera pública pasan. Tras editar, enlaces locales y `git diff --check` pasan. | Unidad 1 cerrada; fase 04 sigue en curso. |
| 2026-10-11 | Verificación en HEAD `dfc359f` | `conflictos_delta` 13/13; workspace, clippy, fmt, frontera pública, enlaces locales y `git diff --check` pasan. | Avance parcial de Unidad 2; no se cierra veredicto ni invariantes. |
| 2026-10-11 | Verificación en HEAD `eb497cc` | `nodo_parent_transaccional` 13/13, `conflictos_delta` 13/13, `proponer_preflight` 4/4, `proponer_transaccional` 10/10; workspace, clippy, fmt y frontera pública pasan; enlaces locales y diff tras editar. | Unidad 2 cerrada en core; persistencia Parent y resto de fase 04 pendientes. |
| 2026-10-11 | Verificación en HEAD `27289a9` | `cargo test --locked -p universo-okf --test padre_persistible` 16 passed/1 ignored; `cargo test --locked --workspace --all-targets`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` y `python tools/check_public_boundary.py` pasan. | Unidad 3 cerrada para carga y serialización en memoria; apply/publicación siguen pendientes. |
| 2026-10-11 | Verificación integrada en HEAD `2475dc3` (sobre `61b22fd`) | Workspace 377 passed/5 ignored; `snapshot_transaccional` 2/2; captura 8/8 y harness 1 ignored; clippy, fmt y `check_public_boundary.py` pasan. `2475dc3` solo califica métricas, sin agregar pruebas. | U4 cerrada como primitiva interna; U5 conserva avance parcial. |
| 2026-10-11 | Harness U5 en HEAD `2475dc3` | 2 writers: total 103,390 ms, espera 55,635/27,817/54,842 ms, 5 busy de 7 (71,4%); 8 writers: total 413,403 ms, espera 1.423,391/177,924/363,207 ms, 92 busy de 100 (92,0%). Máximo de intervalos marcados: 1 en ambos escenarios. | Una muestra release orientativa; los marcadores no cubren toda la vida del lock. |
| 2026-10-11 | `43c4f98`, `cddc3e3`; verificación en HEAD `cddc3e3` | Harness privado `stage_bundle_transaccional`: 10.000 auxiliares, 104.857.600 bytes, 51.158,840 ms, 2,050 MB/s y 1,955 MiB/s. El test Windows confirma root junction/reparse y canary intacto al rechazar el lock, antes del staging; el test focal, workspace, clippy, fmt y frontera pública pasan. | U6 cerrada para root anclado/staging; no conecta a apply. Runtime orientativo, sin SLA. |
| 2026-10-11 | `46877ed`, `a5d9371`, `5f66ad1`, `6f72af2`, `67e8908` | Observer crate-private read-only de root/backup/stage bajo lock y parent anclado; usa identidad y digest raw, representa `root_new_validated=false` y no decide ni muta artefactos. `recovery::observer` 7/7 y `sibling_read_only_tests` 5/5 en Windows, incluidos parent mismatch, mismo basename bajo otro parent, identidad/digest mismatch, ausencia, tipo y junction con canary. Clippy workspace `-D warnings` compila todos los targets en Windows y Linux; incluye regresión FIFO Unix. Workspace: 227 OKF passed/2 ignored. | Avance parcial U7; conserva Journal v1 y no integra apply/recovery ni restart. |
| 2026-10-12 | `6cf6cfc`, `603ae4d`, `d1b42ec`, `a082467` | Estado histórico en `a082467`: `eliminar.rs` verifica identidad y registro canónico; `recovery/runner.rs` solo ejecuta `Prepared`/`BackupReady` + root `Old` + backup/stage ausentes → `RemoveJournal`. Matriz local Windows+WSL aprobada; GitHub Actions run `38232932504` completado con éxito. | Avance parcial U7 en ese commit; `Installed`/`Committed` con root `Old` se rechazaban. |
| 2026-10-12 | `c5a09cc` | `recovery/runner.rs` admite también `Installed` únicamente para root `Old`, backup/stage `Absent` y `root_new_validated=false` → `KeepOld` + `RemoveJournal`; `root=New` no validado, `root=Unknown` y `Committed` fallan cerrado. `runner_tests.rs`: `installed_root_old_sin_vecinos_elimina_journal_y_conserva_root` y `committed_root_old_sin_vecinos_falla_y_conserva_estado`. | Avance parcial U7; las demás filas, integración a apply, cleanup/restart completo y harness runtime siguen pendientes. |
| 2026-10-12 | `9fa50f5`, `76b898a`, `701de13`, `7bdbc07`, `57a46f0`, `2138c1e`, `d1b0ef6`, `23e7718` | Reapertura verificada del backup sibling anclado y pruebas de parent/reparse; executor crate-private `RestoreOld` para `Prepared`/`BackupReady`/`Installed` con root `Absent`, backup `Old`, stage `Absent`, `root_new_validated=false`, renombrado reconciliado y journal retirado. Reexport interno en `recovery.rs`; cobertura de conflictos, stage y matriz pura. CI run `38240021138` pasó solo en la base `701de13`. | Estado en `23e7718`: avance parcial U7; dispatcher y caller de apply aún no existían. Cleanup de stage, `DiscardNewRestoreOld`, validación de `New`, `Committed` y recovery completo seguían pendientes. |
| 2026-10-12 | CI `38242930608`, SHA `db71e3c` | Run completado con éxito para el estado con runner `RestoreOld` previo al dispatcher. | Evidencia histórica del runner anterior; no valida el dispatcher final. |
| 2026-10-12 | `1dddce1`, `a4d01e5`, `85f7e89`, `7656d15`, `0e8f0d4` | Preflight compartido una sola vez y dispatcher crate-private que enruta solo `KeepOld`/`RestoreOld`; no hay caller desde apply/producción. CI run `38246129604` pasó para la base `a4d01e5` únicamente; no acredita el dispatcher final. | U7 parcial; cleanup de stage, `DiscardNewRestoreOld`, validación de `New`, `Committed`, recovery completo y conexión desde apply siguen pendientes. |
| 2026-10-12 | `56a6516` | Ambos calculadores de snapshot aplican antes de ordenar las cuotas inclusivas del árbol raw; `snapshot_resource_limit` al excederlas. Los tokens v1 admitidos conservan sus bytes. Test focal `cargo test --locked -p universo-okf transaccion::snapshot::tests::presupuesto_del_arbol`: 2/2. Runtime: N/A, documentación de una función pura. | U8 parcial; `manifiesto_raw`, `invariantes_raw`, `version_motor` y `policy_id` aún no están cubiertos. |
| 2026-10-12 | `02c7adde` | `calcular_token_snapshot` suma `manifiesto_raw` e `invariantes_raw` con cuota inclusiva independiente de 128 MiB antes de validar/ordenar el árbol y de iniciar el hashing; conserva las cuotas separadas del árbol. Los tokens v1 dentro de cuota conservan sus bytes; `version_motor` y `policy_id` siguen sin cota. `presupuesto_de_configuracion`: 3/3; `presupuesto_del_arbol`: 1/1. Runtime: N/A, cambio en calculadores puros. | U8 parcial; no impone cota total de memoria ni del parser. Sigue sin caller desde apply/CLI. La captura externa por ruta queda acotada en `2357a82`, pero falta integrar su suministro a `ConfigRawCapturada` antes del lock. |
| 2026-10-13 | `2357a82` | La captura privada por ruta limita durante la lectura la suma de `manifiesto_raw` e `invariantes_raw` a 128 MiB inclusivos, con cuota independiente del árbol raw. Reutiliza los buffers ya inventariados del root; para fuentes externas lee como máximo el remanente más un byte centinela. Una ruta externa absoluta normalizada se lee una vez y se cobra por cada campo lógico que la use. Test focal `cargo test --locked -p universo-okf transaccion::captura::tests::`: 24 passed/1 ignored. El mismo comando ejercita archivos reales en directorios temporales; no hay harness release independiente para esta ruta. | U8 sigue parcial. No limita `version_motor` ni `policy_id`, no promete límite de RSS/parser, deduplicación por identidad física ni atomicidad ante modificaciones hostiles del filesystem; la captura por ruta conserva TOCTOU. Pendientes el contrato que crea/suministra `ConfigRawCapturada` antes del lock y el caller de apply/CLI. Rollback de esta actualización documental: revertir solo `CHANGELOG.md` y los dos planes; el comportamiento está aislado en `2357a82`. |
| 2026-10-13 | `63e59d1` | El CAS bajo lock valida la suma combinada de `manifiesto_raw` e `invariantes_raw` antes de abrir root; la captura anclada también prevalida antes de clonar/inventariar. Exceso: `snapshot_resource_limit` redactado, root/canary intacto y guard retenido. Dentro de cuota, CAS precede provenance/parseo. `ConfigRawCapturada` ya llega materializada, así que no se limita la asignación del constructor. Focal Windows `cargo test --locked -p universo-okf transaccion::captura::tests::`: 26 passed/1 ignored. Los casos usan filesystem real en tempdirs y comprueban el root/canary y la retención del lock. Matriz local workspace: Windows 691/6 y WSL 699/6 (passed/ignored); sin CI remoto publicado para este SHA. | U8 parcial: contrato/proveedor con lectura acotada antes del lock, callers de apply/CLI y límites de `version_motor`/`policy_id` pendientes; no se afirma límite total de memoria/parser ni atomicidad ante cambios hostiles. Rollback documental: revertir este commit, limitado a `CHANGELOG.md` y los dos planes; el comportamiento permanece en `63e59d1`. |
| 2026-10-13 | `4b94261` | `ConfigRawCapturada::nueva` ahora devuelve `Result` y hace cumplir la cuota combinada de 128 MiB como invariante. Convierte ambas entradas a `Arc<[u8]>` antes de revisar la suma; el conteo es lógico por campo, incluso si se comparte el mismo `Arc`. No limita lectura, asignación/materialización previa, RSS ni I/O antes del lock. El preflight consumidor de `63e59d1` sigue validando antes de abrir el root bajo lock y antes de inventariar. Focal local Windows `cargo test --locked -p universo-okf transaccion::captura::tests::`: 29 passed/1 ignored. Matriz local workspace: Windows 694/6 y WSL 702/6 (passed/ignored); no hay CI remoto publicado para este SHA. | U8 sigue parcial: proveedor con lectura acotada antes del lock, callers de apply/CLI y límites de `version_motor`/`policy_id` pendientes. La captura privada por ruta conserva su límite durante lectura, independiente de esta invariante. No se afirma una cota total de memoria/parser ni atomicidad ante cambios hostiles. Rollback documental: revertir solo `CHANGELOG.md` y los dos planes; el comportamiento del constructor permanece en `4b94261`. |
| 2026-10-13 | `09aee6e8567de43d1165e6d140e47f1c72335361` | `proponer` usa una cuota agregada de 1.000.000 de unidades de evaluación para todas las fórmulas expandidas. La evaluación empieza después de construir el candidato, detectar conflictos y completar la expansión; las 100.000 instancias expandidas tienen una cuota separada. Agotamiento: `evaluation_budget_exhausted`, sin candidato ni resultado parcial. `evaluar` sigue limitado por llamada y el `check` CLI baseline por fórmula. Matriz local workspace Windows 698/6 y WSL 706/6 (passed/ignored); sin CI remoto publicado para este SHA. | Fase 04 sigue en curso: U7 continúa como foco y U8 permanece parcial. La cuota incluye el preflight de cada fórmula, pero no cubre la preparación previa al contador: construcción del candidato, detección de conflictos y expansión; tampoco acota memoria ni tiempo total. |
| 2026-10-13 | `609936aa9853ede0c2953a777299485bc3210852` | CAS privado por ruta con configuración externa: el match conserva nombres, tipos y bytes del árbol, bytes externos y no crea lockfile sibling. Con guard de escritura del caller retenido, otro escritor sigue `Busy`. Si cambia la configuración externa inválida, el CAS retorna `stale` redactado antes del parseo y sin cambiar contenido o namespace. Focal `cargo test --locked -p universo-okf transaccion::captura::tests::`: 32 passed/0 failed/1 ignored. Matriz local workspace Windows 701/6 y WSL 709/6 (passed/ignored); no hay CI remoto publicado para este SHA. Runtime: pruebas con filesystem real en directorios temporales; no hay harness release independiente. Rollback documental: revertir solo `CHANGELOG.md` y los dos planes; el comportamiento de captura ya está en `609936a`. | U8 sigue parcial. No hay escrituras intencionales de contenido/namespace; las pruebas no cubren atime, permisos, metadatos ni identidad. La captura por ruta conserva TOCTOU y no garantiza atomicidad ante cambios hostiles. El helper es crate-private, sin caller de apply/CLI ni simulación pública. |

| 2026-10-13 | `eef28f55c4009248d74cc82c2188e310f3ce8e41` | Amplía una prueba existente del dispatcher a seis combinaciones de `KeepOld`/`RestoreOld` en `Prepared`/`BackupReady`/`Installed`, con una observación inicial por ejecución. `KeepOld` comprueba que se retire solo el journal esperado; `RestoreOld` recupera el fingerprint old y retira journal/backup/stage. | U7 sigue parcial; dispatcher crate-private, sin caller de apply/producción. No es un test focal nuevo. |
| 2026-10-13 | `a3cc3c9c2e5d166f066d6f7bfccf6cdec9214211` | Añade un test privado del CAS por ruta con journal sibling: match/stale comparan inventarios observados de root, configuración y parent, bytes del journal y `ArtifactFingerprint` del root. No invoca el dispatcher ni hay conteo focal registrado para este SHA. | U8 sigue parcial; CAS privado, sin caller de apply/CLI ni simulación pública. No se afirman atime ni atomicidad. |
| 2026-10-13 | `51cd5c4fbf9607dffac1825a9739cac9e92d97e2` | Extiende la prueba del dispatcher con inventario recursivo: rutas, tipos y bytes de archivos normales; para el lock abierto, solo presencia, tipo y longitud. `RestoreOld` compara contra el modelo backup→root y retiro del journal. Matriz workspace independiente: Windows 702 passed/0 failed/6 ignored (37 suites); WSL Ubuntu 710/0/6 (37 suites); fmt y clippy pasan en ambos. Conteos workspace, no focales. | U7 sigue parcial y sin caller productivo; no se garantiza atime, permisos, metadatos ni atomicidad ante cambios hostiles. |
| 2026-10-14 | `34b7e0a`, `428d432`, `980a3f0`, `86bb79b` | Corte histórico: `KeepOld` limpia stage `New`/`PartialNew` con root `Old` y backup `Absent` en tres fases; identidad durable, reobservación antes del journal y retry tras cleanup parcial. Cuatro regresiones visibles; CI `38505451293` pasa Linux (716/0/6) y Windows (708/0/6), 38 resúmenes, unittest de herramientas 7/OS, fmt, clippy y frontera pública. `38504988001` tenía tests/fmt verdes y fallo clippy por dos préstamos, corregidos en `86bb79b`. | U7 parcial, sin caller apply/producción ni publicación completa; en ese corte `RestoreOld` requería stage `Absent` y `DiscardNewRestoreOld` seguía pendiente. No se afirma atomicidad/durabilidad; writers cooperantes y TOCTOU externo fuera del contrato. |
| 2026-10-14 | `c888347`, `c763f85`; CI `38509652854` | `RestoreOld` limpia/reobserva stage identificado como `New`/`PartialNew` antes de restaurar old; `DiscardNewRestoreOld` cubre solo las filas exactas de `BackupReady`/`Installed`, con rename y cleanup reconciliados por reobservación. Ocho pruebas/casos de recovery nuevos visibles; foco `recovery::`: 60 passed en `c888347`, 65 en `c763f85`. Los tests de retry sueltan y readquieren el lock en el mismo proceso, sin kill/restart. CI para `c763f8527d412778e99eb26c81194f96fdc0c545`: Linux 723/0/6, Windows 715/0/6 (38 resúmenes); tools 7/OS, fmt, clippy y frontera pública pasan. | U7 sigue parcial: sin caller apply/producción ni publicación completa; `KeepValidated`, `Committed` y recovery completo pendientes. Runtime harness: N/A; las pruebas focales ejercitan filesystem y rename ambiguo inyectado, pero no prueban crash durability. No se afirma atomicidad/durabilidad; writers Felix cooperantes y TOCTOU externo fuera del contrato. |
| 2026-10-14 | `5c11f76`; CI `38512944346` | `PartialOld` solo se clasifica en backup cuando coincide la identidad durable de `old`, difiere el digest raw y las identidades de `old`/`new` son distintas; identidad distinta o identidades old/new iguales quedan `Unknown`. En cualquier slot produce `FailClosed`/`NO_STEPS`; cuatro pruebas de recovery nuevas. CI: Linux 727/0/6, Windows 719/0/6 (38 resúmenes), tools 7/OS, fmt, clippy y frontera pública pasan. | U7 sigue parcial y sin caller apply/producción. Es observación y rechazo: no añade capacidad, cleanup, paso del runner ni orden. La limpieza del backup `PartialOld` se pospone por no tener frontera segura ante cruce de mount/bind/FUSE; bloqueo de diseño, no probado por este commit. Los retries previos solo readquieren lock en el mismo proceso; no kill/restart ni crash durability. |
| 2026-10-14 | `401a4be`, `316bdb8`, `cedc11c` | Rechaza `FileId` Windows todo cero/todo `0xFF` para identidad requerida; el marcador exacto solo permite fallback canónico en el recorrido bundle de solo lectura, y `Unsupported` genérico no hace fallback. Los preflights Windows omiten el sentinel exacto solo en tests. | U7/fase 04 siguen parciales. La barrera mount/volume y la prueba de bind mount Linux con `st_dev` compartido siguen pendientes; estos commits no afirman cobertura total ni cleanup seguro. |
| 2026-10-14 | CI `38517718310` | Éxito para `cedc11c3414d4f704fc8344eb5c6d562a02ec2aa`: jobs `verify` Linux y `verify-windows` completados; `cargo test --workspace`, fmt, clippy y frontera pública pasan. | Evidencia del snapshot enviado, no del límite mount/volume; U7 sigue en curso y el progreso global permanece en 4/9. |
| 2026-10-14 | `752f56e` | Añade guardia parcial de scope a la creación del sibling de staging: compara root-parent antes de `mkdir` y parent-stage después de reabrir por handle; Linux `STATX_MNT_ID`, Windows `VolumeSerialNumber`; error, ausencia o mismatch fallan cerrado. Solo cubre esa operación; no prueba bind mounts ni mounts anidados. | U7/04 siguen parciales. No cubre inventario, captura, staging descendente, cleanup o recovery; no hay consumidor productivo del indicador de sibling huérfano posible. |
| 2026-10-14 | `9f48304` | Conserva `stage_io` cuando falla `mkdir_at` y propaga el diagnóstico de sibling vacío huérfano posible para cualquier error desde el intento de `mkdir`; no devuelve capability parcial ni ejecuta cleanup automático. | Error/ausencia/mismatch fallan cerrado. El indicador aún no tiene consumidor productivo; no modifica journal, recovery, cleanup o walkers. |
| 2026-10-14 | CI `38571014167` | Éxito para `9f4830411b4ae5d7964320e1ad87e6eaac9b00a5`: jobs `verify` Linux y `verify-windows` completados; pruebas del workspace, fmt, clippy y frontera pública pasan. | Evidencia del commit; no acredita bind mount real, mounts anidados ni integración de la frontera en las operaciones descendentes. U7/04 siguen parciales y el progreso global continúa en 4/9. |
| 2026-10-14 | `bb06df3` | El cleanup explícito de `StageAnclado` con capability captura scope del parent, preflight/verifica root y handles por scope, y revalida parent/objeto antes de cada unlink/rmdir/reapertura y rmdir final. Error, ausencia o mismatch fallan con `stage_cleanup_io`; el error conserva la capability para retry. Un fallo tras unlink puede dejar borrado parcial; carreras externas check→operación siguen fuera del contrato. | En ese corte seguían pendientes bind mount real, mounts anidados, inventario anclado y staging descendente. Los runners existentes ya llamaban `StageAnclado::limpiar` y heredaban el probe; faltaban cobertura end-to-end de recovery que ejercitara esa frontera y caller productivo de los runners. También seguía sin consumidor productivo el indicador de sibling huérfano posible. U7/04 permanecían parciales; progreso 4/9. |
| 2026-10-14 | CI `38576523094` | Éxito para `bb06df309c633f25b15f3e3b3bfd74c24b06a076`; jobs `verify` Linux y `verify-windows` completados, con workspace, fmt, clippy y frontera pública aprobados. | Acredita el commit, no bind mount real, mounts anidados ni integración general de scope. U7/04 siguen parciales; progreso 4/9. |
| 2026-10-14 | `a4583c2` | El inventario del root anclado comprueba scope parent/root antes de enumerar y cada handle de directorio/archivo antes de descender/leer; error, ausencia o mismatch producen `snapshot_io`. Captura anclada, `StageAnclado::fingerprint_raw`/recaptura y observer de recovery heredan por los wrappers. | Cobertura sintética, sin bind mount real ni mounts anidados; `VolumeSerialNumber` de Windows no identifica instancia de montaje. Backend por ruta/bundle, escrituras de staging y TOCTOU externo siguen pendientes/fuera de contrato. U7/04 siguen parciales; progreso 4/9. |
| 2026-10-14 | CI `38582798497` | Éxito para `a4583c24414850b9627bcfdc919d31d0cf502b70`; jobs `verify` Linux y `verify-windows` completados. | Evidencia del commit, no de mounts reales/anidados, cobertura del backend por ruta/bundle o escrituras de staging. U7/04 siguen parciales; progreso 4/9. |
| 2026-10-14 | `3e8ec0a` | Añade recuperación owned de la capability de `ErrorEscrituraStage` como `Result<StageAnclado, Self>`, para cleanup/retry de un stage parcial. |
| 2026-10-14 | `22494c8` | Si la capability está ausente, `Err(Self)` devuelve intacto el error de escritura; conserva código, stage parcial y diagnóstico en vez de descartarlos. |
| 2026-10-14 | `75d223e` | La capability writable captura scope inicial y lo propaga a descendientes. Valida parent antes de open/mkdir/create, child handle después de abrir y file handle antes de `write_all`; parent y stage deben coincidir con el scope inicial, por lo que también rechaza un cambio conjunto. Error/ausencia/mismatch producen `stage_io`; puede quedar un archivo vacío, y la capability se recupera para cleanup/retry. Token no persiste en journal. Pruebas sintéticas cubren rechazo conjunto y cleanup/retry, no bind mount real ni mounts anidados. Sin rename/apply/publicación; sibling huérfano ambiguo sigue sin capability ni cleanup automático. |
| 2026-10-14 | CI `38591028005` | Éxito para `75d223e`; jobs Linux y Windows completados. | Acredita el snapshot, no bind mount real, mounts anidados, `VolumeSerialNumber` como instancia de montaje, backend por ruta/bundle ni TOCTOU. U7/04 siguen parciales; progreso 4/9. |
| Unidad | Evidencia requerida | Resultado |
|---|---|---|
| 0 | Core `delta_transaccional` + OKF `delta_json_v1`: contrato typed, parser JSON v1, canonicalización, deduplicación typed, rechazo de duplicados JSON y colisiones reservadas | Cerrada; evidencia arriba |
| 1 | Candidato aislado; retiros/altas `Edge` y `HasLab` de arista, `Ext`, ordinales finales y reemplazo de etiqueta por handles | Cerrada como unidad; Unidad 2 añadió `Parent`/`HasLab` nodo y propuesta core. Construcción `Type`, apply/publicación filesystem, stale de extremo a extremo y CLI siguen fuera |
| 2 | Core `nodos`, `conflictos` y `propuesta`: Parent/HasLab de nodo, conflictos funcionales y veredicto puro | Cerrada en core; Unidad 3 añade carga/serialización `Parent` en memoria; `Type` en `proponer`, filesystem, stale de extremo a extremo y CLI siguen pendientes |
| 3 | OKF `padre_persistible`: carga optativa y serialización determinista de `Parent` a documentos en memoria | Cerrada; no incluye escritura en filesystem ni publicación |
| 4 | OKF snapshot: framing estable, replay, captura raw y CAS antes del parseo | Cerrada como primitiva interna; no escribe, publica ni conecta apply. La captura por ruta conserva TOCTOU; el root anclado de U6 está probado por separado |
| 5 | `writer_lock_transaccional` y validación de raíz | Avance parcial; lock probado en 2/8 writers, pero aún no se integra a apply/recovery. Solo serializa escritores Felix cooperantes |
| 6 | Root anclado y `stage_bundle_transaccional` privado | Cerrada; aliases, root reparse, staging byte a byte, recarga/comparación y cleanup/root intacto tienen evidencia focal. No está conectada a apply |
| 7 | `journal_temporal::eliminar` + `journal::recovery::runner` | Avance parcial: preflight compartido y dispatcher crate-private para filas exactas `KeepOld`, `RestoreOld` y `DiscardNewRestoreOld`. `KeepOld` limpia stage `New`/`PartialNew` con root `Old`, backup `Absent` y validación false en tres fases. Desde `c888347`, `RestoreOld` acepta root `Absent`, backup `Old`, validación false y stage `Absent`/`New`/`PartialNew` en `Prepared`/`BackupReady`/`Installed`; reabre por identidad durable (digest completo para `New`), limpia/reobserva, restaura backup y retira journal al final; cleanup ambiguo continúa solo si la reobservación confirma stage ausente, y los restos parciales conservan journal para retry. Desde `c763f85`, `DiscardNewRestoreOld` acepta exactamente `BackupReady` + root `New` + backup `Old` + stage `Absent` + validación false/true, o `Installed` con validación false; renombra root a stage, reobserva, limpia/reobserva y restaura old. Desde `bb06df3`, cleanup explícito de `StageAnclado` verifica scope del parent y de cada handle por preflight y antes de las operaciones destructivas; falla cerrado con `stage_cleanup_io` y conserva capability para retry, pero puede dejar borrado parcial tras unlink. Los pasos de cleanup de los runners existentes ya invocan `StageAnclado::limpiar` y heredan el probe real; faltan cobertura end-to-end de recovery que ejercite esa frontera y caller productivo de los runners. Root `New` junto a cualquier stage sibling falla cerrado. Ocho pruebas/casos de recovery nuevos visibles; CI `38509652854` pasa Linux 723/0/6 y Windows 715/0/6 (38 resúmenes), tools 7/OS, fmt, clippy y frontera pública. `KeepValidated`, `Committed`, otros estados no soportados y el ciclo completo de recovery siguen pendientes; U7 conserva alcance parcial. |
| 8–10 | `propose_transaccional`, `apply_transaccional`, `docs_transaccional` y métricas | U8 sigue parcial: helper crate-private de CAS bajo lock (`aa5d496`) abre root desde `lock.ubicacion()` y delega al CAS anclado; pruebas match/stale/lock/no mutación (`8250f0d`) y el caso root dejó de ser directorio → `TipoNoAdmitido` / `snapshot_entry_type_not_allowed`, redactado (`08e4659`). `SnapshotToken::from_hex` público (`885657aa`; pruebas `032668e`) acepta exactamente 64 bytes ASCII `0-9a-f` en minúscula y devuelve el error fijo/redactado `snapshot_token_invalid`. El commit `56a6516` limita con cuotas inclusivas el árbol raw en ambos calculadores antes del ordenamiento; los tokens v1 dentro de cuota conservan sus bytes. `02c7adde` añade al token una cuota combinada e inclusiva de 128 MiB para `manifiesto_raw` + `invariantes_raw`, independiente de la cuota del árbol y comprobada antes de validarlo/ordenarlo y de iniciar el hashing; `version_motor` y `policy_id` siguen sin cota. Esta validación de slices recibidas no promete un límite de memoria total ni del parser. `2357a82` limita durante la lectura la configuración obtenida por la variante privada de captura por ruta: suma los dos campos bajo una cuota inclusiva independiente de 128 MiB; reutiliza buffers del root, y cachea una ruta externa absoluta normalizada para leerla una vez, pero la cobra por cada campo que la usa. La lectura externa admite el remanente más el centinela. `63e59d1` añade el preflight consumidor antes de abrir root bajo lock y antes de clonar/inventariar en la captura anclada; el exceso da `snapshot_resource_limit` redactado, deja root/canary intacto y conserva el guard. Dentro de cuota, el CAS continúa antes de provenance/parseo. `4b94261` hace fallible `ConfigRawCapturada::nueva` y aplica la cuota como invariante después de convertir a `Arc<[u8]>`; cuenta por campo lógico, incluso con un `Arc` compartido. No limita lectura ni materialización previa, RSS o I/O antes del lock, así que el preflight consumidor sigue siendo defensa. Esta invariante se distingue de la captura privada por ruta, que limita la lectura durante su ejecución. El proveedor con lectura acotada antes del lock y los callers de apply/CLI siguen pendientes; `version_motor` y `policy_id` continúan sin cota. No implica cota total de memoria/parser ni atomicidad ante cambios hostiles. Matriz local U8 en `4b94261`: workspace Windows 694/6 y WSL 702/6 (passed/ignored); no hay CI remoto publicado para ese SHA. CI `38246129604` pasó solo para la base `a4d01e5`; CI final del parser sigue pendiente. En `609936a`, las pruebas focales del CAS por ruta con configuración externa pasan 32/0/1; la matriz local es Windows 701/6 y WSL 709/6, sin CI remoto para ese SHA. Match, stale redactado antes del parseo, no creación de lockfile sibling y retención del lock del caller están cubiertos. No se prueban atime, permisos, metadatos de entradas ni identidades individuales; `a3cc3c9` sí compara el `ArtifactFingerprint` del root (identidad del directorio raíz y digest raw), no la identidad de cada archivo o sibling. La ruta conserva TOCTOU/no atomicidad y el helper no integra apply/CLI ni simulación pública. U7 sigue parcial; U9–10 pendientes. |

## Orden original y foco vigente

El plan original colocaba la Fase 05 después de esta fase; la demo pública ya
está completa con el subconjunto de propuesta/evaluación que necesitaba. El
foco vigente sigue en Fase 04, Unidad 7; cerrar el recovery transaccional de
esta fase no es un requisito pendiente de la demo 05.

[05 — Demo pública](05-demo-publica.md) (completa).
