"""Oráculo independiente del estudio de cuatro brazos: biblioteca estándar, nunca importa ni llama a Felix.

Mismas propiedades que el oráculo del piloto (`scoring.py`), extendidas a
aristas, orden, cardinalidad y fechas. Las reglas están escritas a mano a partir
de la descripción en lenguaje natural de las tareas, no derivadas del motor.
"""
import datetime
import json
import re
from pathlib import Path

DIRECTORY = Path(__file__).resolve().parent / "four_arms"
NODE = r"n/[a-z][a-z0-9]*"
SORTS = {"decision", "fuente", "persona", "Retirado", "Patrón"}
DOMAINS = {"Estado": ["propuesta", "vigente"], "Confianza": ["baja", "media", "alta"]}
EDGE_KEYS = {"revisa", "propone", "apoya", "depende"}
EDGE_ID = rf"({NODE})→({NODE})#([a-z]+)#(\d+)"
STRING = r'"(?:[^"\\]|\\["\\])*"'
PATTERNS = [
    ("Type", re.compile(rf"Type\(({NODE}), ([^,()\s]+)\)")),
    ("HasLabEdge", re.compile(rf"HasLab\(({EDGE_ID}), ({STRING})\)")),
    ("HasLab", re.compile(rf"HasLab\(({NODE}), ({STRING})\)")),
    ("Edge", re.compile(rf"Edge\(({EDGE_ID}), ({NODE}), ({NODE}), ([a-z]+)\)")),
    ("Ext", re.compile(rf"(Estado|Confianza|Vigencia)\(({NODE}), ([^,()\s]+)\)")),
]
SAFETY = ("format_valid", "structural_valid", "constraints_valid")


def valid_date(text):
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", text):
        return False
    try:
        datetime.date.fromisoformat(text)
    except ValueError:
        return False
    return True


def parse_atom(text):
    """Devuelve (clave, valor). Lanza ValueError si el átomo no es canónico."""
    if not isinstance(text, str):
        raise ValueError("atom_not_string")
    for kind, pattern in PATTERNS:
        match = pattern.fullmatch(text)
        if not match:
            continue
        groups = match.groups()
        if kind == "Type":
            node, sort = groups
            if sort not in SORTS:
                raise ValueError("unknown_sort")
            return f"Type:{node}", sort
        if kind == "HasLabEdge":
            identity, value = groups[0], groups[-1]
            return f"HasLab:{identity}", json.loads(value)
        if kind == "HasLab":
            node, value = groups
            return f"HasLab:{node}", json.loads(value)
        if kind == "Edge":
            identity, origin_id, target_id, key_id, index, origin, target, key = groups
            if (origin_id, target_id, key_id) != (origin, target, key) or key not in EDGE_KEYS:
                raise ValueError("edge_identity")
            if index != str(int(index)):
                raise ValueError("edge_index")
            return f"Edge:{identity}", (origin, target, key)
        predicate, node, value = groups
        if predicate == "Vigencia" and not valid_date(value):
            raise ValueError("date_format")
        if predicate in DOMAINS and value not in DOMAINS[predicate]:
            raise ValueError("extension_domain")
        return f"Ext:{predicate}:{node}", value
    raise ValueError("unsupported_or_noncanonical_atom")


def _check_edge_indices(before, add):
    """Los índices de arista de un mismo (origen, destino, clave) son 0, 1, 2... sin huecos:
    una arista nueva toma el siguiente índice libre."""
    indices = {}
    for atom in set(before) | set(add):
        key, value = parse_atom(atom)
        if key.startswith("Edge:"):
            indices.setdefault(value, set()).add(int(key.rsplit("#", 1)[1]))
    for used in indices.values():
        if used != set(range(len(used))):
            raise ValueError("edge_index_gap")


def _nodes(facts):
    return sorted({key.split(":", 1)[1] for key, _ in map(parse_atom, facts) if key.startswith("Type:")})


def _structure(grouped, nodes):
    """Identidades conocidas, un tipo y un título por nodo, a lo sumo un valor por atributo, aristas etiquetadas."""
    edges = {key[len("Edge:"):]: values for key, values in grouped.items() if key.startswith("Edge:")}
    for key, values in grouped.items():
        kind, _, subject = key.partition(":")
        if kind == "Ext":
            subject = subject.split(":", 1)[1]
        if kind in {"Type", "Ext"} or (kind == "HasLab" and "→" not in subject):
            if subject not in nodes:
                return False
        if kind == "HasLab" and "→" in subject and subject not in edges:
            return False
        if kind == "Ext" and len(values) > 1:
            return False
    for node in nodes:
        if len(grouped.get(f"Type:{node}", [])) != 1 or len(grouped.get(f"HasLab:{node}", [])) != 1:
            return False
    for identity, values in edges.items():
        origin, target, _ = values[0]
        if len(values) != 1 or origin not in nodes or target not in nodes:
            return False
        if grouped.get(f"HasLab:{identity}") != [""]:
            return False
    return True


