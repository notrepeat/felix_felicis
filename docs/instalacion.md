# Instalación

Felix se instala como dos ejecutables: `universo` (la CLI) y `universo-mcp`
(el servidor MCP local). No hay paquete en crates.io ni en gestores del
sistema; hay dos caminos.

## Desde el código, con Cargo

Necesitas Git y Rust. El repositorio fija la toolchain `1.88.0` en
`rust-toolchain.toml`; con `rustup` se instala sola la primera vez.

```sh
git clone <url-del-repositorio> felix_felicis
cd felix_felicis
git checkout v0.2.1        # o la versión que quieras; sin esto, la rama actual
cargo install --locked --path crates/universo-cli
cargo install --locked --path crates/universo-mcp
```

Los binarios quedan en `~/.cargo/bin` (en Windows,
`%USERPROFILE%\.cargo\bin`), que `rustup` agrega al `PATH`. `--locked` usa
exactamente las dependencias de `Cargo.lock`. Para instalar en otro directorio,
agrega `--root <dir>` y pon `<dir>/bin` en el `PATH`.

## Binarios precompilados

Desde la primera versión publicada con el workflow de release
(`.github/workflows/release.yml`), cada release trae un archivo por
plataforma: Linux x86_64 (`x86_64-unknown-linux-gnu`), macOS Apple Silicon
(`aarch64-apple-darwin`) y Windows x86_64 (`x86_64-pc-windows-msvc`). Cada uno
contiene `universo`, `universo-mcp`, `LICENSE` y `README.md`, y va acompañado
de un `.sha256`. Las versiones hasta `v0.2.1` no los tienen: instala desde el
código.

En Linux (en macOS, `shasum -a 256 -c` en lugar de `sha256sum -c`):

```sh
sha256sum -c felix-felicis-<versión>-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf felix-felicis-<versión>-x86_64-unknown-linux-gnu.tar.gz
```

En Windows, con PowerShell:

```powershell
$archivo = "felix-felicis-<versión>-x86_64-pc-windows-msvc.zip"
$esperado = (Get-Content "$archivo.sha256").Split(" ")[0]
if ((Get-FileHash $archivo -Algorithm SHA256).Hash -ne $esperado) { throw "la suma no coincide" }
Expand-Archive $archivo -DestinationPath .
```

Copia los dos ejecutables a un directorio del `PATH`. Los binarios no están
firmados: macOS y Windows pueden advertir al abrirlos la primera vez. El de
Linux se compila en Ubuntu 24.04 y necesita glibc 2.39 o posterior (Ubuntu
24.04, Debian 13 o equivalentes); en sistemas más viejos, o en macOS Intel y
Linux ARM, que no tienen binario, instala desde el código.

## Comprobar la instalación

```sh
universo --version
universo-mcp --version
universo init mi-criterio
universo guard mi-criterio/bundle --manifiesto mi-criterio/manifest.toml --invariantes mi-criterio/invariants.sx
```

`guard` debe responder `guard: pass`. A partir de ahí, el [README](../README.md)
describe los comandos, [guardia](guardia.md) el hook de pre-commit y
[MCP](mcp.md) cómo conectar un agente: un cliente MCP lanza
`universo-mcp --root <bundle> --manifiesto <m.toml> --invariantes <i.sx>` por
`stdio`. Usa rutas absolutas en la configuración del cliente, porque el
directorio de trabajo del proceso lo decide el cliente.

## Desinstalar

```sh
cargo uninstall universo-cli
cargo uninstall universo-mcp
```

o borra los dos ejecutables si los copiaste a mano. Los repositorios de
conocimiento son directorios normales y no se tocan.
