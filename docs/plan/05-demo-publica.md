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

Estas casillas evalúan la demo pública completa y autónoma: un clon limpio
debe recorrer la historia con el runner de un comando y comparar las salidas
con goldens. El registro siguiente conserva evidencia parcial de pasos
individuales; por sí sola, esa evidencia no cierra los criterios de la demo
integral.

- [ ] Un clon limpio reproduce la demo sin configuración privada.
- [ ] El caso contradictorio falla por la razón esperada.
- [ ] El testigo señala archivos y átomos concretos.
- [ ] La reparación pasa `validate`, `check` y round-trip.
- [ ] CI compara la salida con los goldens.
- [ ] La demo completa tarda y consume memoria medidos, no prometidos.

## Verificación prevista

Pendiente: `./examples/software-architecture/demo.ps1` aún no existe. Su
creación y ejecución siguen siendo el objetivo de esta verificación.

```powershell
./examples/software-architecture/demo.ps1
cargo test --workspace --test demo_arquitectura
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-10-13 | `ae99906`, `c76e292` | Matriz local sobre `c76e292`: Windows, smoke pass y workspace 702 passed/0 failed/6 ignored (37 suites); WSL Ubuntu, smoke pass y 710 passed/0 failed/6 ignored (37 suites). `fmt`, `clippy` y `python tools/check_public_boundary.py` pasan en ambos. Cada smoke compila la CLI y ejecuta dos invocaciones directas (`admit`/`reject`), cuatro en total; no son pruebas Cargo. `admit`: exit 0, JSON v2 y resumen `sources/instances/evaluated/failed = 3/3/3/0`. `reject`: exit 1 y `1/1/1/1`; se validan streams, estructura/valores permitidos y ausencia de los marcadores conocidos de rutas/etiquetas/textos/IDs de la fixture. | Parcial: solo evaluación estática. No demuestra reparación, persistencia, round-trip/readmisión, goldens, `validate` ni recovery. La fase 05 sigue pendiente. |
| 2026-10-13 | `2534fd4`, `e0fc2e6` | Matriz local independiente sobre `e0fc2e6`: Windows y WSL Ubuntu pasan la prueba focal `demo_arquitectura` (1 passed/0 failed/0 ignored) y el smoke CLI admit/reject. `cargo test --workspace`: Windows 703 passed/0 failed/6 ignored (38 suites), WSL 711/0/6 (38 suites). `fmt`, `clippy` y `python tools/check_public_boundary.py` pasan en ambos; no hay resultado de CI remoto registrado para esta matriz. El test sintético admite el delta vacío sin mutar el bundle base y rechaza en core domain→adapter con testigo fuente 0, línea 2, columna 1; comprueba el base intacto tras el rechazo y tras construir en memoria el contradictorio. En esa versión, el test construye el delta correctivo domain→port→adapter y Felix lo admite. Solo el corregido se serializa/recarga; se comparan átomos/anexos y la segunda serialización es byte-estable para los tres Markdown. CLI `validate` y `check` se ejecutan solo sobre el corregido (3 nodos, 2 aristas, `failed=0`). | Parcial: corrección explícita y round-trip solo del corregido. En `e0fc2e6` el test aún construía el escenario en memoria; la demo autónoma, goldens/comparación y los demás límites siguen pendientes. |
| 2026-10-13 | `8fe29c1`, `434f4fc` | Matriz local sobre `434f4fc`: Windows y WSL Ubuntu pasan la prueba focal `demo_arquitectura` (1 passed/0 failed/0 ignored) y el smoke CLI admit/reject. `cargo test --workspace`: Windows 703 passed/0 failed/6 ignored (38 suites), WSL Ubuntu 711 passed/0 failed/6 ignored (38 suites). `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `python tools/check_public_boundary.py` pasan en ambos. `8fe29c1` añade la fixture durable y `434f4fc` hace que el test consuma su bundle, manifiesto, invariante y deltas. `proponer` rechaza domain→adapter en core con testigo fuente 0, línea 2, columna 1; el candidato contradictorio se construye en memoria y la corrección domain→port→adapter es explícita en la fixture. Solo se serializa/recarga el candidato corregido; CLI `validate` y `check` corren solo sobre ese corregido (3 nodos, 2 aristas, `failed=0`). | Parcial: el test consume la fixture durable, pero no existe runner autónomo `demo.ps1` ni goldens/comparación. No prueba rechazo por CLI en esta historia, generación automatizada de la corrección, persistencia del contradictorio, `apply`, publicación, atomicidad ni recovery. La fase 05 sigue pendiente. |

Las matrices sobre `e0fc2e6` y `434f4fc` son locales; esta sección no registra
un resultado de CI remoto para esos SHA. El smoke previo cubre evaluación
estática por CLI y la prueba focal acredita los pasos parciales descritos en
la fila más reciente. Esa evidencia aún no satisface la demo integral con
runner y goldens; la fase 05 sigue pendiente y todas las casillas de
aceptación permanecen sin marcar. Los límites restantes están detallados en
esa misma fila.

## Siguiente fase

[06 — Loop de agentes por MCP](06-loop-de-agentes-mcp.md).
