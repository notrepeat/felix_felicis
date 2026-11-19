"""Per-run trajectories derived from recorded runs: adoption, first delta, rejection, repair, final state.

Read-only and post hoc: it never calls a model or the Felix engine. Each delta the agent sent to a
Felix tool is re-scored with the independent oracle (`scoring.evaluate`), so Felix verdicts can be
compared with an external judgement. Felix verifies invariants, not the task goal: agreement is
measured on validity (format, structure, constraints) only.
"""
import argparse
import json
from collections import Counter
from pathlib import Path

from scoring import evaluate
from summarize_paired import distribution

HERE = Path(__file__).resolve().parent
LIMITS = ["post_hoc_derivation_not_preregistered", "per_call_tokens_unattributable",
          "per_call_latency_unrecorded", "baseline_internal_repair_unobserved",
          "felix_checks_validity_not_task_goal"]


def _canonical(delta):
    if not isinstance(delta, dict):
        return None
    try:
        return delta.get("schema"), frozenset(delta.get("add", [])), frozenset(delta.get("remove", []))
    except TypeError:
        return None


def _valid(score):
    return score["format_valid"] and score["structural_valid"] and score["constraints_valid"]


def _compact(score):
    return {"success": score["success"], "valid": _valid(score), "severity": score["severity"],
            "violations": list(score["violations"])}


def _verdict(entry):
    if entry.get("budget_denied"):
        return "budget_denied"
    response = entry.get("response", {})
    if "error" in response:
        return "protocol_error"
    result = response.get("result", {})
    if result.get("isError"):
        return "tool_error"
    status = result.get("structuredContent", {}).get("status")
    return status if status in ("admit", "reject") else "unknown"


def _agreement(verdict, score):
    if verdict not in ("admit", "reject") or score is None:
        return None
    valid = _valid(score)
    if verdict == "admit":
        return "agree_valid" if valid else "false_admit"
    return "false_reject" if valid else "agree_invalid"


def trajectory(row, case, oracle):
    steps = []
    for index, entry in enumerate(row.get("tool_calls", []), start=1):
        params = entry.get("request", {}).get("params", {})
        delta = params.get("arguments", {}).get("delta")
        verdict = _verdict(entry)
        score = evaluate(case, oracle, delta) if delta is not None else None
        steps.append({"step": index, "tool": params.get("name"), "has_delta": delta is not None,
                      "felix_verdict": verdict, "oracle": _compact(score) if score else None,
                      "validity_agreement": _agreement(verdict, score), "_delta": delta,
                      "response_bytes": len(json.dumps(entry.get("response", {}), ensure_ascii=False).encode())})

    executed = [s for s in steps if s["felix_verdict"] != "budget_denied"]
    proposals = [s for s in steps if s["has_delta"]]
    final_delta = row.get("final_delta")
    final_score = row["score"]
    final_success = row.get("status") == "completed" and final_score["success"]

    if proposals:
        first = {"source": "tool_call", "step": proposals[0]["step"], "intermediate_observable": True,
                 "oracle": proposals[0]["oracle"]}
    else:
        first = {"source": "final_answer" if final_delta is not None else None, "step": None,
                 "intermediate_observable": False, "oracle": _compact(final_score)}

    rejects = [s["step"] for s in steps if s["felix_verdict"] == "reject"]
    admit_after = next((s["step"] for s in steps if rejects and s["step"] > rejects[0]
                        and s["felix_verdict"] == "admit" and s["has_delta"]), None)
    repair = {"attempted": bool(rejects) and any(s["step"] > rejects[0] for s in proposals),
              "admitted": admit_after is not None,
              "calls_from_first_rejection_to_admit": admit_after - rejects[0] if admit_after else None}

    final_key = _canonical(final_delta)
    admitted = {_canonical(s["_delta"]) for s in proposals if s["felix_verdict"] == "admit"}
    final = {"success": final_success, "oracle": _compact(final_score),
             "verified_by_felix": final_key is not None and final_key in admitted,
             "matches_last_proposal": (_canonical(proposals[-1]["_delta"]) == final_key) if proposals else None}

    if final_delta is None:
        locus = "no_delta"
    else:
        first_ok = first["oracle"]["success"]
        locus = ("none" if first_ok and final_success else "repaired" if final_success else
                 "introduced_later" if first_ok else "persisted")

    usage = row.get("usage") or []
    tokens = lambda key: sum(e.get(key, 0) for e in usage) if usage else None
    agreements = Counter(s["validity_agreement"] for s in steps if s["validity_agreement"])
    for step in steps:
        del step["_delta"]
    return {"case_id": row["case_id"], "repetition": row["repetition"], "arm": row["arm"], "status": row.get("status"),
            "adoption": {"tools_available": row["arm"] == "with_felix" and bool(row.get("mcp_ready")),
                         "adopted": bool(executed), "first_tool": executed[0]["tool"] if executed else None,
                         "first_proposal_step": proposals[0]["step"] if proposals else None},
            "steps": steps, "first_delta": first,
            "rejection": {"count": len(rejects), "first_step": rejects[0] if rejects else None},
            "repair": repair, "final": final, "error_locus": locus,
            "felix_vs_oracle": {key: agreements.get(key, 0) for key in ["agree_valid", "agree_invalid", "false_admit", "false_reject"]},
            "cost": {"elapsed_seconds": row.get("elapsed_seconds"), "input_tokens": tokens("input_tokens"),
                     "output_tokens": tokens("output_tokens"), "tool_calls_executed": len(executed),
                     "tool_calls_budget_denied": len(steps) - len(executed),
                     "tool_response_bytes": sum(s["response_bytes"] for s in executed)}}


