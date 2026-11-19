# 09 — Hacer que cada versión denote evidencia real

El objetivo es que un tag no signifique «trabajo acumulado», sino un conjunto
preciso de capacidades demostradas, con CI, demo e informe enlazados.

## Estado

| Campo | Valor |
|---|---|
| Estado | `v0.6.0` publicada (2026-11-19) |
| Depende de | 07, 08 |
| Desbloquea | Decisión sobre capacidades avanzadas |

## Gates propuestos

| Versión | Gate mínimo |
|---|---|
| `v0.2.0` | Evaluador, testigos, transacciones y demo CLI reproducible. |
| `v0.3.0` | Loop MCP completo y equivalente al CLI. |
| `v0.4.0` | Criterio distribuible ([fase 10](10-criterio-distribuible.md)): repositorio de criterio, captura, promoción y MCP con varios universos. |
| `v0.5.0` | Evaluación comparativa y benchmarks publicados (D5 de la fase 10 la movió desde `v0.4.0`). |
| `v0.6.0` | Uso diario sin Rust: instaladores, `universo conectar` y `universo grafo`. |
| `v1.0.0` | Contratos declarados estables y evidencia sostenida en uso real. |

Mientras la API sea `0.x`, los parches corrigen comportamiento compatible y
los incrementos menores pueden cambiar contratos con migración documentada.

## Migración a 0.2.0

