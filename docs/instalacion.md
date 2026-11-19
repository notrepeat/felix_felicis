# Instalación

Felix se instala como dos ejecutables: `universo` (la CLI) y `universo-mcp`
(el servidor MCP local). **No necesitas Rust ni nada de su ecosistema** para
instalarlo ni usarlo: cada release trae binarios precompilados y un
instalador. Compilar desde el código queda para quien desarrolla Felix.

## Con el instalador (recomendado)

Las releases posteriores a `v0.5.0` traen, junto a los binarios, `instalar.sh`
(Linux y macOS) e `instalar.ps1` (Windows). Cada uno baja el archivo de tu
plataforma desde esa misma release y verifica su suma SHA-256: si no coincide,
no instala nada. Después prepara los dos ejecutables completos en una carpeta de
tu usuario antes de reemplazar ninguno, sin pedir permisos de administrador:
si la descarga o la verificación fallan, queda la versión anterior intacta. La suma viene de la misma release, así que detecta
una descarga corrupta o incompleta, no una release adulterada; cada instalador
trae también su propio `.sha256`.

En Linux o macOS, descarga `instalar.sh` desde la página de la release y
ejecútalo:

```sh
sh instalar.sh
```

Deja `universo` y `universo-mcp` en `~/.local/bin`. Si esa carpeta no está en
tu `PATH`, te dice qué línea agregar según tu shell (bash, zsh o fish). En
Linux necesita glibc 2.39 o posterior; en sistemas con musl, como Alpine, lo
dice y no instala. Solo necesita `curl` o `wget`, `tar` y
`sha256sum` o `shasum`, que vienen con el sistema.

En Windows, descarga `instalar.ps1`, revísalo, desbloquéalo y ejecútalo en
PowerShell:

```powershell
Unblock-File .\instalar.ps1
.\instalar.ps1
```

Si tu política de ejecución no permite scripts, usa
`powershell -ExecutionPolicy Bypass -File .\instalar.ps1` sólo para esa vez.

Deja `universo.exe` y `universo-mcp.exe` en
`%LOCALAPPDATA%\Programs\felix-felicis\bin` y agrega esa carpeta al `PATH` de
tu usuario, sin tocar el resto de sus entradas; abre una terminal nueva para
usarlos. Si `universo-mcp.exe` está en uso, por ejemplo por Claude Code, lo
reemplaza igual. En Windows ARM corre el binario x86_64 emulado.

Para dejar los binarios en otra carpeta, define `FELIX_BIN_DIR`. Para actualizar, corre
el instalador de la release nueva.

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

Copia los dos ejecutables a un directorio del `PATH` (por ejemplo,
`~/.local/bin`); es lo mismo que hace el instalador. Los binarios no están
firmados: macOS y Windows pueden advertir al abrirlos la primera vez. El de
Linux se compila en Ubuntu 24.04 y necesita glibc 2.39 o posterior (Ubuntu
24.04, Debian 13 o equivalentes); en sistemas más viejos, o en macOS Intel y
Linux ARM, que no tienen binario, instala desde el código.

## Desde el código (para desarrollar Felix)

Sólo hace falta para trabajar en Felix o para una plataforma sin binario
(macOS Intel, Linux ARM o glibc anterior a 2.39). Necesitas Git y Rust. El repositorio fija la toolchain `1.88.0` en
`rust-toolchain.toml`; con `rustup` se instala sola la primera vez.

```sh
git clone <url-del-repositorio> felix_felicis
cd felix_felicis
git checkout v0.6.0        # o la versión que quieras; sin esto, la rama actual
cargo install --locked --path crates/universo-cli
cargo install --locked --path crates/universo-mcp
```

Los binarios quedan en `~/.cargo/bin` (en Windows,
`%USERPROFILE%\.cargo\bin`), que `rustup` agrega al `PATH`. `--locked` usa
exactamente las dependencias de `Cargo.lock`. Para instalar en otro directorio,
agrega `--root <dir>` y pon `<dir>/bin` en el `PATH`.

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

## Skill de Claude Code para detectar criterio

`skills/felix-criterio/SKILL.md` enseña a un agente qué frases son criterio
(instrucciones generales, validaciones explícitas o implícitas), qué no lo es,
a preguntar ante la duda y a no guardar nada al cerrar una sesión sin tu
confirmación. `evaluation/run_criterio_deteccion.py` mide su precisión sobre un
corpus ficticio (fase 10, U13).

## Conectar un repositorio de criterio a Claude Code

Dentro del repositorio de criterio (el que crea `universo init --git`):

```sh
universo conectar            # muestra qué cambiaría, sin escribir nada
universo conectar --aplicar  # lo hace
universo conectar --verificar
```

