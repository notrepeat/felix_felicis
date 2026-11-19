# 12 — Apply con costo proporcional a lo que cambia

Hoy cada apply reescribe el bundle completo en un directorio de staging y
hace `fsync` de cada archivo. Capturar una nota en un universo de 5 000 notas
cuesta 16 s, y casi todo ese tiempo se va en sincronizar archivos que no
cambiaron. Este plan propone que el stage siga siendo un árbol completo,
pero con los archivos sin cambios enlazados (hard links) desde el root
vigente en lugar de copiados y sincronizados. Así, el `fsync` de archivos
pasa a ser proporcional a lo que cambia y el protocolo de publicación de la
[fase 04](04-operacion-transaccional.md) queda intacto.

Es un cambio en el núcleo transaccional: **no se implementa nada sin tu OK
explícito** (decisión A0).

## Estado

| Campo | Valor |
|---|---|
| Estado | propuesto (2026-11-19); espera tu decisión sobre A0–A10 |
| Depende de | 04 (completa), hallazgo 3 de la [fase 11](11-evaluacion-criterio.md#hallazgos-de-los-benchmarks-2026-11-19) |
| Desbloquea | `capturar` y `promover` usables con miles de notas; no es gate de ninguna versión |

## Evidencia

**Benchmark de Felix** (`evaluation/results/bench-criterio.json`, binario
`release` 0.4.0, commit `f976bc8`, mediana de tres repeticiones, Linux
x86_64):

| Notas | `capturar` | `promover` |
|---|---|---|
| 100 | 0,28 s | 0,31 s |
| 1 000 | 2,6 s | 3,1 s |
| 5 000 | 16 s | 17 s |

Según el hallazgo 3 de la fase 11 (medición manual, no está en el JSON), la
misma captura con 1 000 notas, simulada (`--dry-run`: captura, CAS y
evaluación, sin stage), tarda 0,1 s, y aplicarla tarda 3,4 s en esa misma
medición. La diferencia es el apply: unos 3 ms por nota.

**Microbenchmark del sistema de archivos** (2026-11-19, este equipo, btrfs
sobre `/home`, Python, 1 000 archivos de 1 500 bytes, dos corridas con
resultados dentro del 5 %; `fsync` del directorio al final en todos los
casos):

| Estrategia | Tiempo |
|---|---|
| copiar y `fsync` por archivo (lo que hace hoy el stage) | 6,3 s |
| copiar sin `fsync` por archivo | 0,025 s |
| copiar y un solo `syncfs` | 0,07–0,09 s |
| hard link | 0,022 s |
| hard link y `fsync` por archivo (inodo limpio) | 6,2 s |
| reflink (`FICLONE`) | 0,038 s |

Se concluyen dos cosas:

1. El costo es el `fsync` por archivo, no la copia: copiar 1 000 archivos
   cuesta 25 ms.
2. En btrfs, el `fsync` de un inodo limpio cuesta lo mismo que el de uno
   sucio. Enlazar sólo sirve si **además** se omite el `fsync` de los
   archivos enlazados (decisión A3).

El microbenchmark da unos 6 ms por `fsync`, y el apply real, unos 3 ms por
nota. El orden de magnitud coincide. La U0 descompone el tiempo en el propio
binario antes de cambiar nada.

## Dónde está el costo

En el apply anclado (`aplicar.rs`), primero corre `recuperar_pendiente_v2`
y después `preparar_apply_con_hook`, que hace la primera CAS. Luego
`materializar_staging_anclado` (`crates/universo-okf/src/transaccion/stage.rs`)
pasa por `crear_staging_anclado_con_probe` hasta llegar a
`escribir_plan_en_stage_anclado_con_probe`. Esa función:

1. Crea cada directorio del plan.
2. Por cada `EntradaStage` de tipo `Auxiliar` o `DocumentoNuevo`, crea el
   archivo con `crear_nuevo_con_probe`, escribe sus bytes desde memoria y
   llama a `durabilidad::sincronizar_archivo(&archivo, Punto::StageArchivo)`.
   **Es un `fsync` por archivo del bundle**, haya cambiado o no.
3. Hace `fsync` de cada directorio, del más profundo al stage
   (`Punto::StageDirectorio`).

Después, `materializar_staging_anclado_y_recargar_con` relee el stage
completo con `recargar_staging_anclado` y lo compara byte a byte con
`inventario_objetivo(plan)`. Por último, `validar_candidato_recargado`
compara el resultado con el candidato.

El `fsync` por archivo no es el único costo lineal. El protocolo relee el
árbol completo y calcula su digest muchas veces:

- la segunda CAS del root (`preparar_apply.rs`, después del stage);
- `validar_artefactos` antes de cada paso de publicación
  (`publicar_prepared.rs`, cuatro llamadas, e `instalar_prepared.rs`), que
  recaptura el root, calcula `stage.fingerprint_raw()` y observa root,
  backup y stage con `observar_siblings_recovery_v2_raw`;
- `validar_fila` después de cada rename (`instalar_prepared.rs`);
- la recaptura del sello (`aplicar.rs`).

Cada pasada cuesta del orden de una búsqueda completa (`search_todos`:
0,018 s con 1 000 notas y 0,10 s con 5 000), y hay decenas por apply. Con
5 000 notas, esas lecturas pueden sumar varios segundos. Hoy las esconde el
`fsync`; después de este plan serán el costo dominante. Por eso U0 las mide
antes de fijar un umbral (A8). Reducirlas es otro trabajo: ver «Fuera de
alcance».

## Garantías de la fase 04 y cómo se conservan

| Garantía | Qué cambia | Por qué se conserva |
|---|---|---|
| Publicación atómica | Nada en la publicación | El stage sigue siendo un árbol completo y se publica con el mismo `rename stage → root`. Lo enlazado es contenido, no protocolo. |
| Recovery antes de la CAS | Nada | `recuperar_pendiente_v2` corre bajo el lock antes de `preparar_apply_con_hook`. El enlace ocurre después de la primera CAS y antes de la segunda, igual que hoy la copia. |
| Journal, rollback y cleanup | Nada en formatos ni estados | Journal/Context v2 e Intent v3 no cambian de versión (A4). `Prepared`, `backup_ready`, `installed` y `committed` siguen operando con directorios. Borrar un stage o un backup con enlaces sólo retira nombres: el inodo sigue vivo en el root. Un binario anterior puede recuperar una transacción empezada por uno nuevo. |
| Verificación del stage | El código de error de una carrera (A10) | `recargar_staging_anclado` sigue releyendo **todos** los bytes del stage, enlazados incluidos, y los compara con el objetivo. `validar_artefactos` y `validar_fila` siguen exigiendo las huellas `old` y `new` en cada paso. Hoy, un archivo del root sustituido después de la primera CAS no toca el stage (se escribe desde memoria), y la segunda CAS lo informa como `stale`. Con enlaces, lo detecta antes la recarga (`InventarioInconsistente` o `EnlaceNoPermitido`). A10 decide si se convierte en `stale`. |
| Privacidad | Códigos nuevos sin datos | El fallback y sus errores usan códigos fijos, sin rutas ni bytes (U1). Un enlace conserva los permisos del original; la copia de hoy no los conserva (ver «Observación lateral»). |
| Windows | Primitiva nueva por handle | El enlace se hace con `NtSetInformationFile(FileLinkInformation)`, con `RootDirectory` igual al handle del directorio del stage. Es el mismo patrón que ya usa `renombrar_noreplace.rs` con `FileRenameInformation`. Si el volumen no admite hard links, se copia como hoy (A5). |
| Durabilidad ante caída del SO | Cómo se logra en Linux; en Windows, según A3 | La fase 04 no promete durabilidad ante pérdida de energía, pero `docs/operacion-transaccional.md` («Durabilidad ante caída del sistema») documenta que cada archivo del stage está sincronizado antes de `Prepared`. Así recovery encuentra tras una caída la huella `new` que registró el journal. Con A3 recomendada, Linux lo conserva con un solo `syncfs` del volumen del stage antes de `Prepared`. Sin esa sincronización, un root con datos no durables (un editor que no sincroniza) dejaría root nuevo y backup con el mismo inodo dañado, y recovery fallaría cerrado (`FailClosed`). |
| Backup como copia independiente | Se pierde para los archivos sin cambios | Entre el rename `root → backup` y la limpieza, backup y root nuevo comparten inodos. Una edición *in situ* ajena los altera a los dos (ver «Riesgos»). |

## Diseño propuesto

1. **Clasificar entradas en el plan (puro).** `planificar_staging` marca
   cada archivo del objetivo como *sin cambios* cuando sus bytes son
   idénticos a los de la misma ruta en el snapshot capturado (A2). Los
   auxiliares siempre lo están. Un documento administrado lo está si
   `serializar_candidato` produjo exactamente los mismos bytes. La
   comparación es en memoria: el snapshot ya tiene los buffers.
2. **Enlazar en vez de copiar.** Para cada archivo sin cambios, el stage
   crea un hard link desde el root anclado, siempre con **un solo
   componente** (A9). El directorio padre de la fuente se abre componente a
   componente sin seguir enlaces, como ya hace
   `abrir_directorio_stage_con_probe` en el stage:
   - Unix: `linkat(padre_fuente_fd, nombre, padre_stage_fd, nombre, 0)`.
   - Windows: el archivo se abre por handle relativo a su padre y se enlaza
     con `NtSetInformationFile(FileLinkInformation)`, con `RootDirectory`
     igual al handle del padre en el stage.

   Después, los dos nombres se abren sin seguir enlaces: tienen que ser
   archivos regulares con el mismo inodo o `FileId`. Antes y después se
   valida el scope igual que en `crear_nuevo_con_probe`. La durabilidad sigue
   A3.
3. **Fallback por archivo.** Si el enlace falla con un error de una lista
   cerrada (A5) y la fuente, abierta sin seguir enlaces, es un archivo
   regular, ese archivo se copia desde memoria y se sincroniza como hoy.
   Cualquier otro caso aborta el stage con el `CodigoEscrituraStage` que
   corresponda (falla cerrado), con la evidencia retenida de siempre.
4. **Escribir lo que cambió.** Los archivos con cambios se crean, escriben
   y sincronizan como hoy (`Punto::StageArchivo`). Los directorios se
   sincronizan como hoy: es `O(directorios)`, no `O(archivos)`.
5. **Todo lo demás, igual.** La recarga completa del stage, la segunda CAS,
   `Prepared`, los renombres, `installed`, la limpieza del backup, el sello
   y el historial no cambian.

Resultado esperado: los `fsync` de archivo por apply pasan a ser
`O(archivos cambiados)`, más `O(directorios)` y, en Linux, un `syncfs`.
Siguen siendo `O(N)` el `linkat` de cada archivo, la limpieza del backup y,
sobre todo, las relecturas completas del protocolo descritas en «Dónde está
el costo». Este plan quita el término dominante de hoy, pero no deja el
apply en `O(Δ)`.

## Alternativas descartadas

- **Stage parcial con instalación por archivo.** Es el único diseño
  realmente `O(Δ)`, pero instalar archivo por archivo en el root vivo rompe
  la publicación atómica: un lector (MCP, `search`) vería un estado mixto.
  Además, el journal tendría que enumerar archivos y recovery tendría que
  revertirlos uno por uno, lo que cambia los formatos y los estados de la
  fase 04. Descartado (A6).
- **Generaciones con un puntero (symlink o junction).** Cambia la forma del
  repositorio Git del usuario, y la fase 04 rechaza links dentro y en el
  root. Descartado.
- **Copiar sin `fsync` por archivo.** Cuesta lo mismo que enlazar (0,025 s
  contra 0,022 s), pero los archivos nuevos no son durables. Tras el commit
  y el borrado del backup, una caída puede dejar vacíos archivos que antes
  sí eran durables: es pérdida de datos. Un enlace conserva los bloques que
  ya estaban en disco. Descartado.
- **Sólo cambiar `fsync` por un `syncfs`.** Es el cambio más chico y conserva
  la durabilidad actual en Linux (0,07 s por 1 000 archivos). Pero no existe
  en Windows sin privilegios de administrador y sigue copiando todo. Como
  mecanismo único, se descarta. Sí se usa como parte de A3, sólo en Linux,
  aunque sincroniza también datos ajenos del mismo volumen y su latencia
  depende de otros procesos.
- **Reflinks.** Dan un inodo propio sin aliasing, pero no funcionan en ext4
  ni en NTFS (ReFS sí, con `FSCTL_DUPLICATE_EXTENTS_TO_FILE`), y el `fsync`
  del clon vuelve a costar por archivo. Quedan como backend posible más
  adelante (A7).

## Unidades de trabajo

Cada unidad sigue TDD: primero la prueba que falla, después el cambio
mínimo. Cada una pasa por una revisión independiente y la verificación
completa (Linux y Windows en CI) antes de su commit. Ninguna empieza sin A0.

| # | Unidad | Prueba que la abre | Archivos |
|---|---|---|---|
| U0 | Medir la línea base. En pruebas, contar `Punto::StageArchivo` por apply. En el binario `release`, separar el tiempo de stage, recarga, segunda CAS, `validar_artefactos`/`validar_fila`, publicación, limpieza y sello. Como `durabilidad::registro` es un no-op fuera de las pruebas, esto requiere una feature de Cargo de medición, apagada en los binarios distribuidos, que emita sólo nombres de fase y duraciones, sin rutas. También medir en el CI de Windows el costo de `FlushFileBuffers` sobre archivos limpios (para A3) | Una prueba de caracterización afirma que hoy un apply sobre N archivos registra N `StageArchivo`; una prueba de la feature afirma que su salida no contiene rutas ni bytes | `durabilidad.rs`, `aplicar_tests.rs`, `evaluation/bench_criterio.py` |
| U1 | Primitiva anclada para enlazar dentro del root y el stage con un solo componente, en Unix y en Windows. Valida scope, no sigue enlaces, verifica la identidad después de enlazar y clasifica los errores en fallback o fatal | El enlace de un symlink, un directorio, un reparse point, una fuente con un directorio intermedio sustituido por symlink o una ruta fuera del scope falla cerrado; `EXDEV` y `ERROR_NOT_SAME_DEVICE` piden fallback; `EPERM` sobre un directorio no pide fallback; el éxito deja el mismo `FileId` o inodo | `anclado/operaciones.rs`, módulo nuevo `anclado/enlazar.rs` y sus pruebas de Unix y Windows |
| U2 | Clasificación de entradas sin cambios en `PlanStaging` (puro) | Un plan con un documento modificado y N sin cambios marca exactamente N+auxiliares como enlazables; un documento recanonicalizado (bytes distintos) no se enlaza | `stage.rs` (`planificar_staging_desde_inventario`, `FuenteEntradaStage`) |
| U3 | El stage enlaza lo que no cambió, con fallback | Con `durabilidad::registro`, un apply que cambia un documento registra 1 `StageArchivo`, no N; con fallback forzado, registra N como hoy; una fuente sustituida entre la captura y el enlace devuelve el código decidido en A10 | `stage.rs` (`escribir_plan_en_stage_anclado_con_probe`), `transaccion/stage/stage_bundle_transaccional.rs` |
| U4 | Rollback, recovery y cleanup con un stage enlazado | La matriz de fallos inyectados por `Punto` de la fase 04 se repite con un stage enlazado: tras cada corte, el root queda byte a byte en old o en new, y tras la limpieza cada archivo vuelve a un solo enlace. Los preflights de limpieza e inventario aceptan archivos con más de un enlace. Una edición *in situ* del root después de la segunda CAS falla cerrado. En Windows, un handle abierto sin `FILE_SHARE_DELETE` sobre un archivo del root nuevo no impide limpiar el backup, o el resultado queda documentado | `journal/recovery/runner_tests.rs`, `anclado/limpieza.rs` (sólo pruebas, salvo hallazgo) |
| U5 | Aplicar A3 y actualizar la sección «Durabilidad ante caída del sistema» de `docs/operacion-transaccional.md` | En Linux, una prueba registra un solo `Punto` nuevo (`syncfs` del stage) antes de `ContextoPublicado`, la primera escritura del protocolo; en Windows, lo que diga A3 | `durabilidad.rs` (código y doc del módulo), `docs/operacion-transaccional.md`, fase 04 (límites) |
| U6 | Repetir el benchmark y actualizar el hallazgo 3 de la fase 11 | `evaluation/test_bench_criterio.py` sigue en verde; el JSON nuevo queda en un commit, con la referencia a ese commit | `evaluation/results/bench-criterio.json`, fase 11 |

## Criterios de aceptación

- [ ] Un apply que cambia k archivos de un bundle de N registra k
      `Punto::StageArchivo`, no N. Se verifica con `durabilidad::registro`
      en Linux y Windows.
- [ ] El inventario del root después del apply es byte a byte el del
      candidato, igual que hoy (pruebas actuales de `stage_bundle_transaccional`
      y de apply sin cambios).
- [ ] La matriz de fallos inyectados de la fase 04 pasa con stages
      enlazados y con fallback forzado.
- [ ] Con hard links no disponibles (fallback forzado en las pruebas), el
      resultado y los `Punto` coinciden con el comportamiento actual.
- [ ] Ninguna salida, error ni evidencia nueva contiene rutas, bytes ni
      tokens. Se verifica con las pruebas de canarios de privacidad.
- [ ] En el equipo de referencia, `capturar` cumple el umbral que fije A8
      después de U0.
- [ ] La verificación completa y el CI de Linux y Windows están en verde.

## Decisiones

| # | Decisión | Opciones | Recomendación |
|---|---|---|---|
| A0 | Tocar el núcleo transaccional | sí, por unidades / no por ahora | sí, unidad por unidad, con tu OK antes de U1 (U0 sólo mide) |
| A1 | Mecanismo | hard links con fallback / reflinks / stage parcial / sólo `syncfs` | hard links con fallback a copia |
| A2 | Qué es «sin cambios» | bytes idénticos al snapshot / mtime y tamaño / hash | bytes idénticos, en memoria: es lo mismo que ya verifica la recarga |
| A3 | Durabilidad de los archivos enlazados | (a) un `syncfs` del stage en Linux antes de `Prepared`; en Windows, `FlushFileBuffers` por archivo enlazado si U0 lo muestra barato, y si no, un límite documentado / (b) sin sincronizar en ninguna plataforma (recovery puede fallar cerrado tras una caída si el root traía datos no durables) / (c) `fsync` por archivo (sin ganancia) | (a): conserva en Linux lo que documenta `docs/operacion-transaccional.md`; en Windows decides con el dato de U0 |
| A4 | Formato de journal, context e intent | sin cambios / versión nueva | sin cambios: recovery no distingue un stage enlazado de uno copiado |
| A5 | Errores que piden fallback | lista cerrada / cualquier error | lista cerrada, y sólo si la fuente, abierta sin seguir enlaces, es un archivo regular. Unix: `EXDEV`, `EMLINK`, `EPERM`, `ENOTSUP`/`EOPNOTSUPP`. Windows: `ERROR_NOT_SUPPORTED`, `ERROR_INVALID_FUNCTION`, `ERROR_NOT_SAME_DEVICE`, `ERROR_TOO_MANY_LINKS`; `ERROR_SHARING_VIOLATION` también, si U1 lo confirma. Los demás errores fallan cerrado |
| A6 | Stage parcial con instalación por archivo | descartar / estudiar aparte | descartar: rompe la publicación atómica |
| A7 | Reflinks | ahora / después / nunca | después, sólo si U6 muestra que hace falta |
| A8 | Umbral de rendimiento | fijarlo ahora / fijarlo después de U0 | después de U0, porque las relecturas del protocolo pueden dominar. Propuesta inicial: el tiempo de stage con 5 000 notas baja al menos ×10 y el `capturar` total al menos ×3, con el mismo generador y equipo que `bench-criterio.json` |
| A9 | Cómo se nombra la fuente del enlace en Unix | `linkat` con un solo componente desde el padre abierto sin seguir enlaces / `linkat` con la ruta completa desde el root / `AT_EMPTY_PATH` (requiere `CAP_DAC_READ_SEARCH`) / `/proc/self/fd` | un solo componente, con verificación de identidad después: una ruta completa podría atravesar un directorio intermedio sustituido por un symlink, y las otras opciones dependen de capacidades o de procfs. Si se admite otro Unix además de Linux, verificar el tipo después de enlazar |
| A10 | Código cuando la recarga falla en una entrada enlazada | `InventarioInconsistente` como error técnico / repetir la CAS y devolver `stale` | repetir la CAS: si el root cambió, `stale` le dice al agente que vuelva a simular, como hoy; si la CAS coincide (lo alterado fue el stage), se mantiene `InventarioInconsistente` |

## Riesgos

- **Aliasing de inodos (el principal).** Desde el enlace hasta la limpieza
  del backup, un archivo sin cambios es el mismo inodo en el stage (o en el
  root nuevo) y en el root viejo (o en el backup). Una edición *in situ*
  hecha fuera de Felix, por ejemplo un editor que reescribe sin renombrar,
  altera los dos.
  - **No se publica sin verificar.** Después de la segunda CAS,
    `validar_artefactos` (`publicar_prepared.rs`) exige la huella `old` en el
    root y `new` en el stage antes de cada paso. `validar_fila`
    (`instalar_prepared.rs`) exige `old` en el backup y `new` en el root
    después de cada rename. Como la edición cambia las dos huellas, el paso
    siguiente falla cerrado, igual que hoy.
  - **El backup deja de ser una copia independiente de `old`.** Si la
    edición ocurre después de `root → backup`, recovery no encuentra ni
    `old` ni `new` intactos y falla cerrado (`FailClosed`, ver
    `journal/recovery.rs`). Hoy, en el mismo caso, el backup conserva `old`
    para restaurarlo a mano.

  Las dos cosas quedan fuera del contrato (el lock es advisory), pero la
  segunda es una pérdida real de capacidad de recuperación manual. U4 fija
  el comportamiento con una prueba, y U5 lo declara en los límites de la
  fase 04 y en `docs/operacion-transaccional.md`.
- **Windows.**
  - No hay hard links en FAT/exFAT y no siempre en ReFS: el fallback lo
    cubre.
  - En NTFS, el modo de compartición es del archivo, no del nombre. Si un
    editor, un antivirus o un indexador tiene abierto un archivo del root
    nuevo sin `FILE_SHARE_DELETE`, borrar su otro nombre en el backup puede
    fallar. Hoy nadie tiene abierta la copia del backup. Si pasa, el apply
    queda como `prepared_finalization_unverified` y el siguiente necesita
    recovery. U4 lo prueba en Windows. Si se confirma, hay que decidir entre
    aceptar ese costo, enlazar sólo en Unix o reintentar la limpieza.
  - U1 verifica qué derechos de acceso necesita `FileLinkInformation`, y el
    CI de Windows es el gate.
- **Volúmenes sin hard links o con montajes cruzados.** El costo vuelve al
  de hoy (fallback), sin perder corrección. El stage es sibling del root y
  la validación de scope ya exige el mismo sistema de archivos.
- **Comprobaciones del número de enlaces o de `FileId`.** Hoy ningún
  inventario ni preflight de `crates/universo-okf/src/transaccion` mira el
  número de enlaces de un archivo, pero una comprobación futura podría
  romper esto en silencio. U4 lo fija con pruebas.
- **Git.** Enlazar cambia el `ctime` de los archivos del root. `git status`
  vuelve a leerlos una vez, igual que hoy con los inodos nuevos. No afecta
  la corrección.
- **Costo residual.** Las relecturas completas del protocolo pueden dejar
  el apply en varios segundos con 5 000 notas, aun sin `fsync` por archivo.
  Además, los directorios siguen con un `fsync` cada uno, así que un bundle
  con muchas carpetas mantiene un término lineal. U0 mide las dos cosas.

## Observación lateral (no forma parte de este plan)

La copia actual crea los archivos del stage con `crear_nuevo_con_probe`
(`anclado/operaciones.rs`), sin modo explícito, así que no hereda los
permisos del archivo original. Una nota con permisos restringidos en el root
podría quedar con los permisos por defecto después de un apply. No se
verificó en este plan. Si se confirma, es un arreglo propio de privacidad,
independiente de esta propuesta. Los enlaces la evitan para los archivos sin
cambios, pero no para los que cambian.

## Fuera de alcance

- Reducir el `O(N)` de lectura: la recarga del stage, las CAS, las
  repeticiones de `validar_artefactos` y `validar_fila`, y el sello. Si U0
  muestra que dominan, se propone como plan aparte. Un caché de digests por
  archivo no puede usar sólo el inodo como clave: una edición *in situ* no
  lo cambia, y con enlaces el mismo inodo vive en dos árboles.
- Hacer configurable el presupuesto de evaluación (hallazgo 2 de la fase 11).
- Cambiar el protocolo de publicación, los formatos del journal o la
  semántica de recovery.

## Verificación prevista

Por unidad, antes de cada commit:

- `cargo test --workspace --locked -j 2`
- `cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings`
- `cargo fmt --all --check`
- `python3 tools/check_public_boundary.py`
- `python3 tools/check_public_cli_demo.py`
- Los unittests de `tools/` y `evaluation/`

También el CI de Linux y Windows en verde para el commit exacto, y una
revisión independiente.

## Registro de evidencia

| Fecha | Commit | Evidencia |
|---|---|---|
| 2026-11-19 | — | Plan propuesto. Microbenchmark del sistema de archivos (tabla de «Evidencia»), local y no reproducible en CI. Una revisión independiente contra el código corrigió el costo de las relecturas, el análisis de aliasing, el enlace por componente (A9), la durabilidad (A3) y el riesgo de compartición en Windows. |
