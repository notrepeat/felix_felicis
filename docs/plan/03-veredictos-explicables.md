# 03 — Convertir fallos lógicos en evidencia accionable

El objetivo es que un rechazo no sea un booleano. Debe decir qué invariante
falló, qué átomos sostienen la contradicción y qué falta cuando la fórmula
incumplida exige existencia.

## Estado

| Campo | Valor |
|---|---|
| Estado | pendiente |
| Depende de | 02 |
| Desbloquea | 04, 05 |

## Entregables

1. Veredicto `admit`, `reject`, `gaps` o combinación de rechazo y vacíos.
2. Asignación concreta que falsifica cada invariante.
3. Testigo universal mínimo por inclusión.
4. Prescripción estructurada para fallos existenciales o `forall-exists`.
5. Serialización textual y JSON deterministas.
6. Telemetría honesta: `n`, cantidad de invariantes, variables y duración.

## Criterios de aceptación

- [ ] Cada rechazo reproduce la violación al evaluarse aisladamente.
- [ ] Quitar cualquier átomo de un testigo deja de sostener esa violación.
- [ ] Los testigos no prometen mínimo global ni unicidad.
- [ ] Un vacío identifica la asignación universal y la forma faltante.
- [ ] Dos ejecuciones sobre la misma entrada producen bytes idénticos.
- [ ] Modo `decidir` devuelve el primer resultado; `enumerar`, todos.

## Casos obligatorios

- Invariante universal violado por presencia.
- Invariante existencial violado por ausencia.
- Conflicto funcional de tipo, etiqueta, padre o incidencia.
- Entrada con contradicción y vacío simultáneos.

## Verificación prevista

```powershell
cargo test -p universo-core veredicto
cargo test -p universo-core testigo
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| — | — | — | — |

## Siguiente fase

[04 — Operación transaccional](04-operacion-transaccional.md).