`--aplicar` revisa cinco pasos y hace sólo los que faltan:

- `binario`: guarda la ruta de este `universo` en `git config felix.bin`, para
  que los hooks no dependan del `PATH`. Usa siempre el mismo binario, el
  instalado: correrlo con otro (por ejemplo, `target/debug/universo`) cambia
  la ruta guardada;
- `hooks_archivos`: pone al día `.githooks/` si son de una versión anterior.
  Como están versionados, queda un `git commit` pendiente. Un hook que no
  escribió Felix no se toca: el paso lo informa;
- `hooks_activos`: activa los hooks (`git config core.hooksPath .githooks`);
- `mcp`: registra el MCP `felix-criterio` en Claude Code a nivel de usuario
  (`claude mcp add --scope user`), con `universo-mcp` y el repositorio en rutas
  absolutas. El nombre es uno por usuario: si ya apunta a otro repositorio de
  criterio, la vista previa lo dice y `--aplicar` lo reemplaza. Un servidor con
  el mismo nombre en otro alcance (local o de proyecto) no se toca;
- `skill`: deja la skill de esta versión en `~/.claude/skills/felix-criterio/`.

Sólo toca la configuración local de Git de ese repositorio, la de Claude Code y
la carpeta de la skill: no hace commits, no cambia la shell ni otros programas.
Para revisar el MCP usa `claude mcp get`, que arranca el servidor registrado
para ver su estado (con un límite de 60 s). Si `claude` no está instalado,
aplica el resto, muestra el comando del MCP para correrlo a mano y termina con
1 hasta que el MCP quede registrado. Repítelo en cada computador donde clones el repositorio.

## Medir el uso del criterio

```sh
universo metricas                 # texto
universo metricas --format json   # para guardar una línea base y comparar después
```

Lo calcula todo desde lo que el repositorio ya guarda:

- **Actividad:** capturas, refuerzos, objeciones y promociones, separadas por
  autor (un autor `agente:…` es un agente; el resto, tú) y por semana.
- **Precisión del agente:** de lo que capturaron los agentes, qué parte
  confirmaste (reforzada en dos proyectos o promovida) frente a lo objetado o
  retirado.
- **Maduración:** días desde la captura hasta la promoción a `vigente`.
- **Estancadas:** candidatas sin segundo proyecto desde hace más de 30 días
  (`--estancada-dias`). `--hasta AAAA-MM-DD` fija la fecha de corte.
- **Uso del MCP**, si lo activas con `universo conectar --aplicar --medir-uso`.
  Cada llamada de un agente deja una línea en `.git/felix-uso-mcp.jsonl` (en un
  worktree, en su directorio de Git) con la fecha, la herramienta, el universo y
  si falló. Nunca guarda el contenido: ni la consulta ni la respuesta, y un
  universo que no existe queda como `otro`. Está dentro de Git, así que no se
  versiona y cada computador mide el suyo. Para
  apagarlo: `git config --unset felix.medirUso` y
  `universo conectar --aplicar`.

## Ver el criterio como grafo

```sh
universo grafo                       # todos los universos del repositorio
universo grafo --universo vigente    # uno solo
universo grafo --salida grafo.html   # dónde escribir la página
universo grafo <bundle> --manifiesto <m.toml> --invariantes <i.sx>  # cualquier bundle OKF
```

Genera una página HTML que se abre en el navegador:

- nodos coloreados por tipo; los criterios promovidos se ven huecos, las
  candidatas más claras y los retirados atenuados;
- relaciones;
- las promociones como líneas punteadas entre universos;
- anillos para las candidatas listas para promover (verde) y las objetadas
  (rojo).

Un clic en un nodo muestra sus atributos, relaciones y cuerpo. Sin `--salida`,
la página va al directorio temporal con un nombre nuevo cada vez
(`felix-grafo-<repositorio>-…html`); la ruta sale en la terminal. Incluye los datos del bundle y no carga
nada de la red: no la compartas si el repositorio es privado. Es de sólo
lectura; los cambios siguen pasando por `capturar`, `promover` y los demás
comandos.

## Desinstalar

Borra los dos ejecutables: `~/.local/bin/universo` y `~/.local/bin/universo-mcp`
(en Windows, la carpeta `%LOCALAPPDATA%\Programs\felix-felicis`). Si los
instalaste desde el código, `cargo uninstall universo-cli universo-mcp`. Si
usaste `universo conectar`, quita también el MCP con
`claude mcp remove felix-criterio --scope user` y la skill de
`~/.claude/skills/felix-criterio/`. Los repositorios de criterio son
directorios normales y no se tocan.
