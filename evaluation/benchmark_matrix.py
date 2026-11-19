#!/usr/bin/env python3
"""Repeat the declared core matrix; no LLM/provider costs are estimated here."""
import argparse
import json
import hashlib
import os
import platform
from pathlib import Path
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repetitions', type=int, default=5)
    p.add_argument('--output', type=Path)
    args = p.parse_args()
    if not 1 <= args.repetitions <= 30:
        p.error('repetitions must be between 1 and 30')
    command = ['cargo', 'run', '--release', '--locked', '--offline', '-p', 'universo-core', '--example', 'benchmark_matrix', '--', str(args.repetitions)]
    raw = subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True, timeout=300)
    data = json.loads(raw.stdout)
    data['environment'] = {'os': platform.system(), 'machine': platform.machine(), 'python': platform.python_version(), 'rustc': subprocess.check_output(['rustc', '--version'], cwd=ROOT, text=True).strip(), 'profile': 'release', 'logical_cpus': os.cpu_count(), 'git_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()}
    data['source_sha256'] = {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in ['crates/universo-core/examples/benchmark_matrix.rs', 'evaluation/benchmark_matrix.py']}
    data['outcome_counts'] = {status: sum(row['status'] == status for row in data['samples']) for status in sorted({row['status'] for row in data['samples']})}
    groups = {}
    for row in data['samples']:
        key = tuple(row[k] for k in ['nodes', 'edges', 'invariants', 'variables', 'delta_atoms'])
        groups.setdefault(key, []).append(row['elapsed_us'])
    assert len(groups) == 14 and all(len(v) == args.repetitions for v in groups.values())
    data['summary'] = [dict(zip(['nodes', 'edges', 'invariants', 'variables', 'delta_atoms'], k), median_us=statistics.median(v), min_us=min(v), max_us=max(v), samples=len(v)) for k, v in groups.items()]
    for point in data['summary']:
        rows = [row for row in data['samples'] if all(row[k] == point[k] for k in ['nodes', 'edges', 'invariants', 'variables', 'delta_atoms'])]
        point['outcomes'] = {status: sum(row['status'] == status for row in rows) for status in sorted({row['status'] for row in rows})}
    data['limits'] = 'Single host; one-factor-at-a-time, warm core only; excludes file I/O, process startup, model inference and billing. No RSS or SLA claim.'
    output = json.dumps(data, indent=2) + '\n'
    if args.output:
        args.output.write_text(output, encoding='utf-8')
    else:
        print(output, end='')

if __name__ == '__main__':
    main()
