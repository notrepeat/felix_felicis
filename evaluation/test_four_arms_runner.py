"""Tests del runner de cuatro brazos con un agente guionizado (sin LLM)."""
import json
import os
from collections import Counter
from pathlib import Path
import unittest

import four_arms_scoring as scoring
import run_four_arms as runner
from test_four_arms_dataset import REFERENCES, WRONG

BINARY = Path(os.environ.get("UNIVERSO", runner.REPO / "target" / "debug" / ("universo.exe" if os.name == "nt" else "universo")))
NULO = runner.NULO


def literal(case_id):
    """La ejecución literal del objetivo: el primer delta erróneo de cada tarea."""
    return WRONG[case_id][0][0]


class PlanTests(unittest.TestCase):
    def setUp(self):
        self.tasks, _ = scoring.load()

    def test_calendario_cuatro_brazos_por_tarea_y_repeticion_y_determinista(self):
        rows = runner.schedule(self.tasks["cases"])
        self.assertEqual(len(rows), 96)
        por_bloque = Counter((case, repetition) for case, repetition, _ in rows)
        self.assertEqual(set(por_bloque.values()), {4})
        for (case, repetition), _ in por_bloque.items():
            self.assertEqual(sorted(a for c, r, a in rows if (c, r) == (case, repetition)), sorted(runner.ARMS))
        self.assertEqual(rows, runner.schedule(self.tasks["cases"]))
        self.assertNotEqual(rows, runner.schedule(self.tasks["cases"], seed=1))
        # El orden de los brazos varía: ningún brazo va siempre primero.
        primeros = Counter(rows[i][2] for i in range(0, 96, 4))
        self.assertEqual(set(primeros), set(runner.ARMS))

    def test_el_plan_congelado_es_el_del_estudio_corrido(self):
        # El estudio 9c está cerrado: el plan del repositorio es exactamente el
        # que registró la corrida. Las fuentes pueden evolucionar después; en
        # ese caso el runner se niega a correr (plan_changed) hasta que haya un
        # plan, y un estudio, nuevos.
        congelado = json.loads(runner.PLAN.read_text(encoding="utf-8"))
        corrida = json.loads((runner.RESULTS / "four-arms-v2-estudio" / "run.json").read_text(encoding="utf-8"))
        self.assertEqual(runner.digest(congelado), corrida["plan_sha256"])
        self.assertEqual(congelado["runs"], 96)
        self.assertLessEqual(congelado["runs"], runner.MAX_CORRIDAS)
        filas = [json.loads(l) for l in (runner.RESULTS / "four-arms-v2-estudio" / "results.jsonl")
                 .read_text(encoding="utf-8").splitlines()]
        self.assertEqual({f["plan_sha256"] for f in filas}, {corrida["plan_sha256"]})
        self.assertEqual(len(filas), 96)

    def test_todos_los_brazos_reciben_la_misma_informacion_inicial(self):
        case = self.tasks["cases"][0]
        base = runner.prompt(case, "A_baseline")
        for arm in ["C_obligatorio", "D_revision"]:
            self.assertEqual(runner.prompt(case, arm), base)
        self.assertEqual(runner.prompt(case, "B_felix_opcional"), base.replace(runner.INSTRUCCION, runner.INSTRUCCION + runner.NOTA_MCP, 1))
        self.assertIn(runner.NOTA_CONTEXTO, base)
        ocultos = set(case["facts"]) - set(case["visible_facts"])
        self.assertTrue(ocultos)
        self.assertFalse(any(json.dumps(h, ensure_ascii=False) in base for h in ocultos))
        self.assertNotIn("required", base)
        self.assertNotIn("mutable_keys", base)


