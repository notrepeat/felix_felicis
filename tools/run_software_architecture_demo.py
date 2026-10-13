"""Run the durable software-architecture fixture against the public CLI."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "examples" / "software-architecture"
EXPECTED = FIXTURE / "expected"


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


def verify_json_contract(state: State, command: str, expected: bytes) -> None:
    try:
        document = json.loads(expected)
    except (json.JSONDecodeError, UnicodeDecodeError):
        raise DemoFailure(f"{state.name} {command}: el golden JSON no es válido.") from None
    if not isinstance(document, dict) or document.get("command") != command:
        raise DemoFailure(f"{state.name} {command}: el golden no coincide con el comando.")

    if command == "validate":
        if (
            document.get("status") != "valid"
            or document.get("errors") != []
            or document.get("summary") != {"edges": state.edges, "nodes": 3}
        ):
            raise DemoFailure(f"{state.name} validate: el contrato del golden no es válido.")
        return

    expected_failed = 1 if state.check_status == "reject" else 0
    if (
        document.get("status") != state.check_status
        or document.get("errors") != []
        or document.get("summary")
        != {"evaluated": 1, "failed": expected_failed, "instances": 1, "sources": 1}
    ):
        raise DemoFailure(f"{state.name} check: el contrato del golden no es válido.")
    explanation = document.get("explanations")
    objects = explanation.get("objects") if isinstance(explanation, dict) else None
    if not isinstance(objects, list):
        raise DemoFailure(f"{state.name} check: falta la explicación esperada.")
    if state.check_status == "admit" and objects:
        raise DemoFailure(f"{state.name} check: la admisión no debía tener witness.")
    if state.check_status == "reject":
        if (
            len(objects) != 1
            or not isinstance(objects[0], dict)
            or any(
                objects[0].get(key) != value
                for key, value in (("source_index", 0), ("line", 2), ("column", 1))
            )
        ):
            raise DemoFailure("contradictory check: el witness no coincide con la fixture.")


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
