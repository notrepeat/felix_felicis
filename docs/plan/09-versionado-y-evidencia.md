# 09 — Hacer que cada versión denote evidencia real

El objetivo es que un tag no signifique «trabajo acumulado», sino un conjunto
preciso de capacidades demostradas, con CI, demo e informe enlazados.

## Estado

| Campo | Valor |
|---|---|
| Estado | pendiente |
| Depende de | 07, 08 |
| Desbloquea | Decisión sobre capacidades avanzadas |

## Gates propuestos

| Versión | Gate mínimo |
|---|---|
| `v0.2.0` | Evaluador, testigos, transacciones y demo CLI reproducible. |
| `v0.3.0` | Loop MCP completo y equivalente al CLI. |
| `v0.4.0` | Evaluación comparativa y benchmarks publicados. |
| `v1.0.0` | Contratos declarados estables y evidencia sostenida en uso real. |

Mientras la API sea `0.x`, los parches corrigen comportamiento compatible y
los incrementos menores pueden cambiar contratos con migración documentada.

## Migración prevista para 0.2.0 (propuesta, no publicada)

La siguiente versión menor es el destino recomendado para acumular estos
cambios de contrato pre-1.0. El gate sigue pendiente: no se suben versiones de
los cinco crates, no se crea un tag y no se presenta `0.2.0` como release hasta
que termine la matriz completa de release.

| Cambio | Acción del consumidor |
|--------|-----------------------|
| `Regla::LimiteBundleExcedido` y `ErrorEstructura::HijoDuplicadoEnLote` agregan variantes a enums exhaustivos. | Añadir ramas para ambas variantes antes de actualizar. |
| `cargar_bundle` limita la carga desde disco a 128 MiB de Markdown leído, 100.000 entradas, profundidad 128 y 16 MiB de rutas lógicas acumuladas. | Reducir o dividir bundles que excedan una cuota. Los límites son inclusivos y se comparten entre todas las raíces del layout. |
| Las raíces `layout.directorios` deben ser disjuntas; una ruta no puede ser igual ni ancestro/descendiente de otra. | Separar las raíces solapadas en directorios disjuntos. |
| Errores de lectura exponen `regla` y `path` para clasificación/localización. | No analizar `detalle`: es texto humano de diagnóstico, puede variar por plataforma y no es un contrato estable. |

Las cuotas se aplican a `cargar_bundle` sobre disco, no a `parsear_bundle`, que
recibe el conjunto de documentos ya materializado en memoria. Son límites de
input lógico y no una garantía de RSS exacto o tiempo máximo.

## Checklist de release

- [ ] Todas las fases exigidas por el gate figuran como `completo`.
- [ ] Los registros de evidencia enlazan commits o artefactos verificables.
- [ ] `cargo test`, formato, Clippy y guard de privacidad pasan en CI.
- [ ] La demo pública se ejecuta desde un clon limpio.
- [ ] Las notas separan capacidades, límites y cambios incompatibles.
- [ ] Las versiones de todos los crates coinciden con la release.
- [ ] El tag anotado apunta al commit exacto validado por CI.
- [ ] La release enlaza resultados, benchmarks y documentación relevante.

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

## Decisión posterior

Solo después de `v0.4.0` se decide, con evidencia, si corresponde invertir en
revisión AGM, recuperación avanzada y reglas de evolución DPO/SqPO.
