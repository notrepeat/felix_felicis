# Universo del motor

Felix verificándose a sí mismo. Este universo describe el motor tal como está
en el código y la documentación: crates y sus dependencias, binarios, comandos
de la CLI, flags, herramientas del MCP, códigos de error, documentos y
versión. Las reglas dicen qué tiene que estar al día con qué: un comando
nuevo tiene su línea en la ayuda y en una guía, una herramienta del MCP tiene
su fila en `docs/mcp.md`, un código de error que ve el usuario está
documentado y probado, una crate no depende de una capa superior.

No es un ejemplo ficticio: es el primer universo real y público del proyecto, y
el primer usuario de las mejoras de la
[propuesta 14](../docs/propuestas/14-uso-diario-por-agentes.md).

## Archivos

| Archivo | Qué es |
|---|---|
| `manifest.toml` | Tipos, aristas y atributos (`capa` de las crates, `clase` de los documentos). |
| `invariants.sx` | Reglas admitidas: el estado sellado las cumple y `guard` las exige. |
| `reglas-completas.sx` | Todas las reglas, incluidas las que el motor todavía no cumple (la deuda). Hoy, las mismas diez de `invariants.sx`. |
| `extraer.py` | Escribe un borrador del bundle desde `crates/`, `universo --help`, `universo-mcp --help`, `tools/list` del MCP y `docs/`. |
| `actualizar.py` | Extrae, calcula el `delta` contra el bundle admitido y lo pasa por `propose`; con `--aplicar`, lo publica. |
| `bundle/`, `.bundle.felix.seal`, `.bundle.felix.log` | Estado admitido, su sello e historial. Sólo cambian con `actualizar.py --aplicar`. |

## Uso

```sh
python universo-motor/actualizar.py             # ¿el bundle refleja el código? (no escribe)
python universo-motor/actualizar.py --aplicar   # lo pone al día; commitea bundle, sello y log
python universo-motor/actualizar.py --deuda     # además, qué reglas pendientes fallan
universo guard universo-motor/bundle --manifiesto universo-motor/manifest.toml \
  --invariantes universo-motor/invariants.sx
```

Después de cambiar código, ayuda o documentos, corre `actualizar.py --aplicar` y
commitea el resultado junto con el cambio. Si cambiaste la ayuda de un binario,
describe los de la rama, como hace CI: `--descrito target/debug/universo
--descrito-mcp target/debug/universo-mcp` (la ayuda de `universo-mcp` existe
desde la versión siguiente a la 0.8.0; con el binario instalado de la 0.8.0,
`actualizar.py` ve un delta que no es real). Nunca edites `bundle/` a mano:
`guard` lo detecta.

## En CI

El job `verify` instala la release `v0.8.0` (con su suma SHA-256) y la usa para
verificar; los binarios de la rama sólo se usan para extraer su ayuda y sus
herramientas. Así, un bug nuevo del motor no puede esconder sus propias fallas.
Al publicar una versión nueva, sube `FELIX_VERIFICA` en `.github/workflows/ci.yml`.

| Paso | Bloquea |
|---|---|
| `guard` con `invariants.sx` | Sí |
| `actualizar.py` (¿el bundle está al día?) | No, por ahora |
| `reglas aplicar reglas-completas.sx --dry-run` (deuda) | No |

## Deuda al arrancar (2026-11-24), pagada

Al arrancar, seis de las diez reglas fallaban en 43 casos: desajustes reales
entre el código y la documentación. Se corrigieron en la rama
`deuda-universo-motor` y las diez reglas están en `invariants.sx`;
`actualizar.py --deuda` termina con 0.

