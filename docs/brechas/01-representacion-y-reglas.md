# 01 — Representación del conocimiento y lenguaje de reglas

Afirmaciones: A1 («lo que sabes y tus reglas se guardan como universo formal»)
y A2 («verificador determinista»). [Volver al maestro](00-maestro.md).

## Qué existe

| Capacidad | Estado | Evidencia |
|---|---|---|
| Cinco átomos: `Type`, `Edge`, `Parent`, `HasLab`, `Ext` | Implementado y probado | `crates/universo-core/src/atomos.rs:80-104` (reportado) |
| `Ext`: atributo unario de nodo, dominio cerrado, un valor por predicado | Implementado y probado | `crates/universo-core/src/manifiesto.rs:74-81`, `transaccion/ext.rs:74,83` (reportado) |
| Cuerpo Markdown y frontmatter no mapeado como anexo opaco, fuera de la garantía formal | Implementado | `crates/universo-core/src/anexo.rs:1-3` (reportado); `docs/06-limites.md` |
| Lenguaje `.sx`: `forall`/`exists` con guarda, `and`/`or`/`not`/`=>`/`=`, predicados de estructura, `tc Parent`, `Ext` | Implementado y probado | `crates/universo-core/src/sexpr.rs:926-989`, `formula.rs:108-153` (reportado) |
| Límites: profundidad 32, 64 ligaduras, 100.000 instancias, 1.000.000 unidades | Implementado y probado | `sexpr.rs:494`, `invariante.rs:158`, `invariante/expansion.rs:16`, `transaccion/propuesta.rs:17` (reportado) |
| Evaluador Tarski + oráculo; corpus 46 casos sin falsos admit | Implementado y probado | `evaluation/formal-and-performance.md`; fase 02 |
| Preorden ⪯ de atrincheramiento del modelo teórico | **Ausente** | sin coincidencias en `crates/` (verificado) |

A2 es la parte más sólida del sistema.

## Qué no se puede representar hoy

- **El contenido de una afirmación** como objeto lógico: el cuerpo es opaco.
- **Valores abiertos**: números, fechas, vigencia, grados de confianza (solo si
  se reducen a una lista cerrada de constantes).
- **Atributos multivaluados** y **atributos sobre aristas** (p. ej. la fuente de
  una relación); una arista solo lleva etiqueta.
- **Excepciones con nombre** y prioridad entre reglas.

## Qué reglas no se pueden expresar

- Cardinalidad («al menos dos fuentes»).
- Orden y fechas («vigente hasta»), aritmética, umbrales.
- Expresiones regulares sobre etiquetas.
- Clausura transitiva sobre relaciones distintas de `Parent`.
- **Reglas con id o mensaje humano**: `SentenciaFuente` guarda solo sentencia,
  línea y columna (`sexpr.rs:159-163`, reportado). Esto empobrece los rechazos
  (ver [02](02-rechazo-testigos-y-reparacion.md)).

## Brechas

| # | Brecha | Tamaño | Compuerta |
|---|---|---|---|
| R1 | Id y mensaje por regla en `.sx` | S–M | No |
| R2 | Cardinalidad y comparación de orden en `.sx`, con presupuesto | M–L | No (decisión de diseño) |
| R3 | Representar criterio: fechas/vigencia, confianza, fuente, multivalor, atributos de arista | L | Decisión de diseño: cambia OKF y semántica |
| R4 | Preorden ⪯ (prerrequisito de AGM) | L | **Sí**: AGM excluido por el plan |

R1 es barato y mejora mucho la utilidad del rechazo. R3 es la condición para
que «biblioteca de criterio» sea literal y no solo «grafo tipado con reglas».
