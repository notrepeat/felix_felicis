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
| **Sin fsync de los Markdown del stage ni de directorios.** `sync_all` solo aparece en contexto/intent y journal temporal | `rg sync_all\|sync_data\|fsync\|FlushFileBuffers\|WRITE_THROUGH` fuera de tests: solo `anclado/archivo_temporal.rs:86,124` y `anclado/journal_temporal.rs:255` (verificado) | Ausente |
| Consecuencia razonada, no probada: ante caída del SO los renombrados y el borrado del backup pueden persistir antes que los datos del root nuevo | — | Riesgo alto |
| **Stage huérfano tras error de preparación**: el error retiene el stage (`anclado/duenio_transaccion/preparar_apply.rs:166-168`), ni `aplicar.rs` ni la CLI lo limpian, y el recovery exige stages ausentes (`recovery_v2/dispatch.rs:84`) | verificado en código; sin prueba que lo ejercite | Bloquea apply siguientes |
| Sin comando `status`/`recover`: ante `FailClosed` la CLI solo informa un código | reportado | Ausente |
| `check` no toma lock ni mira journal/stage/backup | sin coincidencias de `LockEscritura`/`journal` en `crates/universo-cli/src/main.rs` (verificado) | Ausente |
| **Lock advisory**: solo excluye escritores Felix cooperantes con la misma ruta canónica | `crates/universo-okf/src/transaccion/lock.rs:1-8` (verificado) | Por diseño |
| Editor con buffer abierto que guarda tras el commit pisa el cambio admitido sin aviso (lost update) | reportado, por razonamiento sobre el lock advisory | Ausente |
| Dos rutas o bind mounts al mismo directorio producen lockfiles distintos | reportado, no probado | Riesgo medio |
| macOS y otros Unix fallan cerrado (sin corrupción, sin apply) | `anclado.rs:980-983`, `renombrar_move_platform.rs:25-30` (reportado) | Por diseño |
| Una sola prueba de proceso muerto vía caller público | `instalar_prepared_apply_tests.rs` (reportado) | Cobertura parcial |

## Brechas

| # | Brecha | Tamaño | Compuerta |
|---|---|---|---|
| D1 | fsync de archivos del stage y de directorios padre tras rename/unlink (Linux), `FlushFileBuffers`/`MOVEFILE_WRITE_THROUGH` (Windows) | M | No |
| D2 | Limpieza segura del stage retenido o diagnóstico claro que lo explique | S–M | No |
| D3 | `universo status` (estado transaccional, stage/backup/journal) y `universo recover` | M | No |
| D4 | `check` consciente de la transacción (toma lock o avisa si hay recovery pendiente) | S | No |
| D5 | Detección de cambios fuera de Felix: guardar el token del último estado admitido y avisar si el bundle difiere (ver [04](04-agentes-y-control-obligatorio.md)) | M | No |
| D6 | Más cortes de proceso muerto vía caller público; prueba de caída de SO si se agrega D1 | M | No |
| D7 | Retirar o marcar el runner v1 sin uso productivo | S | No |
| D8 | Apply en macOS | M | No (decisión de plataforma) |

D1–D4 son la diferencia entre «transaccional ante caída del proceso» (cierto
hoy) y «nunca a medio escribir» (lo que prometemos).
