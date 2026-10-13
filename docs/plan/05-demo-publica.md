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
| 2026-10-13 | `ae99906`, `c76e292` | Matriz local sobre `c76e292`: Windows, smoke pass y workspace 702 passed/0 failed/6 ignored (37 suites); WSL Ubuntu, smoke pass y 710 passed/0 failed/6 ignored (37 suites). `fmt`, `clippy` y `python tools/check_public_boundary.py` pasan en ambos. Cada smoke compila la CLI y ejecuta dos invocaciones directas (`admit`/`reject`), cuatro en total; no son pruebas Cargo. `admit`: exit 0, JSON v2 y resumen `sources/instances/evaluated/failed = 3/3/3/0`. `reject`: exit 1 y `1/1/1/1`; se validan streams, estructura/valores permitidos y redacción. | Parcial: solo evaluación estática. No demuestra reparación, persistencia, round-trip/readmisión, goldens, `validate` ni recovery. La fase 05 sigue pendiente. |

La matriz es local y no aporta un resultado de CI remoto. El smoke no satisface
los criterios de reparación y readmisión de esta fase; todas las casillas de
aceptación permanecen sin marcar.

## Siguiente fase

[06 — Loop de agentes por MCP](06-loop-de-agentes-mcp.md).
