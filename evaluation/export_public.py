#!/usr/bin/env python3
"""Export synthetic evaluation evidence without transport IDs or host metadata.

Run after capture; never feed this transformation back into the experiment.
"""
import argparse
import json
from pathlib import Path

ROW_FIELDS = set('case_id family repetition arm status mcp_ready returncode elapsed_seconds prompt prompt_sha256 configured_model_alias provider_model_snapshot reasoning_effort sampling_seed schedule_seed usage cost_usd cost_basis final_delta score tool_calls observed_tool_item_types stderr_bytes stdout_bytes final_invalid felix_rejected_proposals'.split())
SMOKE_FIELDS = set('schema excluded_from_paired_results mcp_ready returncode timeout overflow usage item_types tool_calls stderr_bytes configured_model_alias provider_model_snapshot cli_version preregistered_commit prompt'.split())
DOMAIN_FIELDS = set('schema tool status summary diagnostics provenance persisted code invariant_selection invariants values_included atoms nodes'.split())


def clean_nested(value):
    if isinstance(value, list):
        return [clean_nested(item) for item in value]
    if isinstance(value, dict):
        return {key: clean_nested(item) for key, item in value.items()
                if key.lower() not in {'_meta', 'thread_id', 'threadid', 'session_id', 'sessionid', 'turn_id', 'turnid', 'callid', 'windowid', 'window_id'}}
    return value


def tool_call(value):
    params = value.get('request', {}).get('params', {})
    name = params.get('name')
    arguments = params.get('arguments', {})
    keys = {'schema', 'node', 'depth', 'include_values'} if name == 'context' else {'schema', 'delta'}
    safe_args = clean_nested({key: item for key, item in arguments.items() if key in keys})
    if isinstance(safe_args.get('delta'), dict):
        safe_args['delta'] = {key: item for key, item in safe_args['delta'].items() if key in {'schema', 'add', 'remove'}}
    response = value.get('response', {})
    if 'result' in response:
        result = response['result']
        safe_result = {'isError': bool(result.get('isError', False))}
        if isinstance(result.get('structuredContent'), dict):
            safe_result['structuredContent'] = clean_nested({key: item for key, item in result['structuredContent'].items() if key in DOMAIN_FIELDS})
        else:
            safe_result['content'] = [{'type': 'text', 'text': 'evaluation_tool_budget_exhausted' if value.get('budget_denied') else 'tool_result_redacted'}]
        safe_response = {'result': safe_result}
    else:
        safe_response = {'error': {'code': response.get('error', {}).get('code'), 'message': 'mcp_protocol_error'}}
    exported = {'request': {'method': 'tools/call', 'params': {'name': name, 'arguments': safe_args}}, 'response': safe_response}
    if value.get('budget_denied'):
        exported['budget_denied'] = True
    return exported


def export(value, smoke=False):
    allowed = SMOKE_FIELDS if smoke else ROW_FIELDS
    result = {key: item for key, item in value.items() if key in allowed}
    result['tool_calls'] = [tool_call(call) for call in value.get('tool_calls', [])]
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('source', type=Path)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--smoke', action='store_true')
    args = parser.parse_args()
    if args.source.resolve() == args.destination.resolve():
        raise ValueError('capture_and_public_export_must_be_separate')
    text = args.source.read_text()
    if args.smoke:
        output = json.dumps(export(json.loads(text), True), ensure_ascii=False, indent=2) + '\n'
    else:
        output = ''.join(json.dumps(export(json.loads(line)), ensure_ascii=False) + '\n' for line in text.splitlines())
    args.destination.parent.mkdir(parents=True, exist_ok=True)
    args.destination.write_text(output)


if __name__ == '__main__':
    main()
