# 05 — Contar la prueba completa con una demo reproducible

El objetivo es que cualquier persona pueda observar, sin datos privados, una
contradicción, su testigo, una corrección y la readmisión del candidato.

## Estado

| Campo | Valor |
|---|---|
| Estado | pendiente |
| Depende de | 01–04 |
| Desbloquea | 06, 07 |

## Escenario elegido

Arquitectura de software: un componente de dominio no puede depender de un
adaptador externo. La propuesta inválida crea esa dependencia; la reparación
introduce un puerto y revierte la dirección.

## Entregables

```text
examples/software-architecture/
├── bundle/
├── invariants/
├── proposals/
│   ├── contradictory/
│   ├── gap/
│   └── valid/
├── expected/
└── README.md
```

1. Comando único para ejecutar toda la historia.
2. Salidas golden legibles y JSON.
3. Caso válido, contradicción por presencia y vacío por ausencia.
4. Prueba end-to-end ejecutada en CI.
5. Explicación de qué demuestra y qué no demuestra la demo.

## Criterios de aceptación

- [ ] Un clon limpio reproduce la demo sin configuración privada.
- [ ] El caso contradictorio falla por la razón esperada.
- [ ] El testigo señala archivos y átomos concretos.
- [ ] La reparación pasa `validate`, `check` y round-trip.
- [ ] CI compara la salida con los goldens.
- [ ] La demo completa tarda y consume memoria medidos, no prometidos.

## Verificación prevista

```powershell
./examples/software-architecture/demo.ps1
cargo test --workspace --test demo_arquitectura
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| — | — | — | — |

## Siguiente fase

[06 — Loop de agentes por MCP](06-loop-de-agentes-mcp.md).