Cambios de contrato pre-1.0 publicados en `0.2.0`; los añadidos después de esta
tabla están en las [notas de versión](../release/v0.2.0.md#cambios-incompatibles).

| Cambio | Acción del consumidor |
|--------|-----------------------|
| `ErrorPath::{SegmentoTerminaEnPunto, SegmentoTerminaEnEspacio, DispositivoReservado, CaracterNoPortable, SegmentoDemasiadoLargo { limite_utf16 }}`, `ErrorManifiesto::{DirectoriosDuplicados, DirectoriosSolapados, DirectoriosRutaInvalida, SinIotaRutaInvalida, IgnoradosRutaInvalida}`, `ErrorEstructura::HijoDuplicadoEnLote`, `ErrorSnapshot::LimiteInventarioExcedido` y `Regla::LimiteBundleExcedido` agregan variantes a enums exhaustivos. | Añadir ramas para todas las variantes antes de actualizar; tratar `limite_utf16` como el límite que informa el error. |
| `BundlePath`, `layout.directorios` y `layout.sin_iota` exigen segmentos portables a Windows; `layout.ignorados` exige un nombre de un solo segmento portable. Los componentes físicos de layout admiten 255 unidades UTF-16; los componentes intermedios de `BundlePath`, 255, y su último stem, 248 por el temporal `.md.tmp`. Se rechazan caracteres no admitidos, puntos/espacios finales y dispositivos DOS reservados, incluidos `COM¹`–`COM³` y `LPT¹`–`LPT³`. | Renombrar archivos/directorios no portables o demasiado largos y actualizar `layout.directorios`, `layout.sin_iota`, `layout.ignorados`, wikilinks y referencias `Parent` explícitas a las rutas afectadas. |
| `cargar_bundle` limita la carga desde disco a 128 MiB de Markdown leído, 100.000 entradas, profundidad 128 y 16 MiB de rutas lógicas acumuladas; aplica además un presupuesto separado de 16 MiB a todas las rutas canónicas físicas relativas retenidas por el índice de raíces y `procesados`. Cada copia retenida cuenta; los alias de diamante omitidos no se cobran otra vez. | Reducir o dividir bundles que excedan una cuota. Los límites son inclusivos y se comparten entre todas las raíces del layout. |
| La cuota de inventario de snapshots (`ErrorSnapshot::LimiteInventarioExcedido`) cuenta los bytes crudos de todos los archivos regulares, no solo Markdown; véase [operación transaccional](04-operacion-transaccional.md). | Ajustar el inventario completo al límite documentado, no únicamente sus documentos Markdown. |
| Las raíces `layout.directorios` deben ser disjuntas tanto por identidad exacta como por una clave Unicode completa en mayúsculas, conservadora para detectar colisiones potenciales (`Σ`/`ς`, `ᾀ`/`ᾈ`). La clave puede sobre-rechazar expansiones (`ß`/`SS`) y no afirma equivalencia exacta con Windows; el staging físico en Windows usa `CompareStringOrdinal`. Tampoco se admiten relaciones ancestro/descendiente (p. ej. `Docs` y `docs/sub`), incluso si aparecen al resolver symlinks/junctions. | Renombrar raíces para que sean disjuntas según la clave declarada y según las relaciones físicas comprobadas. |
| Errores de lectura exponen `regla` y `path` para clasificación/localización. | No analizar `detalle`: es texto humano de diagnóstico, puede variar por plataforma y no es un contrato estable. Si se exceden varias cuotas, tampoco es estable cuál se reporta primero. |

Las cuotas se aplican a `cargar_bundle` sobre disco, no a `parsear_bundle`, que
recibe el conjunto de documentos ya materializado en memoria. Son límites de
input lógico y no una garantía de RSS exacto o tiempo máximo.

## Checklist de release

- [x] Las capacidades del gate `v0.2.0` (evaluador, testigos, transacciones, demo CLI) están implementadas y probadas; la fase 08 corrió contra la instancia privada.
- [x] Los registros de evidencia enlazan commits o artefactos verificables.
- [x] `cargo test`, formato, Clippy y guard de privacidad pasan en CI.
- [x] La demo pública se ejecuta en el CI desde un checkout limpio (`check_public_cli_demo.py`, `run_software_architecture_demo.py`).
- [x] Las notas separan capacidades, límites y cambios incompatibles.
- [x] Las versiones de todos los crates coinciden con la release.
- [x] El tag anotado apunta al commit exacto validado por CI.
- [x] La release enlaza resultados, benchmarks y documentación relevante.

## Política de progreso

- No mover un gate para hacer coincidir una fecha.
- No etiquetar una capacidad que solo exista en documentación.
- No ocultar regresiones de utilidad detrás de corrección formal.
- No declarar `v1.0.0` mientras `validate`, `check`, `propose` o MCP cambien
  sin una política de compatibilidad.

## Verificación prevista

```powershell
git status --short
cargo test --workspace
git tag --list --sort=-version:refname
```

## Registro de evidencia

| Fecha | Tag | Commit | CI | Resultado |
|---|---|---|---|---|
| 2026-09-21 | `v0.1.0` | `7c4b81b` | verde | Base pública: modelo, OKF, AST y parser; sin check semántico. |
| 2026-11-18 | `v0.2.0` | ver el tag | verde (Linux y Windows) | [Notas](../release/v0.2.0.md): evaluador, criterio en las reglas, testigos localizados, transacciones, MCP, orquestador y estudio de cuatro brazos; fase 08 corrida contra la instancia privada. |
| 2026-11-18 | `v0.2.1` | ver el tag | verde (Linux y Windows) | [Notas](../release/v0.2.1.md): restricción de rango por aristas en el veredicto; compatible. |
| 2026-11-18 | `v0.3.0` | `220a24a` | verde (Linux y Windows); release con binarios para Linux, macOS y Windows | [Notas](../release/v0.3.0.md): búsqueda, historial consultable con `diff`, `why` y `search` en el MCP, instalación. Gate «loop MCP completo y equivalente al CLI»: el loop se cumple; la equivalencia es parcial a propósito (el MCP no escribe ni expone el historial). |
| 2026-11-19 | `v0.4.0` | `2a9bbe0` | verde (Linux y Windows); release con binarios para Linux, macOS y Windows | [Notas](../release/v0.4.0.md): criterio distribuible (fase 10). Gate redefinido por D5: esta fase como `v0.4.0` y la evaluación comparativa en `v0.5.0`. La fase sigue abierta hasta U10 (uso real). |
| 2026-11-19 | `v0.5.0` | `e029cbc` | verde (Linux y Windows); release con binarios para Linux, macOS y Windows | [Notas](../release/v0.5.0.md): evaluación del criterio (fase 11, neutral por la regla) y benchmarks publicados; arreglo del evaluador para guardas Edge con extremo fijado. |
| 2026-11-19 | `v0.6.0` | `9b5b128` | verde (Linux y Windows); release con binarios para Linux, macOS y Windows e instaladores sin Rust | [Notas](../release/v0.6.0.md): `universo conectar`, `universo grafo`, hooks con `felix.bin` e instaladores; probado en un entorno sin Rust. |

## Decisión posterior

Solo después de `v0.5.0` (evaluación comparativa) se decide, con evidencia, si corresponde invertir en
revisión AGM, recuperación avanzada y reglas de evolución DPO/SqPO.
