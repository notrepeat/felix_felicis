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
  retirar y añadir explícitamente.
- `Ext` requiere predicado declarado, sujeto nodo existente y constante de
  valor perteneciente al dominio declarado. Para el mismo par
  `(predicado, nodo)`, reemplazar un valor exige `remove` del valor vigente y
  `add` del nuevo; `add` no sobrescribe y `remove` identifica el valor exacto.
- El snapshot token se calcula en OKF con framing de longitudes y separación
  de dominio/version (`FELIX-SNAPSHOT-v1`) para cada campo. Incluye, por cada
  archivo de root, path normalizado, longitud del path, bytes del path,
  longitud del contenido y bytes crudos; paths ordenados por bytes
  normalizados. También incluye manifiesto e invariantes raw, versión de motor
  y política, cada uno con longitud explícita. Leer cada entrada una vez a
  buffers inmutables; parsear desde esos mismos bytes y calcular el token
  desde ellos, sin relectura que permita TOCTOU. Simulación conserva lectura
  legacy de links; apply los rechaza antes del snapshot transaccional.
- Persistir una relación `Parent` requiere override opt-in: un `BundlePath`
  exacto o raíz vacía. En `Option<Option<BundlePath>>`, `None` devuelve
  `parent_not_persistable`, `Some(Some(path))` persiste ese `BundlePath`
  exacto y `Some(None)` persiste raíz vacía.
- La API pura de `universo-core` es
  `proponer(estructura, delta, invariantes) -> Result<ResultadoPropuesta, ErrorPropuesta>`;
  construye/evalúa candidato y separa errores técnicos del veredicto
  funcional, sin filesystem ni publicación. La API de `universo-okf`/CLI
  `aplicar(..., expected_snapshot, ...)` adquiere lock, hace CAS del snapshot,
  reevalúa con core y coordina staging/publicación solo para `admit`.
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
  sobre un lockfile reutilizable; la dependencia Rust que provee
  el lock queda por elegir. No borrar ni reemplazar el lockfile. El OS libera
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
  construir el candidato, que corresponde a Unidad 1. Completada en los
  commits `1074902ec4687af435f2772de77febd161a59e68` (240 líneas añadidas) y
  `a54688ca274fa46f1a6a438df7988c13ef1176de` (247 líneas añadidas), dos
  commits revisables de menos de 400 líneas cada uno y sin cambio de alcance.
- [ ] 1 — Candidate, Edge y Ext: `crates/universo-core/src/transaccion/candidato.rs`,
  `crates/universo-core/src/transaccion/aristas.rs`,
  `crates/universo-core/src/transaccion/ext.rs`.
  Avance parcial: el core clona la estructura en un candidato aislado y admite
  solo retiros `Edge` y `HasLab` de arista. Resuelve cada identidad contra el
  snapshot a handles, retira en lote, compacta los grupos afectados preservando
  el orden de supervivientes y conserva sus labels por handle. Un retiro
  `HasLab` solo se acepta si el mismo delta retira la arista del mismo handle y
  la etiqueta coincide exactamente. El constructor rechaza como
  `unsupported_operation` todo `add` y toda remoción fuera de esos casos,
  incluidos `Ext`; no hay aún una ruta de admisión que evalúe invariantes
  funcionales.

  `crates/universo-core/tests/candidato_delta.rs`
  (`cargo test -p universo-core --test candidato_delta`) pasa 3/3 casos de
  aislamiento, preservación de entrada y rechazo redactado de operaciones no
  soportadas. `crates/universo-core/tests/delta_aristas.rs`
  (`cargo test -p universo-core --test delta_aristas`) pasa 5/5 casos de
  retiros paralelos —incluidos los ordinales 1 y 10 entre 11 aristas—,
  compactación, orden independiente de entrada, labels por handle, fallo sin
  mutación y condición `HasLab`/`Edge` del mismo handle. El slice aún no cubre
  los adds con ordinales finales contiguos, ni
  `crates/universo-core/tests/ext_transaccional.rs` y
  `crates/universo-okf/tests/ext_roundtrip.rs`.

  Runtime previsto: 10.000 átomos base + 1.000 cambios y grupos de
  1/100/10.000 aristas, midiendo ms y bytes del candidato. No hay medidas aún:
  resolver R retiros buscando cada identidad en una adyacencia de grado D puede
  costar O(R×D); medir y decidir una optimización queda pendiente. Rollback
  previsto: descartar candidato. Objetivo por slice: <400 líneas authored.
