# felix_felicis

`felix_felicis` contiene el motor experimental Universo para representar
conocimiento como un multigrafo tipado, definir invariantes de primer orden y
serializar estructuras como bundles OKF. `universo-core` ofrece evaluación
de referencia de fórmulas y la CLI incluye `check` para validar y evaluar
invariantes sobre un bundle local.

El repositorio contiene solamente el motor, el fundamento matemático y datos
ficticios. Las bases de conocimiento de sus usuarios viven fuera de este
repositorio y se entregan al CLI mediante una ruta.

## Estado

Implementado:

- modelo tipado en memoria;
- parser y serializador de bundles OKF;
- AST y parser de invariantes FO bisortidos;
- validación estática, expansión de esquemas y evaluador de referencia en la API de `universo-core`;
- `check --explain decide|enumerate` opt-in, con explicaciones redactadas en texto o JSON v2; el resultado baseline conserva JSON v1;
- CLI (`atomos`, `validate`, `check`; `verificar` es alias compatible de `validate`);
- pruebas unitarias, de propiedades, compile-fail y round-trip.

En construcción:

- revisión, recuperación por patrones y operaciones transaccionales (fase 04, parcial);
- servidor MCP.

## Probar el ejemplo

```powershell
cargo run -p universo-cli -- validate examples/knowledge-base --manifiesto manifests/example.toml
cargo run -p universo-cli -- atomos examples/knowledge-base --manifiesto manifests/example.toml
cargo run -p universo-cli -- check examples/knowledge-base --manifiesto manifests/example.toml --invariantes examples/knowledge-base/invariants.sx
```

`validate` comprueba estructura, no invariantes. `verificar` conserva el mismo
comportamiento como alias. El archivo [de invariantes públicos](examples/knowledge-base/invariants.sx)
contiene fórmulas diseñadas para el bundle ficticio. La
[fixture de rechazo](examples/knowledge-base/rejected-invariants.sx) es
intencionalmente falsa y está separada del ejemplo normal; al ejecutarla,
`check` devuelve `reject` y código 1.

`check` evalúa las invariantes estáticamente sobre el bundle: no modifica ni
repara archivos, no persiste resultados y no ejecuta recovery.

```powershell
cargo run -p universo-cli -- check examples/knowledge-base --manifiesto manifests/example.toml --invariantes examples/knowledge-base/rejected-invariants.sx --format json
cargo run -p universo-cli -- check examples/knowledge-base --manifiesto manifests/example.toml --invariantes examples/knowledge-base/rejected-invariants.sx --explain decide --format json
```

El comando usa código 0 para `admit`, 1 para `reject` o `error` y 2 para uso
inválido. En texto, el resultado va por `stdout` y los diagnósticos por
`stderr`; con `--format json` y uso válido, el proceso `universo-cli` escribe un
objeto JSON en una sola línea por `stdout` y deja su propio `stderr` vacío. Los
errores de uso no producen JSON y escriben el diagnóstico por `stderr`. El
schema es v1 sin `--explain` y v2 cuando se solicita. Al invocarlo mediante
`cargo run`, Cargo puede emitir sus propios mensajes de compilación,
lanzamiento o fallo.
Sin `--explain`, un `reject` informa solo cantidades, no contraejemplos,
asignaciones ni contenido del bundle. `--explain decide|enumerate` solicita una
explicación redactada en un pase opt-in separado. Un archivo vacío o solo con
comentarios se admite con cero invariantes, por lo que no equivale a una
comprobación sustantiva.

En el JSON baseline v1, `status` es `admit`, `reject` o `error`. `summary`
cuenta las fuentes, instancias expandidas, fórmulas evaluadas y fallidas; es
`null` en errores. `errors` contiene diagnósticos tipados (`data`, `authorship`
o `resource`) con código y ubicación cuando aplica. Los errores no incluyen
texto de fórmulas ni contraejemplos, aunque las rutas de diagnóstico del
baseline v1 pueden ser sensibles y revelar rutas locales o identificadores.

Para el comando `check`, una admisión de este ejemplo tiene la forma:

```json
{
  "schema_version": 1,
  "command": "check",
  "status": "admit",
  "summary": { "sources": 3, "instances": 3, "evaluated": 3, "failed": 0 },
  "errors": []
}
```

La fixture falsa produce `status: "reject"`, `failed: 1` y código 1; no añade
una explicación porque ese comando no solicita `--explain`.

## Desarrollo

```powershell
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
python tools/check_public_boundary.py
python tools/check_public_cli_demo.py
python tools/run_software_architecture_demo.py
```

El smoke compila el binario y lo ejecuta directamente para comprobar los
streams propios de la CLI, aislados de los mensajes del wrapper `cargo run`.
El [runner de la fixture pública de arquitectura](examples/software-architecture/README.md)
compara `validate` y `check` en JSON y texto para cuatro estados versionados
con 16 goldens. Inspecciona esos snapshots; no reproduce la secuencia de
deltas. También valida el contrato de las explicaciones y su presupuesto,
rechaza claves JSON duplicadas y comprueba que las salidas y los goldens no
contengan marcadores concretos de esta fixture.

El estudio que fundamenta el diseño comienza en
[`docs/00-indice.md`](docs/00-indice.md).

El plan ejecutable para demostrar que el sistema funciona está en
[`docs/plan/00-plan-maestro.md`](docs/plan/00-plan-maestro.md).

## Datos privados

No copies una base de conocimiento real dentro de este repositorio. Consulta
[`PUBLIC_BOUNDARY.md`](PUBLIC_BOUNDARY.md) antes de publicar cambios.
Las herramientas reciben rutas locales y leen los bundles; `atomos` muestra
átomos y algunos diagnósticos pueden incluir rutas del bundle o rutas absolutas
de archivos. No publiques sus salidas cuando se ejecuten sobre una instancia
privada. `check` separa errores de autoría, datos y recursos de un rechazo
semántico. El resultado baseline no incluye contraejemplos; para pedir una
explicación redactada usa `--explain decide` o `--explain enumerate`. La salida
`--explain` omite IDs y etiquetas de entidades, texto de fórmula y rutas en
diagnósticos de error, pero revela predicados, valores de verdad y resultados derivados,
estructura de reglas, posiciones de fuente y contadores. No publiques salidas
de `--explain` generadas sobre instancias privadas. La comprobación de
marcadores del runner corresponde a la fixture pública; no certifica salidas
de bundles privados.

## Licencia

GNU Affero General Public License v3.0 o posterior
(`AGPL-3.0-or-later`). Puedes usar, estudiar, modificar y redistribuir el
software. Al distribuir copias debes cumplir sus condiciones. Si modificas
el programa y tu versión permite interacción remota por red, la sección 13
exige ofrecer a quienes interactúan una vía gratuita para obtener el código
fuente correspondiente. Consulta [`LICENSE`](LICENSE) y la
[sección 13 de la AGPLv3](https://www.gnu.org/licenses/agpl-3.0.html).

Esto no exige enviar un pull request a este repositorio. Aun así, las
contribuciones upstream son bienvenidas.
