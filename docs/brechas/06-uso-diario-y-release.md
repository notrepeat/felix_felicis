# 06 — Uso diario, ergonomía y release

Afirmación: A10 («biblioteca personal» que se usa cada día).
[Volver al maestro](00-maestro.md).

## Superficie actual

CLI `universo` (`crates/universo-cli/src/main.rs:27`, verificado): `atomos`,
`validate`/`verificar`, `check [--explain]`, `propose [--emit-snapshot-token |
--apply --expect-snapshot <token>]`. Códigos 0/1/2. MCP: `context`, `propose`,
`check`, `why` (solo lectura).

## Cómo se agrega conocimiento hoy

- **Hechos**: editando Markdown a mano (lo que se salta Felix, ver
  [04](04-agentes-y-control-obligatorio.md)) o escribiendo un delta
  `felix-delta-v1` en JSON a mano, con átomos como cadenas, p. ej.
  `Edge(n/domain→n/port#depends#0, n/domain, n/port, depends)`.
- **Notas nuevas**: un delta **no puede crear nodos**. `Type` solo actúa sobre
  identidades existentes y las aristas exigen extremos existentes
  (`crates/universo-core/src/transaccion/tipos.rs:1-2,41-48`,
  `nodos.rs:62`, `aristas.rs:55-56`; verificado).
- **Reglas**: editando `invariants.sx` a mano.
- El manifiesto exige declarar de antemano sortes, aristas y dominios.

## Faltantes para uso diario

| # | Brecha | Evidencia | Tamaño | ¿Decide Romeo? |
|---|---|---|---|---|
| U1 | **Hecho (crear)** 2026-11-18: `felix-delta-v2` con `create`, apply y recovery; borrar no (retirar es retipar). Crear y borrar notas (nodos) por delta, con transacción y replay | ver arriba | L | Toca contratos de fase 04 |
| U2 | **Hecho** 2026-11-18 (variante borrador): `universo delta <raiz> --desde <borrador>` y `update` de cuerpos en v2. Generar el delta desde una edición: «edito la nota, `universo propose` calcula el delta contra el último estado admitido» | sin comando | M | Diseño de UX |
| U3 | **Hecho** (2026-11-17): `universo init <dir>` con manifiesto, invariantes con `(regla ...)` y una nota, sellado. `universo init` y plantillas de manifiesto | sin comando (reportado) | S | No |
| U4 | **Parcial** (2026-11-18): `log`, `show` (qué agregó y retiró cada cambio) y `search` (P3) hechos; falta `diff` entre dos estados. Búsqueda, `log`, `diff` | `log`, `show` y `search`; sin `diff`; ver [05](05-procedencia-y-vision.md) | M | Diseño |
| U5 | Mensajes de `check`/`propose` útiles para depurar sin romper la redacción por defecto | `main.rs:769-853` (reportado) | S | Privacidad vs. utilidad |
| U6 | **Hecho en lo reversible** (2026-11-18): `cargo install --locked --path` probado de punta a punta (CLI, guardia, búsqueda y MCP con los binarios instalados), `--version` en ambos binarios, [guía de instalación](../instalacion.md) y workflow de release (`.github/workflows/release.yml`) que compila Linux x86_64, macOS arm64 y Windows x86_64, prueba los binarios y sube archivos con `.sha256` a un borrador de release al empujar un tag. Pendiente de decisión: publicar en crates.io (irreversible, requiere cuenta; `publish = false` sigue) y firmar binarios. Original: Instalación: `publish = false` en `Cargo.toml:9` (verificado), sin binarios ni workflow de release | `cargo install`, workflow de release sin ejecutar aún sobre un tag | S–M | Canal de distribución y licencia (licencia: AGPL-3.0-or-later, ya fijada) |
| U7 | ~~CI corre solo un test Python de `tools/` y ningún test de `evaluation/`~~ Corregido 2026-11-17: CI corre `tools/test_*.py` y `evaluation/test_*.py` en Linux y Windows (Windows sin ejecución confirmada todavía). `cargo test` y `clippy` con `--locked` desde 2026-11-17 | — | S | No |

## Fases 08 y 09

| Fase | Falta | Bloqueo |
|---|---|---|
| 08 Integración privada | Consumir un tag como dependencia git (no probado; `v0.1.0` es anterior a `check`/`propose`/MCP); suite privada; guard anti-filtración más allá de la lista negra de `tools/check_public_boundary.py`; informe agregado | **Revisión de licencia AGPL-3.0-or-later** y decisión de iniciar |
| 09 Versionado | Subir versión de los cinco crates, notas de release orientadas al usuario (hoy `CHANGELOG.md` es un registro de commits), tag anotado sobre commit validado en CI, checklist de 8 puntos | Depende de 08 según el plan; el gate técnico de `v0.2.0` ya se cumple. **Decisión de Romeo** |

U1 y U2 son los que más acercan Felix a un uso real: sin ellos, alimentar la
biblioteca a través de la barrera exige escribir átomos en JSON a mano.
