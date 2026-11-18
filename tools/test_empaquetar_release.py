"""Tests de tools/empaquetar_release.py (U6: binarios de release)."""
import hashlib
import stat
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import empaquetar_release as er  # noqa: E402


class EmpaquetarTest(unittest.TestCase):
    def preparar(self, base: Path, windows: bool) -> Path:
        bins = base / "bin"
        bins.mkdir()
        sufijo = ".exe" if windows else ""
        for nombre in er.BINARIOS:
            (bins / f"{nombre}{sufijo}").write_bytes(f"binario {nombre}".encode())
        return bins

    def test_linux_tar_gz_con_binarios_ejecutables_licencia_y_checksum(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            bins = self.preparar(base, windows=False)
            archivo = er.empaquetar(
                "x86_64-unknown-linux-gnu", "v0.2.1", bins, base / "dist"
            )
            self.assertEqual(
                archivo.name, "felix-felicis-v0.2.1-x86_64-unknown-linux-gnu.tar.gz"
            )
            with tarfile.open(archivo) as tar:
                miembros = {m.name: m for m in tar.getmembers()}
            raiz = "felix-felicis-v0.2.1-x86_64-unknown-linux-gnu"
            self.assertEqual(
                set(miembros),
                {f"{raiz}/universo", f"{raiz}/universo-mcp", f"{raiz}/LICENSE", f"{raiz}/README.md"},
            )
            self.assertTrue(miembros[f"{raiz}/universo"].mode & stat.S_IXUSR)
            self.assertFalse(miembros[f"{raiz}/LICENSE"].mode & stat.S_IXUSR)
            suma = (base / "dist" / f"{archivo.name}.sha256").read_text()
            esperado = hashlib.sha256(archivo.read_bytes()).hexdigest()
            self.assertEqual(suma, f"{esperado}  {archivo.name}\n")

    def test_windows_zip_con_exe(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            bins = self.preparar(base, windows=True)
            archivo = er.empaquetar("x86_64-pc-windows-msvc", "v0.2.1", bins, base / "dist")
            self.assertTrue(archivo.name.endswith(".zip"))
            with zipfile.ZipFile(archivo) as z:
                nombres = set(z.namelist())
            raiz = "felix-felicis-v0.2.1-x86_64-pc-windows-msvc"
            self.assertIn(f"{raiz}/universo.exe", nombres)
            self.assertIn(f"{raiz}/universo-mcp.exe", nombres)

    def test_el_archivo_es_reproducible_con_los_mismos_binarios(self):
        for target, windows in [
            ("x86_64-unknown-linux-gnu", False),
            ("x86_64-pc-windows-msvc", True),
        ]:
            with tempfile.TemporaryDirectory() as d:
                base = Path(d)
                bins = self.preparar(base, windows=windows)
                a = er.empaquetar(target, "v1", bins, base / "a")
                b = er.empaquetar(target, "v1", bins, base / "b")
                self.assertEqual(a.read_bytes(), b.read_bytes(), target)

    def test_el_zip_guarda_modos_unix_sea_cual_sea_el_host(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            bins = self.preparar(base, windows=True)
            archivo = er.empaquetar("x86_64-pc-windows-msvc", "v1", bins, base / "dist")
            with zipfile.ZipFile(archivo) as z:
                infos = {i.filename.split("/")[-1]: i for i in z.infolist()}
            self.assertEqual(infos["universo.exe"].create_system, 3)
            self.assertEqual(infos["universo.exe"].external_attr >> 16, 0o755)
            self.assertEqual(infos["LICENSE"].external_attr >> 16, 0o644)

    def test_falta_un_documento_es_error_de_empaquetado(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            bins = self.preparar(base, windows=False)
            original = er.RAIZ_REPO
            er.RAIZ_REPO = base
            try:
                with self.assertRaises(er.ErrorEmpaquetado):
                    er.empaquetar("x86_64-unknown-linux-gnu", "v1", bins, base / "dist")
            finally:
                er.RAIZ_REPO = original

    def test_falta_un_binario_o_version_invalida_es_error(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            bins = self.preparar(base, windows=False)
            (bins / "universo-mcp").unlink()
            with self.assertRaises(er.ErrorEmpaquetado):
                er.empaquetar("x86_64-unknown-linux-gnu", "v1", bins, base / "dist")
            (bins / "universo-mcp").write_bytes(b"binario")
            for version in ["", "v1/../x", "v 1", "v1\n"]:
                with self.assertRaises(er.ErrorEmpaquetado):
                    er.empaquetar("x86_64-unknown-linux-gnu", version, bins, base / "dist")


if __name__ == "__main__":
    unittest.main()
