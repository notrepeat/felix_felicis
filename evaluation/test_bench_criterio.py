"""Test del benchmark del repositorio de criterio (fase 11), con un tamaño mínimo."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import bench_criterio as bench  # noqa: E402

BINARIO = bench.REPO / "target/debug" / ("universo.exe" if os.name == "nt" else "universo")


@unittest.skipUnless(BINARIO.is_file(), "requiere target/debug/universo (cargo build)")
class BenchTest(unittest.TestCase):
    def test_un_repositorio_generado_es_valido_y_se_miden_las_cuatro_operaciones(self):
        with tempfile.TemporaryDirectory() as tmp:
            raiz = bench.generar(Path(tmp) / "r", 12, BINARIO)
            for universo in ("candidatos", "vigente"):
                bench.ejecutar(BINARIO, raiz, "guard", "--universo", universo)
            medida = bench.medir(raiz, BINARIO, repeticiones=2)
        self.assertEqual(set(medida), set(bench.OPERACIONES))
        for op, m in medida.items():
            self.assertEqual(len(m["segundos"]), 2, op)
            self.assertTrue(all(s >= 0 for s in m["segundos"]), op)
            self.assertIsNotNone(m["mediana"], op)
            self.assertTrue(m["ok"], op)
            self.assertIsNone(m["error"], op)



class FalloTest(unittest.TestCase):
    def test_un_tamano_que_falla_se_registra_en_vez_de_abortar(self):
        def falla(destino, notas, binario):
            raise RuntimeError("universo seal falló (1): seal: error\ncode: evaluation_budget_exhausted\n")

        originales = bench.generar, bench.ejecutar
        bench.generar = falla
        bench.ejecutar = lambda *a, **k: "universo 0.0.0\n"
        try:
            with tempfile.TemporaryDirectory() as tmp:
                salida = Path(tmp) / "b.json"
                codigo = bench.main(["--tamanos", "5", "--salida", str(salida)])
                datos = json.loads(salida.read_text(encoding="utf-8"))
        finally:
            bench.generar, bench.ejecutar = originales
        self.assertEqual(codigo, 0)
        self.assertEqual(datos["resultados"][0]["error"], "evaluation_budget_exhausted")
        self.assertNotIn("operaciones", datos["resultados"][0])


if __name__ == "__main__":
    unittest.main()
