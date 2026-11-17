#!/usr/bin/env python3
"""Cliente MCP sintético: consulta, rechaza, corrige y readmite sin escribir."""
import argparse
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading


class Client:
    def __init__(self, binary, root, manifest, invariants):
        self.process = subprocess.Popen([str(binary), "--root", str(root), "--manifiesto", str(manifest), "--invariantes", str(invariants)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, encoding="utf-8")
        self.responses = queue.Queue()
        self.identifier = 0
        threading.Thread(target=self._read, daemon=True).start()
        self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "synthetic-agent", "version": "1"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def _read(self):
        for line in self.process.stdout:
            self.responses.put(line)

    def send(self, payload):
        self.process.stdin.write(json.dumps(payload) + "\n")
        self.process.stdin.flush()

    def request(self, method, params):
        self.identifier += 1
        self.send({"jsonrpc": "2.0", "id": self.identifier, "method": method, "params": params})
        while True:
            result = json.loads(self.responses.get(timeout=15))
            if result.get("id") == self.identifier:
                assert "error" not in result
                return result["result"]

    def tool(self, name, **arguments):
        result = self.request("tools/call", {"name": name, "arguments": {"schema": "felix-mcp-v1", **arguments}})
        assert not result.get("isError", False)
        return result["structuredContent"]

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=15)
        self.process.stdin.close()
        self.process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    suffix = ".exe" if os.name == "nt" else ""
    parser.add_argument("--binary", default="target/debug/universo-mcp" + suffix)
    parser.add_argument("--cli", default="target/debug/universo" + suffix)
    parser.add_argument("--transcript", type=Path, help="Escribir únicamente respuestas de la fixture sintética")
    args = parser.parse_args()
    binary, cli = Path(args.binary).resolve(), Path(args.cli).resolve()
    try:
        with tempfile.TemporaryDirectory(prefix="felix-mcp-demo-") as name:
            parent = Path(name)
            root = parent / "bundle"
            (root / "n").mkdir(parents=True)
            node = root / "n/item.md"
            original = b"---\ntype: a\ntitle: Item\n---\nSynthetic body never disclosed\n"
            node.write_bytes(original)
            manifest, invariants = parent / "manifest.toml", parent / "invariants.sx"
            manifest.write_text('version = "mcp/1"\n[nodos]\nsortes = ["a", "b", "Retirado", "Patrón"]\n[layout]\ndirectorios = ["n"]\n', encoding="utf-8")
            invariants.write_text("(Type @n/item b)\n", encoding="utf-8")
            before = {str(p.relative_to(parent)): p.read_bytes() for p in parent.rglob("*") if p.is_file()}
            client = Client(binary, root, manifest, invariants)
            transcript = []
            try:
                context = client.tool("context", node="n/item", include_values=True)
                transcript.append(context)
                words = context["invariants"][0]["formula"].strip("()").split()
                assert words[0] == "Type"
                desired = f"Type({words[1].removeprefix('@')}, {words[2]})"
                old = next(atom for atom in context["atoms"] if atom.startswith("Type("))
                rejected = {"schema": "felix-delta-v1", "add": [desired], "remove": []}
                proposal = client.tool("propose", delta=rejected)
                assert proposal["status"] == "reject"
                transcript.append(proposal)
                why = client.tool("why", delta=rejected)
                transcript.append(why)
                assert why["diagnostics"][0]["class"] == "Type"
                corrected = {**rejected, "remove": [old]}
                admitted = client.tool("propose", delta=corrected)
                assert admitted["status"] == "admit"
                transcript.append(admitted)
                assert client.tool("check", delta=corrected)["status"] == "admit"
            finally:
                client.close()
            # Independent CLI executable, same inputs and status/counts.
            for delta, expected in [(rejected, proposal), (corrected, admitted)]:
                delta_path = parent / "delta.json"
                delta_path.write_text(json.dumps(delta), encoding="utf-8")
                result = subprocess.run([str(cli), "propose", str(root), "--manifiesto", str(manifest), "--invariantes", str(invariants), "--delta", str(delta_path), "--format=json"], capture_output=True, timeout=30)
                actual = json.loads(result.stdout)
                assert actual["status"] == expected["status"] and actual["summary"] == expected["summary"]
                assert result.returncode == (0 if expected["status"] == "admit" else 1)
                assert not result.stderr
            delta_path.unlink()
            restarted = Client(binary, root, manifest, invariants)
            try:
                assert restarted.tool("propose", delta=corrected) == admitted
            finally:
                restarted.close()
            after = {str(p.relative_to(parent)): p.read_bytes() for p in parent.rglob("*") if p.is_file()}
            assert before == after and sorted(p.name for p in parent.iterdir()) == ["bundle", "invariants.sx", "manifest.toml"]
            assert "Synthetic body never disclosed" not in json.dumps(transcript)
            if args.transcript:
                args.transcript.write_text(json.dumps({"schema": "felix-mcp-demo-v1", "agent": "deterministic-synthetic", "responses": transcript}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            print(json.dumps({"example": "mcp-correction-loop", "reject_then_admit": True, "cli_parity": True, "restart_equal": True, "bundle_unchanged": True}))
    except (OSError, ValueError, KeyError, AssertionError, StopIteration, queue.Empty, subprocess.SubprocessError):
        print("demo_mcp_failed", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
