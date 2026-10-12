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

Estado de entrega: las Unidades 0–4 están cerradas dentro de sus límites; U5
conserva avance parcial y U6 está cerrada para root anclado y staging. Fase 04
sigue en curso: lock, captura y staging aún no están conectados a apply ni
publican candidatos.

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
- [x] 1 — Candidate, Edge y Ext: `crates/universo-core/src/transaccion/candidato.rs`,
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
  Rollback: no publicar cambios. El presupuesto del evaluador está acotado por
  fórmula, no tiene una cota agregada para todas las fórmulas de una propuesta;
  definir y probar ese hardening sigue pendiente, sin promesa de costo global.
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
  journal/recovery, CLI y una cota global del presupuesto de evaluación.

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
- [ ] 7 — Journal, commit y recovery: `crates/universo-okf/src/transaccion/journal.rs`,
  `crates/universo-okf/src/transaccion/publisher.rs`.
  Test `crates/universo-okf/tests/journal_recuperacion_transaccional.rs`
  (`cargo test -p universo-okf --test journal_recuperacion_transaccional`);
  `restart_durante_cleanup_recupera_new` cubre `installed` sin backup con
  validación del candidato y `committed` limpiando restos;
  `inventario_completo_arbol_y_artifacts_tras_restart` compara el mapa
  completo de árbol y artefactos sibling tras reinicio, incluidos cleanup en
  `prepared`/`backup_ready`. Runtime: recovery y commit de 100 MiB,
  ms/renombre. En Unix, el precheck de identidad antes de `unlinkat` reduce
  pero no elimina la carrera ante mutación hostil del namespace; el lock solo
  garantiza coordinación entre escritores Felix cooperantes. Rollback: old en
  prepared/backup_ready, new validado en
  installed/committed. Objetivo por slice: <400 líneas authored.

  Contrato de recursos compartido por los inventarios de snapshot/stage/recovery
  y por la carga desde disco con `bundle::cargar_bundle`: 128 MiB raw, 100.000
  entradas, profundidad máxima 128 y 16 MiB acumulados de rutas lógicas. Los
  límites son inclusivos. El inventario cuenta bytes raw de todos los archivos
  regulares; `cargar_bundle` solo suma documentos Markdown efectivamente leídos.
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
cargo test -p universo-okf --test journal_recuperacion_transaccional
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

| Unidad | Evidencia requerida | Resultado |
|---|---|---|
| 0 | Core `delta_transaccional` + OKF `delta_json_v1`: contrato typed, parser JSON v1, canonicalización, deduplicación typed, rechazo de duplicados JSON y colisiones reservadas | Cerrada; evidencia arriba |
| 1 | Candidato aislado; retiros/altas `Edge` y `HasLab` de arista, `Ext`, ordinales finales y reemplazo de etiqueta por handles | Cerrada como unidad; Unidad 2 añadió `Parent`/`HasLab` nodo y propuesta core. Construcción `Type`, apply/publicación filesystem, stale de extremo a extremo y CLI siguen fuera |
| 2 | Core `nodos`, `conflictos` y `propuesta`: Parent/HasLab de nodo, conflictos funcionales y veredicto puro | Cerrada en core; Unidad 3 añade carga/serialización `Parent` en memoria; `Type` en `proponer`, filesystem, stale de extremo a extremo y CLI siguen pendientes |
| 3 | OKF `padre_persistible`: carga optativa y serialización determinista de `Parent` a documentos en memoria | Cerrada; no incluye escritura en filesystem ni publicación |
| 4 | OKF snapshot: framing estable, replay, captura raw y CAS antes del parseo | Cerrada como primitiva interna; no escribe, publica ni conecta apply. La captura por ruta conserva TOCTOU; el root anclado de U6 está probado por separado |
| 5 | `writer_lock_transaccional` y validación de raíz | Avance parcial; lock probado en 2/8 writers, pero aún no se integra a apply/recovery. Solo serializa escritores Felix cooperantes |
| 6 | Root anclado y `stage_bundle_transaccional` privado | Cerrada; aliases, root reparse, staging byte a byte, recarga/comparación y cleanup/root intacto tienen evidencia focal. No está conectada a apply |
| 7 | `journal_recuperacion_transaccional` | Avance parcial: observador read-only anclado y clasificación de root/backup/stage con cobertura focal; faltan decisión/ejecución de recovery, integración al apply y pruebas de restart. |
| 8–10 | `propose_transaccional`, `apply_transaccional`, `docs_transaccional` y métricas | Pendiente |

## Siguiente fase

[05 — Demo pública](05-demo-publica.md).