- [ ] 2 — Conflictos funcionales: `crates/universo-core/src/transaccion/conflictos.rs`.
  Test `crates/universo-core/tests/conflictos_delta.rs`
  (`cargo test -p universo-core --test conflictos_delta`) separa
  tipo/etiqueta/incidencia de invariantes y permite igual etiqueta en sujetos
  distintos salvo invariante. Runtime: 1.000 conflictos, ms y diagnósticos/s;
  rollback: no publicar cambios. Objetivo por slice: <400 líneas authored.
- [ ] 3 — Persistencia `Parent`: core `crates/universo-core/src/transaccion/padre.rs`
  y `crates/universo-core/src/manifiesto.rs`; OKF
  `crates/universo-okf/src/manifiesto_toml.rs`, `crates/universo-okf/src/bundle.rs`,
  `crates/universo-okf/src/serializar.rs`. Test
  `crates/universo-okf/tests/padre_persistible.rs`
  (`cargo test -p universo-okf --test padre_persistible`) cubre BundlePath
  exacto, raíz vacía, `parent_not_persistable` y carga/serialización. Runtime:
  1.000 cambios, ms; rollback: conservar padre anterior. Objetivo por slice:
  <400 líneas authored.
- [ ] 4 — Snapshot/stale: `crates/universo-okf/src/transaccion/snapshot.rs`.
  Test `crates/universo-okf/tests/snapshot_transaccional.rs`
  (`cargo test -p universo-okf --test snapshot_transaccional`) compara replay,
  framing/domain `FELIX-SNAPSHOT-v1`, paths normalizados con longitudes y
  bytes crudos de todos los archivos, manifest/invariants raw, versión y
  política de motor; confirma mismos bytes parseados y `stale` al cambiar
  cualquier campo.
  Runtime: bundle 1/10/100 MiB, MB/s y costo de framing; rollback: no aplicar
  token stale. Objetivo por slice: <400 líneas authored.
- [ ] 5 — Lock single-writer: `crates/universo-okf/src/transaccion/lock.rs`.
  Test `crates/universo-okf/tests/writer_lock_transaccional.rs`
  (`cargo test -p universo-okf --test writer_lock_transaccional`) confirma un
  solo commit; `proceso_muerto_libera_lock_y_restart_recupera` mata child con
  lock y verifica liberación OS más recovery antes de stale recheck. Runtime:
  2/8 writers, espera y tasa busy; rollback: quitar solo artefactos fallidos,
  lockfile reutilizable. Objetivo por slice: <400 líneas authored.
- [ ] 6 — Root y builder de staging: `crates/universo-okf/src/transaccion/stage.rs`.
  Test `crates/universo-okf/tests/stage_bundle_transaccional.rs`
  (`cargo test -p universo-okf --test stage_bundle_transaccional`) rechaza
  root symlink/junction/reparse, verifica aliases al mismo parent+basename
  canónico y rechaza cualquier symlink/junction/reparse dentro de root para
  apply. Copia solo archivos regulares/directorios auxiliares byte a byte;
  reload/compara candidato. Runtime: 10.000 auxiliares/100 MiB, MB/s y bytes
  copiados; rollback: borrar staging, raíz intacta. Objetivo por slice:
  <400 líneas authored.
