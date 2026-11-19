"""Comprueba el contrato JSON público de las dos fixtures de demo."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SUMMARY_KEYS = {"sources", "instances", "evaluated", "failed"}
V2_KEYS = {
    "command",
    "errors",
    "explanations",
    "schema_version",
    "status",
    "summary",
}
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


class SmokeFailure(Exception):
    """Fallo breve y seguro de reportar en CI."""


def _run(command: list[str], timeout: int) -> subprocess.CompletedProcess[bytes]:
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


def build_cli() -> Path:
    cargo = shutil.which("cargo")
    if cargo is None:
        raise SmokeFailure("no se encontró Cargo para compilar la CLI.")

    try:
        build = _run(
            [
                cargo,
                "build",
                "--locked",
                "--quiet",
                "-p",
                "universo-cli",
                "--bin",
                "universo",
            ],
            300,
        )
    except subprocess.TimeoutExpired as error:
        raise SmokeFailure("la compilación de la CLI excedió el tiempo límite.") from error
    except OSError as error:
        raise SmokeFailure("no se pudo iniciar la compilación de la CLI.") from error
    if build.returncode != 0:
        raise SmokeFailure(f"la compilación de la CLI terminó con código {build.returncode}.")

    try:
        metadata = _run(
            [cargo, "metadata", "--locked", "--no-deps", "--format-version", "1"],
            60,
        )
    except subprocess.TimeoutExpired as error:
        raise SmokeFailure("Cargo metadata excedió el tiempo límite.") from error
    except OSError as error:
        raise SmokeFailure("no se pudo consultar el directorio de compilación.") from error
    if metadata.returncode != 0:
        raise SmokeFailure(
            f"Cargo metadata terminó con código {metadata.returncode}."
        )

    try:
        target_directory = Path(json.loads(metadata.stdout)["target_directory"])
    except (KeyError, json.JSONDecodeError, UnicodeDecodeError, TypeError) as error:
        raise SmokeFailure("Cargo metadata no devolvió un directorio de compilación.") from error

    executable_name = "universo.exe" if os.name == "nt" else "universo"
    executable = target_directory / "debug" / executable_name
    if not executable.is_file():
        raise SmokeFailure("no se encontró el ejecutable compilado de la CLI.")
    return executable


def _string_values(value: Any) -> list[str]:
    if isinstance(value, dict):
        return [item for nested in value.values() for item in _string_values(nested)]
    if isinstance(value, list):
        return [item for nested in value for item in _string_values(nested)]
    return [value] if isinstance(value, str) else []


def assert_redacted(value: dict[str, Any], case: str) -> None:
    strings = _string_values(value)
    text = "\n".join(strings).casefold()
    markers = {
        str(ROOT),
        ROOT.as_posix(),
        "examples/knowledge-base",
        "manifests/example.toml",
        "examples/knowledge-base/invariants.sx",
        "examples/knowledge-base/rejected-invariants.sx",
        "knowledge/cache-policy.md",
        "knowledge/cache-policy",
        "knowledge/index.md",
        "knowledge/reliability.md",
        "knowledge/reliability",
        "Cache only reproducible responses",
        "Example knowledge base",
        "Prefer predictable behavior",
        "Maturity x accepted",
    }
    normalized_text = text.replace("\\", "/")
    if any(
        marker.casefold().replace("\\", "/") in normalized_text
        for marker in markers
        if marker
    ):
        raise SmokeFailure(f"{case}: la salida reveló una ruta o etiqueta de la fixture.")
    if any(value.casefold() in {"cache-policy", "index", "reliability"} for value in strings):
        raise SmokeFailure(f"{case}: la salida reveló un identificador de la fixture.")


def exact_keys(value: Any, expected: set[str]) -> bool:
    return isinstance(value, dict) and set(value) == expected


def is_nonnegative_int(value: Any) -> bool:
    return type(value) is int and value >= 0


def check_counterexample_tree(node: Any) -> bool:
    if not isinstance(node, dict):
        return False
    rule = node.get("rule")
    if not isinstance(rule, str) or not isinstance(node.get("value"), bool):
        return False

    if rule == "atom":
        return (
            exact_keys(node, {"rule", "value", "predicate"})
            and isinstance(node["predicate"], str)
            and node["predicate"] in PREDICATES
        )
    if rule == "not":
        return exact_keys(node, {"rule", "value", "inner"}) and check_counterexample_tree(
            node["inner"]
        )
    if rule in {"and", "or"}:
        return (
            exact_keys(node, {"rule", "value", "children"})
            and isinstance(node["children"], list)
            and all(check_counterexample_tree(child) for child in node["children"])
        )
    if rule == "implies":
        return (
            exact_keys(node, {"rule", "value", "antecedent", "consequent"})
            and check_counterexample_tree(node["antecedent"])
            and (
                node["consequent"] is None
                or check_counterexample_tree(node["consequent"])
            )
        )
    if rule in {"forall", "exists"}:
        instances = node.get("instances")
        return (
            exact_keys(node, {"rule", "value", "search_exhausted", "instances"})
            and isinstance(node["search_exhausted"], bool)
            and isinstance(instances, list)
            and all(
                exact_keys(instance, {"value", "guard", "body"})
                and isinstance(instance["value"], bool)
                and all(
                    child is None or check_counterexample_tree(child)
                    for child in (instance["guard"], instance["body"])
                )
                for instance in instances
            )
        )
    return False


def check_explanation_object(item: Any) -> bool:
    if not exact_keys(item, OBJECT_KEYS):
        return False
    if (
        item["kind"] != "counterexample"
        or not is_nonnegative_int(item["source_index"])
        or not is_nonnegative_int(item["line"])
        or not is_nonnegative_int(item["column"])
        or not (
            item["instance_index"] is None
            or is_nonnegative_int(item["instance_index"])
        )
        or not check_counterexample_tree(item["counterexample"])
        or item["presence_core"] is not None
        or item["gap"] is not None
    ):
        return False
    return True


def check_explanation(value: Any, case: str, expected_status: str) -> None:
    if not exact_keys(value, EXPLANATION_KEYS):
        raise SmokeFailure(f"{case}: estructura raíz de explicación inesperada.")
    if (
        value["mode"] != "decide"
        or value["reason"] is not None
        or value["truncated"] is not False
        or not isinstance(value["objects"], list)
    ):
        raise SmokeFailure(f"{case}: estructura de explicación decide inesperada.")
    budget = value["budget"]
    if (
        not exact_keys(budget, BUDGET_KEYS)
        or any(not is_nonnegative_int(budget[key]) for key in BUDGET_KEYS)
        or budget["limit"] != 1_000_000
        or budget["consumed"] > budget["limit"]
    ):
        raise SmokeFailure(f"{case}: presupuesto de explicación inesperado.")

    objects = value["objects"]
    if expected_status == "admit":
        if objects:
            raise SmokeFailure(f"{case}: una admisión no debía producir contraejemplos.")
    elif len(objects) != 1 or not check_explanation_object(objects[0]):
        raise SmokeFailure(f"{case}: estructura del contraejemplo inesperada.")
    else:
        counterexample = objects[0]["counterexample"]
        if counterexample["rule"] != "forall" or counterexample["value"] is not False:
            raise SmokeFailure(f"{case}: el contraejemplo esperado no fue encontrado.")


def check_case(
    executable: Path,
    *,
    case: str,
    invariant_file: str,
    expected_status: str,
    expected_code: int,
    expected_summary: dict[str, int],
) -> None:
    command = [
        str(executable),
        "check",
        str(ROOT / "examples" / "knowledge-base"),
        "--manifiesto",
        str(ROOT / "manifests" / "example.toml"),
        "--invariantes",
        str(ROOT / "examples" / "knowledge-base" / invariant_file),
        "--explain",
        "decide",
        "--format",
        "json",
    ]
    try:
        result = _run(command, 60)
    except subprocess.TimeoutExpired as error:
        raise SmokeFailure(f"{case}: la CLI excedió el tiempo límite.") from error
    except OSError as error:
        raise SmokeFailure(f"{case}: no se pudo iniciar la CLI.") from error

    if result.returncode != expected_code:
        raise SmokeFailure(f"{case}: código de salida inesperado ({result.returncode}).")
    if result.stderr:
        raise SmokeFailure(f"{case}: stderr debía estar vacío para esta invocación JSON.")
    if not result.stdout.endswith(b"\n") or result.stdout.count(b"\n") != 1:
        raise SmokeFailure(f"{case}: stdout debía contener una línea JSON con newline final.")

    try:
        document = json.loads(result.stdout)
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise SmokeFailure(f"{case}: stdout no contenía JSON válido.") from error
    if not isinstance(document, dict) or set(document) != V2_KEYS:
        raise SmokeFailure(f"{case}: estructura raíz JSON v2 inesperada.")
    if document["schema_version"] != 2 or document["command"] != "check":
        raise SmokeFailure(f"{case}: versión o comando JSON inesperado.")
    if document["status"] != expected_status or document["errors"] != []:
        raise SmokeFailure(f"{case}: estado o lista de errores inesperados.")

    summary = document["summary"]
    if (
        not isinstance(summary, dict)
        or set(summary) != SUMMARY_KEYS
        or any(type(summary.get(key)) is not int for key in SUMMARY_KEYS)
        or summary != expected_summary
    ):
        raise SmokeFailure(f"{case}: resumen inesperado.")

    check_explanation(document["explanations"], case, expected_status)

    assert_redacted(document, case)


def main() -> int:
    try:
        executable = build_cli()
        check_case(
            executable,
            case="admit",
            invariant_file="invariants.sx",
            expected_status="admit",
            expected_code=0,
            expected_summary={"sources": 3, "instances": 3, "evaluated": 3, "failed": 0},
        )
        check_case(
            executable,
            case="reject",
            invariant_file="rejected-invariants.sx",
            expected_status="reject",
            expected_code=1,
            expected_summary={"sources": 1, "instances": 1, "evaluated": 1, "failed": 1},
        )
    except SmokeFailure as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print("OK: CLI pública admit/reject y JSON v2 verificados")
    return 0


if __name__ == "__main__":
    sys.exit(main())
