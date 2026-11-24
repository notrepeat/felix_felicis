#!/usr/bin/env python3
"""Extrae el universo del motor desde el código, la ayuda de los binarios y docs/.

Uso:
  python universo-motor/extraer.py <borrador> [--universo <bin>] [--universo-mcp <bin>]

Escribe <borrador>/bundle con una nota por crate, binario, comando, flag,
herramienta del MCP, código de error, documento y versión. No toca el bundle
admitido: el borrador se lleva al universo con `universo delta` y
`universo propose` (ver universo-motor/README.md).

Fuentes:
- crates y versiones: `crates/*/Cargo.toml` ([dependencies], sin dev-dependencies);
- comandos: la ayuda general de `universo --help`;
- flags: literales `"--x"` del código de universo-cli y universo-mcp;
- herramientas: `tools/list` de `universo-mcp --criterio` sobre un repositorio temporal;
- códigos: literales snake_case devueltos como código en el código fuente (sin pruebas);
- documentos: README.md, CHANGELOG.md, CONTRIBUTING.md y docs/**/*.md.
"""
import argparse
import json
import re
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
CAPAS = {"universo-core": "c1", "universo-okf": "c2", "universo-testkit": "c3",
         "universo-cli": "c3", "universo-mcp": "c3"}
BINARIOS = {"universo": "universo-cli", "universo-mcp": "universo-mcp"}
# Flags que el código de un binario sólo pasa a otro programa: `git config`, `claude mcp add`
# y la línea de `universo-mcp` que escribe `universo conectar`. No son flags de ese binario.
FLAGS_AJENAS = {"universo": {"--get", "--local", "--scope", "--registrar-uso"},
                # `--worker`: el proceso hijo que el servidor lanza para cada llamada.
                "universo-mcp": {"--worker"}}
# Convenciones que no se documentan una por una.
FLAGS_CONVENCION = {"--help", "--version"}
CODIGO = re.compile(
    r'(?:=>\s*|internal_error\(|invalid_params\(|\bcode:\s*|\bcodigo:\s*|"code"\s*:\s*)'
    r'"([a-z][a-z0-9]*(?:_[a-z0-9]+)+)"')


def slug(texto):
    return re.sub(r"[^a-z0-9]+", "-", texto.lower()).strip("-")


def es_de_pruebas(ruta):
    return "tests" in ruta.parts or ruta.name == "tests.rs"


def fuente_sin_pruebas(ruta):
    if es_de_pruebas(ruta):
        return ""
    return ruta.read_text(encoding="utf-8").split("#[cfg(test)]")[0]


def fuente_de_pruebas(ruta):
    texto = ruta.read_text(encoding="utf-8")
    if es_de_pruebas(ruta):
        return texto
    partes = texto.split("#[cfg(test)]", 1)
    return partes[1] if len(partes) == 2 else ""


def clase_de(rel):
    if rel == "README.md":
        return "readme"
    if rel == "CHANGELOG.md":
        return "changelog"
    partes = Path(rel).parts
    if partes[0] != "docs":
        return "otro"
    if len(partes) == 2:
        return "guia"
    return {"release": "release", "plan": "plan", "brechas": "brecha",
            "propuestas": "propuesta"}.get(partes[1], "otro")


def menciona(texto, termino):
    return re.search(r"(?<![\w-])" + re.escape(termino) + r"(?![\w-])", texto) is not None


def herramientas_mcp(bin_mcp, bin_cli):
    with tempfile.TemporaryDirectory(prefix="felix-motor-") as t:
        repo = Path(t) / "c"
        subprocess.run([bin_cli, "init", str(repo), "--git"], check=True, capture_output=True)
        p = subprocess.Popen([bin_mcp, "--criterio", str(repo)], stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)

        def enviar(m):
            p.stdin.write(json.dumps(m) + "\n")
            p.stdin.flush()

        enviar({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "extraer", "version": "0"}}})
        p.stdout.readline()
        enviar({"jsonrpc": "2.0", "method": "notifications/initialized"})
        enviar({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})
        nombres = sorted(h["name"] for h in json.loads(p.stdout.readline())["result"]["tools"])
        p.stdin.close()
        p.wait(timeout=10)
        return nombres


