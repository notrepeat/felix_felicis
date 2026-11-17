# 06 — Cerrar el loop de propuesta y corrección por MCP

El objetivo es que un agente consulte contexto, proponga un delta, reciba un
veredicto estructurado y corrija su propuesta sin acceso directo de escritura
al bundle.

## Estado

| Campo | Valor |
|---|---|
| Estado | en curso |
| Depende de | 05 |
| Desbloquea | 07 |

Contrato implementado y límites: [guía MCP](../mcp.md). Se añadió por decisión
del usuario una comparación con [Code Mode local opcional](../codemode-comparacion.md),
sin mover bundles a la nube. La validación local pasó; falta CI del commit exacto
para marcar el cierre.

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
| 2026-11-17 | `8ac2c69` | MCP directo: workspace Rust 1501/0/19, 45 suites; fmt/clippy y demo con CLI real, reinicio y bytes intactos. | Local pasa; CI exacto pendiente. |
| 2026-11-17 | Adaptador opcional, commit pendiente | SDK Cloudflare real local: aislamiento 8/8 y comparación A/B de 12 muestras. | Local pasa; no se infiere ahorro de tokens ni utilidad de un LLM. |

## Siguiente fase

[07 — Validación empírica](07-validacion-empirica.md).
