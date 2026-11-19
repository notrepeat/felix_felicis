"""Tests de la evaluación del criterio (fase 11), sin LLM."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import run_criterio_uso as uso  # noqa: E402

BINARIO = uso.REPO / "target/debug" / ("universo.exe" if os.name == "nt" else "universo")


class DatasetTest(unittest.TestCase):
    def test_el_dataset_es_valido(self):
        criterios, tareas, oraculo = uso.cargar()
        self.assertEqual(len(criterios["vigentes"]), 18)
        self.assertEqual(len(criterios["candidatos"]), 6)
        self.assertEqual(len(tareas["cases"]), 16)
        con = [t for t in tareas["cases"] if oraculo["cases"][t["id"]]["criterio"]]
        self.assertEqual(len(con), 12)
        vigentes = {c["id"] for c in criterios["vigentes"]}
        for t in con:
            esperado = oraculo["cases"][t["id"]]
            self.assertIn(esperado["criterio"], vigentes)
            self.assertIn(esperado["opcion"], t["opciones"])
        # Cada tarea con criterio apunta a un criterio vigente distinto.
        self.assertEqual(len({oraculo["cases"][t["id"]]["criterio"] for t in con}), 12)

    def test_la_respuesta_correcta_no_se_concentra_en_una_posicion(self):
        _, _, oraculo = uso.cargar()
        posiciones = [v["opcion"] for v in oraculo["cases"].values() if v["opcion"]]
        self.assertEqual(sorted(posiciones.count(p) for p in "abc"), [4, 4, 4])

    def test_las_tareas_no_nombran_criterios_ni_el_oraculo(self):
        casos = json.dumps(uso.cargar()[1]["cases"], ensure_ascii=False).lower()
        for palabra in ("criterio", "oracle", "oráculo", "v01"):
            self.assertNotIn(palabra, casos)

    def test_un_dataset_mal_formado_se_rechaza(self):
        criterios, tareas, oraculo = uso.cargar()
        for romper in (
            lambda o: o["cases"]["t01"].update(opcion="z"),
            lambda o: o["cases"]["t01"].update(criterio="v99"),
            lambda o: o["cases"].pop("k04"),
            lambda o: o["cases"]["k01"].update(opcion="a"),
        ):
            roto = json.loads(json.dumps(oraculo))
            romper(roto)
            with self.assertRaises(ValueError):
                uso.validar(criterios, tareas, roto)


class PromptTest(unittest.TestCase):
    def setUp(self):
        self.criterios, self.tareas, _ = uso.cargar()
        self.caso = self.tareas["cases"][0]

    def test_cada_brazo_recibe_lo_suyo(self):
        a = uso.prompt(self.caso, "A_sin_criterio", self.criterios)
        b = uso.prompt(self.caso, "B_criterio_mcp", self.criterios)
        c = uso.prompt(self.caso, "C_criterio_prompt", self.criterios)
        for texto in (a, b, c):
            self.assertIn(self.caso["escenario"], texto)
            self.assertIn(self.caso["opciones"]["a"], texto)
        for v in self.criterios["vigentes"]:
            self.assertNotIn(v["titulo"], a)
            self.assertNotIn(v["titulo"], b)
            self.assertIn(v["titulo"], c)
        for cand in self.criterios["candidatos"]:
            self.assertNotIn(cand["titulo"], c)
        self.assertIn("search", b)
        self.assertNotIn("search", a)
        self.assertNotIn("search", c)

    def test_el_esquema_restringe_la_opcion_a_las_de_la_tarea(self):
        esquema = uso.esquema(self.caso)
        self.assertEqual(esquema["properties"]["opcion"]["enum"], sorted(self.caso["opciones"]))
        self.assertEqual(set(esquema["required"]), {"opcion", "justificacion", "criterio_aplicado"})


def fila(caso, brazo, opcion, cita=None, rep=1, estado="completed"):
    respuesta = None if estado != "completed" else {"opcion": opcion, "criterio_aplicado": cita}
    return {"case": caso, "arm": brazo, "repetition": rep, "status": estado, "respuesta": respuesta}


class PuntajeTest(unittest.TestCase):
    def setUp(self):
        self.oraculo = uso.cargar()[2]

    def test_adherencia_invencion_y_sin_respuesta(self):
        filas = [
            fila("t01", "A_sin_criterio", "c"),
            fila("t02", "A_sin_criterio", "a"),
            fila("k01", "A_sin_criterio", "a", "El dueño prefiere el azul"),
            fila("k02", "A_sin_criterio", "a"),
            fila("t03", "A_sin_criterio", None, estado="timeout"),
        ]
        r = uso.puntuar(filas, self.oraculo)["A_sin_criterio"]
        self.assertEqual((r["adherencia_aciertos"], r["adherencia_total"]), (1, 2))
        self.assertEqual((r["inventado"], r["control_total"]), (1, 2))
        self.assertEqual(r["sin_respuesta"], 1)

    def test_una_cita_vacia_no_es_invencion(self):
        self.assertFalse(uso.cita_criterio({"criterio_aplicado": "  "}))
        self.assertFalse(uso.cita_criterio({"criterio_aplicado": None}))
        self.assertTrue(uso.cita_criterio({"criterio_aplicado": "x"}))

    def filas_todo_o_nada(self):
        filas = []
        for t, esperado in self.oraculo["cases"].items():
            if esperado["criterio"]:
                for rep in (1, 2, 3):
                    filas.append(fila(t, "A_sin_criterio", "z", rep=rep))
                    filas.append(fila(t, "B_criterio_mcp", esperado["opcion"], "x", rep=rep))
        return filas

    def test_bootstrap_pareado_por_tarea(self):
        filas = self.filas_todo_o_nada()
        d = uso.diferencia(filas, self.oraculo, "B_criterio_mcp", "A_sin_criterio", semilla=1, muestras=500)
        self.assertEqual((d["diferencia"], d["ic95"]), (1.0, [1.0, 1.0]))
        d = uso.diferencia(filas, self.oraculo, "A_sin_criterio", "A_sin_criterio", semilla=1, muestras=500)
        self.assertEqual(d["diferencia"], 0.0)

    def test_la_regla_de_decision(self):
        def resumen(adh_a, adh_b, adh_c, inv_a, inv_b, ic):
            return {
                "brazos": {"A_sin_criterio": {"adherencia": adh_a, "inventado_tasa": inv_a},
                           "B_criterio_mcp": {"adherencia": adh_b, "inventado_tasa": inv_b},
                           "C_criterio_prompt": {"adherencia": adh_c, "inventado_tasa": 0}},
                "B_menos_A": {"diferencia": round(adh_b - adh_a, 4), "ic95": ic},
                "B_menos_C": {"diferencia": round(adh_b - adh_c, 4), "ic95": None},
            }
        d = uso.decidir(resumen(0.4, 0.8, 0.82, 0, 0, [0.2, 0.6]))
        self.assertEqual((d["criterio_ayuda"], d["mcp_aporta"]), (True, True))
        self.assertFalse(uso.decidir(resumen(0.4, 0.8, 0.9, 0, 0, [0.2, 0.6]))["mcp_aporta"])
        self.assertFalse(uso.decidir(resumen(0.4, 0.5, 0.5, 0, 0, [-0.1, 0.3]))["criterio_ayuda"])
        self.assertFalse(uso.decidir(resumen(0.4, 0.8, 0.8, 0.0, 0.25, [0.2, 0.6]))["criterio_ayuda"])
        # Sin filas comparables no hay decisión positiva.
        vacio = {"brazos": {b: {"adherencia": None, "inventado_tasa": None} for b in uso.BRAZOS},
                 "B_menos_A": {"diferencia": None, "ic95": None}, "B_menos_C": {"diferencia": None, "ic95": None}}
        self.assertEqual(uso.decidir(vacio)["criterio_ayuda"], False)
        self.assertEqual(uso.decidir(vacio)["mcp_aporta"], False)
        self.assertFalse(uso.decidir(resumen(0.4, 0.5, 0.5, 0, 0, [0.02, 0.2]))["criterio_ayuda"])

    def test_el_informe_resume_los_tres_brazos(self):
        filas = self.filas_todo_o_nada()
        informe = uso.informe(filas, self.oraculo, semilla=1, muestras=200)
        self.assertEqual(informe["brazos"]["B_criterio_mcp"]["adherencia"], 1.0)
        self.assertEqual(informe["brazos"]["A_sin_criterio"]["adherencia"], 0.0)
        self.assertIn("decision", informe)


class PlanTest(unittest.TestCase):
    def test_el_plan_tiene_144_filas_y_cada_par_corre_los_tres_brazos(self):
        plan = uso.construir_plan("claude-sonnet-5-5")
        self.assertEqual(plan["runs"], 144)
        pares = {}
        for f in plan["schedule"]:
            pares.setdefault((f["case"], f["repetition"]), []).append(f["arm"])
        self.assertEqual(len(pares), 48)
        for brazos in pares.values():
            self.assertEqual(sorted(brazos), sorted(uso.BRAZOS))
        for fuente in uso.FUENTES:
            self.assertIn(fuente, plan["source_sha256"])
        self.assertEqual(plan, uso.construir_plan("claude-sonnet-5-5"))


class AgenteClaudeTest(unittest.TestCase):
    def test_solo_b_tiene_el_mcp_y_sin_escritura(self):
        agente = uso.AgenteClaude("/usr/bin/true", "/usr/bin/true", "modelo")
        caso = uso.cargar()[1]["cases"][0]
        sin = agente.command(Path("/tmp/w"), Path("/tmp/l"), Path("/tmp/h"), caso, False)
        con = agente.command(Path("/tmp/w"), Path("/tmp/l"), Path("/tmp/h"), caso, True)
        self.assertEqual(sin[0], "bwrap")
        self.assertIn("--clearenv", sin)
        self.assertEqual(sin[sin.index("--tools") + 1], "")
        self.assertNotIn("--mcp-config", sin)
        servidor = json.loads(con[con.index("--mcp-config") + 1])
        self.assertIn("--criterio", servidor["mcpServers"]["felix"]["args"])
        permitidas = con[con.index("--allowedTools") + 1].split(",")
        self.assertEqual(sorted(permitidas), ["mcp__felix__context", "mcp__felix__search"])
        excluidas = con[con.index("--disallowedTools") + 1].split(",")
        self.assertEqual(sorted(excluidas), ["mcp__felix__check", "mcp__felix__propose", "mcp__felix__why"])
        # El directorio con el repositorio de criterio se monta en sólo lectura.
        i = con.index("/work")
        self.assertEqual((con[i - 2], con[i - 1]), ("--ro-bind", "/tmp/w"))


@unittest.skipUnless(BINARIO.is_file(), "requiere target/debug/universo (cargo build)")
class UniversoTest(unittest.TestCase):
    def test_el_universo_ficticio_tiene_vigentes_y_candidatas_y_no_admite_escritura(self):
        criterios = uso.cargar()[0]
        with tempfile.TemporaryDirectory() as tmp:
            raiz = uso.construir_universo(criterios, Path(tmp) / "criterio", BINARIO)
            self.assertEqual(uso.contar(raiz, BINARIO), {"vigente": 18, "candidatos": 6, "promovidos": 18})
            toml = (raiz / "felix.toml").read_text(encoding="utf-8")
            self.assertIn("escritura_agentes = false", toml)
            self.assertNotIn("escritura_agentes = true", toml)
            self.assertFalse((raiz / ".git").exists())


class CorridaTest(unittest.TestCase):
    def test_el_agente_guionizado_recorre_el_plan_y_reanuda(self):
        oraculo = uso.cargar()[2]

        def politica(caso, brazo):
            esperado = oraculo["cases"][caso["id"]]
            opcion = esperado["opcion"] if brazo != "A_sin_criterio" and esperado["opcion"] else "a"
            return {"opcion": opcion, "justificacion": "j", "criterio_aplicado": None}

        plan = uso.construir_plan("guion")
        plan["schedule"] = plan["schedule"][:9]
        with tempfile.TemporaryDirectory() as tmp:
            salida = Path(tmp) / "r.jsonl"
            agente = uso.AgenteGuionizado(politica)
            uso.correr(plan, agente, salida, universo=None)
            self.assertEqual(len(uso.leer_filas(salida)), 9)
            uso.correr(plan, agente, salida, universo=None)
            self.assertEqual(len(uso.leer_filas(salida)), 9)
            self.assertEqual(len(agente.llamadas), 9)
            self.assertTrue(all(f["status"] == "completed" for f in uso.leer_filas(salida)))

    def test_un_corte_antes_de_la_primera_fila_no_rompe_el_informe(self):
        class SinCuota(uso.AgenteGuionizado):
            def responder(self, caso, brazo, texto, universo):
                raise uso.ErrorDeEntorno("provider_usage_limit")

        plan = uso.construir_plan("guion")
        with tempfile.TemporaryDirectory() as tmp:
            salida = Path(tmp) / "r.jsonl"
            self.assertEqual(uso.correr(plan, SinCuota(None), salida, universo=None), "provider_usage_limit")
            self.assertEqual(uso.leer_filas(salida), [])
            informe = uso.informe([], uso.cargar()[2], muestras=10)
            self.assertFalse(informe["decision"]["criterio_ayuda"])

    def test_una_ultima_linea_truncada_se_vuelve_a_correr_y_otro_plan_no_se_mezcla(self):
        plan = uso.construir_plan("guion")
        plan["schedule"] = plan["schedule"][:3]
        politica = lambda c, b: {"opcion": "a", "justificacion": "", "criterio_aplicado": None}  # noqa: E731
        with tempfile.TemporaryDirectory() as tmp:
            salida = Path(tmp) / "r.jsonl"
            uso.correr(plan, uso.AgenteGuionizado(politica), salida, universo=None)
            lineas = salida.read_text(encoding="utf-8").splitlines()
            salida.write_text("\n".join(lineas[:2] + [lineas[2][:15]]) + "\n", encoding="utf-8")
            agente = uso.AgenteGuionizado(politica)
            self.assertIsNone(uso.correr(plan, agente, salida, universo=None))
            self.assertEqual(len(agente.llamadas), 1)
            otro = uso.construir_plan("otro-modelo")
            otro["schedule"] = otro["schedule"][:3]
            self.assertEqual(uso.correr(otro, agente, salida, universo=None), "resume_plan_mismatch")

    def test_un_corte_del_proveedor_conserva_lo_medido(self):
        class Cuota(uso.AgenteGuionizado):
            def responder(self, caso, brazo, texto, universo):
                if len(self.llamadas) == 2:
                    raise uso.ErrorDeEntorno("provider_usage_limit")
                return super().responder(caso, brazo, texto, universo)

        plan = uso.construir_plan("guion")
        plan["schedule"] = plan["schedule"][:6]
        with tempfile.TemporaryDirectory() as tmp:
            salida = Path(tmp) / "r.jsonl"
            agente = Cuota(lambda c, b: {"opcion": "a", "justificacion": "", "criterio_aplicado": None})
            corte = uso.correr(plan, agente, salida, universo=None)
            self.assertEqual(corte, "provider_usage_limit")
            self.assertEqual(len(salida.read_text(encoding="utf-8").splitlines()), 2)


if __name__ == "__main__":
    unittest.main()
