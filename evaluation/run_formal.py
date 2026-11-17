#!/usr/bin/env python3
"""Run checked-in labelled corpus and functional-conflict cases through core."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    run = subprocess.run(['cargo', 'test', '--locked', '--offline', '-p', 'universo-core', '--test', 'formal_corpus', '--', '--nocapture'], cwd=ROOT, check=True, capture_output=True, text=True)
    match = re.search(r'FORMAL_CORPUS cases=(\d+) admit=(\d+) reject=(\d+) technical_error=(\d+) false_admit=(\d+) ast_variants=(\d+)', run.stdout)
    if not match:
        raise RuntimeError('formal_corpus_summary_missing')
    result = dict(zip(['cases', 'admit', 'reject', 'technical_error', 'false_admit', 'ast_variants'], map(int, match.groups())))
    result.update(schema='felix-formal-corpus-v1', functional_conflict_cases=3, functional_conflict_classes=['Type', 'HasLab', 'Parent'], expected_labels='hand-authored synthetic fixture; exact match, not learned from evaluator', source_sha256={p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in ['evaluation/formal_corpus.tsv', 'crates/universo-core/tests/formal_corpus.rs', 'crates/universo-core/tests/support/formal_fixture.rs']})
    encoded = json.dumps(result, indent=2) + '\n'
    if args.output:
        args.output.write_text(encoded, encoding='utf-8')
    else:
        print(encoded, end='')

if __name__ == '__main__':
    main()
