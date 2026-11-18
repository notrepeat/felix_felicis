#!/usr/bin/env python3
"""Readonly MCP bridge, ledger v2: per-call latency and wire bytes.

Same behaviour as ``mcp_proxy.py`` (fixed child, per-trial call budget,
synthetic-only ledger, readiness marker) plus, for every ``tools/call``:

- ``request_bytes``: UTF-8 length of the line received from the agent, without
  the trailing newline.
- ``response_bytes``: UTF-8 length of the line written to the agent, without
  the newline. JSON objects from the child are re-serialised (as in v1), so this
  is generally not the length of the child's original line.
- ``latency_ms``: monotonic time from the moment the request has been fully
  written and flushed to the child's stdin until the proxy has read the child's
  complete response line, before parsing it. It includes the child's work and
  pipe transit, plus the proxy's own handling of earlier child lines (parsing,
  ledger writes; normally well under a millisecond). It excludes delivery to
  the agent, which goes through a separate writer thread, so a slow agent does
  not inflate later calls. Clamped at 0 for the rare race where the response is
  read before the flush returns. ``null`` when there is no paired response.

Pairing: only JSON objects with an ``id`` and without ``method`` are responses.
Server notifications and server-to-client requests are passed on to the agent
(re-serialised like any other JSON object) and never consume a pending call.

Ledger flags besides ``forwarded``:

- ``budget_denied``: over the per-trial limit; never reaches the child.
- ``rejected: "duplicate_id"``: a ``tools/call`` reusing the id of a request
  still in flight. The agent receives JSON-RPC error -32600; it is not
  forwarded and does not consume budget.
- ``no_id``: a ``tools/call`` without id (a notification). It is forwarded and
  consumes budget, but cannot be paired: ``response`` is ``null``.
- ``unanswered``: forwarded but never answered. This covers the child exiting
  or becoming unwritable (the agent then receives JSON-RPC error -32603 so it
  does not wait forever) and calls still pending at shutdown: after the agent
  closes stdin the child gets 2 s to finish before it is killed.

Lines that are not a JSON object (invalid JSON, batches, scalars, nesting too
deep to parse) and objects that cannot be re-encoded as UTF-8 (lone surrogates)
are forwarded verbatim and never paired. Ledger lines are ASCII-escaped JSON.

Tokens per call are not observable here: the agent CLI reports tokens per run.
``mcp_proxy.py`` stays frozen because pilot 173 records its SHA-256.

Usage: mcp_proxy_v2.py LEDGER LIMIT [CHILD_COMMAND...]
"""
import json
import queue
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
INVALID_REQUEST = -32600
INTERNAL_ERROR = -32603


def _wire_bytes(line):
    return len(line.rstrip(b'\r\n'))


def _encode(value, raw):
    """Line sent to the agent: re-serialised ``value``, or ``raw`` if not encodable."""
    try:
        return json.dumps(value, ensure_ascii=False).encode('utf-8')
    except UnicodeEncodeError:
        return raw.rstrip(b'\r\n')


def _parse_object(line):
    """Return the JSON object in ``line``, or None for anything else."""
    try:
        value = json.loads(line)
    except (ValueError, RecursionError):
        return None
    return value if isinstance(value, dict) else None


def _id_key(message):
    # json.dumps keeps 1 and "1" apart, as JSON-RPC does.
    return json.dumps(message['id'], sort_keys=True)


