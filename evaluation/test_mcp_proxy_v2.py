import hashlib
import json
import subprocess
import sys
import tempfile
import textwrap
import threading
import time
import unittest
from pathlib import Path

import mcp_proxy_v2 as proxy

HERE = Path(__file__).resolve().parent

# Fake MCP server. Behaviour depends on the tool name of each tools/call:
#   slow     sleeps 100 ms before answering
#   die      exits without answering
#   surrogate answers with a lone surrogate escape (valid JSON, not encodable as UTF-8)
#   deep     first sends a line nested 200000 levels deep
#   compact  answers with separators=(',',':') and ASCII-escaped non-ASCII text
#   big      answers with a ~1 MB text
#   hang     never answers
#   roots    first sends a server->client request reusing the call id
#   notify   first sends a server notification (no id)
#   noise    first sends a non-JSON line, a JSON batch and a notification
# Any other name answers immediately with 'echo:<arguments.n>'.
# Requests without an id get no answer.
FAKE_SERVER = textwrap.dedent('''
    import json, sys, time
    STARTUP_DELAY = 0.0
    time.sleep(STARTUP_DELAY)

    def send(value, **options):
        sys.stdout.write(json.dumps(value, **options) + '\\n')
        sys.stdout.flush()

    for line in sys.stdin:
        try:
            request = json.loads(line)
        except ValueError:
            continue
        ident, method = request.get('id'), request.get('method')
        if method == 'tools/list':
            tools = [{'name': n} for n in ('context', 'propose', 'check', 'why')]
            send({'jsonrpc': '2.0', 'id': ident, 'result': {'tools': tools}})
            continue
        if method != 'tools/call' or 'id' not in request:
            continue
        name = request['params']['name']
        arguments = request['params'].get('arguments', {})
        text = 'echo:%s' % arguments.get('n')
        options = {}
        if name == 'hang':
            continue
        if name == 'die':
            sys.exit(0)
        if name == 'slow':
            time.sleep(0.1)
        if name == 'surrogate':
            sys.stdout.write('{"jsonrpc":"2.0","id":%s,"result":{"text":"lone \\\\ud800"}}\\n' % json.dumps(ident))
            sys.stdout.flush()
            continue
        if name == 'deep':
            sys.stdout.write('[' * 200000 + '\\n')
        if name == 'compact':
            text = 'r\\u00e9gla \\u2713 ' + 'x' * 40
            options = {'separators': (',', ':')}
        if name == 'big':
            text = 'x' * 1_000_000
        if name == 'roots':
            send({'jsonrpc': '2.0', 'id': ident, 'method': 'roots/list'})
        if name == 'notify':
            send({'jsonrpc': '2.0', 'method': 'notifications/message', 'params': {}})
        if name == 'noise':
            sys.stdout.write('not json at all\\n')
            send([{'jsonrpc': '2.0', 'method': 'notifications/batched'}])
            send({'jsonrpc': '2.0', 'method': 'notifications/message', 'params': {}})
        send({'jsonrpc': '2.0', 'id': ident, 'result': {'content': [{'type': 'text', 'text': text}]}}, **options)
''')


def rpc(identifier, method, params=None):
    return json.dumps({'jsonrpc': '2.0', 'id': identifier, 'method': method, 'params': params or {}})


def call(identifier, name, **arguments):
    return rpc(identifier, 'tools/call', {'name': name, 'arguments': arguments})


def parse(raw):
    try:
        return json.loads(raw)
    except (ValueError, RecursionError):
        return None


