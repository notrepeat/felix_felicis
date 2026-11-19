# Plantilla: memoria documental

Instancia del [capítulo 07](../../docs/07-memoria-documental.md) sin tocar el
motor: el porqué de cada criterio es grafo. `manifest.toml` declara los tipos
`Pregunta`, `Resolución` y `RegistroSupersesión`, las aristas `justifica`,
`responde`, `motiva`, `implica` y `supersede`, y los predicados `Standing` y
`Ts` (fecha). `invariants.sx` escribe ID1–ID4 como reglas con nombre.

```text
universo check bundle --manifiesto manifest.toml --invariantes invariants.sx --explain decide
```

Si un criterio pasa a `standing: firm` sin una resolución que lo justifique,
`check` lo rechaza y el texto de `--explain` muestra `rule: id=ID1 ...`.

Límites actuales: las fechas de `Ts` son un dominio cerrado declarado en el
manifiesto (no hay fechas abiertas) y no hay cardinalidad («al menos dos
fuentes»). La supersesión se recorre con dos aristas `supersede` a través del
registro: `desplazante → registro → desplazado`. Datos ficticios.