- [ ] 7 — Journal, commit y recovery: `crates/universo-okf/src/transaccion/journal.rs`,
  `crates/universo-okf/src/transaccion/publisher.rs`.
  Test `crates/universo-okf/tests/journal_recuperacion_transaccional.rs`
  (`cargo test -p universo-okf --test journal_recuperacion_transaccional`);
  `restart_durante_cleanup_recupera_new` cubre `installed` sin backup con
  validación del candidato y `committed` limpiando restos;
  `inventario_completo_arbol_y_artifacts_tras_restart` compara el mapa
  completo de árbol y artefactos sibling tras reinicio, incluidos cleanup en
  `prepared`/`backup_ready`. Runtime: recovery y commit de 100 MiB,
  ms/renombre; rollback: old en prepared/backup_ready, new validado en
  installed/committed. Objetivo por slice: <400 líneas authored.
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

- [ ] Un delta válido produce el candidato esperado.
- [ ] Root symlink/junction/reparse se rechaza; una raíz válida se canoniza
  una vez y todos los artefactos sibling se derivan del parent+basename
  canónicos, de modo que aliases convergen.
- [ ] `add`/`remove` se comportan como parche transaccional explícito, sin
  atribuir al delta minimalidad o prioridad de AGM/R.
- [ ] `add`/`remove` se canonicalizan con orden determinista; el solapamiento
  se rechaza y un snapshot distinto se informa como `stale`.
- [ ] Los retiros Edge/HasLab se resuelven a handles del snapshot antes de
  mutar; las removals por `(origen, tipo, destino)` compactan supervivientes
  en orden snapshot. Adds canónicos exigen ordinales finales contiguos.
- [ ] `Ext` valida predicado declarado/dominio; cambiar valor para
  `(predicado, nodo)` exige remove+add. `HasLab` es funcional por sujeto;
  etiquetas iguales en sujetos distintos se permiten salvo invariante.
- [ ] Cambiar `HasLab` del mismo sujeto exige remove+add. `Parent` se persiste
  solo con override de `BundlePath` exacto o raíz vacía; ausente produce
  `parent_not_persistable`.
- [ ] La salida separa error técnico de veredicto funcional y redacta valores
  privados por defecto.
- [ ] `universo-core::proponer(...)` no hace I/O; `universo-okf`/CLI
  `aplicar(..., expected_snapshot, ...)` hace recovery, CAS, re-evaluación y
  publicación bajo lock.
- [ ] Los conflictos funcionales se distinguen de las violaciones de
  invariantes y se informan sin publicar valores privados por defecto.
- [ ] Un delta rechazado no cambia memoria ni archivos.
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
- [ ] El mismo delta contra el mismo estado produce el mismo veredicto.
- [ ] Un delta contra un estado diferente se detecta y no se aplica a ciegas.
- [ ] La optimización incremental queda diferida hasta tener mediciones.

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
cargo test -p universo-okf --test stage_bundle_transaccional
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
| 2026-10-11 | Verificación en HEAD `f42cdb9` | Workspace, clippy, fmt y frontera pública pasan en el HEAD de código; enlaces, frontera, fmt y diff pasan tras editar los planes. | Aprobada; no incluye medición de rendimiento. |

| Unidad | Evidencia requerida | Resultado |
|---|---|---|
| 0 | Core `delta_transaccional` + OKF `delta_json_v1`: contrato typed, parser JSON v1, canonicalización, deduplicación typed, rechazo de duplicados JSON y colisiones reservadas | Cerrada; evidencia arriba |
| 1 | Candidato aislado; retiros `Edge` y `HasLab` de arista resueltos a handles del snapshot, en lote y con compactación/labels por handle | Parcial; adds, `Ext`, invariantes funcionales y rendimiento pendientes |
| 2–4 | Core `ext_transaccional`, `conflictos_delta`; OKF `ext_roundtrip`, `padre_persistible`, `snapshot_transaccional` | Pendiente |
| 5–7 | `writer_lock_transaccional`, `stage_bundle_transaccional`, `journal_recuperacion_transaccional` | Pendiente |
| 8–10 | `propose_transaccional`, `apply_transaccional`, `docs_transaccional` y métricas | Pendiente |

## Siguiente fase

[05 — Demo pública](05-demo-publica.md).
