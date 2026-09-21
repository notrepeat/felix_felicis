# 01 — Separar validación estructural de check semántico

El objetivo es impedir que una operación de sintaxis se confunda con una
garantía lógica. Al terminar, cada comando tendrá un nombre, alcance, salida y
código de proceso inequívocos.

## Estado

| Campo | Valor |
|---|---|
| Estado | pendiente |
| Depende de | — |
| Desbloquea | 02, 04, 05 |

## Punto de partida

- [x] La CLI carga manifiestos y bundles OKF.
- [x] `verificar` detecta errores de forma y estructura.
- [ ] La CLI distingue formalmente `validate` de `check`.
- [ ] Existe un contrato de salida legible y otro estructurado.

## Entregables

1. `validate`: valida manifiesto, formato OKF y coherencia estructural.
2. `check`: evalúa invariantes sobre una estructura ya válida.
3. Códigos de salida estables para éxito, veredicto negativo y error de uso.
4. Salida JSON versionada para consumidores automáticos.
5. Política documentada para retirar o conservar `verificar` como alias.

## Criterios de aceptación

- [ ] Un bundle malformado falla en `validate` sin ejecutar invariantes.
- [ ] Un bundle bien formado pero contradictorio pasa `validate` y falla `check`.
- [ ] Un error de autoría de invariantes se distingue de un error de datos.
- [ ] Los códigos de salida están cubiertos por pruebas de CLI.
- [ ] README y `--help` usan la misma terminología.

## Fuera de alcance

- Evaluar todavía las fórmulas.
- Aplicar propuestas o escribir el bundle.
- Exponer herramientas MCP.

## Verificación prevista

```powershell
cargo test -p universo-cli
cargo run -p universo-cli -- validate examples/knowledge-base --manifiesto manifests/example.toml
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| — | — | — | — |

## Siguiente fase

[02 — Evaluador de referencia](02-evaluador-de-referencia.md).
