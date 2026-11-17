#!/usr/bin/env python3
"""Fixed readonly MCP bridge with per-trial call budget and synthetic-only ledger."""
import json
import subprocess
import sys
import threading


def main():
    ledger_path, limit = sys.argv[1], int(sys.argv[2])
    child = subprocess.Popen([
        '/tools/universo-mcp', '--root', '/work/bundle',
        '--manifiesto', '/work/manifest.toml', '--invariantes', '/work/invariants.sx',
    ], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    lock = threading.Lock()
    pending = {}
    listings = set()
    calls = 0

    def emit(value):
        with lock:
            sys.stdout.write(json.dumps(value, ensure_ascii=False) + '\n')
            sys.stdout.flush()

    def receive():
        for line in child.stdout:
            value = json.loads(line)
            with lock:
                if value.get('id') in listings:
                    listings.remove(value.get('id'))
                    names = [tool.get('name') for tool in value.get('result', {}).get('tools', [])]
                    if len(names) == 4 and set(names) == {'context', 'propose', 'check', 'why'}:
                        with open(ledger_path + '.ready', 'w', encoding='utf8') as ready:
                            json.dump({'tools': sorted(names)}, ready)
                request = pending.pop(value.get('id'), None)
                if request:
                    with open(ledger_path, 'a', encoding='utf8') as ledger:
                        ledger.write(json.dumps({'request': request, 'response': value}, ensure_ascii=False) + '\n')
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
                    with lock:
                        with open(ledger_path, 'a', encoding='utf8') as ledger:
                            ledger.write(json.dumps({'request': request, 'response': response, 'budget_denied': True}) + '\n')
                    emit(response)
                    continue
                with lock:
                    pending[request.get('id')] = request
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
