# 01 — Fijar el contrato de `validate` y `check`

La fase 01 entrega la validación estructural y fija la interfaz de `check`.
La evaluación semántica, incluido el criterio de rechazo, se completa junto
con el evaluador de referencia en la fase 02.

## Estado

| Campo | Valor |
|---|---|
| Estado | en curso |
| Depende de | — |
| Desbloquea | 02, 04, 05 |
| Progreso | 6/7 criterios de aceptación verificados (86%); falta el cierre semántico de `check` junto con la fase 02. |

## Punto de partida

- [x] La CLI actual carga manifiestos y bundles; `verificar` solo comprueba la carga y la estructura.
- [x] Existe un AST y un parser de invariantes, pero no un evaluador semántico.
- [x] Este documento fija el alcance de `validate`, la interfaz futura de `check`, los códigos de proceso y JSON v1.
- [x] `validate`, `--help` y JSON v1 están implementados y cubiertos por pruebas de CLI.
- [ ] La aceptación semántica de `check` se cumple junto con la fase 02.

## Contrato de comandos

| Comando | Alcance | Disponibilidad |
|---|---|---|
| `validate <raiz> --manifiesto <archivo.toml> [--format <formato>]` | Carga el manifiesto y el bundle; comprueba formato OKF y coherencia estructural. No lee ni evalúa invariantes. | Fase 01. |
| `check <raiz> --manifiesto <archivo.toml> --invariantes <archivo> [--format <formato>]` | Valida primero los datos; solo después interpreta y evalúa las invariantes. | Interfaz fijada en fase 01; ejecución semántica en fase 02. |
| `verificar <raiz> --manifiesto <archivo.toml> [--format <formato>]` | Alias exacto de `validate`. En JSON, el campo `command` conserva el nombre canónico `validate`. | Se conserva durante la línea 0.x. Retirarlo después requiere al menos una versión publicada con aviso de deprecación. |
| `atomos <raiz> --manifiesto <archivo.toml>` | Imprime los átomos de la estructura cargada. | Ya disponible. |

`--format` acepta `text` o `json`; `text` es el formato predeterminado. `--help` imprime la ayuda en texto por `stdout` y termina con código 0, incluso si se especificó `--format json`.

`validate` comprueba el manifiesto, el subconjunto OKF admitido y la coherencia estructural del bundle (nodos, aristas, referencias y dominios declarados). Los metadatos no reconocidos se conservan como datos opacos; no se evalúan como invariantes. Las rutas del layout y los archivos leídos deben permanecer dentro de la raíz del bundle. Una ruta declarada ausente, fuera de la raíz o repetida de forma que duplique nodos es un error, no una validación vacía exitosa.

`validate` y `check` son operaciones de lectura. Un error estructural no es un veredicto semántico: `check` no evalúa invariantes si el manifiesto o el bundle no pasan la carga estructural. Las claves estructurales duplicadas se rechazan para impedir que un valor repetido se descarte silenciosamente.

## Códigos de salida y canales

| Código | Significado |
|---|---|
| `0` | La validación estructural pasó; en `check`, todas las invariantes pasaron; o se solicitó y pudo imprimir ayuda. |
| `1` | Datos/manifiesto inválidos, error de autoría, rechazo semántico o fallo al escribir la salida. El resultado tipado distingue las causas de dominio; un fallo de escritura puede impedir que se emita el resultado. |
| `2` | Error de uso: comando, opción o argumento inválido o ausente. |

- En texto, resúmenes y veredictos van por `stdout`; diagnósticos de datos, manifiesto o autoría van por `stderr`.
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

JSON v1 no promete todavía el esquema del resultado de `check`. La fase 02 fijará sus veredictos y errores tipados sin cambiar el significado ni los campos de `validate` v1.

## Criterios de aceptación

- [x] Un bundle malformado devuelve código 1 en `validate`; el comando no carga ni evalúa invariantes.
- [x] Un bundle estructuralmente válido devuelve código 0 en `validate`; la validación no evalúa invariantes.
- [x] `validate` distingue éxito, datos/manifiesto inválidos y error de uso con los códigos 0, 1 y 2.
- [x] `validate` rechaza rutas del layout que escapen de la raíz, symlinks de archivos fuera de ella, directorios declarados ausentes y claves estructurales duplicadas.
- [x] JSON v1 respeta el esquema, los canales y la línea única definidos arriba para resultados válidos e inválidos.
- [x] `verificar` produce el mismo resultado que `validate`; `--help` termina con 0 y muestra la terminología del README.
- [ ] En la fase 02, un bundle válido pero contradictorio pasa `validate` y falla `check`; un error de autoría se distingue de un error de datos y no se comunica como veredicto negativo.

## Fuera de alcance

- Evaluar fórmulas o extraer testigos; corresponde a las fases 02 y 03.
- Aplicar propuestas o escribir el bundle.
- Exponer herramientas MCP.

## Verificación prevista

```powershell
cargo test -p universo-okf
cargo test -p universo-cli --test cli
cargo clippy -p universo-okf --all-targets -- -D warnings
cargo run -p universo-cli -- validate examples/knowledge-base --manifiesto manifests/example.toml
cargo run -p universo-cli -- validate examples/knowledge-base --manifiesto manifests/example.toml --format json
cargo run -p universo-cli -- --help
python tools/check_public_boundary.py
```

La evidencia de `check` se agrega al registro de esta fase después de que la fase 02 entregue el evaluador y sus pruebas end-to-end.

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-10-10 | `c0f75238acad97cdd011c3c162bc746beec2c075` — `size:exception` (488 líneas; +475/−13) | `cargo test -p universo-okf` (50/50), `cargo clippy -p universo-okf --all-targets -- -D warnings` y pruebas de frontera/duplicados. El código y sus regresores permanecen juntos. | Aprobado |
| 2026-10-10 | `199711e27bcbc234e972327be9913846817efcf4` — `size:exception` (609 líneas; +562/−47) | `cargo test -p universo-cli --test cli` (14/14); `--help`=0, JSON válido=0, manifiesto ausente en JSON=1 y formato desconocido=2. Se mantiene la CLI con sus pruebas y dependencias. | Aprobado |
| 2026-10-10 | — | `python tools/check_public_boundary.py`, verificación de enlaces Markdown locales y `git diff --check`. | Aprobado |

## Siguiente paso

Implementar el evaluador de referencia en la fase 02. Esa fase verificará y
cerrará conjuntamente con esta los criterios semánticos de `check`.
