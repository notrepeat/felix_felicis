#!/usr/bin/env python3
"""Measure read-only propose over 1/10/100 MiB synthetic auxiliary files.

Build the chosen debug/release binary first. No timings are CI thresholds.
"""
import argparse
import json
import pathlib
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    args = parser.parse_args()
    binary = str(pathlib.Path(args.binary).resolve())
    with tempfile.TemporaryDirectory(prefix="felix-propose-metrics-") as directory:
        parent = pathlib.Path(directory)
        root = parent / "root"
        (root / "n").mkdir(parents=True)
        (root / "n" / "a.md").write_text("---\ntype: a\ntitle: A\n---\n", encoding="utf-8")
        manifest = parent / "m.toml"
        manifest.write_text('version="t/1"\n[nodos]\nsortes=["a","Retirado","Patrón"]\n[layout]\ndirectorios=["n"]\n', encoding="utf-8")
        invariants = parent / "i.sx"
        invariants.write_text("(Type @n/a a)\n", encoding="utf-8")
        delta = parent / "d.json"
        delta.write_text('{"schema":"felix-delta-v1","add":[],"remove":[]}', encoding="utf-8")
        for mib in (1, 10, 100):
            with (root / "aux.bin").open("wb") as output:
                for _ in range(mib):
                    output.write(b"x" * 1048576)
            start = time.monotonic_ns()
            result = subprocess.run([binary, "propose", str(root), "--manifiesto", str(manifest), "--invariantes", str(invariants), "--delta", str(delta), "--format=json"], capture_output=True, check=True)
            elapsed = time.monotonic_ns() - start
            if result.stderr or json.loads(result.stdout)["status"] != "admit":
                raise RuntimeError("unexpected propose result")
            print(json.dumps({"raw_aux_mib": mib, "elapsed_ns": elapsed, "exit_code": result.returncode}))


if __name__ == "__main__":
    main()
