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

## Entregables

1. Modelo canónico de delta: átomos a añadir y retirar.
2. Aplicación sobre copia completa en v1.
3. API `propose(estructura, delta, invariantes) -> veredicto`.
4. Persistencia atómica solo después de `admit`.
5. Comando CLI `propose` con modo de simulación predeterminado.
6. Registro del hash o identidad del estado evaluado.

## Criterios de aceptación

- [ ] Un delta válido produce el candidato esperado.
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
