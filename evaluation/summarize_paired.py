"""Summarize all preregistered runs, including failures, without model claims."""
import argparse
import json
import statistics
from collections import Counter, defaultdict
from pathlib import Path


def distribution(values):
    return {"n": len(values), "min": min(values), "median": statistics.median(values), "max": max(values)} if values else {"n": 0}


def summarize(plan, rows):
    expected = {(item["case"], item["repetition"], item["arm"]) for item in plan["schedule"]}
    seen = {}
    for row in rows:
        key = row["case_id"], row["repetition"], row["arm"]
        if key not in expected or key in seen:
            raise ValueError("unexpected_or_duplicate_run")
        seen[key] = row
    arms = {}
    for arm in ["without_felix", "with_felix"]:
        selected = [row for row in rows if row["arm"] == arm]
        budget = sum(key[2] == arm for key in expected)
        usage = Counter()
        missing_usage = 0
        for row in selected:
            if not row.get("usage"):
                missing_usage += 1
            for event in row.get("usage", []):
                for key, value in event.items():
                    if isinstance(value, int) and not isinstance(value, bool):
                        usage[key] += value
        outcomes = Counter(row["status"] for row in selected)
        arms[arm] = {
            "planned_runs": budget, "observed_runs": len(selected), "missing_runs": budget - len(selected),
            "status_counts": dict(sorted(outcomes.items())),
            "successes": sum(row["status"] == "completed" and row["score"]["success"] for row in selected),
            "final_invalid": sum(row["final_invalid"] for row in selected),
            "goal_unsatisfied": sum(not row["score"]["goal_satisfied"] for row in selected),
            "unrelated_regressions": sum(row["score"]["format_valid"] and not row["score"]["preserved"] for row in selected),
            "elapsed_seconds": distribution([row["elapsed_seconds"] for row in selected]),
            "edits": distribution([row["score"]["edits"] for row in selected if row["score"].get("edits") is not None]),
            "extra_edits": distribution([row["score"]["extra_edits"] for row in selected if row["score"].get("extra_edits") is not None]),
            "reported_usage_totals": dict(sorted(usage.items())), "runs_missing_usage": missing_usage,
            "mcp_calls_attempted": sum(len(row.get("tool_calls", [])) for row in selected),
            "mcp_calls_executed": sum(not tool.get("budget_denied", False) for row in selected for tool in row.get("tool_calls", [])),
            "mcp_calls_budget_denied": sum(tool.get("budget_denied", False) for row in selected for tool in row.get("tool_calls", [])),
            "rejected_proposals": sum(row.get("felix_rejected_proposals", 0) for row in selected),
            "success_after_observed_rejection": sum(row.get("felix_rejected_proposals", 0) > 0 and row["score"]["success"] and row["status"] == "completed" for row in selected),
            "cost_usd": None,
        }
    pair_counts = Counter()
    pairs = []
    for case, repetition in sorted({(key[0], key[1]) for key in expected}):
        baseline, felix = seen.get((case, repetition, "without_felix")), seen.get((case, repetition, "with_felix"))
        if baseline is None or felix is None:
            category = "incomplete"
        elif baseline["prompt_sha256"] != felix["prompt_sha256"] or baseline["configured_model_alias"] != felix["configured_model_alias"]:
            raise ValueError("unmatched_pair_input_or_model")
        else:
            left = baseline["status"] == "completed" and baseline["score"]["success"]
            right = felix["status"] == "completed" and felix["score"]["success"]
            category = "both_success" if left and right else "felix_only_success" if right else "baseline_only_success" if left else "both_failure"
        pair_counts[category] += 1
        pairs.append({"case_id": case, "repetition": repetition, "category": category})
    return {"schema": "felix-paired-summary-v1", "complete": len(seen) == len(expected),
            "planned_runs": len(expected), "observed_runs": len(rows), "arms": arms,
            "paired_counts": dict(sorted(pair_counts.items())), "pairs": pairs,
            "interpretation_limits": ["small_synthetic_pilot", "two_repetitions_not_independent_tasks", "model_alias_not_provider_snapshot", "sampling_seed_unavailable", "no_observed_monetary_cost", "baseline_internal_repair_unobserved"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    plan = json.loads((args.directory / "plan.json").read_text(encoding="utf-8"))
    rows = [json.loads(line) for line in (args.directory / "results.jsonl").read_text(encoding="utf-8").splitlines() if line.strip()]
    print(json.dumps(summarize(plan, rows), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
