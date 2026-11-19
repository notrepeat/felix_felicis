import copy
import json
import unittest
from pathlib import Path

from trajectory import summarize_trajectories, trajectory

HERE = Path(__file__).resolve().parent
TASKS = {case["id"]: case for case in json.loads((HERE / "tasks.json").read_text(encoding="utf-8"))["cases"]}
ORACLE = json.loads((HERE / "oracle.json").read_text(encoding="utf-8"))["cases"]

GOOD = {"schema": "felix-delta-v1", "add": ["Parent(n/b, n/a)", "Parent(n/a, n/c)"], "remove": ["Parent(n/a, n/b)"]}
# Creates a cycle n/a -> n/b -> n/a: structurally invalid for the oracle.
CYCLIC = {"schema": "felix-delta-v1", "add": ["Parent(n/b, n/a)"], "remove": []}
# Valid structure, but misses half of the goal.
PARTIAL = {"schema": "felix-delta-v1", "add": ["Parent(n/b, n/a)"], "remove": ["Parent(n/a, n/b)"]}


def call(tool, delta=None, status=None, error=None, budget_denied=False):
    arguments = {"schema": "felix-mcp-v1"}
    if delta is not None:
        arguments["delta"] = delta
    if budget_denied:
        response = {"result": {"isError": True, "content": [{"type": "text", "text": "evaluation_tool_budget_exhausted"}]}}
    elif error == "tool":
        response = {"result": {"isError": True, "content": [{"type": "text", "text": "invalid_arguments"}]}}
    elif error == "protocol":
        response = {"error": {"code": -32601, "message": "unknown"}}
    else:
        response = {"result": {"isError": False, "structuredContent": {"status": status, "tool": tool}}}
    entry = {"request": {"method": "tools/call", "params": {"name": tool, "arguments": arguments}}, "response": response}
    if budget_denied:
        entry["budget_denied"] = True
    return entry


def row(arm="with_felix", final=GOOD, calls=(), status="completed", repetition=1):
    from scoring import evaluate
    return {"case_id": "parent_rotate", "repetition": repetition, "arm": arm, "status": status,
            "mcp_ready": True if arm == "with_felix" else None, "elapsed_seconds": 4.0,
            "usage": [{"input_tokens": 1000, "output_tokens": 50}], "final_delta": final,
            "score": evaluate(TASKS["parent_rotate"], ORACLE["parent_rotate"], final), "tool_calls": list(calls)}


def build(item):
    return trajectory(item, TASKS[item["case_id"]], ORACLE[item["case_id"]])