def comandos_de(ayuda):
    uso, descritos = set(), set()
    seccion = None
    for linea in ayuda.splitlines():
        if linea.startswith("Uso:"):
            seccion = "uso"
            continue
        if linea.startswith("Comandos:"):
            seccion = "comandos"
            continue
        if seccion == "uso" and linea.strip().startswith("universo "):
            for alternativa in linea.strip()[len("universo "):].split(" | "):
                cabeza = alternativa.split()[0]
                uso.update(c for c in cabeza.split("|") if re.fullmatch(r"[a-z][a-z-]*", c))
        elif seccion == "comandos":
            m = re.match(r"  ([a-z][a-z-]*)\s+\S", linea)
            if m:
                descritos.add(m[1])
    return uso, descritos


class Bundle:
    def __init__(self):
        self.notas = {}

    def nota(self, carpeta, stem, tipo, titulo, attrs=None):
        if stem not in self.notas:
            self.notas[stem] = {"carpeta": carpeta, "tipo": tipo, "titulo": titulo,
                                "attrs": attrs or {}, "aristas": {}}
        return stem

    def arista(self, origen, clave, destino):
        lista = self.notas[origen]["aristas"].setdefault(clave, [])
        if destino not in lista:
            lista.append(destino)

    def escribir(self, destino):
        for stem, n in sorted(self.notas.items()):
            carpeta = destino / "bundle" / n["carpeta"]
            carpeta.mkdir(parents=True, exist_ok=True)
            lineas = ["---", f"type: {n['tipo']}",
                      f"title: {json.dumps(n['titulo'], ensure_ascii=False)}"]
            lineas += [f"{k}: {v}" for k, v in sorted(n["attrs"].items())]
            for clave, destinos in sorted(n["aristas"].items()):
                lineas.append(f"{clave}:")
                lineas += [f'  - "[[{d}]]"' for d in sorted(destinos)]
            lineas += ["---", ""]
            (carpeta / f"{stem}.md").write_text("\n".join(lineas), encoding="utf-8")


