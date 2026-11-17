import hashlib
import json
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

import mcp_proxy_v2 as proxy

HERE = Path(__file__).resolve().parent

FAKE_SERVER = textwrap.dedent('''
    import json, sys, time
    for line in sys.stdin:
        request = json.loads(line)
        if request.get('method') == 'tools/list':
            tools = [{'name': n} for n in ('context', 'propose', 'check', 'why')]
            result = {'tools': tools}
        else:
            time.sleep(0.05)
            result = {'content': [{'type': 'text', 'text': 'admit-' + 'x' * 40}]}
        sys.stdout.write(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}) + '\\n')
        sys.stdout.flush()
''')


def rpc(identifier, method, params=None):
    return json.dumps({'jsonrpc': '2.0', 'id': identifier, 'method': method, 'params': params or {}})


class ProxyV2Tests(unittest.TestCase):
    def run_proxy(self, lines, limit):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        directory = Path(temporary.name)
        server = directory / 'fake_server.py'
        server.write_text(FAKE_SERVER)
        ledger = directory / 'tools.jsonl'
        completed = subprocess.run(
            [sys.executable, str(HERE / 'mcp_proxy_v2.py'), str(ledger), str(limit), sys.executable, str(server)],
            input=''.join(line + '\n' for line in lines).encode(), capture_output=True, timeout=30, check=True)
        outputs = [json.loads(line) for line in completed.stdout.decode().splitlines()]
        entries = [json.loads(line) for line in ledger.read_text().splitlines()] if ledger.exists() else []
        return outputs, entries, directory

    def test_forwarded_call_records_latency_and_bytes(self):
        request = rpc(2, 'tools/call', {'name': 'check', 'arguments': {}})
        outputs, entries, _ = self.run_proxy([rpc(1, 'tools/list'), request], limit=8)
        self.assertEqual([o['id'] for o in outputs], [1, 2])
        self.assertEqual(len(entries), 1)
        entry = entries[0]
        self.assertEqual(entry['schema'], proxy.LEDGER_SCHEMA)
        self.assertTrue(entry['forwarded'])
        self.assertNotIn('budget_denied', entry)
        self.assertEqual(entry['request'], json.loads(request))
        self.assertEqual(entry['response'], outputs[1])
        self.assertEqual(entry['request_bytes'], len(request.encode()))
        self.assertEqual(entry['response_bytes'], len(json.dumps(outputs[1]).encode()))
        self.assertIsInstance(entry['latency_ms'], float)
        self.assertGreaterEqual(entry['latency_ms'], 50.0)

    def test_budget_denied_call_has_bytes_and_null_latency(self):
        calls = [rpc(i, 'tools/call', {'name': 'check', 'arguments': {}}) for i in (2, 3)]
        outputs, entries, _ = self.run_proxy([rpc(1, 'tools/list')] + calls, limit=1)
        self.assertEqual(len(entries), 2)
        # Ledger order is completion order (as in v1); index by request id.
        by_id = {entry['request']['id']: entry for entry in entries}
        self.assertTrue(by_id[2]['forwarded'])
        denied = by_id[3]
        self.assertTrue(denied['budget_denied'])
        self.assertFalse(denied['forwarded'])
        self.assertIsNone(denied['latency_ms'])
        self.assertEqual(denied['request_bytes'], len(calls[1].encode()))
        self.assertEqual(denied['response_bytes'], len(json.dumps(denied['response'], ensure_ascii=False).encode()))
        self.assertEqual(denied['response']['result']['content'][0]['text'], 'evaluation_tool_budget_exhausted')
        self.assertIn(denied['response'], outputs)

    def test_non_call_messages_are_not_ledgered_but_ready_marker_is_written(self):
        outputs, entries, directory = self.run_proxy([rpc(1, 'tools/list')], limit=8)
        self.assertEqual(len(outputs), 1)
        self.assertEqual(entries, [])
        ready = json.loads((directory / 'tools.jsonl.ready').read_text())
        self.assertEqual(ready, {'tools': ['check', 'context', 'propose', 'why']})

    def test_default_child_matches_v1_sandbox_paths(self):
        self.assertEqual(proxy.DEFAULT_CHILD, [
            '/tools/universo-mcp', '--root', '/work/bundle',
            '--manifiesto', '/work/manifest.toml', '--invariantes', '/work/invariants.sx'])

    def test_pilot_173_sources_untouched(self):
        plan = json.loads((HERE / 'results' / 'pilot-173' / 'plan.json').read_text())
        for name, expected in plan['source_sha256'].items():
            self.assertEqual(hashlib.sha256((HERE / name).read_bytes()).hexdigest(), expected, name)


if __name__ == '__main__':
    unittest.main()
