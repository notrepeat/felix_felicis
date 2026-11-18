import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import run_paired as runner


class RunnerTests(unittest.TestCase):
    def test_schedule_balanced_and_reproducible(self):
        cases = json.loads((runner.HERE / 'tasks.json').read_text(encoding='utf-8'))['cases']
        selected = runner.select_cases(cases, 173)
        self.assertEqual(len(selected), 6)
        rows = runner.schedule(selected, 173)
        self.assertEqual(rows, runner.schedule(selected, 173))
        self.assertEqual(len(rows), 24)
        for case in selected:
            self.assertEqual(sum(c['id'] == case['id'] for c, _, _ in rows), 4)
            first_arms = [rows[i][2] for i in range(0, len(rows), 2) if rows[i][0]['id'] == case['id']]
            self.assertEqual(set(first_arms), {'with_felix', 'without_felix'})
        for index in range(0, len(rows), 2):
            self.assertEqual(rows[index][:2], rows[index + 1][:2])
            self.assertNotEqual(rows[index][2], rows[index + 1][2])

    def test_same_prompt_and_no_oracle_materialized(self):
        case = json.loads((runner.HERE / 'tasks.json').read_text(encoding='utf-8'))['cases'][0]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runner.materialize(case, root)
            self.assertEqual(set(p.name for p in root.iterdir()), {'bundle', 'manifest.toml', 'invariants.sx', 'schema.json'})
            self.assertNotIn('oracle', runner.prompt(case))

    def test_mcp_readiness_requires_exact_tool_catalog(self):
        with tempfile.TemporaryDirectory() as directory:
            logs = Path(directory)
            self.assertFalse(runner.mcp_ready(logs))
            marker = logs / 'tools.jsonl.ready'
            marker.write_text(json.dumps({'tools': ['check', 'context', 'propose', 'why']}))
            self.assertTrue(runner.mcp_ready(logs))
            marker.write_text(json.dumps({'tools': ['check', 'context', 'propose', 'apply']}))
            self.assertFalse(runner.mcp_ready(logs))

    # run_paired.py is frozen by pilot 173 and kills the process group with os.killpg,
    # which exists only on POSIX; the paired runner is Linux-only (Docker sandbox).
    @unittest.skipUnless(hasattr(os, 'killpg'), 'run_paired.bounded_capture needs os.killpg (POSIX only)')
    def test_output_flood_killed_reaped_and_bounded(self):
        child = subprocess.Popen([sys.executable, '-c', 'import os\nwhile True: os.write(1,b"x"*65536)'],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        stdout, _, timeout, overflow = runner.bounded_capture(child, b'', 3, limit=4096)
        self.assertTrue(overflow)
        self.assertFalse(timeout)
        self.assertLessEqual(len(stdout), 4096)
        self.assertIsNotNone(child.poll())

    @unittest.skipUnless(hasattr(os, 'killpg'), 'run_paired.bounded_capture needs os.killpg (POSIX only)')
    def test_timeout_includes_blocked_input_and_reaps(self):
        child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        _, _, timeout, overflow = runner.bounded_capture(child, b'x' * 1_000_000, .1)
        self.assertTrue(timeout)
        self.assertFalse(overflow)
        self.assertIsNotNone(child.poll())


if __name__ == '__main__':
    unittest.main()
