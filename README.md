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

```powershell
cargo run -p universo-cli -- check examples/knowledge-base --manifiesto manifests/example.toml --invariantes examples/knowledge-base/rejected-invariants.sx --format json
cargo run -p universo-cli -- check examples/knowledge-base --manifiesto manifests/example.toml --invariantes examples/knowledge-base/rejected-invariants.sx --explain decide --format json
```

El comando usa código 0 para `admit`, 1 para `reject` o `error` y 2 para uso
inválido. En texto, el resultado va por `stdout` y los diagnósticos por
`stderr`; con `--format json`, emite un objeto JSON v1 en una sola línea por
`stdout` y deja `stderr` vacío, siempre que no se solicite `--explain`. Sin esa
opción, un `reject` informa solo cantidades, no contraejemplos, asignaciones ni
contenido del bundle. `--explain decide|enumerate` solicita una explicación
redactada en un pase opt-in separado; en JSON, esa salida usa schema v2. Un
archivo vacío o solo con comentarios se admite con cero invariantes, por lo que
no equivale a una comprobación sustantiva.

En el JSON baseline v1, `status` es `admit`, `reject` o `error`. `summary`
cuenta las fuentes, instancias expandidas, fórmulas evaluadas y fallidas; es
`null` en errores. `errors` contiene diagnósticos tipados (`data`, `authorship`
o `resource`) con código y ubicación cuando aplica. Los errores no incluyen
texto de fórmulas ni contraejemplos, aunque las rutas de diagnóstico pueden
ser sensibles y revelar rutas locales o identificadores; no publiques salidas
privadas.

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
```

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
explicación redactada usa `--explain decide` o `--explain enumerate`.

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