class ProxyV2Tests(unittest.TestCase):
    def start_proxy(self, limit=8, startup_delay=0.0):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        directory = Path(temporary.name)
        server = directory / 'fake_server.py'
        server.write_text(FAKE_SERVER.replace('STARTUP_DELAY = 0.0', 'STARTUP_DELAY = %r' % startup_delay),
                          encoding='utf-8')
        ledger = directory / 'tools.jsonl'
        process = subprocess.Popen(
            [sys.executable, str(HERE / 'mcp_proxy_v2.py'), str(ledger), str(limit), sys.executable, str(server)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.addCleanup(lambda: process.poll() is None and process.kill())
        return process, ledger, directory

    def finish(self, process, ledger, stdin=b''):
        stdout, stderr = process.communicate(stdin, timeout=30)
        self.assertEqual(process.returncode, 0, stderr.decode('utf-8', 'replace'))
        raw = stdout.split(b'\n')
        self.assertEqual(raw[-1], b'', 'every line written to the agent ends with a newline')
        raw = raw[:-1]
        entries = ([json.loads(line) for line in ledger.read_text(encoding='utf-8').splitlines()]
                   if ledger.exists() else [])
        return raw, entries

    def run_proxy(self, lines, limit=8):
        process, ledger, directory = self.start_proxy(limit)
        raw, entries = self.finish(process, ledger, ''.join(line + '\n' for line in lines).encode())
        return raw, [parse(line) for line in raw], entries, directory

    def run_proxy_until_answered(self, lines, ids, limit=8, startup_delay=0.0):
        """Like ``run_proxy``, but keep stdin open until the agent has every response in ``ids``.

        Closing stdin starts the proxy's shutdown, which gives the child 2 s before killing it
        and marking pending calls ``unanswered``. Waiting for the answers first keeps a slow
        child (Windows runners) from turning that grace period into a test failure.
        """
        process, ledger, directory = self.start_proxy(limit, startup_delay)
        raw, answered, pending = [], threading.Event(), set(ids)

        def pump():
            for line in process.stdout:
                raw.append(line)
                value = parse(line)
                ident = value.get('id') if isinstance(value, dict) and 'method' not in value else None
                if isinstance(ident, (int, str)):
                    pending.discard(ident)
                if not pending:
                    answered.set()

        reader = threading.Thread(target=pump, daemon=True)
        reader.start()
        process.stdin.write(''.join(line + '\n' for line in lines).encode())
        process.stdin.flush()
        got_all = answered.wait(30)
        process.stdin.close()
        reader.join(30)
        process.wait(timeout=30)
        self.assertTrue(got_all, 'missing responses for ids %r' % sorted(pending, key=repr))
        self.assertEqual(process.returncode, 0, process.stderr.read().decode('utf-8', 'replace'))
        for line in raw:
            self.assertTrue(line.endswith(b'\n'), 'every line written to the agent ends with a newline')
        raw = [line[:-1] for line in raw]
        entries = [json.loads(line) for line in ledger.read_text(encoding='utf-8').splitlines()]
        return raw, [parse(line) for line in raw], entries, directory

    def by_id(self, entries):
        return {entry['request'].get('id'): entry for entry in entries}

    def test_forwarded_call_records_latency_and_bytes(self):
        request = call(2, 'slow', n=1)
        raw, outputs, entries, _ = self.run_proxy([rpc(1, 'tools/list'), request])
        self.assertEqual([o['id'] for o in outputs], [1, 2])
        self.assertEqual(len(entries), 1)
        entry = entries[0]
        self.assertEqual(entry['schema'], proxy.LEDGER_SCHEMA)
        self.assertTrue(entry['forwarded'])
        self.assertNotIn('budget_denied', entry)
        self.assertEqual(entry['request'], json.loads(request))
        self.assertEqual(entry['response'], outputs[1])
        self.assertEqual(entry['request_bytes'], len(request.encode()))
        self.assertEqual(entry['response_bytes'], len(raw[1]))
        self.assertIsInstance(entry['latency_ms'], float)
        # 100 ms sleep; margin for coarse Windows clocks (~16 ms ticks).
        self.assertGreaterEqual(entry['latency_ms'], 80.0)

    def test_response_bytes_measure_the_line_the_agent_receives(self):
        # Defect A: the child writes compact, ASCII-escaped JSON; the agent gets a re-serialisation.
        raw, outputs, entries, _ = self.run_proxy([call(2, 'compact')])
        self.assertEqual(len(entries), 1)
        self.assertIn('régla ✓', outputs[0]['result']['content'][0]['text'])
        self.assertEqual(entries[0]['response_bytes'], len(raw[0]))

    def test_slow_agent_does_not_inflate_latency_of_next_call(self):
        # Defect B: a 1 MB response the agent does not read for 1 s must not delay the next measurement.
        process, ledger, _ = self.start_proxy()
        process.stdin.write((call(2, 'big') + '\n' + call(3, 'fast', n=3) + '\n').encode())
        process.stdin.flush()
        time.sleep(1.0)
        raw, entries = self.finish(process, ledger)
        self.assertEqual([parse(line)['id'] for line in raw], [2, 3])
        latency = self.by_id(entries)[3]['latency_ms']
        self.assertLess(latency, 500.0)

    def test_latency_starts_after_request_is_written(self):
        # Defect B: time blocked writing a large request to a child that is not reading yet is not latency.
        process, ledger, _ = self.start_proxy(startup_delay=1.0)
        raw, entries = self.finish(process, ledger, (call(2, 'fast', n=1, pad='x' * 1_000_000) + '\n').encode())
        self.assertEqual(len(entries), 1)
        self.assertLess(entries[0]['latency_ms'], 500.0)

    def test_unanswered_forwarded_call_is_ledgered_on_shutdown(self):
        # Defect F.
        raw, outputs, entries, _ = self.run_proxy([call(9, 'hang'), call(10, 'fast', n=10)])
        entry = self.by_id(entries)[9]
        self.assertTrue(entry['forwarded'])
        self.assertTrue(entry['unanswered'])
        self.assertIsNone(entry['response'])
        self.assertIsNone(entry['latency_ms'])
        self.assertIsNone(entry['response_bytes'])
        self.assertFalse(self.by_id(entries)[10].get('unanswered', False))

    def test_duplicate_in_flight_id_is_rejected_not_mismatched(self):
        # Defect C: the second request reuses id 5 while the first is still in flight.
        raw, outputs, entries, _ = self.run_proxy([call(5, 'slow', n=1), call(5, 'fast', n=2)])
        self.assertEqual(len(entries), 2)
        forwarded = [e for e in entries if e['forwarded']]
        rejected = [e for e in entries if not e['forwarded']]
        self.assertEqual(len(forwarded), 1)
        self.assertEqual(forwarded[0]['request']['params']['arguments'], {'n': 1})
        self.assertEqual(forwarded[0]['response']['result']['content'][0]['text'], 'echo:1')
        self.assertEqual(rejected[0]['rejected'], 'duplicate_id')
        self.assertEqual(rejected[0]['request']['params']['arguments'], {'n': 2})
        self.assertEqual(rejected[0]['response']['error']['code'], -32600)
        self.assertIn(rejected[0]['response'], outputs)
        self.assertEqual(rejected[0]['response_bytes'], len(raw[outputs.index(rejected[0]['response'])]))
        self.assertNotIn('echo:2', b'\n'.join(raw).decode())

    def test_call_without_id_is_not_paired_with_server_notification(self):
        # Defect D.
        no_id = json.dumps({'jsonrpc': '2.0', 'method': 'tools/call', 'params': {'name': 'check', 'arguments': {}}})
        raw, outputs, entries, _ = self.run_proxy([no_id, call(7, 'notify', n=7)])
        self.assertEqual(len(entries), 2)
        for entry in entries:
            self.assertNotIn('method', entry['response'] or {})
        orphan = [e for e in entries if 'id' not in e['request']][0]
        self.assertTrue(orphan['no_id'])
        self.assertIsNone(orphan['response'])
        self.assertIsNone(orphan['latency_ms'])
        self.assertEqual(self.by_id(entries)[7]['response']['result']['content'][0]['text'], 'echo:7')

    def test_server_request_with_same_id_is_not_taken_as_response(self):
        # Defect E.
        raw, outputs, entries, _ = self.run_proxy([call(1, 'roots', n=1)])
        self.assertEqual(len(entries), 1)
        self.assertEqual(entries[0]['response']['result']['content'][0]['text'], 'echo:1')
        self.assertIn({'jsonrpc': '2.0', 'id': 1, 'method': 'roots/list'}, outputs)

    def test_non_json_and_batch_lines_do_not_kill_the_bridge(self):
        # Defect G, child side and agent side.
        raw, outputs, entries, _ = self.run_proxy(['garbage from agent', call(7, 'noise', n=7), call(8, 'fast', n=8)])
        self.assertIn(b'not json at all', raw)
        self.assertIn([{'jsonrpc': '2.0', 'method': 'notifications/batched'}], outputs)
        answered = self.by_id(entries)
        self.assertEqual(answered[7]['response']['result']['content'][0]['text'], 'echo:7')
        self.assertEqual(answered[8]['response']['result']['content'][0]['text'], 'echo:8')

    def test_child_exit_with_call_in_flight_answers_the_agent(self):
        # The agent must not wait forever when the child dies with a call pending.
        process, ledger, _ = self.start_proxy()
        process.stdin.write((call(4, 'die') + '\n').encode())
        process.stdin.flush()
        lines = []
        reader = threading.Thread(target=lambda: lines.append(process.stdout.readline()), daemon=True)
        reader.start()
        reader.join(10)
        process.stdin.close()
        self.assertTrue(lines and lines[0], 'no answer while the agent keeps stdin open')
        answer = json.loads(lines[0])
        self.assertEqual(answer['id'], 4)
        self.assertEqual(answer['error']['code'], proxy.INTERNAL_ERROR)
        process.wait(timeout=30)
        entries = [json.loads(line) for line in ledger.read_text(encoding='utf-8').splitlines()]
        self.assertEqual(len(entries), 1)
        self.assertTrue(entries[0]['unanswered'])
        self.assertIsNone(entries[0]['response'])

    def test_unencodable_or_deep_lines_do_not_kill_the_bridge(self):
        lone = call(3, 'fast', n=3, s='\ud800')
        raw, outputs, entries, _ = self.run_proxy_until_answered(
            [call(1, 'surrogate'), call(2, 'deep', n=2), lone], ids={1, 2, 3})
        answered = self.by_id(entries)
        self.assertEqual(set(answered), {1, 2, 3})
        for entry in entries:
            self.assertFalse(entry.get('unanswered', False), entry)
        self.assertEqual(answered[1]['response']['result']['text'], 'lone \ud800')
        self.assertIn(answered[1]['response_bytes'], [len(line) for line in raw])
        self.assertEqual(answered[2]['response']['result']['content'][0]['text'], 'echo:2')
        self.assertEqual(answered[3]['request']['params']['arguments']['s'], '\ud800')

    def test_budget_denied_call_has_bytes_and_null_latency(self):
        calls = [call(i, 'check') for i in (2, 3)]
        raw, outputs, entries, _ = self.run_proxy([rpc(1, 'tools/list')] + calls, limit=1)
        self.assertEqual(len(entries), 2)
        # Ledger order is completion order (as in v1); index by request id.
        by_id = self.by_id(entries)
        self.assertTrue(by_id[2]['forwarded'])
        denied = by_id[3]
        self.assertTrue(denied['budget_denied'])
        self.assertFalse(denied['forwarded'])
        self.assertIsNone(denied['latency_ms'])
        self.assertEqual(denied['request_bytes'], len(calls[1].encode()))
        self.assertEqual(denied['response_bytes'], len(raw[outputs.index(denied['response'])]))
        self.assertEqual(denied['response']['result']['content'][0]['text'], 'evaluation_tool_budget_exhausted')

    def test_non_call_messages_are_not_ledgered_but_ready_marker_is_written(self):
        raw, outputs, entries, directory = self.run_proxy([rpc(1, 'tools/list')])
        self.assertEqual(len(outputs), 1)
        self.assertEqual(entries, [])
        ready = json.loads((directory / 'tools.jsonl.ready').read_text(encoding='utf-8'))
        self.assertEqual(ready, {'tools': ['check', 'context', 'propose', 'why']})

    def test_default_child_matches_v1_sandbox_paths(self):
        self.assertEqual(proxy.DEFAULT_CHILD, [
            '/tools/universo-mcp', '--root', '/work/bundle',
            '--manifiesto', '/work/manifest.toml', '--invariantes', '/work/invariants.sx'])

    def test_pilot_173_sources_untouched(self):
        plan = json.loads((HERE / 'results' / 'pilot-173' / 'plan.json').read_text(encoding='utf-8'))
        for name, expected in plan['source_sha256'].items():
            self.assertEqual(hashlib.sha256((HERE / name).read_bytes()).hexdigest(), expected, name)


if __name__ == '__main__':
    unittest.main()
