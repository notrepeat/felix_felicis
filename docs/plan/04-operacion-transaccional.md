# 04 — Evaluar propuestas sin corromper el universo vigente

El objetivo es recibir un delta, construir un candidato aislado y publicar el
cambio únicamente cuando el check lo admita. Todo rechazo debe dejar estado y
bytes intactos.

## Estado

| Campo | Valor |
|---|---|
| Estado | pendiente |
| Depende de | 03 |
| Desbloquea | 05, 08 |

La fase 03 asigna a esta fase la aceptación de conflictos funcionales de un
delta (tipo, etiqueta, padre e incidencia). Esta asignación de alcance cierra
el criterio 17 de fase 03; no afirma que la detección o diagnóstico ya estén
implementados.

## Entregables

1. Modelo canónico de delta: átomos a añadir y retirar.
2. Detección y diagnóstico de conflictos funcionales de tipo, etiqueta,
   padre e incidencia al combinar el delta con el estado vigente.
3. Aplicación sobre copia completa en v1.
4. API `propose(estructura, delta, invariantes) -> veredicto`.
5. Persistencia atómica solo después de `admit`.
6. Comando CLI `propose` con modo de simulación predeterminado.
7. Registro del hash o identidad del estado evaluado.

## Criterios de aceptación

- [ ] Un delta válido produce el candidato esperado.
- [ ] Los conflictos funcionales se distinguen de las violaciones de
  invariantes y se informan sin publicar valores privados por defecto.
- [ ] Un delta rechazado no cambia memoria ni archivos.
- [ ] Un fallo de escritura no deja archivos parciales.
- [ ] El mismo delta contra el mismo estado produce el mismo veredicto.
- [ ] Un delta contra un estado diferente se detecta y no se aplica a ciegas.
- [ ] La optimización incremental queda diferida hasta tener mediciones.

## Prueba de seguridad central

1. Calcular hashes del bundle.
2. Proponer una contradicción conocida.
3. Confirmar `reject`.
4. Volver a calcular hashes.
5. Exigir igualdad byte a byte.

## Verificación prevista

```powershell
cargo test --workspace transaccion
cargo test -p universo-cli propose
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| — | — | — | — |

## Siguiente fase

[05 — Demo pública](05-demo-publica.md).
