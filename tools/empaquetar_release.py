"""Empaqueta los binarios de release (U6) en un archivo por plataforma.

Uso: python tools/empaquetar_release.py --target <triple> --version <v>
       --bin-dir <dir con universo y universo-mcp> --out <dir>

Escribe `felix-felicis-<v>-<triple>.tar.gz` (o `.zip` en Windows) con los dos
binarios, LICENSE y README.md bajo un directorio raíz del mismo nombre, y al
lado `<archivo>.sha256` en el formato de `sha256sum`. Con los mismos binarios
el archivo sale idéntico byte a byte (fechas, dueños, modos y orden fijos);
los binarios mismos no se compilan de forma reproducible.
"""
import argparse
import gzip
import hashlib
import io
import re
import sys
import tarfile
import zipfile
from pathlib import Path

RAIZ_REPO = Path(__file__).resolve().parent.parent
BINARIOS = ("universo", "universo-mcp")
DOCUMENTOS = ("LICENSE", "README.md")
VERSION_VALIDA = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*")
FECHA_FIJA = 315532800  # 1980-01-01, lo mínimo que admite zip.


class ErrorEmpaquetado(Exception):
    pass


def _contenido(target: str, bin_dir: Path) -> list[tuple[str, bytes, bool]]:
    sufijo = ".exe" if "windows" in target else ""
    entradas = []
    for nombre in BINARIOS:
        ruta = bin_dir / f"{nombre}{sufijo}"
        if not ruta.is_file():
            raise ErrorEmpaquetado(f"falta el binario {ruta.name}")
        entradas.append((ruta.name, ruta.read_bytes(), True))
    for nombre in DOCUMENTOS:
        ruta = RAIZ_REPO / nombre
        if not ruta.is_file():
            raise ErrorEmpaquetado(f"falta {nombre}")
        entradas.append((nombre, ruta.read_bytes(), False))
    return entradas


def _tar_gz(raiz: str, entradas, destino: Path) -> None:
    datos = io.BytesIO()
    with tarfile.open(fileobj=datos, mode="w", format=tarfile.PAX_FORMAT) as tar:
        for nombre, contenido, ejecutable in entradas:
            info = tarfile.TarInfo(f"{raiz}/{nombre}")
            info.size = len(contenido)
            info.mode = 0o755 if ejecutable else 0o644
            info.mtime = FECHA_FIJA
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            tar.addfile(info, io.BytesIO(contenido))
    with open(destino, "wb") as salida:
        with gzip.GzipFile(filename="", mode="wb", fileobj=salida, mtime=0) as gz:
            gz.write(datos.getvalue())


def _zip(raiz: str, entradas, destino: Path) -> None:
    with zipfile.ZipFile(destino, "w", compression=zipfile.ZIP_DEFLATED) as z:
        for nombre, contenido, ejecutable in entradas:
            info = zipfile.ZipInfo(f"{raiz}/{nombre}", date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3  # Unix, para que los modos valgan en todo host.
            info.external_attr = (0o755 if ejecutable else 0o644) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, contenido)


def empaquetar(target: str, version: str, bin_dir: Path, out: Path) -> Path:
    if not VERSION_VALIDA.fullmatch(version) or not VERSION_VALIDA.fullmatch(target):
        raise ErrorEmpaquetado("versión o target inválido")
    entradas = _contenido(target, bin_dir)
    raiz = f"felix-felicis-{version}-{target}"
    out.mkdir(parents=True, exist_ok=True)
    windows = "windows" in target
    archivo = out / f"{raiz}{'.zip' if windows else '.tar.gz'}"
    (_zip if windows else _tar_gz)(raiz, entradas, archivo)
    suma = hashlib.sha256(archivo.read_bytes()).hexdigest()
    (out / f"{archivo.name}.sha256").write_text(f"{suma}  {archivo.name}\n", newline="\n")
    return archivo


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--target", required=True)
    p.add_argument("--version", required=True)
    p.add_argument("--bin-dir", required=True, type=Path)
    p.add_argument("--out", required=True, type=Path)
    a = p.parse_args(argv)
    try:
        archivo = empaquetar(a.target, a.version, a.bin_dir, a.out)
    except ErrorEmpaquetado as e:
        print(f"empaquetar: {e}", file=sys.stderr)
        return 1
    print(archivo)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
