# Guardia: detectar cambios hechos fuera de Felix

El lock de Felix es advisory: una persona o un agente con acceso a los archivos
puede editar el Markdown, el manifiesto o las invariantes sin pasar por
`propose`. La guardia no lo impide; lo **detecta** de inmediato.

```text
universo seal  <raiz> --manifiesto <m.toml> --invariantes <i.sx> [--autor <a>] [--motivo <m>] [--format text|json]
universo guard <raiz> --manifiesto <m.toml> --invariantes <i.sx> [--format text|json]
```

## Sello

El sello es un archivo `.<raiz>.felix.seal` junto al root con el token de
snapshot (árbol del bundle, manifiesto e invariantes) del último estado que
Felix admitió:

- `propose --apply` lo retira antes de publicar y escribe el nuevo después de
  `Committed`, bajo el mismo lock, sólo si el árbol recapturado es exactamente
  el que se verificó e instaló (una edición ajena en esa ventana no queda
  sellada). La salida de apply admitido incluye `"sealed": true|false`; con
  `false` el texto añade `seal: not written`. Un apply interrumpido deja el
  estado sin sello; si además falló el retiro previo, queda el sello anterior,
  que `guard` informa como `mismatch` (alarma, nunca falso `match`).
- `seal` acepta a mano el estado actual, sólo si no hay transacción pendiente y
  las invariantes se cumplen. Úsalo para el primer sello o tras revisar un
  cambio manual correcto. Si había un sello de otro estado, deja en el
  historial una entrada `kind: "seal"` (con `--autor` y `--motivo`), que
  `universo log` muestra y que `universo diff` informa como `external_change`:
  no sabe qué cambió fuera de Felix, así que no compone a través de ella.
  JSON: `"external_change_recorded": true|false`. Un sello ilegible (tras un
  merge de Git queda con marcadores de conflicto) también cuenta como otro
  estado. Un sello atrasado respecto del historial (un apply que no pudo
  sellar) no: la última entrada ya deja ese estado. Si el historial no se
  puede escribir, `seal` falla con `seal_history_io` y no sella.

## Cambiar reglas o manifiesto

Editar `invariants.sx` o `manifest.toml` a mano hace fallar `guard`. Para
cambiarlos sin romper el historial:

```text
universo reglas aplicar <nuevo.sx> [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]
universo manifiesto aplicar <nuevo.toml> [--autor <a>] [--motivo <m>] [--dry-run] [--format text|json]
```

Dentro de un repositorio de criterio actúan sobre el universo elegido; fuera,
`universo reglas <raiz> aplicar <nuevo.sx> --manifiesto <m> --invariantes <i>`.
Bajo el lock exigen que el estado actual sea el sellado (si no,
`config_seal_mismatch`: una edición externa, o un universo nunca sellado, se
acepta antes con `seal`, que deja la edición en el historial) y comprueban que el bundle cumple la configuración nueva. Si
no, responden `reject` con id y mensaje de cada regla que falla y no escriben
nada. Si la cumple, instalan el archivo de forma atómica, sellan y añaden al
historial una entrada `kind: "config"` con el efecto sobre los átomos; `diff`
la cruza y la lista (`config changed (entry n)`, `config_changes`).

## Guard

`guard` combina tres comprobaciones y sale con 0 sólo si todas pasan:

| Campo JSON | Valores | Falla si |
|---|---|---|
| `transaction` | `clean`, `pending` | hay journal, stage, backup u otro sibling pendiente |
| `invariants` | `admit`, `reject` | el estado actual viola alguna invariante (`failed_invariants`) |
| `seal` | `match`, `mismatch`, `absent` | el estado no es el último admitido, o no hay sello |

La salida no incluye rutas ni datos del bundle. `guard` no toma el lock: es una
foto que puede quedar vieja si otro escritor actúa en paralelo.

## Hook de pre-commit y CI

[`examples/guardia/pre-commit`](../examples/guardia/pre-commit) ejecuta `guard`
con las variables `UNIVERSO`, `FELIX_ROOT`, `FELIX_MANIFIESTO` y
`FELIX_INVARIANTES`. Evalúa el árbol de trabajo, no el índice de git. En CI basta
un paso que ejecute el mismo comando sobre el checkout.

## Qué cubre el sello

El token incluye los bytes de cada archivo del bundle (Markdown y auxiliares),
la estructura de directorios (también los vacíos), el manifiesto y las
invariantes. No incluye permisos, propietario ni fechas de modificación: un
`chmod` hecho a mano no se detecta. Un sello que no es un archivo regular
(symlink, directorio) o está malformado hace fallar `guard` con `seal_invalid`.
Un temporal `.<raiz>.felix.seal.tmp` huérfano no bloquea: lo reemplaza el
siguiente `seal` o apply.

## Límites

- Detecta, no impide: quien edita a mano puede también ejecutar `seal`. El sello
  sirve contra cambios accidentales o de agentes sin ese paso, no contra un
  adversario con acceso al directorio.
- El sello no guarda historial ni procedencia (quién, cuándo, por qué).
- Tras un apply interrumpido, `recover` completa o revierte la transacción pero
  no vuelve a sellar. Antes de `seal`, revisa el estado resultante (por ejemplo
  con `git diff` en el repositorio de conocimiento): `seal` acepta cualquier
  estado que cumpla las invariantes.
