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
├── states/
│   ├── contradictory/
│   ├── corrected/
│   └── gap/
├── expected/
└── README.md
```

1. Comando único para ejecutar toda la historia.
2. Salidas golden legibles y JSON.
3. Caso válido, contradicción por presencia y ausencia de un testigo existencial.
4. Prueba end-to-end ejecutada en CI.
5. Explicación de qué demuestra y qué no demuestra la demo.

El caso `gap` parte de un bundle no vacío: conserva tres nodos y una arista,
pero retira la arista `domain→port` y su `HasLab`, dejando sin testigo la
obligación existencial.

## Criterios de aceptación

Estas casillas evalúan la demo pública completa y autónoma: un clon limpio
debe recorrer la historia con el runner de un comando y comparar las salidas
con goldens. En `fab86f8`, el runner compara cuatro estados durables y 16
salidas, pero no reproduce la secuencia de deltas; la prueba Rust recorre esa
parte por separado y mapea el testigo abstracto a la arista y etiqueta
concretas solo dentro del test. Por esta política integral, la evidencia de
componentes no cierra ninguna casilla mientras falte un recorrido completo con
el runner.

- [ ] Un clon limpio reproduce la demo sin configuración privada.
- [ ] El caso contradictorio falla por la razón esperada.
- [ ] El testigo señala archivos y átomos concretos.
- [ ] La reparación pasa `validate`, `check` y round-trip.
- [ ] CI compara la salida con los goldens.
- [ ] La demo completa tarda y consume memoria medidos, no prometidos.

## Comandos de verificación

El runner disponible verifica los cuatro snapshots versionados y sus 16
salidas golden. También valida el contrato de las explicaciones y el presupuesto,
rechaza claves JSON duplicadas y comprueba que stdout, stderr y goldens no
contengan marcadores concretos de la fixture. La prueba de integración
verifica por separado las propuestas contradictoria y gap, su correspondencia
con los snapshots, la corrección explícita y el round-trip del candidato
corregido. El workflow de CI ejecuta el runner y la suite workspace en Linux y
Windows; todavía no hay resultado de CI remoto para `fab86f8`.

```powershell
python tools/run_software_architecture_demo.py
cargo test --locked --workspace --test demo_arquitectura
```

## Registro de evidencia

| Fecha | Commit/PR | Evidencia | Resultado |
|---|---|---|---|
| 2026-10-13 | `ae99906`, `c76e292` | Matriz local sobre `c76e292`: Windows, smoke pass y workspace 702 passed/0 failed/6 ignored (37 suites); WSL Ubuntu, smoke pass y 710 passed/0 failed/6 ignored (37 suites). `fmt`, `clippy` y `python tools/check_public_boundary.py` pasan en ambos. Cada smoke compila la CLI y ejecuta dos invocaciones directas (`admit`/`reject`), cuatro en total; no son pruebas Cargo. `admit`: exit 0, JSON v2 y resumen `sources/instances/evaluated/failed = 3/3/3/0`. `reject`: exit 1 y `1/1/1/1`; se validan streams, estructura/valores permitidos y ausencia de los marcadores conocidos de rutas/etiquetas/textos/IDs de la fixture. | Parcial: solo evaluación estática. No demuestra reparación, persistencia, round-trip/readmisión, goldens, `validate` ni recovery. La fase 05 sigue pendiente. |
| 2026-10-13 | `2534fd4`, `e0fc2e6` | Matriz local independiente sobre `e0fc2e6`: Windows y WSL Ubuntu pasan la prueba focal `demo_arquitectura` (1 passed/0 failed/0 ignored) y el smoke CLI admit/reject. `cargo test --workspace`: Windows 703 passed/0 failed/6 ignored (38 suites), WSL 711/0/6 (38 suites). `fmt`, `clippy` y `python tools/check_public_boundary.py` pasan en ambos; no hay resultado de CI remoto registrado para esta matriz. El test sintético admite el delta vacío sin mutar el bundle base y rechaza en core domain→adapter con testigo fuente 0, línea 2, columna 1; comprueba el base intacto tras el rechazo y tras construir en memoria el contradictorio. En esa versión, el test construye el delta correctivo domain→port→adapter y Felix lo admite. Solo el corregido se serializa/recarga; se comparan átomos/anexos y la segunda serialización es byte-estable para los tres Markdown. CLI `validate` y `check` se ejecutan solo sobre el corregido (3 nodos, 2 aristas, `failed=0`). | Parcial: corrección explícita y round-trip solo del corregido. En `e0fc2e6` el test aún construía el escenario en memoria; la demo autónoma, goldens/comparación y los demás límites siguen pendientes. |
| 2026-10-13 | `8fe29c1`, `434f4fc` | Matriz local sobre `434f4fc`: Windows y WSL Ubuntu pasan la prueba focal `demo_arquitectura` (1 passed/0 failed/0 ignored) y el smoke CLI admit/reject. `cargo test --workspace`: Windows 703 passed/0 failed/6 ignored (38 suites), WSL Ubuntu 711/0/6 (38 suites). `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `python tools/check_public_boundary.py` pasan en ambos. `8fe29c1` añade la fixture durable y `434f4fc` hace que el test consuma su bundle, manifiesto, invariante y deltas. `proponer` rechaza domain→adapter en core con testigo fuente 0, línea 2, columna 1; el candidato contradictorio se construye en memoria y la corrección domain→port→adapter es explícita en la fixture. Solo se serializa/recarga el candidato corregido; CLI `validate` y `check` corren solo sobre ese corregido (3 nodos, 2 aristas, `failed=0`). | Parcial: el test consume la fixture durable, pero no existe runner autónomo `demo.ps1` ni goldens/comparación. No prueba rechazo por CLI en esta historia, generación automatizada de la corrección ni carga/comparación de snapshots de estado. El candidato contradictorio permanecía solo en memoria. No demuestra `apply`, publicación, atomicidad ni recovery. La fase 05 sigue pendiente. |
| 2026-10-13 | `5510329` | Matriz local sobre HEAD `55103297`: Windows pasa `cargo test --locked --workspace --test demo_arquitectura` (1 passed/0 failed/0 ignored), el smoke CLI admit/reject JSON v2 y `cargo test --locked --workspace` (703 passed/0 failed/6 ignored, 38 suites); `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` y `python tools/check_public_boundary.py` pasan. WSL Ubuntu pasa las mismas comprobaciones, con workspace 711/0/6 (38 suites), en checkout aislado idéntico byte a byte de 184 archivos y 5.145.096 bytes. No se comprobó CI remoto. El test carga `states/contradictory` y `states/corrected` y compara átomos/anexos con los candidatos calculados. Los snapshots son datos ilustrativos de fixture, no salidas de `apply` ni correcciones generadas. El contradictorio runtime sigue en memoria, sin serialización/round-trip; solo el corregido pasa por round-trip y CLI `validate`/`check`. | Parcial: se verifica el vínculo entre ambos snapshots durables y candidatos, pero sigue sin runner autónomo, goldens/comparación ni rechazo por CLI en esta historia. No demuestra generación automática de corrección, `apply`, publicación, atomicidad o recovery. Fase 05 pendiente. |
| 2026-10-13 | `71784b0`, `179b5f7`, `e857d1c` | `71784b0` añade 12 goldens JSON/texto; `179b5f7` añade `tools/run_software_architecture_demo.py`; `e857d1c` valida los esquemas JSON y el testigo esperado. La matriz local aislada sobre `e857d1c` pasa en Windows y WSL Ubuntu: runner, 12/12 salidas exactas; prueba focal `demo_arquitectura`, 1 passed/0 failed/0 ignored; `cargo test --locked --workspace`, Windows 703 passed/0 failed/6 ignored y WSL 711/0/6 (38 suites en ambos); `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` y `python tools/check_public_boundary.py` pasan en ambos. Los checkouts aislados son idénticos byte por byte: 196 archivos y 2.429.126 bytes; los 12 goldens permanecieron byte por byte iguales, sin drift. `.github/workflows/ci.yml` invoca el runner en ambos jobs; no se comprobó CI remoto para este SHA. El runner evalúa `validate` y `check` en los estados base, contradictorio y corregido; no reproduce los deltas. El test Rust separado cubre el rechazo con `proponer`, la corrección explícita y el round-trip/CLI del corregido. | Parcial: hay ejecución local reproducible del runner y comparación con goldens, pero la demo integral sigue incompleta. El testigo expone posición de fuente y árbol de predicados, no nombre de archivo ni IDs/argumentos concretos de átomos; falta el caso de vacío por ausencia; no hay métricas de duración o RSS. No demuestra `apply`, publicación, atomicidad ni recovery. La fase 05 permanece pendiente. |
| 2026-10-13 | `3b05f42`, `fab86f8` | Matriz local aislada sobre `fab86f8`: Windows y WSL Ubuntu pasan el smoke CLI público, el runner (4 estados × `validate`/`check` × JSON/texto = 16/16 salidas exactas), `demo_arquitectura` (1 passed/0 failed/0 ignored), `cargo test --locked --workspace` (Windows 703/0/6; WSL 711/0/6; 38 suites en ambos), `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` y `python tools/check_public_boundary.py`. Los checkouts son idénticos byte por byte: 205 archivos, 2.451.333 bytes, SHA-256 canónico `bfd293225bddfd418aa6dfd1146525645ec0a72a0f7e0040f494089ea50a1781`; los 16 goldens LF permanecieron iguales, SHA-256 `1560c1995381ff042502a657d703d3c42523acaf0c3f91c692764218a0523faf`. El test Rust verifica que el delta retira la arista `domain→port` y su `HasLab` y que el snapshot coincide con el candidato; el testigo público sigue abstracto. No hay CI remoto registrado para este SHA. | Parcial: el caso de ausencia existe como obligación existencial sin testigo en un bundle no vacío, y hay checks locales reproducibles. La secuencia de deltas todavía se reparte entre el runner y el test; no se exponen archivos ni átomos concretos al público, ni se miden duración o RSS, ni se demuestra `apply`, publicación, atomicidad o recovery. Bajo la política integral, las seis casillas de fase 05 siguen abiertas. |

Las matrices sobre `e0fc2e6`, `434f4fc`, `55103297`, `e857d1c` y `fab86f8` son
locales; esta sección no registra un resultado de CI remoto para esos commits.
Los dos jobs del workflow ejecutan el runner, pero todavía no hay un run remoto
que lo acredite. El runner compara salidas de cuatro estados versionados; no
reproduce las propuestas ni produce la reparación. El testigo público sigue
siendo abstracto; la correspondencia concreta se verifica solo en el test Rust.
No hay medición de duración o RSS, ni evidencia de `apply`, publicación,
atomicidad o recovery. Como la secuencia integral aún no se reproduce desde un
clon limpio con un solo runner, fase 05 sigue pendiente y las seis casillas de
aceptación permanecen abiertas; el progreso global continúa en 3/9.

## Siguiente fase

[06 — Loop de agentes por MCP](06-loop-de-agentes-mcp.md).