class FalloDelProveedorTests(unittest.TestCase):
    def test_limite_de_uso_y_error_de_api_son_errores_de_entorno(self):
        rechazado = {"type": "rate_limit_event", "rate_limit_info": {"status": "rejected"}}
        self.assertEqual(runner.fallo_del_proveedor([rechazado]), "provider_usage_limit")
        limite = {"type": "result", "is_error": True, "subtype": "success", "result": "Claude AI usage limit reached"}
        self.assertEqual(runner.fallo_del_proveedor([limite]), "provider_usage_limit")
        self.assertEqual(runner.fallo_del_proveedor([{"type": "result", "is_error": True, "result": "API Error: 500"}]),
                         "provider_turn_failed")
        # Un turno normal, con la cuota en uso pero permitida, no es un fallo.
        permitido = {"type": "rate_limit_event", "rate_limit_info": {"status": "allowed"}}
        normal = [permitido, {"type": "result", "is_error": False, "structured_output": runner.NULO}]
        self.assertIsNone(runner.fallo_del_proveedor(normal))

    def test_interpretar_el_stream_de_claude(self):
        delta = {"schema": "felix-delta-v1", "add": ["Estado(n/d13, vigente)"], "remove": []}
        events = [{"type": "system", "subtype": "init"},
                  {"type": "assistant", "message": {"content": [
                      {"type": "tool_use", "name": "mcp__felix__check", "input": {}},
                      {"type": "text", "text": "pienso"}]}},
                  {"type": "assistant", "message": {"content": [
                      {"type": "tool_use", "name": "StructuredOutput", "input": delta}]}},
                  {"type": "result", "is_error": False}]
        self.assertEqual(runner.interpretar_claude(events), (delta, ["mcp__felix__check", "StructuredOutput"]))
        self.assertEqual(runner.interpretar_claude([{"type": "result", "structured_output": delta}])[0], delta)
        self.assertEqual(runner.interpretar_claude([{"type": "result", "result": "texto"}]), (None, []))


class ComandoClaudeTests(unittest.TestCase):
    def test_comando_aislado_sin_ajustes_del_usuario_y_mcp_solo_en_b(self):
        agente = runner.AgenteClaude("/opt/x/claude", "/opt/x/universo-mcp", "claude-sonnet-5-5")
        sin = agente.command(Path("/w"), Path("/l"), Path("/h"), False)
        con = agente.command(Path("/w"), Path("/l"), Path("/h"), True)
        for cmd in (sin, con):
            self.assertEqual(cmd[0], "bwrap")
            self.assertIn("--clearenv", cmd)
            self.assertEqual(cmd[cmd.index("--model") + 1], "claude-sonnet-5-5")
            self.assertEqual(cmd[cmd.index("--tools") + 1], "")
            self.assertEqual(cmd[cmd.index("--setting-sources") + 1], "")
            self.assertIn("--strict-mcp-config", cmd)
            self.assertEqual(cmd[cmd.index("--json-schema") + 1], json.dumps(runner.SCHEMA))
            self.assertEqual(cmd[cmd.index("HOME") + 1], "/agente-home")
            self.assertNotIn("oracle.json", " ".join(cmd))
            self.assertNotIn("gpt", " ".join(cmd))
        self.assertNotIn("--mcp-config", sin)
        configuracion = json.loads(con[con.index("--mcp-config") + 1])
        self.assertEqual(configuracion["mcpServers"]["felix"]["args"], ["/tools/mcp_proxy_v2.py", "/logs/tools.jsonl", "8"])
        self.assertEqual(con[con.index("--allowedTools") + 1],
                         "mcp__felix__check,mcp__felix__context,mcp__felix__propose,mcp__felix__why")


