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

    def test_el_plan_congelado_coincide_con_las_fuentes_actuales(self):
        congelado = json.loads(runner.PLAN.read_text(encoding="utf-8"))
        self.assertEqual(congelado, runner.build_plan(congelado["configured_model_alias"]))
        self.assertEqual(congelado["runs"], 96)
        self.assertLessEqual(congelado["runs"], runner.MAX_CORRIDAS)

    def test_todos_los_brazos_reciben_la_misma_informacion_inicial(self):
        case = self.tasks["cases"][0]
        base = runner.prompt(case, "A_baseline")
        for arm in ["C_obligatorio", "D_revision"]:
            self.assertEqual(runner.prompt(case, arm), base)
        self.assertEqual(runner.prompt(case, "B_felix_opcional"), base.replace(runner.INSTRUCCION, runner.INSTRUCCION + runner.NOTA_MCP, 1))
        self.assertNotIn("required", base)
        self.assertNotIn("mutable_keys", base)


class FalloDelProveedorTests(unittest.TestCase):
    def test_limite_de_uso_y_turno_fallido_son_errores_de_entorno(self):
        aviso = {"type": "item.completed", "item": {"type": "error", "message": "Under-development features enabled"}}
        limite = [aviso, {"type": "turn.started"},
                  {"type": "error", "message": "You’ve hit your usage limit. Visit https://example.invalid"},
                  {"type": "turn.failed", "error": {"message": "You’ve hit your usage limit."}}]
        self.assertEqual(runner.fallo_del_proveedor(limite), "provider_usage_limit")
        self.assertEqual(runner.fallo_del_proveedor([{"type": "turn.failed", "error": {"message": "boom"}}]),
                         "provider_turn_failed")
        # Un aviso de la CLI y un turno completo no son fallos del proveedor.
        normal = [aviso, {"type": "turn.started"}, {"type": "turn.completed", "usage": {"input_tokens": 1}}]
        self.assertIsNone(runner.fallo_del_proveedor(normal))


