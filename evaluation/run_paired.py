#!/usr/bin/env python3
"""Real, isolated Codex/ChatGPT paired pilot. Never substitutes scripted answers."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import signal
import subprocess
import tempfile
import time
import threading

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
DISABLED = ['shell_tool', 'apps', 'browser_use', 'multi_agent', 'multi_agent_v2',
            'memories', 'plugins', 'remote_plugin', 'skill_search', 'view_image', 'image_generation']
SCHEMA = {'type': 'object', 'additionalProperties': False, 'required': ['schema', 'add', 'remove'],
          'properties': {'schema': {'type': 'string', 'enum': ['felix-delta-v1']},
                         'add': {'type': 'array', 'items': {'type': 'string'}},
                         'remove': {'type': 'array', 'items': {'type': 'string'}}}}
INSTRUCTION = ('Solve the synthetic graph-edit task. Return only the required felix-delta-v1 JSON delta. '
               'Use only supplied facts and rules. Do not inspect files, use network, or execute commands. '
               'If verification tools are available, you may use them to check and repair your proposal. '
               'You have at most 8 verification tool calls. Minimize unrelated edits.')


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False).encode()).hexdigest()


def select_cases(cases, seed):
    groups = {}
    for case in sorted(cases, key=lambda c: c['id']):
        groups.setdefault(case['family'], []).append(case)
    rng = random.Random(seed)
    return [rng.choice(groups[family]) for family in sorted(groups)]


def schedule(cases, seed, repetitions=2):
    rng = random.Random(seed)
    rows = []
    first_order = {case['id']: rng.randrange(2) for case in cases}
    for repetition in range(repetitions):
        shuffled = list(cases)
        rng.shuffle(shuffled)
        for case in shuffled:
            arms = ['without_felix', 'with_felix']
            if (first_order[case['id']] + repetition) % 2:
                arms.reverse()
            rows.extend((case, repetition, arm) for arm in arms)
    return rows


def command(codex_dir, binary, work, logs, arm, model):
    home = Path.home()
    auth = home / '.codex/auth.json'
    if not auth.is_file():
        raise RuntimeError('existing_codex_file_login_required')
    cmd = ['bwrap', '--unshare-all', '--share-net', '--die-with-parent', '--new-session', '--clearenv']
    for path in ['/usr', '/bin', '/lib', '/lib64', '/etc/ssl', '/etc/ca-certificates',
                 '/etc/resolv.conf', '/etc/hosts', '/etc/nsswitch.conf']:
        if Path(path).exists():
            cmd += ['--ro-bind', path, path]
    cmd += ['--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp',
            '--dir', str(home / '.codex'), '--ro-bind', str(auth), str(auth),
            '--ro-bind', str(codex_dir), '/opt/codex', '--ro-bind', str(binary), '/tools/universo-mcp',
            '--ro-bind', str(HERE / 'mcp_proxy.py'), '/tools/mcp_proxy.py',
            '--ro-bind', str(work), '/work', '--bind', str(logs), '/logs', '--chdir', '/work',
            '--setenv', 'HOME', str(home), '--setenv', 'PATH', '/usr/bin:/bin', '--setenv', 'LANG', 'C.UTF-8',
            '/opt/codex/bin/codex', 'exec', '--ephemeral', '--ignore-user-config',
            '--skip-git-repo-check', '--sandbox', 'read-only', '--model', model,
            '-c', 'model_reasoning_effort="low"', '-c', 'web_search="disabled"',
            '-c', 'project_doc_max_bytes=0', '--enable', 'skip_host_skill_discovery',
            '--output-schema', '/work/schema.json', '--json']
    for feature in DISABLED:
        cmd += ['--disable', feature]
    if arm == 'with_felix':
        cmd += ['-c', 'mcp_servers.felix.command="/usr/bin/python3"', '-c',
                'mcp_servers.felix.args=["/tools/mcp_proxy.py","/logs/tools.jsonl","8"]',
                '-c', 'mcp_servers.felix.startup_timeout_sec=10',
                '-c', 'mcp_servers.felix.tool_timeout_sec=10']
    cmd += ['-']
    return cmd


def materialize(case, root):
    (root / 'bundle').mkdir()
    for relative, content in case['files'].items():
        path = Path(relative)
        if path.is_absolute() or '..' in path.parts:
            raise ValueError('unsafe_fixture_path')
        target = root / 'bundle' / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)
    (root / 'manifest.toml').write_text(case['manifest'])
    (root / 'invariants.sx').write_text(case['invariants'])
    (root / 'schema.json').write_text(json.dumps(SCHEMA))


def prompt(case):
    return INSTRUCTION + '\n' + json.dumps({k: case[k] for k in ['prompt', 'facts', 'rules', 'invariants']}, ensure_ascii=False)


def parse_events(output):
    events = []
    for line in output.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        events.append(event)
    return events


def bounded_capture(process, input_bytes, timeout, limit=8 * 1024 * 1024):
    buffers = [bytearray(), bytearray()]
    overflow = threading.Event()
    def kill():
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    def reader(stream, target):
        while True:
            chunk = stream.read(65536)
            if not chunk:
                break
            if len(target) + len(chunk) > limit:
                overflow.set(); kill(); break
            target.extend(chunk)
    threads = [threading.Thread(target=reader, args=(stream, target), daemon=True)
               for stream, target in zip([process.stdout, process.stderr], buffers)]
    def write_input():
        try:
            process.stdin.write(input_bytes)
        except (BrokenPipeError, OSError):
            pass
        finally:
            process.stdin.close()
    threads.append(threading.Thread(target=write_input, daemon=True))
    for thread in threads:
        thread.start()
    timed_out = False
    try:
        process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True; kill(); process.wait()
    finally:
        for thread in threads:
            thread.join(timeout=2)
        if any(thread.is_alive() for thread in threads):
            kill()
            for thread in threads:
                thread.join(timeout=2)
        process.stdout.close(); process.stderr.close()
    return bytes(buffers[0]), bytes(buffers[1]), timed_out, overflow.is_set()


def mcp_ready(logs):
    try:
        value = json.loads((logs / 'tools.jsonl.ready').read_text())
        return value == {'tools': ['check', 'context', 'propose', 'why']}
    except (OSError, ValueError):
        return False


def execute(case, repetition, arm, args):
    from scoring import score
    with tempfile.TemporaryDirectory(prefix='felix-paired-') as temporary:
        root = Path(temporary)
        work, logs = root / 'work', root / 'logs'
        work.mkdir(); logs.mkdir()
        materialize(case, work)
        text = prompt(case)
        cmd = command(Path(args.codex_dir).resolve(), Path(args.binary).resolve(), work, logs, arm, args.model)
        start = time.monotonic()
        process = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True, env={'PATH': '/usr/bin:/bin'})
        stdout, stderr, timed_out, overflow = bounded_capture(process, text.encode(), args.timeout)
        elapsed = time.monotonic() - start
        events = parse_events(stdout.decode(errors='replace'))
        messages = [e['item']['text'] for e in events if e.get('type') == 'item.completed'
                    and e.get('item', {}).get('type') == 'agent_message']
        delta = None
        if messages:
            try:
                delta = json.loads(messages[-1])
            except ValueError:
                pass
        usage = [e['usage'] for e in events if e.get('type') == 'turn.completed' and 'usage' in e]
        tool_path = logs / 'tools.jsonl'
        tools = [json.loads(line) for line in tool_path.read_text().splitlines()] if tool_path.exists() else []
        # Publish only synthetic model answers/tool data, never stderr, auth, or provider session IDs.
        items = [e.get('item', {}) for e in events if e.get('type') in ['item.completed', 'item.started']]
        tool_types = sorted({item.get('type') for item in items if item.get('type') not in ['agent_message', 'reasoning', 'error']})
        unexpected_tools = [item for item in items if item.get('type') not in ['agent_message', 'reasoning', 'error']
                            and not (item.get('type') == 'mcp_tool_call' and arm == 'with_felix'
                                     and item.get('server') == 'felix' and item.get('tool') in ['context', 'propose', 'check', 'why'])]
        scored = score(case['id'], delta)
        status = 'timeout' if timed_out else 'completed' if process.returncode == 0 and delta is not None else 'execution_failed'
        if arm == 'with_felix' and not mcp_ready(logs):
            status = 'mcp_unavailable'
        if overflow:
            status = 'output_limit'
        if unexpected_tools:
            status = 'unexpected_tool'
        if status != 'completed':
            scored['success'] = False
        return {'case_id': case['id'], 'family': case['family'], 'repetition': repetition + 1, 'arm': arm,
                'status': status, 'mcp_ready': mcp_ready(logs) if arm == 'with_felix' else None, 'returncode': process.returncode, 'elapsed_seconds': elapsed,
                'prompt': text, 'prompt_sha256': hashlib.sha256(text.encode()).hexdigest(),
                'configured_model_alias': args.model, 'provider_model_snapshot': None,
                'reasoning_effort': 'low', 'sampling_seed': None, 'schedule_seed': args.seed,
                'usage': usage, 'cost_usd': None, 'cost_basis': 'existing_chatgpt_subscription_no_billing_observation',
                'final_delta': delta, 'score': scored, 'tool_calls': tools, 'observed_tool_item_types': tool_types,
                'stderr_bytes': len(stderr), 'stdout_bytes': len(stdout),
                'final_invalid': not (scored['format_valid'] and scored['structural_valid'] and scored['constraints_valid']),
                'felix_rejected_proposals': sum(t['response'].get('result', {}).get('structuredContent', {}).get('status') == 'reject'
                                         for t in tools if t['request'].get('params', {}).get('name') == 'propose')}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--model', default='gpt-6-astra')
    parser.add_argument('--codex-dir', required=True)
    parser.add_argument('--binary', default=str(REPO / 'target/debug/universo-mcp'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seed', type=int, default=173)
    parser.add_argument('--timeout', type=int, default=120)
    parser.add_argument('--pilot', action='store_true', help='only first two paired cases (four runs), included in final schedule')
    parser.add_argument('--resume', action='store_true')
    parser.add_argument('--plan-only', action='store_true')
    args = parser.parse_args()
    cases = json.loads((HERE / 'tasks.json').read_text())['cases']
    selected = select_cases(cases, args.seed)
    rows = schedule(selected, args.seed)
    plan = {'schema': 'felix-real-agent-pilot-v1', 'configured_model_alias': args.model, 'provider_model_snapshot': None,
            'schedule_seed': args.seed, 'sampling_seed': None, 'cases': [c['id'] for c in selected],
            'runs': len(rows), 'timeout_seconds_per_run': args.timeout, 'mcp_tool_call_budget': 8,
            'dataset_sha256': digest(cases),
            'cli_version': subprocess.check_output([str(Path(args.codex_dir) / 'bin/codex'), '--version'], text=True, stderr=subprocess.DEVNULL).strip(),
            'source_sha256': {name: hashlib.sha256((HERE / name).read_bytes()).hexdigest() for name in
                              ['run_paired.py', 'mcp_proxy.py', 'scoring.py', 'tasks.json', 'oracle.json']},
            'disabled_features': DISABLED, 'isolation': 'bubblewrap_readonly_fixtures_no_repository_oracle_or_host_home',
            'output_byte_limit_per_stream': 8 * 1024 * 1024, 'schema_sha256': digest(SCHEMA), 'instruction': INSTRUCTION,
            'schedule': [{'case': c['id'], 'repetition': r + 1, 'arm': a} for c, r, a in rows]}
    args.output.mkdir(parents=True, exist_ok=True)
    plan_path = args.output / 'plan.json'
    if plan_path.exists() and json.loads(plan_path.read_text()) != plan:
        raise RuntimeError('plan_changed_refusing_resume')
    plan_path.write_text(json.dumps(plan, ensure_ascii=False, indent=2) + '\n')
    if args.plan_only:
        print(json.dumps({'planned_runs': len(rows), 'cases': plan['cases']})); return
    if len(rows) > 24 or not 1 <= args.timeout <= 120:
        raise RuntimeError('pilot_budget_exceeded')
    results_path = args.output / 'results.jsonl'
    previous = [json.loads(line) for line in results_path.read_text().splitlines()] if results_path.exists() else []
    if previous and not args.resume:
        raise RuntimeError('results_exist_use_resume')
    done = {(r['case_id'], r['repetition'], r['arm']) for r in previous}
    for case, repetition, arm in rows[:4] if args.pilot else rows:
        if (case['id'], repetition + 1, arm) in done:
            continue
        result = execute(case, repetition, arm, args)
        with results_path.open('a') as output:
            output.write(json.dumps(result, ensure_ascii=False) + '\n')
        print(json.dumps({k: result[k] for k in ['case_id', 'arm', 'status', 'elapsed_seconds']}), flush=True)
        if result['status'] != 'completed':
            raise RuntimeError('pilot_execution_failure_preserved_stop_for_diagnosis')


if __name__ == '__main__':
    main()
