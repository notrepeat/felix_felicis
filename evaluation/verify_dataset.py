"""Preflight fixture consistency before model runs; never exposed to the agent."""
import argparse
import json
import subprocess
import tempfile
from pathlib import Path
from scoring import evaluate, parse_atom


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/universo"))
    args = parser.parse_args()
    binary = args.binary.resolve()
    directory = Path(__file__).parent
    tasks = json.loads((directory / "tasks.json").read_text(encoding="utf-8"))["cases"]
    oracles = json.loads((directory / "oracle.json").read_text(encoding="utf-8"))["cases"]
    checked = []
    for case in tasks:
        oracle = oracles[case["id"]]
        with tempfile.TemporaryDirectory(prefix="felix-paired-preflight-") as name:
            parent = Path(name)
            root = parent / "bundle"
            for relative, text in case["files"].items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text, encoding="utf-8")
            manifest, invariants, delta_path = parent / "manifest.toml", parent / "invariants.sx", parent / "delta.json"
            manifest.write_text(case["manifest"], encoding="utf-8")
            invariants.write_text(case["invariants"], encoding="utf-8")
            atoms = subprocess.run([str(binary), "atomos", str(root), "--manifiesto", str(manifest)], capture_output=True, text=True, timeout=20, check=True)
            assert set(atoms.stdout.splitlines()) == set(case["facts"]), case["id"]
            required = set(oracle["required"])
            removed = [atom for atom in case["facts"] if parse_atom(atom)[0] in oracle["mutable_keys"] and atom not in required]
            added = sorted(required - set(case["facts"]))
            target = {"schema": "felix-delta-v1", "add": added, "remove": removed}
            assert evaluate(case, oracle, target)["success"], case["id"]
            for candidate in [{"schema": "felix-delta-v1", "add": [], "remove": []}, target]:
                delta_path.write_text(json.dumps(candidate), encoding="utf-8")
                result = subprocess.run([str(binary), "propose", str(root), "--manifiesto", str(manifest), "--invariantes", str(invariants), "--delta", str(delta_path), "--format=json"], capture_output=True, text=True, timeout=20)
                payload = json.loads(result.stdout)
                assert result.returncode == 0 and payload["status"] == "admit", (case["id"], payload)
            checked.append(case["id"])
    print(json.dumps({"schema": "felix-dataset-preflight-v1", "cases": checked, "base_valid": True, "target_valid": True, "facts_match_bundle": True}))


if __name__ == "__main__":
    main()