class TrajectoryTests(unittest.TestCase):
    def test_baseline_first_delta_is_final_and_intermediate_unobserved(self):
        result = build(row(arm="without_felix"))
        self.assertFalse(result["adoption"]["tools_available"])
        self.assertFalse(result["adoption"]["adopted"])
        self.assertEqual(result["first_delta"]["source"], "final_answer")
        self.assertFalse(result["first_delta"]["intermediate_observable"])
        self.assertEqual(result["error_locus"], "none")

    def test_available_but_unused_is_not_adoption(self):
        result = build(row())
        self.assertTrue(result["adoption"]["tools_available"])
        self.assertFalse(result["adoption"]["adopted"])
        self.assertIsNone(result["adoption"]["first_tool"])
        self.assertFalse(result["final"]["verified_by_felix"])

    def test_reject_then_admit_is_a_repair_with_its_cost(self):
        calls = [call("check"), call("propose", CYCLIC, "reject"), call("why", CYCLIC, "reject"), call("propose", GOOD, "admit")]
        result = build(row(calls=calls))
        self.assertTrue(result["adoption"]["adopted"])
        self.assertEqual(result["adoption"]["first_tool"], "check")
        self.assertEqual(result["first_delta"]["source"], "tool_call")
        self.assertEqual(result["first_delta"]["step"], 2)
        self.assertFalse(result["first_delta"]["oracle"]["success"])
        self.assertEqual(result["first_delta"]["oracle"]["severity"], "safety")
        self.assertEqual(result["rejection"]["first_step"], 2)
        self.assertEqual(result["rejection"]["count"], 2)
        self.assertTrue(result["repair"]["attempted"])
        self.assertTrue(result["repair"]["admitted"])
        self.assertEqual(result["repair"]["calls_from_first_rejection_to_admit"], 2)
        self.assertTrue(result["final"]["verified_by_felix"])
        self.assertEqual(result["error_locus"], "repaired")
        self.assertEqual(result["cost"]["tool_calls_executed"], 4)
        self.assertGreater(result["cost"]["tool_response_bytes"], 0)

    def test_felix_admit_of_goal_miss_is_not_a_false_admit(self):
        # Felix verifies invariants, not the task goal: admit + valid-but-incomplete is agreement on validity.
        result = build(row(final=PARTIAL, calls=[call("propose", PARTIAL, "admit")]))
        step = result["steps"][0]
        self.assertEqual(step["validity_agreement"], "agree_valid")
        self.assertEqual(result["felix_vs_oracle"]["false_admit"], 0)
        self.assertEqual(result["error_locus"], "persisted")
        self.assertEqual(result["final"]["oracle"]["violations"], ["task_goal"])

    def test_false_admit_and_false_reject_are_counted(self):
        calls = [call("propose", CYCLIC, "admit"), call("propose", GOOD, "reject")]
        result = build(row(calls=calls))
        self.assertEqual(result["felix_vs_oracle"]["false_admit"], 1)
        self.assertEqual(result["felix_vs_oracle"]["false_reject"], 1)

    def test_errors_and_budget_are_not_verdicts(self):
        calls = [call("propose", CYCLIC, error="tool"), call("check", GOOD, error="protocol"), call("check", GOOD, budget_denied=True)]
        result = build(row(calls=calls))
        self.assertEqual([step["felix_verdict"] for step in result["steps"]], ["tool_error", "protocol_error", "budget_denied"])
        self.assertEqual(result["rejection"]["count"], 0)
        self.assertFalse(result["final"]["verified_by_felix"])
        self.assertEqual(result["cost"]["tool_calls_executed"], 2)
        self.assertEqual(result["cost"]["tool_calls_budget_denied"], 1)

    def test_good_first_proposal_broken_later_is_introduced_later(self):
        result = build(row(final=CYCLIC, calls=[call("propose", GOOD, "admit")]))
        self.assertEqual(result["error_locus"], "introduced_later")
        self.assertFalse(result["final"]["matches_last_proposal"])

    def test_verified_ignores_atom_order(self):
        reordered = {"schema": "felix-delta-v1", "add": list(reversed(GOOD["add"])), "remove": GOOD["remove"]}
        result = build(row(final=GOOD, calls=[call("propose", reordered, "admit")]))
        self.assertTrue(result["final"]["verified_by_felix"])
        self.assertTrue(result["final"]["matches_last_proposal"])

    def test_missing_final_answer_is_no_delta(self):
        result = build(row(final=None, status="timeout"))
        self.assertEqual(result["error_locus"], "no_delta")
        self.assertFalse(result["final"]["success"])

    def test_usage_absent_is_null_not_zero(self):
        item = row()
        item["usage"] = []
        self.assertIsNone(build(item)["cost"]["input_tokens"])


class SummaryTests(unittest.TestCase):
    def plan(self, repetitions=1):
        return {"schedule": [{"case": "parent_rotate", "repetition": r, "arm": a}
                             for r in range(1, repetitions + 1) for a in ["without_felix", "with_felix"]]}

    def test_counts_per_arm_and_keeps_limits(self):
        rows = [row(arm="without_felix"), row(calls=[call("propose", CYCLIC, "reject"), call("propose", GOOD, "admit")])]
        summary = summarize_trajectories(self.plan(), rows, TASKS, ORACLE)
        felix = summary["arms"]["with_felix"]
        self.assertEqual(felix["runs"], 1)
        self.assertEqual(felix["adopted"], 1)
        self.assertEqual(felix["first_delta_success"], 0)
        self.assertEqual(felix["error_locus"], {"repaired": 1})
        self.assertEqual(felix["repairs_admitted"], 1)
        self.assertEqual(summary["arms"]["without_felix"]["adopted"], 0)
        self.assertIn("per_call_tokens_unattributable", summary["interpretation_limits"])
        self.assertTrue(summary["complete"])

    def test_duplicates_and_unplanned_runs_are_rejected(self):
        item = row(arm="without_felix")
        with self.assertRaises(ValueError):
            summarize_trajectories(self.plan(), [item, copy.deepcopy(item)], TASKS, ORACLE)
        with self.assertRaises(ValueError):
            summarize_trajectories(self.plan(), [row(arm="without_felix", repetition=2)], TASKS, ORACLE)


if __name__ == "__main__":
    unittest.main()
