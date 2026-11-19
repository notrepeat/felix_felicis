import json
import unittest
from pathlib import Path
from scoring import evaluate, score

BASE = Path(__file__).parent


def delta(add=(), remove=()):
    return {"schema": "felix-delta-v1", "add": list(add), "remove": list(remove)}


class IndependentScoring(unittest.TestCase):
    def test_valid_but_wrong_goal_is_failure(self):
        result = score("label_copy", delta())
        self.assertTrue(result["structural_valid"] and result["constraints_valid"])
        self.assertFalse(result["goal_satisfied"])
        self.assertFalse(result["success"])

    def test_goal_met_with_unrelated_regression_is_failure(self):
        result = score("label_copy", delta(['HasLab(n/a, "Beta")', 'HasLab(n/c, "Unexpected")'], ['HasLab(n/a, "Alpha")', 'HasLab(n/c, "Gamma")']))
        self.assertTrue(result["goal_satisfied"])
        self.assertFalse(result["preserved"])
        self.assertFalse(result["success"])

    def test_goal_met_but_double_type_is_invalid(self):
        result = score("type_b", delta(["Type(n/b, ready)"]))
        self.assertTrue(result["goal_satisfied"])
        self.assertFalse(result["structural_valid"])

    def test_repair_without_maturity_fails_independent_constraint(self):
        result = score("type_a", delta(["Type(n/a, ready)"], ["Type(n/a, draft)"]))
        self.assertTrue(result["structural_valid"])
        self.assertFalse(result["constraints_valid"])

    def test_cycle_detected_without_engine(self):
        result = score("parent_rotate", delta(["Parent(n/b, n/a)"]))
        self.assertFalse(result["structural_valid"])

    def test_each_oracle_has_a_valid_constructible_target(self):
        cases = json.loads((BASE / "tasks.json").read_text(encoding="utf-8"))["cases"]
        oracles = json.loads((BASE / "oracle.json").read_text(encoding="utf-8"))["cases"]
        from scoring import parse_atom
        for case in cases:
            oracle = oracles[case["id"]]
            required = set(oracle["required"])
            removed = [atom for atom in case["facts"] if parse_atom(atom)[0] in oracle["mutable_keys"] and atom not in required]
            added = required - set(case["facts"])
            result = evaluate(case, oracle, delta(added, removed))
            self.assertTrue(result["success"], (case["id"], result))

    def test_malformed_or_unsupported_delta_rejected(self):
        for candidate in [{}, delta(["Type(n/unknown, draft)"]), delta(["Type(n/a, draft)"]*2), delta([], ["Type(n/a, ready)"])]:
            self.assertFalse(score("type_a", candidate)["format_valid"])

    def test_noop_success_has_zero_edits(self):
        result = score("noop_owner", delta())
        self.assertTrue(result["success"])
        self.assertEqual(result["edits"], 0)


if __name__ == "__main__":
    unittest.main()
