"""Run the durable software-architecture fixture against the public CLI."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "examples" / "software-architecture"
EXPECTED = FIXTURE / "expected"
VALIDATE_KEYS = {"schema_version", "command", "status", "summary", "errors"}
CHECK_KEYS = {
    "schema_version",
    "command",
    "status",
    "summary",
    "errors",
    "explanations",
}
VALIDATE_SUMMARY_KEYS = {"nodes", "edges"}
CHECK_SUMMARY_KEYS = {"sources", "instances", "evaluated", "failed"}
EXPLANATION_KEYS = {"mode", "objects", "truncated", "reason", "budget"}
BUDGET_KEYS = {
    "limit",
    "consumed",
    "semantic_units",
    "evidence_units",
    "aggregator_units",
}
OBJECT_KEYS = {
    "kind",
    "source_index",
    "line",
    "column",
    "instance_index",
    "counterexample",
    "presence_core",
    "gap",
}
PREDICATES = {
    "type",
    "edge",
    "parent",
    "has_label_node",
    "has_label_edge",
    "extension",
    "tc_parent",
    "equal_node",
    "equal_edge",
}


class DemoFailure(Exception):
    """A concise, sanitized runner failure."""


@dataclass(frozen=True)
class State:
    name: str
    bundle: str
    edges: int
    check_status: str
    check_code: int


STATES = (
    State("base", "bundle", 0, "admit", 0),
    State("contradictory", "states/contradictory", 1, "reject", 1),
    State("corrected", "states/corrected", 2, "admit", 0),
)


def run_process(
    command: Sequence[str], timeout: int, label: str
) -> subprocess.CompletedProcess[bytes]:
    try:
        return subprocess.run(
            command,
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
            shell=False,
        )
    except subprocess.TimeoutExpired:
        raise DemoFailure(f"{label}: excedió el tiempo límite.") from None
    except OSError:
        raise DemoFailure(f"{label}: no se pudo iniciar el proceso.") from None


def build_cli() -> Path:
    cargo = shutil.which("cargo")
    if cargo is None:
        raise DemoFailure("no se encontró Cargo para compilar la CLI.")

    build = run_process(
        [cargo, "build", "--locked", "--quiet", "-p", "universo-cli", "--bin", "universo"],
        300,
        "compilación de la CLI",
    )
    if build.returncode != 0:
        raise DemoFailure(f"la compilación de la CLI terminó con código {build.returncode}.")

    metadata = run_process(
        [cargo, "metadata", "--locked", "--no-deps", "--format-version", "1"],
        60,
        "consulta de Cargo metadata",
    )
    if metadata.returncode != 0:
        raise DemoFailure(f"Cargo metadata terminó con código {metadata.returncode}.")
    try:
        document = json.loads(metadata.stdout)
        target_directory = document["target_directory"]
        if not isinstance(target_directory, str):
            raise TypeError
    except (KeyError, TypeError, json.JSONDecodeError, UnicodeDecodeError):
        raise DemoFailure("Cargo metadata no devolvió un directorio de compilación válido.") from None

    executable_name = "universo.exe" if os.name == "nt" else "universo"
    executable = Path(target_directory) / "debug" / executable_name
    try:
        executable_exists = executable.is_file()
    except OSError:
        executable_exists = False
    if not executable_exists:
        raise DemoFailure("no se encontró el ejecutable compilado de la CLI.")
    return executable


def read_golden(state: State, command: str, output_format: str) -> bytes:
    extension = "json" if output_format == "json" else "txt"
    path = EXPECTED / f"{state.name}-{command}.{extension}"
    try:
        return path.read_bytes()
    except OSError:
        raise DemoFailure(f"{state.name} {command} {output_format}: falta el golden esperado.") from None


def exact_keys(value: Any, expected: set[str]) -> bool:
    return isinstance(value, dict) and set(value) == expected


def is_nonnegative_int(value: Any) -> bool:
    return type(value) is int and value >= 0


def counterexample_tree_is_valid(node: Any) -> bool:
    if not isinstance(node, dict):
        return False
    rule = node.get("rule")
    if not isinstance(rule, str) or type(node.get("value")) is not bool:
        return False

    if rule == "atom":
        return (
            exact_keys(node, {"rule", "value", "predicate"})
            and isinstance(node["predicate"], str)
            and node["predicate"] in PREDICATES
        )
    if rule == "not":
        return exact_keys(node, {"rule", "value", "inner"}) and counterexample_tree_is_valid(
            node["inner"]
        )
    if rule in {"and", "or"}:
        return (
            exact_keys(node, {"rule", "value", "children"})
            and isinstance(node["children"], list)
            and all(counterexample_tree_is_valid(child) for child in node["children"])
        )
    if rule == "implies":
        return (
            exact_keys(node, {"rule", "value", "antecedent", "consequent"})
            and counterexample_tree_is_valid(node["antecedent"])
            and (
                node["consequent"] is None
                or counterexample_tree_is_valid(node["consequent"])
            )
        )
    if rule in {"forall", "exists"}:
        instances = node.get("instances")
        return (
            exact_keys(node, {"rule", "value", "search_exhausted", "instances"})
            and type(node["search_exhausted"]) is bool
            and isinstance(instances, list)
            and all(
                exact_keys(instance, {"value", "guard", "body"})
                and type(instance["value"]) is bool
                and all(
                    child is None or counterexample_tree_is_valid(child)
                    for child in (instance["guard"], instance["body"])
                )
                for instance in instances
            )
        )
    return False


def explanation_object_is_valid(item: Any) -> bool:
    return (
        exact_keys(item, OBJECT_KEYS)
        and item["kind"] == "counterexample"
        and is_nonnegative_int(item["source_index"])
        and is_nonnegative_int(item["line"])
        and is_nonnegative_int(item["column"])
        and (item["instance_index"] is None or is_nonnegative_int(item["instance_index"]))
        and counterexample_tree_is_valid(item["counterexample"])
        and item["presence_core"] is None
        and item["gap"] is None
    )


def is_architecture_witness(tree: Any) -> bool:
    if (
        not exact_keys(tree, {"rule", "value", "search_exhausted", "instances"})
        or tree["rule"] != "forall"
        or tree["value"] is not False
        or tree["search_exhausted"] is not False
        or not isinstance(tree["instances"], list)
        or len(tree["instances"]) != 4
    ):
        return False

    for instance in tree["instances"][:3]:
        if (
            instance["value"] is not True
            or instance["body"] is not None
            or instance["guard"] != {"rule": "atom", "value": False, "predicate": "edge"}
        ):
            return False

    failing = tree["instances"][3]
    body = failing["body"]
    return (
        failing["value"] is False
        and failing["guard"] == {"rule": "atom", "value": True, "predicate": "edge"}
        and exact_keys(body, {"rule", "value", "children"})
        and body["rule"] == "or"
        and body["value"] is False
        and isinstance(body["children"], list)
        and len(body["children"]) == 2
        and all(
            exact_keys(child, {"rule", "value", "inner"})
            and child["rule"] == "not"
            and child["value"] is False
            and child["inner"] == {"rule": "atom", "value": True, "predicate": "type"}
            for child in body["children"]
        )
    )


def explanation_contract_is_valid(value: Any, state: State) -> bool:
    if (
        not exact_keys(value, EXPLANATION_KEYS)
        or value["mode"] != "decide"
        or value["truncated"] is not False
        or value["reason"] is not None
        or not isinstance(value["objects"], list)
    ):
        return False

    budget = value["budget"]
    if (
        not exact_keys(budget, BUDGET_KEYS)
        or any(not is_nonnegative_int(budget[key]) for key in BUDGET_KEYS)
        or budget["limit"] != 1_000_000
        or budget["consumed"] > budget["limit"]
    ):
        return False

    objects = value["objects"]
    if state.check_status == "admit":
        return objects == []
    return (
        len(objects) == 1
        and explanation_object_is_valid(objects[0])
        and objects[0]["source_index"] == 0
        and objects[0]["line"] == 2
        and objects[0]["column"] == 1
        and objects[0]["instance_index"] is None
        and is_architecture_witness(objects[0]["counterexample"])
    )


def verify_json_contract(state: State, command: str, expected: bytes) -> None:
    try:
        document = json.loads(expected)
    except (json.JSONDecodeError, UnicodeDecodeError):
        raise DemoFailure(f"{state.name} {command}: el golden JSON no es válido.") from None

    if command == "validate":
        summary = document.get("summary") if isinstance(document, dict) else None
        if (
            not exact_keys(document, VALIDATE_KEYS)
            or type(document["schema_version"]) is not int
            or document["schema_version"] != 1
            or document["command"] != "validate"
            or document["status"] != "valid"
            or document["errors"] != []
            or not exact_keys(summary, VALIDATE_SUMMARY_KEYS)
            or any(not is_nonnegative_int(summary[key]) for key in VALIDATE_SUMMARY_KEYS)
            or summary != {"edges": state.edges, "nodes": 3}
        ):
            raise DemoFailure(f"{state.name} validate: el contrato del golden no es válido.")
        return

    summary = document.get("summary") if isinstance(document, dict) else None
    expected_failed = 1 if state.check_status == "reject" else 0
    if (
        not exact_keys(document, CHECK_KEYS)
        or type(document["schema_version"]) is not int
        or document["schema_version"] != 2
        or document["command"] != "check"
        or document["status"] != state.check_status
        or document["errors"] != []
        or not exact_keys(summary, CHECK_SUMMARY_KEYS)
        or any(not is_nonnegative_int(summary[key]) for key in CHECK_SUMMARY_KEYS)
        or summary
        != {"evaluated": 1, "failed": expected_failed, "instances": 1, "sources": 1}
        or not explanation_contract_is_valid(document["explanations"], state)
    ):
        raise DemoFailure(f"{state.name} check: el contrato del golden no es válido.")


def verify_output(
    executable: Path,
    state: State,
    command: str,
    output_format: str,
) -> None:
    bundle = FIXTURE / state.bundle
    argv = [
        str(executable),
        command,
        str(bundle),
        "--manifiesto",
        str(FIXTURE / "manifest.toml"),
    ]
    expected_code = 0
    if command == "check":
        argv.extend(
            [
                "--invariantes",
                str(FIXTURE / "invariants" / "architecture.sx"),
                "--explain",
                "decide",
            ]
        )
        expected_code = state.check_code
    argv.extend(["--format", output_format])

    label = f"{state.name} {command} {output_format}"
    result = run_process(argv, 60, label)
    if result.returncode != expected_code:
        raise DemoFailure(f"{label}: código de salida inesperado ({result.returncode}).")
    if result.stderr:
        raise DemoFailure(f"{label}: stderr debía estar vacío.")

    golden = read_golden(state, command, output_format)
    if output_format == "json":
        verify_json_contract(state, command, golden)
    if result.stdout != golden:
        offset = next(
            (index for index, pair in enumerate(zip(result.stdout, golden)) if pair[0] != pair[1]),
            min(len(result.stdout), len(golden)),
        )
        raise DemoFailure(
            f"{label}: stdout difiere del golden byte por byte "
            f"(salida {len(result.stdout)} bytes, golden {len(golden)} bytes, primer cambio {offset})."
        )


def main() -> int:
    try:
        executable = build_cli()
        for state in STATES:
            for command in ("validate", "check"):
                for output_format in ("json", "text"):
                    verify_output(executable, state, command, output_format)
    except DemoFailure as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print("OK: 3 estados de arquitectura; validate/check JSON y texto coinciden con sus goldens")
    return 0


if __name__ == "__main__":
    sys.exit(main())
