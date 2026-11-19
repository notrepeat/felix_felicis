"""Tests del corpus y el runner de detección de criterio (fase 10, U13), sin LLM."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import run_criterio_deteccion as runner  # noqa: E402


class CorpusTest(unittest.TestCase):
    def setUp(self):
        self.casos = runner.cargar_corpus()

    def test_el_corpus_es_valido_y_tiene_las_tres_categorias(self):
        por_categoria = {c: 0 for c in runner.CATEGORIAS}
        for caso in self.casos:
            por_categoria[caso["categoria"]] += 1
        self.assertGreaterEqual(por_categoria["explicita"], 10)
        self.assertGreaterEqual(por_categoria["implicita"], 5)
        # Más negativos que positivos de cada clase: la precisión se mide
        # contra frases que se parecen a criterio y no lo son.
        self.assertGreaterEqual(por_categoria["ninguna"], 15)

    def test_el_turno_objetivo_de_una_implicita_es_del_agente_y_le_sigue_el_dueno(self):
        for caso in self.casos:
            if caso["categoria"] == "implicita":
                objetivo = caso["objetivo"]
                self.assertEqual(caso["turnos"][objetivo]["rol"], "agente", caso["id"])
                self.assertEqual(caso["turnos"][objetivo + 1]["rol"], "dueño", caso["id"])

    def test_el_corpus_se_declara_ficticio(self):
        datos = json.loads(runner.CORPUS.read_text(encoding="utf-8"))
        self.assertIn("ficticias", datos["nota"])

    def test_un_corpus_mal_formado_se_rechaza(self):
        malos = [
            {"schema": "otro", "casos": []},
            {"schema": "felix-criterio-deteccion-v1", "casos": [
                {"id": "a", "objetivo": 0, "categoria": "talvez", "turnos": [{"rol": "dueño", "texto": "x"}]}]},
            {"schema": "felix-criterio-deteccion-v1", "casos": [
                {"id": "a", "objetivo": 1, "categoria": "ninguna", "turnos": [{"rol": "dueño", "texto": "x"}]}]},
            {"schema": "felix-criterio-deteccion-v1", "casos": [
                {"id": "a", "objetivo": 0, "categoria": "ninguna", "turnos": [{"rol": "dueño", "texto": "x"}]},
                {"id": "a", "objetivo": 0, "categoria": "ninguna", "turnos": [{"rol": "dueño", "texto": "y"}]}]},
        ]
        for malo in malos:
            with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as f:
                json.dump(malo, f)
            with self.assertRaises(ValueError):
                runner.cargar_corpus(f.name)
            Path(f.name).unlink()


class SkillTest(unittest.TestCase):
    def test_la_skill_tiene_frontmatter_y_las_reglas_clave(self):
        texto = runner.SKILL.read_text(encoding="utf-8")
        self.assertTrue(texto.startswith("---\nname: felix-criterio\ndescription: "))
        cuerpo = runner.cargar_skill()
        for clave in ("explicita", "implicita", "ninguna", "Ante la duda", "confirmación", "capture_similar"):
            self.assertIn(clave, cuerpo)
        self.assertNotIn("name: felix-criterio", cuerpo)

    def test_la_skill_no_copia_frases_del_corpus(self):
        """La skill va como sistema en la medición: si repitiera casos del
        corpus, el modelo vería la respuesta. Se permiten marcadores cortos
        («de ahora en adelante»), no tramos de cinco palabras."""
        import re

        palabras = lambda texto: re.findall(r"\w+", texto.lower())  # noqa: E731
        skill = " ".join(palabras(runner.SKILL.read_text(encoding="utf-8")))
        compartidos = set()
        for caso in runner.cargar_corpus():
            for turno in caso["turnos"]:
                w = palabras(turno["texto"])
                for i in range(len(w) - 4):
                    tramo = " ".join(w[i:i + 5])
                    if tramo in skill:
                        compartidos.add(tramo)
        self.assertEqual(compartidos, set())


class PuntuarTest(unittest.TestCase):
    CASOS = [
        {"id": "a", "categoria": "explicita"},
        {"id": "b", "categoria": "implicita"},
        {"id": "c", "categoria": "ninguna"},
        {"id": "d", "categoria": "ninguna"},
    ]

    def test_precision_cobertura_y_categoria(self):
        m = runner.puntuar(self.CASOS, {"a": "implicita", "b": "ninguna", "c": "explicita", "d": "ninguna"})
        self.assertEqual((m["verdaderos_positivos"], m["falsos_positivos"], m["falsos_negativos"],
                          m["verdaderos_negativos"]), (1, 1, 1, 1))
        self.assertEqual(m["precision"], 0.5)
        self.assertEqual(m["cobertura"], 0.5)
        self.assertEqual(m["categoria_correcta"], 0.0)
        self.assertEqual({e["tipo"] for e in m["errores"]}, {"categoria", "falso_positivo", "falso_negativo"})

    def test_sin_respuesta_no_cuenta_como_ninguna_e_invalida_la_corrida(self):
        m = runner.puntuar(self.CASOS, {"a": "explicita", "c": "ninguna"})
        self.assertEqual(m["sin_respuesta"], ["b", "d"])
        self.assertFalse(m["valido"])
        # b y d no suman verdaderos negativos ni falsos negativos.
        self.assertEqual((m["verdaderos_positivos"], m["falsos_negativos"], m["verdaderos_negativos"]), (1, 0, 1))
        self.assertTrue(runner.puntuar(self.CASOS, {i: "ninguna" for i in "abcd"})["valido"])

    def test_el_intervalo_de_wilson(self):
        self.assertIsNone(runner.wilson(0, 0))
        bajo, alto = runner.wilson(19, 20)
        self.assertLess(bajo, 0.95)
        self.assertGreater(bajo, 0.7)
        self.assertLessEqual(alto, 1.0)

    def test_sin_positivos_la_precision_es_nula(self):
        self.assertIsNone(runner.puntuar(self.CASOS, {})["precision"])


class RunnerTest(unittest.TestCase):
    def test_el_oraculo_recorre_el_camino_completo_y_escribe_resultados(self):
        with tempfile.TemporaryDirectory() as tmp:
            salida = Path(tmp) / "r.json"
            self.assertEqual(runner.main(["--agente", "oraculo", "--salida", str(salida)]), 0)
            r = json.loads(salida.read_text(encoding="utf-8"))
        self.assertEqual(r["schema"], "felix-criterio-deteccion-resultados-v1")
        self.assertEqual(r["metricas"]["precision"], 1.0)
        self.assertEqual(r["metricas"]["cobertura"], 1.0)
        self.assertEqual(len(r["casos"]), len(runner.cargar_corpus()))
        self.assertEqual(len(r["skill_sha256"]), 64)

    def test_el_prompt_lleva_la_conversacion_y_el_sistema_la_skill(self):
        casos = runner.cargar_corpus()
        agente = runner.AgenteOraculo(casos)
        runner.correr(agente, casos[:1], "GUIA")
        llamada = agente.llamadas[0]
        self.assertTrue(llamada["sistema"].startswith("GUIA"))
        self.assertIn(runner.INSTRUCCION, llamada["sistema"])
        self.assertIn(casos[0]["turnos"][0]["texto"], llamada["prompt"])
        self.assertIn(f"Turno a clasificar: [{casos[0]['objetivo']}]", llamada["prompt"])

    def test_una_respuesta_fuera_del_esquema_queda_sin_categoria(self):
        class Raro(runner.AgenteOraculo):
            def clasificar(self, caso, sistema_texto, texto):
                return ({"categoria": "quizas"}, "ok") if caso["id"] == "e01" else (None, "timeout")

        casos = runner.cargar_corpus()
        filas, m, corte = runner.correr(Raro(casos), casos, "GUIA")
        self.assertIsNone(corte)
        self.assertTrue(all(f["pred"] is None for f in filas))
        self.assertEqual(filas[0]["estado"], "fuera_de_esquema")
        self.assertEqual(filas[1]["estado"], "timeout")
        self.assertEqual(len(m["sin_respuesta"]), len(casos))
        self.assertFalse(m["valido"])

    def test_un_corte_de_entorno_conserva_lo_clasificado(self):
        class Cuota(runner.AgenteOraculo):
            def clasificar(self, caso, sistema_texto, texto):
                if len(self.llamadas) == 3:
                    raise runner.ErrorDeEntorno("provider_usage_limit")
                return super().clasificar(caso, sistema_texto, texto)

        casos = runner.cargar_corpus()
        filas, m, corte = runner.correr(Cuota(casos), casos, "GUIA")
        self.assertEqual(corte, "provider_usage_limit")
        self.assertEqual(len(filas), 3)
        self.assertFalse(m["valido"])

    def test_claude_corre_aislado_sin_herramientas_ni_ajustes_del_usuario(self):
        agente = runner.AgenteClaude("/usr/bin/true", "claude-sonnet-5-5")
        cmd = agente.command(Path("/tmp/h"), "SISTEMA")
        self.assertEqual(cmd[0], "bwrap")
        self.assertIn("--unshare-all", cmd)
        self.assertIn("--clearenv", cmd)
        self.assertEqual(cmd[cmd.index("--tools") + 1], "")
        self.assertEqual(cmd[cmd.index("--setting-sources") + 1], "")
        self.assertEqual(cmd[cmd.index("--model") + 1], "claude-sonnet-5-5")
        self.assertEqual(cmd[cmd.index("--system-prompt") + 1], "SISTEMA")
        self.assertEqual(json.loads(cmd[cmd.index("--json-schema") + 1]), runner.SCHEMA)
        self.assertNotIn("--mcp-config", cmd)


class ClaudeFalso(runner.AgenteClaude):
    """`claude` sustituido por un script que imprime eventos stream-json."""

    def __init__(self, script, timeout=5):
        super().__init__("/usr/bin/true", "modelo", timeout=timeout)
        self.script = script

    def command(self, home, sistema_texto):
        return [sys.executable, "-c", self.script]


@unittest.skipIf(os.name == "nt", "el agente Claude corre con bwrap: sólo en Linux")
class ClaudeTest(unittest.TestCase):
    CASO = {"id": "x", "turnos": [{"rol": "dueño", "texto": "t"}], "objetivo": 0, "categoria": "ninguna"}

    def clasificar(self, script, timeout=5):
        return ClaudeFalso(script, timeout).clasificar(self.CASO, "S", "P")

    def test_lee_la_salida_estructurada(self):
        evento = {"type": "result", "subtype": "success", "is_error": False,
                  "structured_output": {"categoria": "explicita", "afirmacion": "x"}}
        salida, estado = self.clasificar(f"import json; print(json.dumps({evento!r}))")
        self.assertEqual((salida["categoria"], estado), ("explicita", "ok"))

    def test_timeout_fallo_y_sin_salida_no_son_respuestas(self):
        self.assertEqual(self.clasificar("import time; time.sleep(5)", timeout=0.5), (None, "timeout"))
        self.assertEqual(self.clasificar("import sys; sys.exit(3)"), (None, "execution_failed"))
        self.assertEqual(self.clasificar("print('{}')"), (None, "sin_salida"))

    def test_un_fallo_transitorio_del_proveedor_se_reintenta_una_vez(self):
        with tempfile.TemporaryDirectory() as tmp:
            marca = Path(tmp) / "intento"
            fallo = {"type": "result", "is_error": True, "result": "overloaded"}
            ok = {"type": "result", "is_error": False, "structured_output": {"categoria": "ninguna", "afirmacion": ""}}
            script = (f"import json, pathlib; m = pathlib.Path({str(marca)!r}); "
                      f"print(json.dumps({ok!r} if m.exists() else {fallo!r})); m.touch()")
            self.assertEqual(self.clasificar(script)[1], "ok")
            siempre = f"import json; print(json.dumps({fallo!r}))"
            with self.assertRaises(runner.ErrorDeEntorno):
                self.clasificar(siempre)

    def test_un_limite_del_proveedor_corta_la_corrida(self):
        evento = {"type": "result", "is_error": True, "result": "Usage limit reached"}
        with self.assertRaises(runner.ErrorDeEntorno):
            self.clasificar(f"import json; print(json.dumps({evento!r}))")


if __name__ == "__main__":
    unittest.main()