@unittest.skipUnless(BINARY.is_file(), f"falta {BINARY}; compila con `cargo build -p universo-cli`")
class BrazosTests(unittest.TestCase):
    def setUp(self):
        self.tasks, _ = scoring.load()
        self.cases = {c["id"]: c for c in self.tasks["cases"]}

    def correr(self, case_id, arm, politica):
        agente = runner.AgenteGuionizado(politica)
        return runner.execute(self.cases[case_id], 1, arm, agente, BINARY), agente

    def test_a_y_b_aceptan_la_respuesta_literal_aunque_sea_invalida(self):
        for arm in ["A_baseline", "B_felix_opcional"]:
            record, agente = self.correr("promover_d13", arm, lambda c, r, t: literal(c))
            self.assertEqual(record["rounds"], 1)
            self.assertTrue(record["final_invalid"])
            self.assertFalse(record["score"]["success"])
            self.assertEqual(agente.llamadas[0]["con_mcp"], arm == "B_felix_opcional")

    def test_c_devuelve_el_rechazo_y_publica_la_reparacion(self):
        def politica(case_id, ronda, texto):
            return literal(case_id) if ronda == 1 else REFERENCES[case_id]

        record, agente = self.correr("promover_d13", "C_obligatorio", politica)
        self.assertEqual(record["orquestador"]["resultado"], "publicado")
        self.assertEqual(record["rounds"], 2)
        self.assertTrue(record["repaired"])
        self.assertTrue(record["score"]["success"])
        self.assertFalse(record["first_score"]["success"])
        segundo = agente.llamadas[1]["prompt"]
        self.assertIn("Felix rechazó esa propuesta (revisores: Una decisión vigente necesita", segundo)
        self.assertIn("la incumple la instancia d=n/d13", segundo)
        self.assertIn("Intentos restantes: 2", segundo)

    def test_c_identifica_la_regla_aunque_la_explicacion_agote_su_presupuesto(self):
        # En degradar_f1 la explicación redactada no cabe en su presupuesto: el
        # orquestador simula regla por regla y el agente recibe igual el id.
        def politica(case_id, ronda, texto):
            return literal(case_id) if ronda == 1 else REFERENCES[case_id]

        record, agente = self.correr("degradar_f1", "C_obligatorio", politica)
        self.assertEqual(record["orquestador"]["resultado"], "publicado")
        self.assertTrue(record["repaired"])
        self.assertIn("Felix rechazó esa propuesta (fuente-confiable: Una fuente que apoya", agente.llamadas[1]["prompt"])

    def test_c_con_testigo_puede_seguir_una_cascada_fuera_de_radio(self):
        # La primera propuesta corta la cascada en d03 (lo visible); el testigo
        # señala la arista d04→d03 y el agente guionizado la completa.
        def politica(case_id, ronda, texto):
            return literal(case_id) if ronda == 1 else REFERENCES[case_id]

        record, agente = self.correr("cascada_d02", "C_obligatorio", politica)
        self.assertEqual(record["orquestador"]["resultado"], "publicado")
        self.assertIn("e=n/d04→n/d03#depende#0, a=n/d04, b=n/d03", agente.llamadas[1]["prompt"])

    def test_c_escalado_no_publica_nada_y_el_estado_queda_valido(self):
        record, _ = self.correr("promover_d13", "C_obligatorio", lambda c, r, t: literal(c))
        self.assertEqual(record["orquestador"]["resultado"], "escalado")
        self.assertEqual(record["rounds"], 3)
        self.assertEqual(record["final_delta"], NULO)
        self.assertFalse(record["final_invalid"])
        self.assertFalse(record["score"]["goal_satisfied"])
        # En una tarea imposible, no cambiar nada es el éxito.
        record, _ = self.correr("promover_d11", "C_obligatorio", lambda c, r, t: literal(c))
        self.assertTrue(record["score"]["success"])

    def test_c_devuelve_errores_de_formato_para_reparar(self):
        conflicto = {"schema": "felix-delta-v1", "add": ["Estado(n/d13, vigente)"], "remove": []}

        def politica(case_id, ronda, texto):
            return conflicto if ronda == 1 else REFERENCES[case_id]

        record, agente = self.correr("promover_d13", "C_obligatorio", politica)
        self.assertEqual(record["orquestador"]["resultado"], "publicado")
        self.assertIn("conflicto funcional en 1 clave(s)", agente.llamadas[1]["prompt"])

    def test_d_da_una_revision_neutral_y_para_cuando_se_repite(self):
        record, agente = self.correr("promover_d13", "D_revision", lambda c, r, t: literal(c))
        self.assertEqual(record["rounds"], 2)
        self.assertTrue(record["final_invalid"])
        segundo = agente.llamadas[1]["prompt"]
        self.assertIn(runner.REVISION_NEUTRAL, segundo)
        self.assertNotIn("Felix", segundo)
        self.assertNotIn("revisores", segundo.split(runner.INSTRUCCION, 1)[1].split("\n", 2)[2])

        def mejora(case_id, ronda, texto):
            return literal(case_id) if ronda == 1 else REFERENCES[case_id]

        record, _ = self.correr("promover_d13", "D_revision", mejora)
        self.assertEqual(record["rounds"], 3)
        self.assertTrue(record["score"]["success"])
        self.assertTrue(record["repaired"])

    def test_un_fallo_tecnico_se_trata_igual_en_todos_los_brazos(self):
        # Incluso en una tarea cuya respuesta correcta es no cambiar nada: un
        # fallo nunca puntúa como éxito ni como estado inválido.
        class Caido(runner.AgenteGuionizado):
            def responder(self, case, texto, con_mcp, ronda):
                return {**super().responder(case, texto, con_mcp, ronda), "status": "timeout"}

        for respuesta, agente_cls in [(None, runner.AgenteGuionizado), ("texto", runner.AgenteGuionizado),
                                      (REFERENCES["promover_d11"], Caido)]:
            for arm in runner.ARMS:
                with self.subTest(respuesta=respuesta, arm=arm):
                    agente = agente_cls(lambda c, r, t, respuesta=respuesta: respuesta)
                    record = runner.execute(self.cases["promover_d11"], 1, arm, agente, BINARY)
                    self.assertTrue(record["technical_failure"])
                    self.assertIsNone(record["score"])
                    self.assertIsNone(record["final_invalid"])
                    self.assertEqual(record["rounds"], 1)

    def test_fallo_en_una_ronda_posterior_tambien_es_tecnico_en_c_y_d(self):
        def politica(case_id, ronda, texto):
            return literal(case_id) if ronda == 1 else None

        for arm in ["C_obligatorio", "D_revision"]:
            record, _ = self.correr("promover_d13", arm, politica)
            self.assertTrue(record["technical_failure"], arm)
            self.assertEqual(record["rounds"], 2)

    def test_c_y_d_acumulan_el_historial_igual(self):
        deltas = {1: literal("promover_d13"), 2: WRONG["retirar_revisor_d01"][0][0], 3: REFERENCES["promover_d13"]}
        prompts = {}
        for arm in ["C_obligatorio", "D_revision"]:
            _, agente = self.correr("promover_d13", arm, lambda c, r, t: deltas[r])
            prompts[arm] = agente.llamadas[2]["prompt"]
        for arm, texto in prompts.items():
            for ronda in (1, 2):
                self.assertIn(runner._historia_delta(deltas[ronda]), texto, arm)
        self.assertEqual(prompts["D_revision"].count(runner.REVISION_NEUTRAL), 2)
        self.assertEqual(prompts["C_obligatorio"].count("Felix rechazó"), 2)

    def test_excepcion_del_agente_en_la_primera_ronda_es_fallo_tecnico_en_todos(self):
        class Explota(runner.AgenteGuionizado):
            def responder(self, *argumentos):
                raise ValueError("fallo del agente")

        for arm in runner.ARMS:
            record = runner.execute(self.cases["promover_d13"], 1, arm, Explota(None), BINARY)
            self.assertTrue(record["technical_failure"], arm)
            self.assertIsNone(record["score"])

    def test_un_error_de_entorno_aborta_sin_registrar_la_fila(self):
        class SinEntorno(runner.AgenteGuionizado):
            def responder(self, *argumentos):
                raise runner.ErrorDeEntorno("bwrap_required")

        for arm in runner.ARMS:
            with self.assertRaises(runner.ErrorDeEntorno):
                runner.execute(self.cases["promover_d13"], 1, arm, SinEntorno(None), BINARY)

    def test_sensibilidad_cuenta_fallos_tecnicos_en_el_peor_caso(self):
        filas = [runner.execute(self.cases["promover_d13"], 1, "A_baseline",
                                runner.AgenteGuionizado(lambda c, r, t: respuesta), BINARY)
                 for respuesta in (REFERENCES["promover_d13"], literal("promover_d13"), None)]
        a = runner.summarize(filas)["arms"]["A_baseline"]
        self.assertEqual((a["scored_runs"], a["final_success"], a["final_invalid_state"]), (2, 1, 1))
        self.assertEqual((a["final_success_all_rows"], a["final_invalid_state_worst_case"]), (1, 2))
        self.assertEqual(a["technical_failure_causes"], {"sin_delta": 1})

    def test_una_excepcion_del_runner_queda_registrada_como_fila(self):
        record = runner.execute(self.cases["promover_d13"], 1, "C_obligatorio",
                                runner.AgenteGuionizado(lambda c, r, t: REFERENCES[c]), Path("/no/existe/universo"))
        self.assertEqual((record["status"], record["technical_failure"], record["score"]), ("runner_error", True, None))

    def test_resumen_por_brazo(self):
        records = [self.correr(case_id, arm, lambda c, r, t: REFERENCES[c])[0]
                   for case_id in ["promover_d13", "verificar_dependientes"] for arm in runner.ARMS]
        resumen = runner.summarize(records)["arms"]
        for arm in runner.ARMS:
            self.assertEqual((resumen[arm]["runs"], resumen[arm]["technical_failures"], resumen[arm]["final_success"],
                              resumen[arm]["final_invalid_state"]), (2, 0, 2, 0), arm)
        self.assertEqual(resumen["D_revision"]["rounds"], 4)
        self.assertEqual(resumen["C_obligatorio"]["rounds"], 2)


