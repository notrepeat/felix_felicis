# 01 — Fijar el contrato de `validate` y `check`

La fase 01 entrega la validación estructural y fija la interfaz de `check`.
La evaluación semántica, incluido el criterio de rechazo, se completa junto
con el evaluador de referencia en la fase 02.

## Estado

| Campo | Valor |
|---|---|
| Estado | completo |
| Depende de | — |
| Desbloquea | 02, 04, 05 |
| Progreso | 7/7 criterios de aceptación verificados (100%); `check` se cerró conjuntamente con la fase 02. |

## Punto de partida

- [x] Al iniciar la fase, la CLI cargaba manifiestos y bundles; `verificar` solo comprobaba carga y estructura.
- [x] Al inicio existían el AST y el parser sin evaluación semántica; el evaluador y la CLI `check` se integraron después en la fase 02.
- [x] Este documento fija el contrato de `validate` y `check`, sus códigos de proceso y JSON v1.
- [x] `validate`, `--help` y JSON v1 están implementados y cubiertos por pruebas de CLI.
- [x] La aceptación semántica de `check` se cumple junto con la fase 02; la evidencia está registrada abajo.

## Contrato de comandos

| Comando | Alcance | Disponibilidad |
|---|---|---|
| `validate <raiz> --manifiesto <archivo.toml> [--format <formato>]` | Carga el manifiesto y el bundle; comprueba formato OKF y coherencia estructural. No lee ni evalúa invariantes. | Fase 01. |
| `check <raiz> --manifiesto <archivo.toml> --invariantes <archivo> [--format <formato>]` | Valida primero el bundle; después lee UTF-8, parsea, valida estáticamente todas las fuentes, expande esquemas y evalúa cada instancia. | Disponible y verificado en `a9e1983`; la aceptación se cerró conjuntamente con fase 02. |
| `verificar <raiz> --manifiesto <archivo.toml> [--format <formato>]` | Alias exacto de `validate`. En JSON, el campo `command` conserva el nombre canónico `validate`. | Se conserva durante la línea 0.x. Retirarlo después requiere al menos una versión publicada con aviso de deprecación. |
| `atomos <raiz> --manifiesto <archivo.toml>` | Imprime los átomos de la estructura cargada. | Ya disponible. |

`--format` acepta `text` o `json`; `text` es el formato predeterminado. `--help` imprime la ayuda en texto por `stdout` y termina con código 0, incluso si se especificó `--format json`.

`validate` comprueba el manifiesto, el subconjunto OKF admitido y la coherencia estructural del bundle (nodos, aristas, referencias y dominios declarados). Los metadatos no reconocidos se conservan como datos opacos; no se evalúan como invariantes. Las rutas del layout y los archivos leídos deben permanecer dentro de la raíz del bundle. Una ruta declarada ausente, fuera de la raíz o repetida de forma que duplique nodos es un error, no una validación vacía exitosa.

`validate` y `check` son operaciones de lectura. Un error estructural no es un veredicto semántico: `check` no abre ni parsea el archivo de invariantes si el manifiesto o el bundle no pasan la carga estructural. Las claves estructurales duplicadas se rechazan para impedir que un valor repetido se descarte silenciosamente. Un archivo vacío o solo con comentarios admite cero invariantes por vacuidad; no demuestra una política sustantiva.

## Códigos de salida y canales

| Código | Significado |
|---|---|
| `0` | La validación estructural pasó; `check` dio `admit`; o se solicitó y pudo imprimir ayuda. |
| `1` | `validate` detectó datos/manifiesto inválidos; `check` dio `reject` o `error` (datos, autoría o recursos); o falló la escritura. El resultado tipado distingue las causas; un fallo de escritura puede impedir su emisión. |
| `2` | Error de uso: comando, opción o argumento inválido o ausente. |

- En texto, resúmenes y veredictos van por `stdout`; diagnósticos de `validate` o errores de `check` van por `stderr`. Un rechazo semántico de `check` es un resultado, no un diagnóstico, y va por `stdout`.
- En JSON, el resultado de dominio completo ocupa una línea en `stdout` y `stderr` queda vacío. Un error de uso no produce JSON: imprime la ayuda de uso en `stderr`, deja `stdout` vacío y devuelve 2.
- `--help` es texto de ayuda en `stdout` y devuelve 0 si pudo escribirse; un fallo de escritura devuelve 1.
- Los errores de autoría y los errores de datos usan el mismo código 1, pero códigos de error distintos en el resultado estructurado; un error de autoría nunca se presenta como contradicción evaluada.

## JSON v1 de `validate`

La primera versión estructurada de `validate` tiene esta forma:

```json
{
  "schema_version": 1,
  "command": "validate",
  "status": "valid",
  "summary": { "nodes": 2, "edges": 1 },
  "errors": []
}
```

En un resultado inválido, `status` es `"invalid"`, `summary` es `null` y `errors` contiene los diagnósticos. Cada error tiene `code`, `message`, `path` y `line`; `path` identifica el archivo del bundle o el manifiesto indicado y `line` es `null` cuando no hay línea aplicable. Los errores del manifiesto usan `ManifestIo`, `ManifestSyntax` o `ManifestInvalid`; los errores de bundle usan el nombre de su regla (`Regla`). Si se pasa una ruta absoluta al manifiesto, el diagnóstico puede repetirla; no publiques resultados de una ejecución sobre datos privados en logs públicos.

## JSON v1 de `check`

`check` tiene un esquema versionado separado del de `validate`:

