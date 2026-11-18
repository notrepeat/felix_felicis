# 03 — Transacciones, concurrencia y durabilidad

Afirmaciones: A6 («se aplica de forma transaccional, con lock, concurrencia y
recovery»), A7 («nunca queda a medio escribir») y A8 («ni se contradice sin que
lo sepas»). [Volver al maestro](00-maestro.md).

## Camino productivo

Único escritor: `universo propose … --apply --expect-snapshot <token>`
(`crates/universo-cli/src/propose.rs:7`, verificado) →
`crates/universo-okf/src/transaccion/aplicar.rs:76`. El MCP no puede escribir.

Pasos (reportado, consistente con el plan de fase 04): captura de config y
delta → lock `flock` no bloqueante sobre sibling → recovery pendiente → CAS y
propuesta bajo lock → staging → segunda captura con CAS e identidad de root →
journal v2 `Prepared` → publicación de contexto y journal → renombrado
root→backup (`BackupReady`) y stage→root (`Installed`) con identidad esperada →
`Committed`, limpieza de backup e intent de retiro.

Ningún paso es solo de test: la cadena completa está cableada. En producción
solo actúa el decisor v2; el runner v1 (`KeepValidated…`) no tiene llamador
productivo (`journal/recovery.rs:21`, reportado).

## Lo que no se cumple

| Problema | Evidencia | Estado |
|---|---|---|
| ~~Sin fsync de los Markdown del stage ni de directorios~~ | **Corregido 2026-11-17** en Linux: `transaccion/durabilidad.rs` sincroniza archivos y directorios del stage antes del journal Prepared, y el directorio padre tras cada rename/link/unlink transaccional. Tests: `aplicar::tests::apply_sincroniza_stage_antes_de_instalar_y_cada_cambio_de_nombres` (orden) y `fsync_fallido_del_sucesor_no_borra_backup_y_recover_completa` (fallo inyectado; verificado con mutación). En Windows solo se vacían archivos; las entradas de directorio no. Residuo: en los retiros finales un `fsync` fallido no se informa | Linux: hecho. Windows: parcial |
| Consecuencia razonada: ante caída del SO los renombrados y el borrado del backup podían persistir antes que los datos del root nuevo | El test anterior exige que el backup se borre solo después del sucesor Committed durable. Sin prueba real de pérdida de energía | Mitigado (Linux) |
| ~~Stage huérfano tras error de preparación~~ | **Corregido 2026-11-17**: `aplicar_archivo` lo retira bajo el mismo lock con la capability anclada (`limpiar_stage_de_preparacion_fallida`); si no puede, devuelve `apply_stage_retained`. Tests: `aplicar::tests::fallo_de_preparacion_tras_crear_stage_lo_limpia_y_no_bloquea_el_siguiente_apply` (rojo antes) y dos en `preparar_apply_tests.rs`. La ruta correcta del recovery es `anclado/duenio_transaccion/recovery_v2/dispatch.rs:83` | Hecho; queda el stage sin capability (proceso muerto antes del journal), que se diagnostica con `status` y no se borra |
| ~~Sin comando `status`/`recover`~~ | **Corregido 2026-11-17**: `universo status` (solo lectura, redactado, 0/1) y `universo recover` (recovery v2 bajo lock sin aplicar). Tests: `crates/universo-cli/tests/estado_transaccional.rs`, `transaccion::estado::tests` | Hecho |
| `check` no toma lock ni mira journal/stage/backup | **2026-11-17**: sigue sin lock, pero avisa por stderr si hay estado pendiente, sin cambiar stdout ni código. Test: `check_avisa_por_stderr_sin_cambiar_stdout_ni_codigo` | Aviso hecho |
| **Lock advisory**: solo excluye escritores Felix cooperantes con la misma ruta canónica | `crates/universo-okf/src/transaccion/lock.rs:1-8` (verificado) | Por diseño |
| Editor con buffer abierto que guarda tras el commit pisa el cambio admitido sin aviso (lost update) | reportado, por razonamiento sobre el lock advisory | Ausente |
| Dos rutas o bind mounts al mismo directorio producen lockfiles distintos | reportado, no probado | Riesgo medio |
| macOS y otros Unix fallan cerrado (sin corrupción, sin apply) | `anclado.rs:980-983`, `renombrar_move_platform.rs:25-30` (reportado) | Por diseño |
| Una sola prueba de proceso muerto vía caller público | `instalar_prepared_apply_tests.rs` (reportado) | Cobertura parcial |

## Brechas

| # | Brecha | Tamaño | Compuerta |
|---|---|---|---|
| D1 | **Hecho en Linux; Windows parcial** (2026-11-17). fsync de archivos del stage y de directorios padre tras rename/unlink (Linux), `FlushFileBuffers`/`MOVEFILE_WRITE_THROUGH` (Windows) | M | No |
| D2 | **Hecho** (2026-11-17). Limpieza segura del stage retenido o diagnóstico claro que lo explique | S–M | No |
| D3 | **Hecho** (2026-11-17). `universo status` (estado transaccional, stage/backup/journal) y `universo recover` | M | No |
| D4 | **Hecho como aviso** (2026-11-17). `check` consciente de la transacción (toma lock o avisa si hay recovery pendiente) | S | No |
| D5 | **Hecho** (2026-11-17, `universo guard`/`seal`, [guardia](../guardia.md)). Detección de cambios fuera de Felix: guardar el token del último estado admitido y avisar si el bundle difiere (ver [04](04-agentes-y-control-obligatorio.md)) | M | No |
| D6 | Más cortes de proceso muerto vía caller público; prueba de caída de SO si se agrega D1 | M | No |
| D7 | Retirar o marcar el runner v1 sin uso productivo | S | No |
| D8 | Apply en macOS | M | No (decisión de plataforma) |

D1–D4 son la diferencia entre «transaccional ante caída del proceso» y «nunca a
medio escribir». Tras el paso 2 de la hoja de ruta, en Linux el orden de `fsync`
está probado; sigue sin haber prueba real de caída del SO (D6) y Windows no
sincroniza entradas de directorio.