if __name__ == "__main__":
    unittest.main()


@unittest.skipUnless(BINARY.is_file(), f"falta {BINARY}; compila con `cargo build -p universo-cli`")
class EjecucionTests(unittest.TestCase):
    """El CLI del runner con el agente guionizado, en un temporal."""

    @classmethod
    def setUpClass(cls):
        # Un plan generado con las fuentes actuales: el del repositorio es el del
        # estudio cerrado y ya no coincide con ellas.
        import tempfile

        cls._plan_dir = tempfile.TemporaryDirectory()
        cls.plan = Path(cls._plan_dir.name) / "plan.json"
        cls.plan.write_text(json.dumps(runner.build_plan(runner.MODELO), ensure_ascii=False), encoding="utf-8")

    @classmethod
    def tearDownClass(cls):
        cls._plan_dir.cleanup()

    def correr(self, *argumentos):
        import subprocess
        import sys

        return subprocess.run([sys.executable, str(runner.HERE / "run_four_arms.py"), "--agente", "guionizado",
                               f"--binary={BINARY}", f"--plan={self.plan}", *argumentos],
                              capture_output=True, text=True, timeout=600)

    def test_calibracion_y_estudio_no_se_mezclan_y_resume_es_explicito(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            salida = Path(tmp) / "r"
            primera = self.correr("--fase", "calibracion", f"--output={salida}")
            self.assertEqual(primera.returncode, 0, primera.stderr)
            filas = [json.loads(l) for l in (salida / "results.jsonl").read_text(encoding="utf-8").splitlines()]
            self.assertEqual(len(filas), 16)
            self.assertEqual({(f["arm"], f["fase"]) for f in filas}, {("A_baseline", "calibracion")})
            self.assertIn("results_exist_use_resume", self.correr("--fase", "calibracion", f"--output={salida}").stderr)
            mezcla = self.correr("--fase", "estudio", "--resume", f"--output={salida}")
            self.assertIn("run_manifest_changed_refusing_to_resume", mezcla.stderr)
            manifiesto = json.loads((salida / "run.json").read_text(encoding="utf-8"))
            self.assertEqual(manifiesto["plan_sha256"], runner.digest(json.loads(self.plan.read_text(encoding="utf-8"))))
            self.assertEqual(len(manifiesto["universo_sha256"]), 64)

    def test_claude_no_acepta_otro_destino(self):
        import subprocess
        import sys
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            salida = subprocess.run([sys.executable, str(runner.HERE / "run_four_arms.py"), "--agente", "claude",
                                     f"--plan={self.plan}", f"--output={Path(tmp) / 'otro'}"],
                                    capture_output=True, text=True, timeout=120)
            self.assertIn("con --agente claude el destino es", salida.stderr)
            self.assertFalse((Path(tmp) / "otro").exists())
