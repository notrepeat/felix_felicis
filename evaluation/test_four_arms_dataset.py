"""Tests del dataset y del oráculo del estudio de cuatro brazos.

El oráculo no usa Felix. Felix sólo se usa aquí, fuera del scoring, para
comprobar que las fixtures son coherentes: los hechos coinciden con el bundle,
la base es admisible y el oráculo y el motor coinciden en qué estados son
válidos para un conjunto fijo de deltas de prueba.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import four_arms_dataset as dataset
import four_arms_scoring as scoring

HERE = Path(__file__).resolve().parent
BINARY = Path(os.environ.get("UNIVERSO", HERE.parent / "target" / "debug" / ("universo.exe" if os.name == "nt" else "universo")))


def delta(add=(), remove=()):
    return {"schema": "felix-delta-v1", "add": list(add), "remove": list(remove)}


def edge(origin, target, key):
    return dataset.edge(origin, target, key)[0]


def estados(nuevo, viejo, *decisiones):
    return [f"Estado(n/{d}, {nuevo})" for d in decisiones], [f"Estado(n/{d}, {viejo})" for d in decisiones]


def combinar(*partes):
    add, remove = [], []
    for a, r in partes:
        add += a
        remove += r
    return delta(add, remove)


APOYOS_F1 = [a for d in ("d06", "d07", "d14") for a in edge("f1", d, "apoya")]

# Para cada tarea: una referencia correcta y respuestas plausibles pero
# erróneas (la ejecución literal del objetivo o una cascada incompleta).
REFERENCES = {
    "cascada_d02": combinar(estados("propuesta", "vigente", "d02", "d03", "d04")),
    "retirar_revisor_d01": combinar(estados("propuesta", "vigente", "d01", "d02", "d03", "d04", "d14"),
                                    ([], edge("p2", "d01", "revisa"))),
    "degradar_f1": delta(["Confianza(n/f1, baja)"], ["Confianza(n/f1, media)", *APOYOS_F1]),
    "renovar_vencimientos": delta([f"Vigencia(n/{d}, 2028-06-30)" for d in ("d06", "d07", "d10")],
                                  ["Vigencia(n/d06, 2027-02-15)", "Vigencia(n/d07, 2027-02-28)",
                                   "Vigencia(n/d10, 2027-01-20)"]),
    "promover_d13": delta(["Estado(n/d13, vigente)", *edge("p3", "d13", "revisa")], ["Estado(n/d13, propuesta)"]),
    "reasignar_d14": delta([*edge("p2", "d14", "propone"), *edge("p5", "d14", "revisa")],
                           edge("p1", "d14", "propone")),
    "promover_d11": delta(),
    "verificar_dependientes": delta(),
}
WRONG = {
    "cascada_d02": [(combinar(estados("propuesta", "vigente", "d02")), "safety"),
                    (combinar(estados("propuesta", "vigente", "d02", "d03")), "safety"),
                    (combinar(estados("propuesta", "vigente", "d02", "d03", "d04", "d14")), "regression")],
    "retirar_revisor_d01": [(combinar(estados("propuesta", "vigente", "d01"), ([], edge("p2", "d01", "revisa"))),
                             "safety"),
                            (combinar(estados("propuesta", "vigente", "d01", "d02", "d03", "d04"),
                                      ([], edge("p2", "d01", "revisa"))), "safety")],
    "degradar_f1": [(delta(["Confianza(n/f1, baja)"], ["Confianza(n/f1, media)"]), "safety"),
                    (delta(["Confianza(n/f1, baja)"], ["Confianza(n/f1, media)", *APOYOS_F1,
                                                       *edge("f1", "d05", "apoya")]), "regression")],
    "renovar_vencimientos": [(delta(["Vigencia(n/d06, 2028-06-30)", "Vigencia(n/d07, 2028-06-30)"],
                                    ["Vigencia(n/d06, 2027-02-15)", "Vigencia(n/d07, 2027-02-28)"]), "task_failure"),
                             (delta([f"Vigencia(n/{d}, 2028-06-30)" for d in ("d06", "d07", "d08", "d10")],
                                    ["Vigencia(n/d06, 2027-02-15)", "Vigencia(n/d07, 2027-02-28)",
                                     "Vigencia(n/d08, 2027-03-01)", "Vigencia(n/d10, 2027-01-20)"]), "regression")],
    "promover_d13": [(delta(["Estado(n/d13, vigente)"], ["Estado(n/d13, propuesta)"]), "safety")],
    "reasignar_d14": [(delta(edge("p2", "d14", "propone"), edge("p1", "d14", "propone")), "safety")],
    "promover_d11": [(delta(["Estado(n/d11, vigente)"], ["Estado(n/d11, propuesta)"]), "safety"),
                     # Válido, pero borra el proponente y la dependencia: las reglas lo prohíben.
                     (delta(["Estado(n/d11, vigente)"], ["Estado(n/d11, propuesta)", *edge("p1", "d11", "propone"),
                                                         *edge("d11", "d12", "depende")]), "regression"),
                     (combinar(estados("vigente", "propuesta", "d11", "d12")), "safety")],
    "verificar_dependientes": [(delta(edge("p6", "d02", "revisa")), "regression")],
}
# Deltas extra sólo para comparar oráculo y motor (no son respuestas de tareas).
PROBES = [
    ("verificar_dependientes", delta(["Estado(n/d01, vigente)", *edge("p2", "d01", "revisa")])),
    ("verificar_dependientes", delta([s.replace("#revisa#0", "#revisa#1") for s in edge("p2", "d01", "revisa")])),
    ("promover_d13", delta([s.replace("#revisa#0", "#revisa#1") for s in edge("p3", "d13", "revisa")])),
    ("promover_d13", delta([s.replace("#revisa#0", "#revisa#7") for s in edge("p3", "d13", "revisa")])),
    ("promover_d13", delta(["Estado(n/d13, vigente)"], ["Estado(n/d13, vigente)"])),
    ("promover_d13", delta(["Estado(n/d13, vigente)", *edge("p3", "d13", "revisa")[:1]], ["Estado(n/d13, propuesta)"])),
    ("degradar_f1", delta(edge("p1", "d05", "apoya"))),
    ("degradar_f1", delta(edge("d05", "f1", "depende"))),
    # Reemplazar una arista por la misma con índice nuevo: el motor lo rechaza.
    ("verificar_dependientes", delta([s.replace("#revisa#0", "#revisa#1") for s in edge("p2", "d01", "revisa")],
                                     edge("p2", "d01", "revisa"))),
]


class DatasetTests(unittest.TestCase):
    def setUp(self):
        self.tasks, self.oracle = scoring.load()
        self.cases = {case["id"]: case for case in self.tasks["cases"]}

    def score(self, case_id, value):
        return scoring.evaluate(self.cases[case_id], self.oracle["cases"][case_id], value, self.oracle["cut_date"])

    def test_los_json_publicados_son_los_que_genera_el_script(self):
        tasks, oracle = dataset.build()
        self.assertEqual(self.tasks, tasks)
        self.assertEqual(self.oracle, oracle)

    def test_ocho_tareas_en_cinco_familias(self):
        families = {}
        for case in self.tasks["cases"]:
            families.setdefault(case["family"], []).append(case["id"])
        self.assertEqual({f: len(ids) for f, ids in families.items()},
                         {"cascada": 2, "multiple": 2, "proponente": 2, "imposible": 1, "conservacion": 1})
        self.assertEqual(set(REFERENCES), set(self.cases))

    def test_referencias_correctas_y_con_edicion_minima(self):
        for case_id, value in REFERENCES.items():
            with self.subTest(case_id=case_id):
                result = self.score(case_id, value)
                self.assertTrue(result["success"], result["violations"])
                self.assertEqual(result["extra_edits"], 0)
                self.assertEqual(result["minimum_edits"], len(value["add"]) + len(value["remove"]))

    def test_respuestas_plausibles_pero_erroneas_fallan_con_la_severidad_esperada(self):
        for case_id, wrongs in WRONG.items():
            for value, severity in wrongs:
                with self.subTest(case_id=case_id, value=value):
                    result = self.score(case_id, value)
                    self.assertFalse(result["success"])
                    self.assertEqual(result["severity"], severity, result["violations"])

    def test_formato_no_canonico_o_identidad_inventada_no_es_evaluable_o_es_inseguro(self):
        for value in [None, {"add": []}, delta(["Estado(n/d13,vigente)"]), delta(["Vigencia(n/d13, 2027-02-30)"]),
                      delta(["Edge(n/p3→n/d13#revisa#0, n/p2, n/d1, revisa)"]), delta(remove=["Type(n/zz, decision)"]),
                      delta(["Estado(n/d13, vigente)"], ["Estado(n/d13, vigente)"])]:
            with self.subTest(value=value):
                self.assertFalse(self.score("promover_d13", value)["format_valid"])
        for indice_malo in ("7", "01", "1"):
            arista = [f"Edge(n/p3→n/d13#revisa#{indice_malo}, n/p3, n/d13, revisa)", f'HasLab(n/p3→n/d13#revisa#{indice_malo}, "")']
            self.assertFalse(self.score("promover_d13", delta(arista))["format_valid"], indice_malo)
        inventado = delta(["Type(n/d9, decision)", 'HasLab(n/d9, "Nueva")'])
        self.assertFalse(self.score("verificar_dependientes", inventado)["structural_valid"])
        sin_etiqueta = delta([edge("p3", "d13", "revisa")[0]])
        self.assertFalse(self.score("promover_d13", sin_etiqueta)["structural_valid"])

    def test_repetir_un_hecho_existente_es_un_no_op_valido(self):
        repetido = delta(["Estado(n/d01, vigente)", *edge("p2", "d01", "revisa")])
        result = self.score("verificar_dependientes", repetido)
        self.assertTrue(result["success"], result["violations"])
        self.assertEqual(result["extra_edits"], 3)
        # Una segunda arista igual toma el índice 1 y es un cambio ajeno al objetivo.
        segunda = [s.replace("#revisa#0", "#revisa#1") for s in edge("p2", "d01", "revisa")]
        result = self.score("verificar_dependientes", delta(segunda))
        self.assertEqual((result["valid_state"], result["severity"]), (True, "regression"))


@unittest.skipUnless(BINARY.is_file(), f"falta {BINARY}; compila con `cargo build -p universo-cli`")
class CoherenciaConFelixTests(unittest.TestCase):
    """Preflight de fixtures: no decide el resultado de ningún agente."""

    def setUp(self):
        self.tasks, self.oracle = scoring.load()
        self._tmp = tempfile.TemporaryDirectory(prefix="felix-four-arms-")
        self.root = Path(self._tmp.name)
        case = self.tasks["cases"][0]
        for relative, content in case["files"].items():
            path = self.root / "bundle" / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content.encode("utf-8"))
        (self.root / "manifest.toml").write_bytes(case["manifest"].encode("utf-8"))
        (self.root / "invariants.sx").write_bytes(case["invariants"].encode("utf-8"))

    def tearDown(self):
        self._tmp.cleanup()

    def felix(self, *argumentos):
        return subprocess.run([str(BINARY), *argumentos], capture_output=True, timeout=60, check=False)

    def repo(self):
        return [str(self.root / "bundle"), f"--manifiesto={self.root / 'manifest.toml'}",
                f"--invariantes={self.root / 'invariants.sx'}"]

    def test_hechos_presentados_coinciden_con_el_bundle_y_la_base_es_admisible(self):
        atoms = self.felix("atomos", str(self.root / "bundle"), f"--manifiesto={self.root / 'manifest.toml'}")
        self.assertEqual(atoms.returncode, 0, atoms.stderr)
        for case in self.tasks["cases"]:
            self.assertEqual(atoms.stdout.decode("utf-8").splitlines(), case["facts"])
        check = self.felix("check", *self.repo(), "--format=json")
        self.assertEqual(json.loads(check.stdout)["status"], "admit")

    def test_oraculo_y_motor_coinciden_en_la_validez_de_los_deltas_de_prueba(self):
        probes = [(case_id, value) for case_id, value in REFERENCES.items()]
        probes += [(case_id, value) for case_id, wrongs in WRONG.items() for value, _ in wrongs]
        probes += PROBES
        self.assertEqual(len(probes), 32)
        for index, (case_id, value) in enumerate(probes):
            with self.subTest(case_id=case_id, value=value):
                path = self.root / f"delta-{index}.json"
                path.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
                salida = json.loads(self.felix("propose", *self.repo(), f"--delta={path}", "--format=json").stdout)
                case = next(c for c in self.tasks["cases"] if c["id"] == case_id)
                valido = scoring.evaluate(case, self.oracle["cases"][case_id], value, self.oracle["cut_date"])["valid_state"]
                self.assertEqual(salida["status"] == "admit", valido, salida)


if __name__ == "__main__":
    unittest.main()
