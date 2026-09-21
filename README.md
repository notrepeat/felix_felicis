# felix_felicis

`felix_felicis` contiene el motor experimental Universo para representar
conocimiento como un multigrafo tipado,
comprobar invariantes de primer orden sobre estructuras finitas y serializar
el resultado como bundles OKF.

El repositorio contiene solamente el motor, el fundamento matemático y datos
ficticios. Las bases de conocimiento de sus usuarios viven fuera de este
repositorio y se entregan al CLI mediante una ruta.

## Estado

Implementado:

- modelo tipado en memoria;
- parser y serializador de bundles OKF;
- AST y parser de invariantes FO bisortidos;
- CLI de inspección (`atomos` y `verificar`);
- pruebas unitarias, de propiedades, compile-fail y round-trip.

En construcción:

- evaluador de invariantes y extracción de testigos;
- revisión, recuperación por patrones y operaciones transaccionales;
- servidor MCP.

## Probar el ejemplo

```powershell
cargo run -p universo-cli -- verificar examples/knowledge-base --manifiesto manifests/example.toml
cargo run -p universo-cli -- atomos examples/knowledge-base --manifiesto manifests/example.toml
```

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

## Licencia

GNU Affero General Public License v3.0 o posterior
(`AGPL-3.0-or-later`). Puedes usar, estudiar, modificar y redistribuir el
software. Si distribuyes una versión modificada —o permites que usuarios
interactúen con ella a través de una red— debes ofrecerles el código fuente
correspondiente bajo la misma licencia. Consulta [`LICENSE`](LICENSE).

La licencia obliga a compartir el código con los usuarios de la versión
modificada; no obliga a enviar un pull request a este repositorio. Aun así,
las contribuciones upstream son bienvenidas.
