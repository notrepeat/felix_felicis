# Guardia: detectar cambios hechos fuera de Felix

El lock de Felix es advisory: una persona o un agente con acceso a los archivos
puede editar el Markdown, el manifiesto o las invariantes sin pasar por
`propose`. La guardia no lo impide; lo **detecta** de inmediato.

```text
universo seal  <raiz> --manifiesto <m.toml> --invariantes <i.sx> [--format text|json]
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
  cambio manual correcto.

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