def summarize_trajectories(plan, rows, cases, oracles):
    expected = {(item["case"], item["repetition"], item["arm"]) for item in plan["schedule"]}
    seen = set()
    for row in rows:
        key = row["case_id"], row["repetition"], row["arm"]
        if key not in expected or key in seen:
            raise ValueError("unexpected_or_duplicate_run")
        seen.add(key)
    runs = [trajectory(row, cases[row["case_id"]], oracles[row["case_id"]]) for row in rows]
    arms = {}
    for arm in ["without_felix", "with_felix"]:
        selected = [t for t in runs if t["arm"] == arm]
        arms[arm] = {
            "runs": len(selected),
            "tools_available": sum(t["adoption"]["tools_available"] for t in selected),
            "adopted": sum(t["adoption"]["adopted"] for t in selected),
            "first_delta_success": sum(t["first_delta"]["oracle"]["success"] for t in selected),
            "final_success": sum(t["final"]["success"] for t in selected),
            "error_locus": dict(sorted(Counter(t["error_locus"] for t in selected).items())),
            "rejections": sum(t["rejection"]["count"] for t in selected),
            "repairs_attempted": sum(t["repair"]["attempted"] for t in selected),
            "repairs_admitted": sum(t["repair"]["admitted"] for t in selected),
            "calls_to_repair": distribution([t["repair"]["calls_from_first_rejection_to_admit"] for t in selected
                                             if t["repair"]["admitted"]]),
            "final_verified_by_felix": sum(t["final"]["verified_by_felix"] for t in selected),
            "felix_vs_oracle": {key: sum(t["felix_vs_oracle"][key] for t in selected)
                                for key in ["agree_valid", "agree_invalid", "false_admit", "false_reject"]},
            "tool_calls_executed": distribution([t["cost"]["tool_calls_executed"] for t in selected]),
            "elapsed_seconds": distribution([t["cost"]["elapsed_seconds"] for t in selected
                                             if t["cost"]["elapsed_seconds"] is not None]),
        }
    return {"schema": "felix-trajectory-summary-v1", "complete": seen == expected, "planned_runs": len(expected),
            "observed_runs": len(rows), "arms": arms, "runs": runs, "interpretation_limits": LIMITS}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    plan = json.loads((args.directory / "plan.json").read_text(encoding="utf-8"))
    rows = [json.loads(line) for line in (args.directory / "results.jsonl").read_text(encoding="utf-8").splitlines() if line.strip()]
    cases = {case["id"]: case for case in json.loads((HERE / "tasks.json").read_text(encoding="utf-8"))["cases"]}
    oracles = json.loads((HERE / "oracle.json").read_text(encoding="utf-8"))["cases"]
    print(json.dumps(summarize_trajectories(plan, rows, cases, oracles), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
