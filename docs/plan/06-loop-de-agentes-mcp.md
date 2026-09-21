# 06 — Cerrar el loop de propuesta y corrección por MCP

El objetivo es que un agente consulte contexto, proponga un delta, reciba un
veredicto estructurado y corrija su propuesta sin acceso directo de escritura
al bundle.

## Estado

| Campo | Valor |
|---|---|
| Estado | pendiente |
| Depende de | 05 |
| Desbloquea | 07 |

## Herramientas mínimas

| Tool | Responsabilidad |
|---|---|
| `context` | Recuperar el subgrafo relevante y sus invariantes. |
| `propose` | Presentar un delta sin persistirlo. |
| `check` | Evaluar estado o candidato y devolver el veredicto. |
| `why` | Explicar un testigo o la procedencia disponible. |

## Entregables

1. Servidor MCP por `stdio` con contratos JSON versionados.
2. Adaptador fino: la lógica permanece en `universo-core`.
3. Límites de tamaño, tiempo y profundidad explícitos.
4. Harness que ejecuta la demo completa como cliente MCP.
5. Transcript sintético sin información personal.

## Criterios de aceptación

- [ ] MCP y CLI producen el mismo veredicto para la misma entrada.
- [ ] El agente no puede escribir el bundle saltándose `propose`.
- [ ] Un rechazo contiene datos suficientes para una segunda propuesta.
- [ ] La propuesta corregida es admitida en el escenario público.
- [ ] Reiniciar el servidor no cambia el resultado.
- [ ] Los errores de protocolo se distinguen de los veredictos del dominio.

## Fuera de alcance

- Revisión automática AGM.
- Destilación de conversaciones.
- Recuperación general de subgrafos costosos.

## Verificación prevista

```powershell
cargo test -p universo-mcp
cargo test --workspace --test loop_mcp
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| — | — | — | — |

## Siguiente fase

[07 — Validación empírica](07-validacion-empirica.md).
