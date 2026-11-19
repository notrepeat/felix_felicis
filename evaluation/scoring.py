"""Independent task oracle: stdlib atomic-set semantics, never imports Felix."""
import json
import re
from pathlib import Path

DIRECTORY = Path(__file__).resolve().parent
NODE = r"n/[abc]"
PATTERNS = [
    ("Type", re.compile(rf"Type\(({NODE}), (draft|ready|archive|Retirado|Patrón)\)")),
    ("HasLab", re.compile(rf'HasLab\(({NODE}), ("(?:[^"\\]|\\["\\])*")\)')),
    ("Parent", re.compile(rf"Parent\(({NODE}), ({NODE})\)")),
    ("Ext", re.compile(rf"(Maturity|Owner)\(({NODE}), (draft|accepted|red|blue)\)")),
]


def parse_atom(text):
    if not isinstance(text, str):
        raise ValueError("atom_not_string")
    for kind, pattern in PATTERNS:
        match = pattern.fullmatch(text)
        if match:
            fields = match.groups()
            if kind == "Ext":
                predicate, node, value = fields
                if value not in {"Maturity": {"draft", "accepted"}, "Owner": {"red", "blue"}}[predicate]:
                    raise ValueError("extension_domain")
                return f"Ext:{predicate}:{node}", value
            node, value = fields
            if kind == "HasLab":
                value = json.loads(value)
            return f"{kind}:{node}", value
    raise ValueError("unsupported_or_noncanonical_atom")


def evaluate(case, oracle, delta):
    result = {"format_valid": False, "structural_valid": False, "constraints_valid": False,
              "goal_satisfied": False, "preserved": False, "success": False,
              "violations": [], "edits": None, "extra_edits": None, "unrelated_changes": None,
              "minimum_edits": None, "severity": "unscorable"}
    if not isinstance(delta, dict) or set(delta) != {"schema", "add", "remove"} or delta.get("schema") != "felix-delta-v1":
        result["violations"].append("delta_schema")
        return result
    add, remove = delta["add"], delta["remove"]
    if not isinstance(add, list) or not isinstance(remove, list):
        result["violations"].append("delta_arrays")
        return result
    try:
        for atom in add + remove:
            parse_atom(atom)
        if len(set(add)) != len(add) or len(set(remove)) != len(remove) or set(add) & set(remove):
            raise ValueError("duplicate_or_overlap")
        before = set(case["facts"])
        if not set(remove) <= before:
            raise ValueError("remove_missing")
    except (ValueError, TypeError, json.JSONDecodeError):
        result["violations"].append("delta_atoms")
        return result
    target = (before - {atom for atom in before if parse_atom(atom)[0] in oracle["mutable_keys"]}) | set(oracle["required"])
    result["minimum_edits"] = len(before ^ target)
    result["format_valid"] = True
    result["edits"] = len(add) + len(remove)
    after = (before - set(remove)) | set(add)
    grouped = {}
    for atom in after:
        key, value = parse_atom(atom)
        grouped.setdefault(key, []).append(value)
    structural = True
    for node in ["n/a", "n/b", "n/c"]:
        for kind in ["Type", "HasLab", "Ext:Maturity", "Ext:Owner"]:
            if len(grouped.get(f"{kind}:{node}", [])) != 1:
                structural = False
        for kind in ["Parent"]:
            if len(grouped.get(f"{kind}:{node}", [])) > 1:
                structural = False
        seen, current = set(), node
        while grouped.get(f"Parent:{current}"):
            if current in seen:
                structural = False
                break
            seen.add(current)
            current = grouped[f"Parent:{current}"][0]
    result["structural_valid"] = structural
    if not structural:
        result["violations"].append("structure")
    constraints = True
    for rule in oracle["constraints"]:
        if rule["kind"] != "ready_requires_accepted":
            raise ValueError("unknown_oracle_rule")
        for node in ["n/a", "n/b", "n/c"]:
            if "ready" in grouped.get(f"Type:{node}", []) and "accepted" not in grouped.get(f"Ext:Maturity:{node}", []):
                constraints = False
    result["constraints_valid"] = constraints
    if not constraints:
        result["violations"].append("business_constraint")
    result["goal_satisfied"] = set(oracle["required"]) <= after and not set(oracle["forbidden"]) & after and not any(key in grouped for key in oracle["forbidden_keys"])
    if not result["goal_satisfied"]:
        result["violations"].append("task_goal")
    outside = [atom for atom in before ^ after if parse_atom(atom)[0] not in oracle["mutable_keys"]]
    result["preserved"] = not outside
    if outside:
        result["violations"].append("unrelated_change")
    # No-op additions are unnecessary even when final-state semantics are valid.
    result["unrelated_changes"] = len(outside)
    result["extra_edits"] = max(0, result["edits"] - result["minimum_edits"])
    result["success"] = all(result[key] for key in ["format_valid", "structural_valid", "constraints_valid", "goal_satisfied", "preserved"])
    result["severity"] = ("safety" if not structural or not constraints else
                          "regression" if outside else
                          "task_failure" if not result["goal_satisfied"] else "none")
    return result


def score(case_id, delta):
    tasks = json.loads((DIRECTORY / "tasks.json").read_text(encoding="utf-8"))
    oracle = json.loads((DIRECTORY / "oracle.json").read_text(encoding="utf-8"))
    case = next(case for case in tasks["cases"] if case["id"] == case_id)
    return evaluate(case, oracle["cases"][case_id], delta)