| Regla | Casos | Qué fallaba | Cómo se corrigió |
|---|---:|---|---|
| `comando-documentado` | 6 | `atributo`, `nota`, `relacion`, `objetar`, `resumen` y `verificar` no aparecían como `universo <comando>` en el README ni en las guías | Ejemplos en el README: `objetar` y `resumen` en el recorrido del repositorio de criterio, un bloque de correcciones puntuales y `universo verificar` en la lista de comandos |
| `flag-en-ayuda` | 10 | `universo-mcp --help` no imprimía ayuda (salía `mcp_server_failed`), así que ninguna de sus 7 flags aparecía; en la CLI faltaban `--fecha`, `--forzar` y `--medir-uso` | `universo-mcp --help` y `-h` con sus opciones (con prueba); las tres flags en el Uso y las Opciones de `universo --help` |
| `flag-documentada` | 8 | `--cliente`, `--de`, `--fecha`, `--forzar`, `--limit`, `--nombre` y `--quitar` de la CLI, y `--registrar-uso` del MCP, no estaban en el README ni en las guías | README (`--fecha`, `--forzar`, `--de`, `--limit`, `--quitar`) y `docs/mcp.md` (`--cliente`, `--nombre`, `--registrar-uso`) |
| `herramienta-en-tabla` | 4 | `capture`, `reinforce`, `object` y `metrics` se describían en `docs/mcp.md`, pero no tenían fila en la tabla de herramientas | Sus filas, con argumentos y resultado |
| `codigo-de-superficie-documentado` | 8 | `invalid_tool_arguments`, `unknown_tool`, `tool_schema_invalid`, `response_invalid`, `worker_failed`, `pagination_not_supported`, `promote_source_rejected` y `no_disponible` no aparecían en ningún documento | Tabla de códigos del servidor en `docs/mcp.md`; `promote_source_rejected` en el README y `no_disponible` en `docs/instalacion.md` |
| `codigo-de-superficie-probado` | 7 | Siete códigos de la CLI y del MCP, entre ellos `server_busy`, no aparecían textuales en ninguna prueba | Pruebas de protocolo (`unknown_tool`, `pagination_not_supported`, `invalid_tool_arguments`), unitarias del servidor (`server_busy`, `response_invalid`, `tool_schema_invalid`) y de `promover` cuando el origen rechaza la marca (`promote_source_rejected`) |

Las reglas en deuda vivían en `reglas-completas.sx` y no en `invariants.sx`
porque el motor no sabe sellar un estado con deuda conocida (I8): si
estuvieran en `invariants.sx`, `seal` se negaría y `guard` fallaría siempre. Al
corregir una, se movió a `invariants.sx` con `universo reglas … aplicar`. Una
regla nueva que el motor todavía no cumpla sigue el mismo camino.

## Lo que este universo no ve

- La prosa. Que un documento describa un comportamiento viejo (por ejemplo, un
  formato de delta anterior, o una brecha marcada como abierta que ya se
  cerró) no se detecta: el extractor sólo mira nombres. Es el límite de «dos
  verdades» de la propuesta 14 (I9).
- Las flags por comando: la regla mira si una flag aparece en la ayuda, no si
  aparece en la línea de *ese* comando (la ayuda general de `capturar` omite
  `--autor`, pero `--autor` aparece en otras líneas).
- Los códigos del núcleo que llegan a la superficie: sólo cuentan como «de
  superficie» los que emiten `universo-cli` y `universo-mcp`.
- La extracción es textual (expresiones regulares sobre el código): puede
  perder códigos escritos de otra forma.

## Fricciones del motor que aparecieron al armarlo

- `check` no dice qué reglas fallan; la deuda se lista con
  `reglas aplicar --dry-run`, la única orden que las nombra (I2).
- Localizar los 43 casos exigió 614 llamadas a `check`, sustituyendo la
  variable por `@ruta` (I2).
- La regla `herramienta-en-tabla` nombra `@documentos/doc-docs-mcp`, así que
  no podía sellarse con el bundle vacío; entró después de las notas (I7).
- `delta` no sabe borrar notas (`note_deletion_unsupported`): si un comando o
  un código desaparece del código, `actualizar.py` falla y hay que retirarlo a
  mano (I7).
- Comparar la capa de dos crates necesitó un `esquema` sobre `Dom.Capa` (I3).