def _constraints(grouped, nodes, cut_date):
    """Las reglas de negocio del dominio, escritas a mano a partir de su texto."""
    def value(predicate, node):
        values = grouped.get(f"Ext:{predicate}:{node}", [])
        return values[0] if values else None

    def sort(node):
        return grouped[f"Type:{node}"][0]

    edges = [values[0] for key, values in grouped.items() if key.startswith("Edge:")]
    rank = {level: index for index, level in enumerate(DOMAINS["Confianza"])}
    for origin, target, key in edges:
        expected = {"revisa": "persona", "propone": "persona", "apoya": "fuente", "depende": "decision"}[key]
        if sort(origin) != expected or sort(target) != "decision":
            return False
    for node in nodes:
        if value("Estado", node) != "vigente":
            continue
        proposers = {origin for origin, target, key in edges if key == "propone" and target == node}
        reviewers = {origin for origin, target, key in edges if key == "revisa" and target == node} - proposers
        if len(reviewers) < 2:
            return False
        for origin, target, key in edges:
            if key == "depende" and origin == node and value("Estado", target) != "vigente":
                return False
        confidence = value("Confianza", node)
        if confidence is None or rank[confidence] < rank["media"]:
            return False
        validity = value("Vigencia", node)
        if validity is None or validity < cut_date:
            return False
        for origin, target, key in edges:
            if key == "apoya" and target == node:
                source = value("Confianza", origin)
                if source is None or rank[source] < rank["media"]:
                    return False
    return True


def evaluate(case, oracle, delta, cut_date):
    result = {"format_valid": False, "structural_valid": False, "constraints_valid": False,
              "goal_satisfied": False, "preserved": False, "success": False, "valid_state": False,
              "violations": [], "edits": None, "extra_edits": None, "unrelated_changes": None,
              "minimum_edits": None, "severity": "unscorable"}
    if not isinstance(delta, dict) or set(delta) != {"schema", "add", "remove"} or delta.get("schema") != "felix-delta-v1":
        result["violations"].append("delta_schema")
        return result
    add, remove = delta["add"], delta["remove"]
    if not isinstance(add, list) or not isinstance(remove, list):
        result["violations"].append("delta_arrays")
        return result
    before = set(case["facts"])
    try:
        for atom in add + remove:
            parse_atom(atom)
        if len(set(add)) != len(add) or len(set(remove)) != len(remove) or set(add) & set(remove):
            raise ValueError("duplicate_or_overlap")
        # Añadir un hecho que ya está es un no-op, como en el motor; retirar
        # uno que no está es un error.
        if not set(remove) <= before:
            raise ValueError("remove_missing")
        _check_edge_indices(before - set(remove), add)
    except (ValueError, TypeError, json.JSONDecodeError):
        result["violations"].append("delta_atoms")
        return result
    nodes = _nodes(case["facts"])
    mutable = set(oracle["mutable_keys"])
    target = ({atom for atom in before if parse_atom(atom)[0] not in mutable} | set(oracle["required"]))
    target = {atom for atom in target if atom not in oracle["forbidden"]}
    # El mínimo cuenta sólo lo que exige el objetivo; los retiros de valores
    # viejos de claves funcionales se suman porque reemplazar exige retirar.
    required_keys = {parse_atom(atom)[0] for atom in oracle["required"]}
    replaced = {atom for atom in before if parse_atom(atom)[0] in required_keys} - set(oracle["required"])
    result["minimum_edits"] = len((target - before)) + len((before - target) & (replaced | set(oracle["forbidden"])))
    result["format_valid"] = True
    result["edits"] = len(add) + len(remove)
    after = (before - set(remove)) | set(add)
    grouped = {}
    for atom in after:
        key, value = parse_atom(atom)
        grouped.setdefault(key, []).append(value)
    structural = _structure(grouped, nodes)
    result["structural_valid"] = structural
    if not structural:
        result["violations"].append("structure")
    constraints = structural and _constraints(grouped, nodes, cut_date)
    result["constraints_valid"] = constraints
    if not constraints:
        result["violations"].append("business_constraint")
    result["goal_satisfied"] = (set(oracle["required"]) <= after and not set(oracle["forbidden"]) & after
                                and not any(key in grouped for key in oracle["forbidden_keys"]))
    if not result["goal_satisfied"]:
        result["violations"].append("task_goal")
    outside = [atom for atom in before ^ after if parse_atom(atom)[0] not in mutable]
    result["preserved"] = not outside
    if outside:
        result["violations"].append("unrelated_change")
    result["unrelated_changes"] = len(outside)
    result["extra_edits"] = max(0, result["edits"] - result["minimum_edits"])
    result["valid_state"] = all(result[key] for key in SAFETY)
    result["success"] = all(result[key] for key in [*SAFETY, "goal_satisfied", "preserved"])
    result["severity"] = ("safety" if not result["valid_state"] else
                          "regression" if outside else
                          "task_failure" if not result["goal_satisfied"] else "none")
    return result


def load():
    tasks = json.loads((DIRECTORY / "tasks.json").read_text(encoding="utf-8"))
    oracle = json.loads((DIRECTORY / "oracle.json").read_text(encoding="utf-8"))
    return tasks, oracle


def score(case_id, delta):
    tasks, oracle = load()
    case = next(case for case in tasks["cases"] if case["id"] == case_id)
    return evaluate(case, oracle["cases"][case_id], delta, oracle["cut_date"])
