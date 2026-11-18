#!/usr/bin/env python3
"""Ejemplo reproducible: simula, captura el token opt-in y aplica en un temporal."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/debug/universo.exe" if os.name == "nt" else "target/debug/universo")
    binary = Path(parser.parse_args().binary).resolve()
    try:
        with tempfile.TemporaryDirectory(prefix="felix-apply-example-") as name:
            parent = Path(name)
            root = parent / "bundle"
            (root / "n").mkdir(parents=True)
            node = root / "n" / "item.md"
            node.write_text("---\ntype: a\ntitle: Item\n---\nBody preserved\n", encoding="utf-8")
            manifest = root / "manifest.toml"
            manifest.write_text('version = "example/1"\n[nodos]\nsortes = ["a", "b", "Retirado", "Patrón"]\n[layout]\ndirectorios = ["n"]\n', encoding="utf-8")
            invariants = root / "invariants.sx"
            invariants.write_text("(Type @n/item b)\n", encoding="utf-8")
            (root / "payload.bin").write_bytes(b"auxiliary bytes\x00\xff")
            delta = parent / "delta.json"
            delta.write_text(json.dumps({"schema": "felix-delta-v1", "add": ["Type(n/item, b)"], "remove": ["Type(n/item, a)"]}), encoding="utf-8")
            command = [str(binary), "propose", str(root), "--manifiesto", str(manifest), "--invariantes", str(invariants), "--delta", str(delta), "--format=json"]
            simulation = subprocess.run(command + ["--emit-snapshot-token"], capture_output=True, timeout=60, check=True)
            token = json.loads(simulation.stdout)["snapshot_token"]
            applied = subprocess.run(command + ["--apply", "--expect-snapshot", token], capture_output=True, timeout=60, check=True)
            result = json.loads(applied.stdout)
            assert result["status"] == "admit" and result["mode"] == "apply"
            assert not applied.stderr and token.encode() not in applied.stdout
            assert "type: b" in node.read_text(encoding="utf-8")
            assert (root / "payload.bin").read_bytes() == b"auxiliary bytes\x00\xff"
            assert sorted(p.name for p in parent.glob(".bundle.felix.*")) == [".bundle.felix.lock", ".bundle.felix.log", ".bundle.felix.seal"]
            print(json.dumps({"example": "transactional-apply", "status": "admit", "root_updated": True, "auxiliary_preserved": True, "transaction_siblings_removed": True}))
    except (OSError, ValueError, KeyError, AssertionError, subprocess.SubprocessError):
        print("demo_apply_failed", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
