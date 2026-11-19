import copy
import unittest
from summarize_paired import summarize


class SummaryTests(unittest.TestCase):
    def row(self, arm, success=True, status="completed"):
        return {"case_id": "task", "repetition": 1, "arm": arm, "status": status,
                "prompt_sha256": "same", "configured_model_alias": "same", "elapsed_seconds": 2,
                "final_invalid": not success, "usage": [{"input_tokens": 100}], "tool_calls": [],
                "score": {"success": success, "format_valid": True, "goal_satisfied": success, "preserved": True, "edits": 2, "extra_edits": 0}}

    def plan(self):
        return {"schedule": [{"case": "task", "repetition": 1, "arm": arm} for arm in ["without_felix", "with_felix"]]}

    def test_timeout_is_not_dropped(self):
        summary = summarize(self.plan(), [self.row("without_felix"), self.row("with_felix", False, "timeout")])
        self.assertEqual(summary["paired_counts"], {"baseline_only_success": 1})
        self.assertEqual(summary["arms"]["with_felix"]["status_counts"], {"timeout": 1})
        self.assertTrue(summary["complete"])

    def test_missing_is_incomplete_not_negative_success(self):
        summary = summarize(self.plan(), [self.row("without_felix")])
        self.assertFalse(summary["complete"])
        self.assertEqual(summary["paired_counts"], {"incomplete": 1})

    def test_inputs_must_match_and_duplicates_are_rejected(self):
        row = self.row("with_felix")
        row["prompt_sha256"] = "different"
        with self.assertRaises(ValueError):
            summarize(self.plan(), [self.row("without_felix"), row])
        with self.assertRaises(ValueError):
            summarize(self.plan(), [row, copy.deepcopy(row)])

    def test_missing_usage_does_not_imply_zero_cost(self):
        row = self.row("without_felix")
        row["usage"] = []
        summary = summarize(self.plan(), [row])
        self.assertEqual(summary["arms"]["without_felix"]["runs_missing_usage"], 1)
        self.assertIsNone(summary["arms"]["without_felix"]["cost_usd"])


if __name__ == "__main__":
    unittest.main()
