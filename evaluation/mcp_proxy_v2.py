#!/usr/bin/env python3
"""Readonly MCP bridge, ledger v2: per-call latency and wire bytes.

Same behaviour as ``mcp_proxy.py`` (fixed child, per-trial call budget,
synthetic-only ledger, readiness marker) plus, for every ``tools/call``:

- ``request_bytes`` / ``response_bytes``: UTF-8 length of the JSON-RPC line
  exchanged with the agent, without the trailing newline.
- ``latency_ms``: monotonic time from forwarding the request to the child until
  its response line is read; ``null`` for budget-denied calls, which never
  reach the child.

Tokens per call are not observable here: the agent CLI reports tokens per run.
``mcp_proxy.py`` stays frozen because pilot 173 records its SHA-256.

Usage: mcp_proxy_v2.py LEDGER LIMIT [CHILD_COMMAND...]
"""
import json
import subprocess
import sys
import threading
import time

LEDGER_SCHEMA = 'felix-mcp-ledger-v2'
DEFAULT_CHILD = [
    '/tools/universo-mcp', '--root', '/work/bundle',
    '--manifiesto', '/work/manifest.toml', '--invariantes', '/work/invariants.sx',
]
TOOLS = {'context', 'propose', 'check', 'why'}


def _wire_bytes(line):
    return len(line.rstrip(b'\r\n'))


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    ledger_path, limit = argv[0], int(argv[1])
    child = subprocess.Popen(argv[2:] or DEFAULT_CHILD, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL)
    lock = threading.Lock()
    pending = {}
    listings = set()
    calls = 0

    def emit(value):
        line = json.dumps(value, ensure_ascii=False)
        with lock:
            sys.stdout.write(line + '\n')
            sys.stdout.flush()
        return len(line.encode())

    def record(entry):
        with open(ledger_path, 'a', encoding='utf8') as ledger:
            ledger.write(json.dumps({'schema': LEDGER_SCHEMA, **entry}, ensure_ascii=False) + '\n')

    def receive():
        for line in child.stdout:
            received = time.monotonic_ns()
            value = json.loads(line)
            with lock:
                if value.get('id') in listings:
                    listings.remove(value.get('id'))
                    names = [tool.get('name') for tool in value.get('result', {}).get('tools', [])]
                    if len(names) == 4 and set(names) == TOOLS:
                        with open(ledger_path + '.ready', 'w', encoding='utf8') as ready:
                            json.dump({'tools': sorted(names)}, ready)
                call = pending.pop(value.get('id'), None)
                if call:
                    request, request_bytes, started = call
                    record({'request': request, 'response': value, 'forwarded': True,
                            'request_bytes': request_bytes, 'response_bytes': _wire_bytes(line),
                            'latency_ms': (received - started) / 1e6})
            emit(value)

    reader = threading.Thread(target=receive, daemon=True)
    reader.start()
    try:
        for line in sys.stdin.buffer:
            request = json.loads(line)
            if request.get('method') == 'tools/list':
                with lock:
                    listings.add(request.get('id'))
            if request.get('method') == 'tools/call':
                calls += 1
                if calls > limit:
                    response = {'jsonrpc': '2.0', 'id': request.get('id'), 'result': {
                        'isError': True, 'content': [{'type': 'text', 'text': 'evaluation_tool_budget_exhausted'}]}}
                    response_bytes = emit(response)
                    with lock:
                        record({'request': request, 'response': response, 'budget_denied': True,
                                'forwarded': False, 'request_bytes': _wire_bytes(line),
                                'response_bytes': response_bytes, 'latency_ms': None})
                    continue
                with lock:
                    pending[request.get('id')] = (request, _wire_bytes(line), time.monotonic_ns())
            child.stdin.write(line)
            child.stdin.flush()
    finally:
        child.stdin.close()
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
        reader.join(timeout=2)


if __name__ == '__main__':
    main()