```json
{
  "schema_version": 1,
  "command": "check",
  "status": "admit",
  "summary": { "sources": 3, "instances": 3, "evaluated": 3, "failed": 0 },
  "errors": []
}
```

`status` es `admit`, `reject` o `error`. `summary` cuenta las S-expresiones
top-level parseadas (`sources`), las fórmulas directas más instanciadas
(`instances`), las evaluadas y las falsas (`failed`); es `null` para `error`.
Un `reject` tiene `summary.failed > 0` y `errors` vacío: en fase 02 no devuelve
asignaciones, átomos testigo ni contenido de las fuentes. `errors` usa
`kind` (`data`, `authorship` o `resource`), `code`, `message`, `path`, `line`,
`column` y `location`; los campos sin ubicación aplicable son `null`. Los
errores de datos, autoría o recursos no se presentan como rechazo lógico.

El código y los tests de CLI verifican este esquema en `a9e1983`.
Los campos `path` pueden contener una ruta del bundle o una ruta absoluta de
manifiesto/invariantes; no publiques resultados de ejecuciones privadas. El
esquema de `check` no altera el JSON v1 ya establecido para `validate`.

## Criterios de aceptación

- [x] Un bundle malformado devuelve código 1 en `validate`; el comando no carga ni evalúa invariantes.
- [x] Un bundle estructuralmente válido devuelve código 0 en `validate`; la validación no evalúa invariantes.
- [x] `validate` distingue éxito, datos/manifiesto inválidos y error de uso con los códigos 0, 1 y 2.
- [x] `validate` rechaza rutas del layout que escapen de la raíz, symlinks de archivos fuera de ella, directorios declarados ausentes y claves estructurales duplicadas.
- [x] JSON v1 respeta el esquema, los canales y la línea única definidos arriba para resultados válidos e inválidos.
- [x] `verificar` produce el mismo resultado que `validate`; `--help` termina con 0 y muestra la terminología del README.
- [x] Un bundle válido pero contradictorio pasa `validate` y falla `check`; los errores de autoría/datos/recursos se distinguen y no se comunican como rechazo lógico.

## Fuera de alcance

- Extraer testigos; corresponde a la fase 03. La evaluación de fórmulas quedó cubierta por la fase 02.
- Aplicar propuestas o escribir el bundle.
- Exponer herramientas MCP.

## Verificación prevista

```powershell
cargo test -p universo-okf
cargo test -p universo-cli --test cli
cargo clippy -p universo-okf --all-targets -- -D warnings
cargo run -p universo-cli -- validate examples/knowledge-base --manifiesto manifests/example.toml
cargo run -p universo-cli -- validate examples/knowledge-base --manifiesto manifests/example.toml --format json
cargo test -p universo-cli
cargo run -p universo-cli -- check examples/knowledge-base --manifiesto manifests/example.toml --invariantes examples/knowledge-base/invariants.sx --format json
cargo run -p universo-cli -- check examples/knowledge-base --manifiesto manifests/example.toml --invariantes examples/knowledge-base/rejected-invariants.sx --format json
cargo run -p universo-cli -- --help
python tools/check_public_boundary.py
```

La aceptación de `check` se cerró junto con la fase 02. Los resultados
reproducibles y sus commits se registran abajo.

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-10-10 | `c0f75238acad97cdd011c3c162bc746beec2c075` — `size:exception` (488 líneas; +475/−13) | `cargo test -p universo-okf` (50/50), `cargo clippy -p universo-okf --all-targets -- -D warnings` y pruebas de frontera/duplicados. El código y sus regresores permanecen juntos. | Aprobado |
| 2026-10-10 | `199711e27bcbc234e972327be9913846817efcf4` — `size:exception` (609 líneas; +562/−47) | `cargo test -p universo-cli --test cli` (14/14); `--help`=0, JSON válido=0, manifiesto ausente en JSON=1 y formato desconocido=2. Se mantiene la CLI con sus pruebas y dependencias. | Aprobado |
| 2026-10-10 | `2b5f721d66bef62e753b2481df8c7865da130d38`, `602120977c3b0949730efd7a926c113879afb42c`, `0b3df1162368c0cf1f5adbf4cc577897f146f1fa`, `b8786dbc56bcf63a72fe540bf65ee6975484a861` | Parser, validación estática, evaluación Tarski, oráculo generativo independiente y expansión determinista. `cargo test -p universo-core`: 108 unitarias + 7 integración; clippy core y `cargo fmt --all -- --check` pasan. | Aprobado; fase 02 cerrada |
| 2026-10-10 | `a9e198347ea40835b3686a16a349150fcbb7ff3c` — `feat(cli): check invariant programs` | `cargo test -p universo-cli`: 8/8 pruebas de `check` y 14/14 de CLI; clippy CLI pasa. Ejemplo normal `admit`/0 (3 fuentes, 3 instancias, 3 evaluadas, 0 fallidas); fixture falsa `reject`/1 (1/1); `validate --format json`=`valid`/0 (3 nodos, 1 arista). Tests cubren canales JSON, determinismo, precedencia de datos, compatibilidad de `validate` y ausencia de testigos. | Aprobado; fase 01 cerrada junto con fase 02 |
| 2026-10-10 | — (verificación documental del árbol de trabajo) | `python tools/check_public_boundary.py`, enlaces Markdown locales y `git diff --check`. | Aprobado |

## Siguiente paso

La fase está completa. La extracción de testigos y veredictos explicables
continúa en [fase 03](03-veredictos-explicables.md).
