"""Run the fixture integration story and verify the public CLI snapshots."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import threading
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from time import perf_counter_ns
from typing import Any, Dict, Optional, Sequence, Tuple, Union


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
PRIVATE_FIXTURE_MARKERS = (
    "n/domain",
    "n/port",
    "n/adapter",
    "n/domain→n/adapter#depends#0",
    "n/domain→n/port#depends#0",
    "n/port→n/adapter#depends#0",
    '"domain"',
    '"port"',
    '"adapter"',
    '"dependency"',
    "type: domain",
    "type: port",
    "type: adapter",
    "kind: dependency",
    "Core domain body.",
    "Application boundary.",
    "External adapter.",
    "fixture-note: preserve domain",
    "fixture-note: preserve port",
    "fixture-note: preserve adapter",
)
PRIVATE_FIXTURE_TOKENS = ("domain", "port", "adapter", "depends", "dependency")


class DemoFailure(Exception):
    """A concise, sanitized runner failure."""


class DuplicateJSONKeyError(ValueError):
    """A JSON object contains a key more than once."""


@dataclass(frozen=True)
class State:
    name: str
    bundle: str
    edges: int
    check_status: str
    check_code: int
    invariant: str = "invariants/architecture.sx"
    explain_mode: str = "decide"
    gap_expected: bool = False


STATES = (
    State("base", "bundle", 0, "admit", 0),
    State("contradictory", "states/contradictory", 1, "reject", 1),
    State("corrected", "states/corrected", 2, "admit", 0),
    State(
        "gap",
        "states/gap",
        1,
        "reject",
        1,
        "invariants/architecture-gap.sx",
        "enumerate",
        True,
    ),
)


RSS_SAMPLE_INTERVAL_SECONDS = 0.1
ProcessSnapshot = Dict[int, Tuple[int, Optional[int]]]


def aggregate_process_tree_rss(
    root_pid: int, snapshot: ProcessSnapshot
) -> tuple[int, int]:
    """Sum RSS bytes and process count for root_pid and observed descendants."""
    children: dict[int, list[int]] = {}
    for pid, (parent_pid, _) in snapshot.items():
        children.setdefault(parent_pid, []).append(pid)

    total_bytes = 0
    process_count = 0
    pending = [root_pid]
    visited: set[int] = set()
    while pending:
        pid = pending.pop()
        if pid in visited:
            continue
        visited.add(pid)
        memory = snapshot.get(pid)
        if memory is not None and memory[1] is not None:
            total_bytes += memory[1]
            process_count += 1
        pending.extend(children.get(pid, ()))
    return total_bytes, process_count


def linux_process_memory_snapshot() -> ProcessSnapshot:
    """Read a best-effort PID/PPID/RSS snapshot from Linux procfs."""
    snapshot: ProcessSnapshot = {}
    page_size = os.sysconf("SC_PAGE_SIZE")
    for process_path in Path("/proc").iterdir():
        if not process_path.name.isdecimal():
            continue
        try:
            stat = (process_path / "stat").read_text(encoding="ascii")
            fields = stat[stat.rfind(")") + 2 :].split()
            parent_pid = int(fields[1])
            resident_pages = int((process_path / "statm").read_text(encoding="ascii").split()[1])
        except (IndexError, OSError, ValueError):
            # Processes can exit between the procfs reads.
            continue
        snapshot[int(process_path.name)] = (parent_pid, resident_pages * page_size)
    return snapshot


def windows_process_memory_snapshot() -> ProcessSnapshot:
    """Read a best-effort PID/PPID/working-set snapshot using Win32 APIs."""
    import ctypes
    from ctypes import wintypes

    class ProcessEntry32W(ctypes.Structure):
        _fields_ = [
            ("dwSize", wintypes.DWORD),
            ("cntUsage", wintypes.DWORD),
            ("th32ProcessID", wintypes.DWORD),
            ("th32DefaultHeapID", ctypes.c_size_t),
            ("th32ModuleID", wintypes.DWORD),
            ("cntThreads", wintypes.DWORD),
            ("th32ParentProcessID", wintypes.DWORD),
            ("pcPriClassBase", ctypes.c_long),
            ("dwFlags", wintypes.DWORD),
            ("szExeFile", ctypes.c_wchar * 260),
        ]

    class ProcessMemoryCounters(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("PageFaultCount", wintypes.DWORD),
            ("PeakWorkingSetSize", ctypes.c_size_t),
            ("WorkingSetSize", ctypes.c_size_t),
            ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
            ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
            ("PagefileUsage", ctypes.c_size_t),
            ("PeakPagefileUsage", ctypes.c_size_t),
        ]

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    kernel32.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    kernel32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    kernel32.Process32FirstW.argtypes = [wintypes.HANDLE, ctypes.POINTER(ProcessEntry32W)]
    kernel32.Process32FirstW.restype = wintypes.BOOL
    kernel32.Process32NextW.argtypes = [wintypes.HANDLE, ctypes.POINTER(ProcessEntry32W)]
    kernel32.Process32NextW.restype = wintypes.BOOL
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.CloseHandle.restype = wintypes.BOOL
    psapi.GetProcessMemoryInfo.argtypes = [
        wintypes.HANDLE,
        ctypes.POINTER(ProcessMemoryCounters),
        wintypes.DWORD,
    ]
    psapi.GetProcessMemoryInfo.restype = wintypes.BOOL

    snapshot_handle = kernel32.CreateToolhelp32Snapshot(0x00000002, 0)
    invalid_handle = ctypes.c_void_p(-1).value
    if snapshot_handle == invalid_handle:
        raise OSError(ctypes.get_last_error(), "CreateToolhelp32Snapshot failed")

    parents: dict[int, int] = {}
    try:
        entry = ProcessEntry32W()
        entry.dwSize = ctypes.sizeof(entry)
        has_entry = kernel32.Process32FirstW(snapshot_handle, ctypes.byref(entry))
        while has_entry:
            parents[int(entry.th32ProcessID)] = int(entry.th32ParentProcessID)
            has_entry = kernel32.Process32NextW(snapshot_handle, ctypes.byref(entry))
    finally:
        kernel32.CloseHandle(snapshot_handle)

    children: dict[int, list[int]] = {}
    for pid, parent_pid in parents.items():
        children.setdefault(parent_pid, []).append(pid)
    tree: list[int] = []
    pending = [os.getpid()]
    visited: set[int] = set()
    while pending:
        pid = pending.pop()
        if pid in visited:
            continue
        visited.add(pid)
        tree.append(pid)
        pending.extend(children.get(pid, ()))

    memory_snapshot: ProcessSnapshot = {}
    query_rights = 0x0400 | 0x0010  # PROCESS_QUERY_INFORMATION | PROCESS_VM_READ
    for pid in tree:
        parent_pid = parents.get(pid, 0)
        memory_snapshot[pid] = (parent_pid, None)
        handle = kernel32.OpenProcess(query_rights, False, pid)
        if not handle:
            continue
        try:
            counters = ProcessMemoryCounters()
            counters.cb = ctypes.sizeof(counters)
            if psapi.GetProcessMemoryInfo(
                handle, ctypes.byref(counters), ctypes.sizeof(counters)
            ):
                memory_snapshot[pid] = (parent_pid, int(counters.WorkingSetSize))
        finally:
            kernel32.CloseHandle(handle)
    return memory_snapshot


def process_memory_snapshot() -> ProcessSnapshot | None:
    if sys.platform.startswith("linux"):
        return linux_process_memory_snapshot()
    if os.name == "nt":
        return windows_process_memory_snapshot()
    return None


class ProcessTreeRSSSampler:
    """Sample the runner and observed descendants; the peak is approximate."""

    def __init__(self) -> None:
        self._root_pid = os.getpid()
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._sample_loop, daemon=True)
        self._started = False
        self._samples = 0
        self._peak_bytes = 0
        self._peak_processes = 0

    def _sample(self) -> None:
        try:
            snapshot = process_memory_snapshot()
        except OSError:
            return
        if snapshot is None:
            return
        total_bytes, process_count = aggregate_process_tree_rss(self._root_pid, snapshot)
        if process_count == 0:
            return
        self._samples += 1
        if total_bytes > self._peak_bytes:
            self._peak_bytes = total_bytes
            self._peak_processes = process_count

    def _sample_loop(self) -> None:
        while not self._stop.wait(RSS_SAMPLE_INTERVAL_SECONDS):
            self._sample()

    def start(self) -> None:
        self._sample()
        if sys.platform.startswith("linux") or os.name == "nt":
            if self._samples == 0:
                raise DemoFailure("no se pudo tomar una muestra RSS del proceso del runner.")
            self._started = True
            self._thread.start()

    def stop(self) -> None:
        if not self._started:
            return
        self._stop.set()
        self._thread.join()
        self._sample()
        self._started = False


def metrics_diagnostic(elapsed_ns: int, sampler: ProcessTreeRSSSampler) -> str:
    rss = str(sampler._peak_bytes) if sampler._samples else "unavailable"
    return (
        "METRICS "
        f"elapsed_ns={elapsed_ns} "
        f"rss_tree_peak_sampled_bytes={rss} "
        f"rss_tree_sample_interval_ms={int(RSS_SAMPLE_INTERVAL_SECONDS * 1000)} "
        f"rss_tree_samples={sampler._samples} "
        f"rss_processes_at_peak={sampler._peak_processes}"
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


def run_integration_story() -> None:
    cargo = shutil.which("cargo")
    if cargo is None:
        raise DemoFailure("no se encontró Cargo para ejecutar la historia de integración.")

    result = run_process(
        [
            cargo,
            "test",
            "--locked",
            "-p",
            "universo-cli",
            "--test",
            "demo_arquitectura",
            "--quiet",
        ],
        300,
        "historia de integración de arquitectura",
    )
    if result.returncode != 0:
        raise DemoFailure(
            "la historia de integración de arquitectura falló "
            f"(código {result.returncode})."
        )


def unique_object_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise DuplicateJSONKeyError
        value[key] = item
    return value


def strict_json_loads(raw: Union[bytes, str]) -> Any:
    return json.loads(raw, object_pairs_hook=unique_object_pairs)


@lru_cache(maxsize=1)
def private_fixture_markers() -> tuple[bytes, ...]:
    markers = {marker.encode("utf-8") for marker in PRIVATE_FIXTURE_MARKERS}
    for path in FIXTURE.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(FIXTURE)
        for marker in (
            str(path),
            path.as_posix(),
            relative.as_posix(),
            relative.name,
            (Path("examples/software-architecture") / relative).as_posix(),
        ):
            markers.add(marker.encode("utf-8"))
            markers.add(marker.casefold().encode("utf-8"))

        if path.suffix == ".md" and relative.parts[0] in {"bundle", "states"}:
            node_id = Path(*relative.parts[-2:]).with_suffix("").as_posix()
            markers.add(node_id.encode("utf-8"))
            markers.add(node_id.casefold().encode("utf-8"))

    for proposal in (FIXTURE / "proposals").rglob("delta.json"):
        try:
            delta = json.loads(proposal.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError):
            raise DemoFailure("no se pudo cargar la fixture para comprobar la redacción.") from None
        for atom in (*delta.get("add", []), *delta.get("remove", [])):
            if not isinstance(atom, str):
                raise DemoFailure("la fixture contiene un átomo inválido al comprobar redacción.")
            markers.add(atom.encode("utf-8"))
            markers.add(atom.casefold().encode("utf-8"))
            identity = atom.partition("(")[2].split(",", maxsplit=1)[0].strip()
            if "→" in identity:
                markers.add(identity.encode("utf-8"))
                markers.add(identity.casefold().encode("utf-8"))

    for relative in ("invariants/architecture.sx", "invariants/architecture-gap.sx"):
        try:
            invariant = (FIXTURE / relative).read_text(encoding="utf-8")
        except (OSError, UnicodeError):
            raise DemoFailure("no se pudo cargar la fixture para comprobar la redacción.") from None
        markers.update(
            line.strip().encode("utf-8")
            for line in invariant.splitlines()
            if line.strip() and not line.lstrip().startswith(";")
        )

    for path in (ROOT, FIXTURE):
        for marker in (str(path), path.as_posix()):
            markers.add(marker.encode("utf-8"))
            markers.add(marker.casefold().encode("utf-8"))
    for marker in tuple(markers):
        for ensure_ascii in (False, True):
            escaped = json.dumps(marker.decode("utf-8"), ensure_ascii=ensure_ascii)[1:-1]
            markers.add(escaped.encode("utf-8"))
    return tuple(sorted(markers, key=len, reverse=True))


def verify_redaction(state: State, output_name: str, raw: bytes) -> None:
    markers = private_fixture_markers()
    normalized = raw.replace(b"\\", b"/").lower()
    if any(
        marker in raw or marker.replace(b"\\", b"/").lower() in normalized
        for marker in markers
    ):
        raise DemoFailure(f"{state.name} {output_name}: se expuso un marcador concreto de la fixture.")
    normalized_text = normalized.decode("utf-8", errors="replace")
    if any(
        re.search(rf"(?<!\w){re.escape(token)}(?!\w)", normalized_text)
        for token in PRIVATE_FIXTURE_TOKENS
    ):
        raise DemoFailure(f"{state.name} {output_name}: se expuso un token concreto de la fixture.")


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
        document = strict_json_loads(metadata.stdout)
        target_directory = document["target_directory"]
        if not isinstance(target_directory, str):
            raise TypeError
    except (KeyError, TypeError, ValueError, UnicodeDecodeError):
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


def gap_contract_is_valid(gap: Any) -> bool:
    counts = {
        "available_edges": 1,
        "available_nodes": 3,
        "existential_candidates_evaluated": 3,
        "universal_bindings_examined": 2,
    }
    form = {
        "body": {"kind": "atom", "predicate": "type"},
        "edge_binders": 1,
        "guard": {"kind": "atom", "predicate": "edge"},
        "node_binders": 1,
    }
    return (
        exact_keys(gap, {"class", "counts", "form"})
        and gap["class"] == "forall_exists_without_witness"
        and exact_keys(gap["counts"], set(counts))
        and all(is_nonnegative_int(gap["counts"][key]) for key in counts)
        and gap["counts"] == counts
        and exact_keys(gap["form"], set(form))
        and type(gap["form"]["edge_binders"]) is int
        and type(gap["form"]["node_binders"]) is int
        and exact_keys(gap["form"]["body"], {"kind", "predicate"})
        and exact_keys(gap["form"]["guard"], {"kind", "predicate"})
        and gap["form"] == form
    )


def is_gap_witness(tree: Any) -> bool:
    if (
        not exact_keys(tree, {"rule", "value", "search_exhausted", "instances"})
        or tree["rule"] != "forall"
        or tree["value"] is not False
        or tree["search_exhausted"] is not False
        or not isinstance(tree["instances"], list)
        or len(tree["instances"]) != 2
    ):
        return False

    first, failing = tree["instances"]
    if (
        first["value"] is not True
        or first["body"] is not None
        or first["guard"] != {"rule": "atom", "value": False, "predicate": "type"}
    ):
        return False

    existential = failing["body"]
    if (
        failing["value"] is not False
        or failing["guard"] != {"rule": "atom", "value": True, "predicate": "type"}
        or not exact_keys(existential, {"rule", "value", "search_exhausted", "instances"})
        or existential["rule"] != "exists"
        or existential["value"] is not False
        or existential["search_exhausted"] is not True
        or not isinstance(existential["instances"], list)
        or len(existential["instances"]) != 3
    ):
        return False

    return all(
        instance["value"] is False
        and instance["body"] is None
        and instance["guard"] == {"rule": "atom", "value": False, "predicate": "edge"}
        for instance in existential["instances"]
    )


def explanation_object_is_valid(item: Any, state: State) -> bool:
    structurally_valid = (
        exact_keys(item, OBJECT_KEYS)
        and item["kind"] == "counterexample"
        and is_nonnegative_int(item["source_index"])
        and is_nonnegative_int(item["line"])
        and is_nonnegative_int(item["column"])
        and (item["instance_index"] is None or is_nonnegative_int(item["instance_index"]))
        and counterexample_tree_is_valid(item["counterexample"])
        and item["presence_core"] is None
    )
    if not structurally_valid:
        return False
    if state.gap_expected:
        return gap_contract_is_valid(item["gap"]) and is_gap_witness(item["counterexample"])
    return item["gap"] is None and is_architecture_witness(item["counterexample"])


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
        or value["mode"] != state.explain_mode
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
        and explanation_object_is_valid(objects[0], state)
        and objects[0]["source_index"] == 0
        and objects[0]["line"] == 2
        and objects[0]["column"] == 1
        and objects[0]["instance_index"] is None
    )


def verify_json_contract(state: State, command: str, raw: bytes, output_name: str) -> None:
    try:
        document = strict_json_loads(raw)
    except (ValueError, UnicodeDecodeError):
        raise DemoFailure(
            f"{state.name} {command}: {output_name} JSON no es válido o repite claves."
        ) from None

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
                str(FIXTURE / state.invariant),
                "--explain",
                state.explain_mode,
            ]
        )
        expected_code = state.check_code
    argv.extend(["--format", output_format])

    label = f"{state.name} {command} {output_format}"
    result = run_process(argv, 60, label)
    golden = read_golden(state, command, output_format)
    verify_redaction(state, "stdout", result.stdout)
    verify_redaction(state, "stderr", result.stderr)
    verify_redaction(state, "golden", golden)
    if output_format == "json":
        verify_json_contract(state, command, result.stdout, "stdout")
        verify_json_contract(state, command, golden, "golden")
    if result.returncode != expected_code:
        raise DemoFailure(f"{label}: código de salida inesperado ({result.returncode}).")
    if result.stderr:
        raise DemoFailure(f"{label}: stderr debía estar vacío.")
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
    started_ns = perf_counter_ns()
    sampler = ProcessTreeRSSSampler()
    try:
        sampler.start()
        run_integration_story()
        executable = build_cli()
        for state in STATES:
            for command in ("validate", "check"):
                for output_format in ("json", "text"):
                    verify_output(executable, state, command, output_format)
    except DemoFailure as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1
    finally:
        sampler.stop()

    elapsed_ns = perf_counter_ns() - started_ns
    print(
        "OK: historia de integración aprobada; 4 estados de arquitectura y "
        "16 salidas validate/check JSON/texto coinciden con sus goldens",
        flush=True,
    )
    print(metrics_diagnostic(elapsed_ns, sampler), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
