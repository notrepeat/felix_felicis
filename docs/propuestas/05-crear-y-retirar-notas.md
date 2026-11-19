# Propuesta: crear y retirar notas por delta (paso 5)

Estado: **aprobada (opción C con sintaxis B) e implementada** el 2026-11-18,
fases 1 a 4. Desviación respecto del plan: no se creó `VERSION_MOTOR_V2`; el
contexto guarda v1 si el delta no crea notas (bytes idénticos a los previos) y
v2 si las crea, y un motor anterior falla cerrado ante v2, así que la etiqueta
de semántica no cambia. Fecha original: 2026-11-17. Origen: paso 5 de la [hoja de ruta](../brechas/00-maestro.md) y U1
en [06](../brechas/06-uso-diario-y-release.md).

## Problema

Un delta `felix-delta-v1` sólo cambia hechos sobre identidades que ya existen:
`Type` no crea nodos (`crates/universo-core/src/transaccion/tipos.rs:1-2`) y
`HasLab`/`Edge` exigen sujetos y extremos existentes (`nodos.rs`,
`aristas.rs`). Para agregar una nota hay que crear el Markdown a mano, que es
justo lo que la guardia marca como cambio fuera de Felix. Sin esto, alimentar
la biblioteca a través de la barrera sigue siendo incómodo.

## Por qué necesita decisión

Toca contratos cerrados de la fase 04:

1. **Formato del delta.** Hace falta expresar «nace la identidad `n/x` con tipo,
   etiqueta y cuerpo». Es un esquema nuevo (`felix-delta-v2`) o una extensión de
   v1; v1 es estricto (`SchemaNoSoportado`).
2. **Replay y journal.** El contexto de admisión guarda el delta y el recovery
   lo reproduce con `semantica_archivada` (`VERSION_MOTOR_V1`, `POLICY_ID_V1`).
   Un delta que crea archivos cambia lo que el replay debe reconstruir; exige un
   tag de semántica nuevo y mantener v1 para journals existentes.
3. **Cuerpo Markdown.** Hoy el cuerpo es anexo opaco fuera de la garantía
   formal. Crear una nota obliga a decidir si el delta transporta el cuerpo
   (bytes no verificados) o sólo el frontmatter mínimo.
4. **Retiro.** El capítulo 03 y la adenda de ID4 dicen que un nodo desplazado se
   retipa a `Retirado`, no se borra. Borrar archivos por delta sería una
   decisión nueva (¿se permite? ¿sólo lápidas?).

## Opciones

| Opción | Qué es | A favor | En contra |
|---|---|---|---|
| A. `Nodo` como átomo de v2 | `Nodo(n/x, tipo, "etiqueta")` en `add`; en `remove` sólo si no quedan hechos sobre él | Encaja en el modelo de átomos; el check ya razona sobre identidades | Formato y replay nuevos; el cuerpo queda fuera |
| B. Sección `create` en v2 | `{"create":[{"path":"n/x","type":"T","title":"...","body":"..."}], "add":[...], "remove":[...]}` | Lo más ergonómico; trae el cuerpo | Mezcla hechos formales con bytes opacos; más superficie de validación |
| C. Sólo retipado a `Retirado` + creación por B | Crear sí, borrar nunca; retirar = `Type(n/x, Retirado)` | Coherente con ID4 y el capítulo 03; sin borrado | Las lápidas se acumulan |

Recomendación: **C con la sintaxis de B**: `felix-delta-v2` con `create` (path,
tipo, etiqueta, cuerpo opcional como anexo opaco) y sin borrado de archivos;
retirar sigue siendo retipar. Mantener v1 intacto y versionar la semántica de
replay (`VERSION_MOTOR_V2`) para que los journals v1 sigan recuperándose.

## Trabajo si se aprueba

1. `felix-delta-v2` en `universo-okf/src/delta.rs` y `Delta` con altas de nodo
   en `universo-core` (candidato, conflictos de path, layout de directorios).
2. Serialización del nuevo Markdown en el stage y su inventario esperado.
3. Contexto de admisión y replay v2 con tag de semántica nuevo; tests de
   recovery con un apply cortado en cada fase que crea archivos.
4. CLI/MCP: aceptar v2 en `propose`; `--explain` sin cambios.
5. Documentación y la guía de U2 («generar el delta desde una nota editada»),
   que depende de esta decisión.

Tamaño estimado: L. Riesgo principal: el replay de journals mixtos v1/v2.