def _error(ident, code, message):
    return {'jsonrpc': '2.0', 'id': ident, 'error': {'code': code, 'message': message}}


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    ledger_path, limit = argv[0], int(argv[1])
    child = subprocess.Popen(argv[2:] or DEFAULT_CHILD, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL)
    lock = threading.Lock()
    outgoing = queue.Queue()
    pending = {}
    listings = set()
    child_gone = threading.Event()
    calls = 0

    def write_to_agent():
        out = sys.stdout.buffer
        broken = False
        while True:
            data = outgoing.get()
            if data is None:
                return
            if broken:
                continue
            try:
                out.write(data + b'\n')
                out.flush()
            except OSError:
                broken = True

    def emit(data):
        outgoing.put(data)
        return len(data)

    def record(entry):
        with open(ledger_path, 'a', encoding='utf8') as ledger:
            ledger.write(json.dumps({'schema': LEDGER_SCHEMA, **entry}) + '\n')

    def record_answered(call):
        record({'request': call['request'], 'response': call['response'], 'forwarded': True,
                'request_bytes': call['request_bytes'], 'response_bytes': call['response_bytes'],
                'latency_ms': max(call['received'] - call['sent'], 0) / 1e6})

    def abandon(call):
        """Ledger an unanswered call and unblock the agent. Caller holds ``lock``."""
        record({'request': call['request'], 'response': None, 'forwarded': True, 'unanswered': True,
                'request_bytes': call['request_bytes'], 'response_bytes': None, 'latency_ms': None})
        emit(_encode(_error(call['request']['id'], INTERNAL_ERROR, 'mcp_child_unavailable'), b''))

    def abandon_all():
        with lock:
            for call in pending.values():
                abandon(call)
            pending.clear()

    def handle_child_line(line, received):
        value = _parse_object(line)
        if value is None:
            emit(line.rstrip(b'\r\n'))
            return
        data = _encode(value, line)
        if 'id' in value and 'method' not in value:
            key = _id_key(value)
            with lock:
                if key in listings:
                    listings.remove(key)
                    result = value.get('result')
                    tools = result.get('tools') if isinstance(result, dict) else None
                    names = [t.get('name') for t in tools if isinstance(t, dict)] if isinstance(tools, list) else []
                    if len(names) == 4 and set(names) == TOOLS:
                        with open(ledger_path + '.ready', 'w', encoding='utf8') as ready:
                            json.dump({'tools': sorted(names)}, ready)
                call = pending.pop(key, None)
                if call:
                    call.update(response=value, response_bytes=len(data), received=received)
                    if call['sent'] is not None:
                        record_answered(call)
        emit(data)

    def receive():
        try:
            for line in child.stdout:
                received = time.monotonic_ns()
                try:
                    handle_child_line(line, received)
                except Exception:  # never let one bad line stop the bridge
                    emit(line.rstrip(b'\r\n'))
        finally:
            child_gone.set()
            abandon_all()

    writer = threading.Thread(target=write_to_agent, daemon=True)
    writer.start()
    reader = threading.Thread(target=receive, daemon=True)
    reader.start()
    try:
        for line in sys.stdin.buffer:
            request = _parse_object(line)
            call = None
            if request is not None and request.get('method') == 'tools/list' and 'id' in request:
                with lock:
                    listings.add(_id_key(request))
            if request is not None and request.get('method') == 'tools/call':
                has_id = 'id' in request
                base = {'request': request, 'request_bytes': _wire_bytes(line)}
                with lock:
                    duplicate = has_id and (_id_key(request) in pending or _id_key(request) in listings)
                if duplicate:
                    response = _error(request['id'], INVALID_REQUEST, 'duplicate_request_id')
                    response_bytes = emit(_encode(response, b''))
                    with lock:
                        record({**base, 'response': response, 'forwarded': False, 'rejected': 'duplicate_id',
                                'response_bytes': response_bytes, 'latency_ms': None})
                    continue
                calls += 1
                if calls > limit:
                    response, response_bytes = None, None
                    if has_id:
                        response = {'jsonrpc': '2.0', 'id': request['id'], 'result': {
                            'isError': True, 'content': [{'type': 'text', 'text': 'evaluation_tool_budget_exhausted'}]}}
                        response_bytes = emit(_encode(response, b''))
                    flags = {} if has_id else {'no_id': True}
                    with lock:
                        record({**base, 'response': response, 'budget_denied': True, 'forwarded': False,
                                'response_bytes': response_bytes, 'latency_ms': None, **flags})
                    continue
                if has_id:
                    call = {**base, 'sent': None, 'received': None}
                    with lock:
                        if child_gone.is_set():
                            abandon(call)
                            continue
                        pending[_id_key(request)] = call
                else:
                    with lock:
                        record({**base, 'response': None, 'forwarded': True, 'no_id': True,
                                'response_bytes': None, 'latency_ms': None})
            try:
                child.stdin.write(line)
                child.stdin.flush()
            except OSError:
                child_gone.set()
                if call is not None:
                    with lock:
                        if pending.pop(_id_key(request), None) is call:
                            abandon(call)
                continue
            if call is not None:
                sent = time.monotonic_ns()
                with lock:
                    call['sent'] = sent
                    if call['received'] is not None:
                        record_answered(call)
    finally:
        try:
            child.stdin.close()
        except OSError:
            pass
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
        reader.join(timeout=2)
        abandon_all()
        outgoing.put(None)
        writer.join()


if __name__ == '__main__':
    main()