class ComandoCodexTests(unittest.TestCase):
    def test_comando_aislado_y_mcp_solo_en_el_brazo_b(self):
        import tempfile
        from unittest import mock

        with tempfile.TemporaryDirectory() as home:
            (Path(home) / ".codex").mkdir()
            (Path(home) / ".codex" / "auth.json").write_text("{}", encoding="utf-8")
            with mock.patch.dict(os.environ, {"HOME": home, "USERPROFILE": home}):
                agente = runner.AgenteCodex(home, home, "gpt-6-astra")
                sin = agente.command(Path(home), Path(home), False)
                con = agente.command(Path(home), Path(home), True)
        for cmd in (sin, con):
            self.assertEqual(cmd[0], "bwrap")
            self.assertIn("--clearenv", cmd)
            self.assertEqual(cmd[cmd.index("--sandbox") + 1], "read-only")
            self.assertEqual(cmd[cmd.index("--model") + 1], "gpt-6-astra")
            self.assertNotIn("oracle.json", " ".join(cmd))
            self.assertEqual(cmd[-1], "-")
        self.assertFalse(any("mcp_servers" in parte for parte in sin))
        self.assertIn('mcp_servers.felix.args=["/tools/mcp_proxy_v2.py","/logs/tools.jsonl","8"]', con)


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
            record, agente = self.correr("promover_d1", arm, lambda c, r, t: literal(c))
            self.assertEqual(record["rounds"], 1)
            self.assertTrue(record["final_invalid"])
            self.assertFalse(record["score"]["success"])
            self.assertEqual(agente.llamadas[0]["con_mcp"], arm == "B_felix_opcional")

    def test_c_devuelve_el_rechazo_y_publica_la_reparacion(self):
        def politica(case_id, ronda, texto):
            return literal(case_id) if ronda == 1 else REFERENCES[case_id]

        record, agente = self.correr("promover_d1", "C_obligatorio", politica)
        self.assertEqual(record["orquestador"]["resultado"], "publicado")
        self.assertEqual(record["rounds"], 2)
        self.assertTrue(record["repaired"])
        self.assertTrue(record["score"]["success"])
        self.assertFalse(record["first_score"]["success"])
        segundo = agente.llamadas[1]["prompt"]
        self.assertIn("Felix rechazó esa propuesta (revisores: Una decisión vigente necesita", segundo)
        self.assertIn("Intentos restantes: 2", segundo)

    def test_c_escalado_no_publica_nada_y_el_estado_queda_valido(self):
        record, _ = self.correr("promover_d1", "C_obligatorio", lambda c, r, t: literal(c))
        self.assertEqual(record["orquestador"]["resultado"], "escalado")
        self.assertEqual(record["rounds"], 3)
        self.assertEqual(record["final_delta"], NULO)
        self.assertFalse(record["final_invalid"])
        self.assertFalse(record["score"]["goal_satisfied"])
        # En una tarea imposible, no cambiar nada es el éxito.
        record, _ = self.correr("apoyo_fuente_debil", "C_obligatorio", lambda c, r, t: literal(c))
        self.assertTrue(record["score"]["success"])

    def test_c_devuelve_errores_de_formato_para_reparar(self):
        conflicto = {"schema": "felix-delta-v1", "add": ["Estado(n/d1, vigente)"], "remove": []}

        def politica(case_id, ronda, texto):
            return conflicto if ronda == 1 else REFERENCES[case_id]

        record, agente = self.correr("promover_d1", "C_obligatorio", politica)
        self.assertEqual(record["orquestador"]["resultado"], "publicado")
        self.assertIn("error de formato del delta: add_conflict", agente.llamadas[1]["prompt"])

    def test_d_da_una_revision_neutral_y_para_cuando_se_repite(self):
        record, agente = self.correr("promover_d1", "D_revision", lambda c, r, t: literal(c))
        self.assertEqual(record["rounds"], 2)
        self.assertTrue(record["final_invalid"])
        segundo = agente.llamadas[1]["prompt"]
        self.assertIn(runner.REVISION_NEUTRAL, segundo)
        self.assertNotIn("Felix", segundo)
        self.assertNotIn("revisores", segundo.split(runner.INSTRUCCION, 1)[1].split("\n", 2)[2])

        def mejora(case_id, ronda, texto):
            return literal(case_id) if ronda == 1 else REFERENCES[case_id]

        record, _ = self.correr("promover_d1", "D_revision", mejora)
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
                                      (REFERENCES["apoyo_fuente_debil"], Caido)]:
            for arm in runner.ARMS:
                with self.subTest(respuesta=respuesta, arm=arm):
                    agente = agente_cls(lambda c, r, t, respuesta=respuesta: respuesta)
                    record = runner.execute(self.cases["apoyo_fuente_debil"], 1, arm, agente, BINARY)
                    self.assertTrue(record["technical_failure"])
                    self.assertIsNone(record["score"])
                    self.assertIsNone(record["final_invalid"])
                    self.assertEqual(record["rounds"], 1)

    def test_fallo_en_una_ronda_posterior_tambien_es_tecnico_en_c_y_d(self):
        def politica(case_id, ronda, texto):
            return literal(case_id) if ronda == 1 else None

        for arm in ["C_obligatorio", "D_revision"]:
            record, _ = self.correr("promover_d1", arm, politica)
            self.assertTrue(record["technical_failure"], arm)
            self.assertEqual(record["rounds"], 2)

    def test_c_y_d_acumulan_el_historial_igual(self):
        deltas = {1: literal("promover_d1"), 2: WRONG["retirar_revisor"][0][0], 3: REFERENCES["promover_d1"]}
        prompts = {}
        for arm in ["C_obligatorio", "D_revision"]:
            _, agente = self.correr("promover_d1", arm, lambda c, r, t: deltas[r])
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
            record = runner.execute(self.cases["promover_d1"], 1, arm, Explota(None), BINARY)
            self.assertTrue(record["technical_failure"], arm)
            self.assertIsNone(record["score"])

    def test_un_error_de_entorno_aborta_sin_registrar_la_fila(self):
        class SinEntorno(runner.AgenteGuionizado):
            def responder(self, *argumentos):
                raise runner.ErrorDeEntorno("bwrap_required")

        for arm in runner.ARMS:
            with self.assertRaises(runner.ErrorDeEntorno):
                runner.execute(self.cases["promover_d1"], 1, arm, SinEntorno(None), BINARY)

    def test_sensibilidad_cuenta_fallos_tecnicos_en_el_peor_caso(self):
        filas = [runner.execute(self.cases["promover_d1"], 1, "A_baseline",
                                runner.AgenteGuionizado(lambda c, r, t: respuesta), BINARY)
                 for respuesta in (REFERENCES["promover_d1"], literal("promover_d1"), None)]
        a = runner.summarize(filas)["arms"]["A_baseline"]
        self.assertEqual((a["scored_runs"], a["final_success"], a["final_invalid_state"]), (2, 1, 1))
        self.assertEqual((a["final_success_all_rows"], a["final_invalid_state_worst_case"]), (1, 2))
        self.assertEqual(a["technical_failure_causes"], {"sin_delta": 1})

    def test_una_excepcion_del_runner_queda_registrada_como_fila(self):
        record = runner.execute(self.cases["promover_d1"], 1, "C_obligatorio",
                                runner.AgenteGuionizado(lambda c, r, t: REFERENCES[c]), Path("/no/existe/universo"))
        self.assertEqual((record["status"], record["technical_failure"], record["score"]), ("runner_error", True, None))

    def test_resumen_por_brazo(self):
        records = [self.correr(case_id, arm, lambda c, r, t: REFERENCES[c])[0]
                   for case_id in ["promover_d1", "apoyo_de_persona"] for arm in runner.ARMS]
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

    def correr(self, *argumentos):
        import subprocess
        import sys

        return subprocess.run([sys.executable, str(runner.HERE / "run_four_arms.py"), "--agente", "guionizado",
                               f"--binary={BINARY}", *argumentos], capture_output=True, text=True, timeout=600)

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
            self.assertEqual(manifiesto["plan_sha256"], runner.digest(json.loads(runner.PLAN.read_text(encoding="utf-8"))))
            self.assertEqual(len(manifiesto["universo_sha256"]), 64)

    def test_codex_no_acepta_otro_destino(self):
        import subprocess
        import sys
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            codex = subprocess.run([sys.executable, str(runner.HERE / "run_four_arms.py"), "--agente", "codex",
                                    "--codex-dir=/no/existe", f"--output={Path(tmp) / 'otro'}"],
                                   capture_output=True, text=True, timeout=120)
            self.assertIn("con --agente codex el destino es", codex.stderr)
            self.assertFalse((Path(tmp) / "otro").exists())
