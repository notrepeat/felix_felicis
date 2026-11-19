"""Tests de los instaladores sin Rust (`instalar.sh` y `instalar.ps1`).

Arman un release falso con `empaquetar_release.py` (binarios de mentira) y
corren el instalador contra él, sin red.
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import empaquetar_release as emp  # noqa: E402

VERSION = "v9.9.9"


def release_falso(dir_: Path, target: str) -> Path:
    """Un directorio con el archivo de `target`, su .sha256 y los instaladores
    preparados, como lo deja el workflow."""
    bins = dir_ / "bins"
    bins.mkdir()
    sufijo = ".exe" if "windows" in target else ""
    for nombre in emp.BINARIOS:
        ruta = bins / f"{nombre}{sufijo}"
        ruta.write_text(f"#!/bin/sh\necho '{nombre} 9.9.9'\n")
        ruta.chmod(0o755)
    dist = dir_ / "dist"
    emp.empaquetar(target, VERSION, bins, dist)
    emp.preparar_instaladores(VERSION, "duenio/felix_felicis", dist)
    return dist


class PrepararTest(unittest.TestCase):
    def test_el_release_reemplaza_repositorio_y_version(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            emp.preparar_instaladores(VERSION, "duenio/felix_felicis", out)
            for nombre in ("instalar.sh", "instalar.ps1"):
                texto = (out / nombre).read_text(encoding="utf-8")
                self.assertNotIn("__REPO__", texto)
                self.assertNotIn("__VERSION__", texto)
                self.assertIn("duenio/felix_felicis", texto)
                self.assertIn(VERSION, texto)
                self.assertTrue((out / f"{nombre}.sha256").is_file())

    def test_un_repositorio_o_version_invalidos_se_rechazan(self):
        with tempfile.TemporaryDirectory() as tmp:
            for repo, version in [("sin-barra", VERSION), ("a/b c", VERSION), ("a/b", "v1;rm")]:
                with self.assertRaises(emp.ErrorEmpaquetado):
                    emp.preparar_instaladores(version, repo, Path(tmp))

    def test_las_plantillas_no_nombran_un_repositorio_concreto(self):
        for nombre in ("instalar.sh", "instalar.ps1"):
            texto = (AQUI / nombre).read_text(encoding="utf-8")
            self.assertIn("__REPO__", texto)
            self.assertIn("__VERSION__", texto)


@unittest.skipIf(os.name == "nt" or shutil.which("sh") is None, "instalar.sh es para Linux y macOS")
class InstalarShTest(unittest.TestCase):
    TARGET = "x86_64-unknown-linux-gnu"

    def correr(self, script: Path, base: Path, destino: Path, **extra):
        # HOME temporal: aunque faltara FELIX_BIN_DIR, nada cae en la carpeta real.
        env = {**os.environ, "HOME": str(destino.parent), "FELIX_BASE_URL": base.as_uri(),
               "FELIX_BIN_DIR": str(destino), "FELIX_TARGET": self.TARGET, **extra}
        return subprocess.run(["sh", str(script)], env=env, capture_output=True, text=True, timeout=60)

    def test_instala_los_dos_binarios_sin_rust(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            dist = release_falso(tmp, self.TARGET)
            destino = tmp / "bin"
            r = self.correr(dist / "instalar.sh", dist, destino)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            for nombre in emp.BINARIOS:
                self.assertTrue(os.access(destino / nombre, os.X_OK), nombre)
            self.assertIn("universo 9.9.9", r.stdout)
            self.assertIn("universo conectar", r.stdout)
            # Sin restos temporales en el destino.
            self.assertEqual(sorted(p.name for p in destino.iterdir()), sorted(emp.BINARIOS))

    def test_una_suma_que_no_coincide_no_instala_nada(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            dist = release_falso(tmp, self.TARGET)
            archivo = next(dist.glob("*.tar.gz"))
            archivo.write_bytes(archivo.read_bytes() + b"x")
            destino = tmp / "bin"
            r = self.correr(dist / "instalar.sh", dist, destino)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("suma", r.stderr)
            self.assertFalse(destino.exists() and any(destino.iterdir()))

    def test_un_archivo_incompleto_no_reemplaza_nada(self):
        import hashlib
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            dist = release_falso(tmp, self.TARGET)
            # Rehacer el archivo sólo con `universo`, y su suma.
            archivo = next(dist.glob("*.tar.gz"))
            raiz = archivo.name[: -len(".tar.gz")]
            emp._tar_gz(raiz, [("universo", b"#!/bin/sh\necho nuevo\n", True)], archivo)
            suma = hashlib.sha256(archivo.read_bytes()).hexdigest()
            (dist / f"{archivo.name}.sha256").write_text(f"{suma}  {archivo.name}\n")
            destino = tmp / "bin"
            destino.mkdir()
            (destino / "universo").write_text("viejo")
            r = self.correr(dist / "instalar.sh", dist, destino)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("universo-mcp", r.stderr)
            self.assertEqual((destino / "universo").read_text(), "viejo")
            self.assertEqual(sorted(x.name for x in destino.iterdir()), ["universo"])

    def test_sin_preparar_por_el_release_no_corre(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            env = {**os.environ, "HOME": str(tmp), "FELIX_BIN_DIR": str(tmp / "bin")}
            env.pop("FELIX_BASE_URL", None)
            r = subprocess.run(["sh", str(AQUI / "instalar.sh")], env=env, capture_output=True, text=True,
                               timeout=60)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("release", r.stderr)

    def test_una_plataforma_sin_binario_lo_dice(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            dist = release_falso(tmp, self.TARGET)
            r = self.correr(dist / "instalar.sh", dist, tmp / "bin", FELIX_TARGET="riscv64-desconocido")
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("no hay binario", r.stderr)


@unittest.skipUnless(shutil.which("pwsh"), "requiere PowerShell (pwsh)")
class InstalarPs1Test(unittest.TestCase):
    TARGET = "x86_64-pc-windows-msvc"

    def correr(self, dist: Path, destino: Path):
        env = {**os.environ, "LOCALAPPDATA": str(destino.parent), "FELIX_BASE_URL": str(dist),
               "FELIX_BIN_DIR": str(destino), "FELIX_NO_PATH": "1", "FELIX_NO_VERIFICAR_VERSION": "1"}
        return subprocess.run(["pwsh", "-NoProfile", "-File", str(dist / "instalar.ps1")], env=env,
                              capture_output=True, text=True, timeout=120)

    def test_instala_los_dos_binarios(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            dist = release_falso(tmp, self.TARGET)
            destino = tmp / "bin"
            r = self.correr(dist, destino)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            for nombre in emp.BINARIOS:
                self.assertTrue((destino / f"{nombre}.exe").is_file(), nombre)

    def test_una_suma_que_no_coincide_no_instala_nada(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            dist = release_falso(tmp, self.TARGET)
            archivo = next(dist.glob("*.zip"))
            archivo.write_bytes(archivo.read_bytes() + b"x")
            destino = tmp / "bin"
            r = self.correr(dist, destino)
            self.assertNotEqual(r.returncode, 0)
            self.assertFalse(destino.exists() and any(destino.iterdir()))


if __name__ == "__main__":
    unittest.main()
