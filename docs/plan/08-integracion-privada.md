# 08 — Validar contra uso real sin publicar datos personales

El objetivo es usar la instancia privada como prueba ecológica del motor
público, conservando una frontera física y verificable entre producto y datos.

## Estado

| Campo | Valor |
|---|---|
| Estado | pendiente |
| Depende de | 04 |
| Desbloquea | 09 |

## Entregables

1. El repositorio privado consume un tag público explícito.
2. Suite de integración privada sobre el bundle real.
3. Guard que impide copiar fixtures reales al repositorio público.
4. Informe agregado sin títulos, citas, paths ni identificadores de sesión.
5. Procedimiento para reportar un bug mediante una reproducción ficticia.

## Criterios de aceptación

- [ ] El motor público no conoce la ubicación del repositorio privado.
- [ ] La integración privada puede actualizar de una versión etiquetada a otra.
- [ ] Los fallos reales se reducen a fixtures sintéticos antes de publicarse.
- [ ] El escáner de privacidad sigue verde después de cada reproducción.
- [ ] Ningún log de CI público contiene contenido del bundle privado.
- [ ] Las métricas agregadas no permiten reconstruir conceptos personales.

## Fuera de alcance

- Publicar el historial o los conceptos privados.
- Ejecutar CI público con secretos que monten el bundle real.
- Tratar la instancia privada como dependencia del motor.

## Verificación prevista

```powershell
# Se ejecuta únicamente en el repositorio privado.
cargo test --test private_bundle_integration
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| — | — | — | — |

## Siguiente fase

[09 — Versionado y evidencia](09-versionado-y-evidencia.md).