def main():
    p = argparse.ArgumentParser()
    p.add_argument("borrador", type=Path)
    p.add_argument("--universo", default="universo")
    p.add_argument("--universo-mcp", default="universo-mcp")
    a = p.parse_args()
    b = Bundle()

    # Documentos
    docs = {}
    for ruta in [RAIZ / "README.md", RAIZ / "CHANGELOG.md", RAIZ / "CONTRIBUTING.md",
                 *sorted((RAIZ / "docs").rglob("*.md"))]:
        rel = ruta.relative_to(RAIZ).as_posix()
        stem = b.nota("documentos", "doc-" + slug(rel.removesuffix(".md")), "documento", rel,
                      {"clase": clase_de(rel)})
        docs[stem] = ruta.read_text(encoding="utf-8")
    usuario = {s: t for s, t in docs.items() if b.notas[s]["attrs"]["clase"] in ("readme", "guia")}

    # Crates y versiones
    cargos = sorted((RAIZ / "crates").glob("*/Cargo.toml"))
    datos = {c: tomllib.loads(c.read_text(encoding="utf-8")) for c in cargos}
    for c, d in datos.items():
        nombre = d["package"]["name"]
        b.nota("crates", "crate-" + nombre, "crate", nombre, {"capa": CAPAS[nombre]})
        version = d["package"]["version"]
        ver = b.nota("versiones", "ver-" + slug(version), "version", version)
        b.arista("crate-" + nombre, "en_version", ver)
    # Todas las versiones del CHANGELOG, no sólo la actual: así una versión
    # nunca desaparece del borrador al subir la de las crates (`delta` no
    # borra notas). Las reglas de versión miran la que apuntan las crates.
    for version in re.findall(r"^## (\d+\.\d+\.\d+)\b", docs["doc-changelog"], re.M):
        b.nota("versiones", "ver-" + slug(version), "version", version)
    for c, d in datos.items():
        for dep in d.get("dependencies", {}):
            if dep in CAPAS:
                b.arista("crate-" + d["package"]["name"], "depende_de", "crate-" + dep)
    for stem, n in list(b.notas.items()):
        if n["tipo"] != "version":
            continue
        if re.search(r"^## " + re.escape(n["titulo"]) + r"\b", docs["doc-changelog"], re.M):
            b.arista(stem, "documentado_en", "doc-changelog")
        notas_release = "doc-" + slug(f"docs/release/v{n['titulo']}")
        if notas_release in b.notas:
            b.arista(stem, "documentado_en", notas_release)

    # Binarios y comandos
    for binario in BINARIOS:
        b.nota("binarios", "bin-" + binario, "binario", binario)
    salida = subprocess.run([a.universo, "--help"], capture_output=True, text=True)
    ayuda_cli = salida.stdout + salida.stderr
    salida = subprocess.run([a.universo_mcp, "--help"], capture_output=True, text=True)
    ayudas = {"universo": ayuda_cli, "universo-mcp": salida.stdout + salida.stderr}
    uso, descritos = comandos_de(ayuda_cli)
    for comando in sorted(uso | descritos):
        stem = b.nota("comandos", "cmd-" + comando, "comando", f"universo {comando}")
        b.arista(stem, "de", "bin-universo")
        if comando in descritos:
            b.arista(stem, "en_ayuda", "bin-universo")
        for d, texto in usuario.items():
            if re.search(r"universo\s+(?:\S+\s+)?" + re.escape(comando) + r"(?![\w-])", texto):
                b.arista(stem, "documentado_en", d)

    # Flags
    for binario, crate in BINARIOS.items():
        flags = set()
        for ruta in (RAIZ / "crates" / crate / "src").rglob("*.rs"):
            flags.update(re.findall(r'"(--[a-z][a-z-]+)"', fuente_sin_pruebas(ruta)))
        for flag in sorted(flags - FLAGS_AJENAS[binario] - FLAGS_CONVENCION):
            stem = b.nota("flags", f"flag-{binario}-{flag[2:]}", "flag", f"{binario} {flag}")
            b.arista(stem, "de", "bin-" + binario)
            if menciona(ayudas[binario], flag):
                b.arista(stem, "en_ayuda", "bin-" + binario)
            for d, texto in usuario.items():
                if menciona(texto, flag):
                    b.arista(stem, "documentado_en", d)

    # Herramientas del MCP
    for nombre in herramientas_mcp(a.universo_mcp, a.universo):
        stem = b.nota("herramientas", "herr-" + nombre, "herramienta", nombre)
        b.arista(stem, "de", "bin-universo-mcp")
        for d, texto in docs.items():
            if re.search(r"^\|\s*`" + re.escape(nombre) + r"`\s*\|", texto, re.M):
                b.arista(stem, "documentado_en", d)

    # Códigos
    pruebas = {}
    for cargo in cargos:
        crate = cargo.parent.name
        pruebas[crate] = "".join(fuente_de_pruebas(r) for r in sorted(cargo.parent.rglob("*.rs")))
        for ruta in sorted((cargo.parent / "src").rglob("*.rs")):
            for codigo in CODIGO.findall(fuente_sin_pruebas(ruta)):
                stem = b.nota("codigos", "cod-" + slug(codigo), "codigo", codigo)
                b.arista(stem, "emitido_por", "crate-" + crate)
    for stem, n in b.notas.items():
        if n["tipo"] != "codigo":
            continue
        for d, texto in docs.items():
            if menciona(texto, n["titulo"]):
                b.arista(stem, "documentado_en", d)
        for crate, texto in pruebas.items():
            if menciona(texto, n["titulo"]):
                b.arista(stem, "probado_en", "crate-" + crate)

    b.escribir(a.borrador)
    cuenta = {}
    for n in b.notas.values():
        cuenta[n["tipo"]] = cuenta.get(n["tipo"], 0) + 1
    print(f"extraer: {len(b.notas)} notas; " + ", ".join(f"{k} {v}" for k, v in sorted(cuenta.items())),
          file=sys.stderr)


if __name__ == "__main__":
    main()
