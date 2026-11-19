import json
import unittest
from export_public import export


class ExportTests(unittest.TestCase):
    def test_nested_metadata_and_duplicate_text_removed_semantics_retained(self):
        source = {'case_id': 'synthetic', 'private_host': '/private/canary', 'score': {'success': True}, 'tool_calls': [{
            'request': {'id': 'PRIVATE_ID', 'params': {'name': 'check', '_meta': {'session_id': 'PRIVATE_SESSION'},
                        'arguments': {'schema': 'felix-mcp-v1', '_meta': {'token': 'PRIVATE_TOKEN'}}}},
            'response': {'id': 'PRIVATE_ID', 'result': {'_meta': {'turn_id': 'PRIVATE_TURN'},
                'structuredContent': {'schema': 'felix-mcp-v1', 'status': 'admit', 'summary': {'conflicts': 0, '_meta': {'windowId': 'PRIVATE_WINDOW'}}},
                'content': [{'type': 'text', 'text': 'PRIVATE_DUPLICATE'}]}}}]}
        result = export(source)
        self.assertNotIn('PRIVATE', json.dumps(result))
        self.assertNotIn('/private/', json.dumps(result))
        self.assertEqual(result['score'], source['score'])
        self.assertEqual(result['tool_calls'][0]['response']['result']['structuredContent']['status'], 'admit')

    def test_budget_denial_kept_without_raw_error(self):
        result = export({'tool_calls': [{'budget_denied': True, 'request': {'params': {'name': 'check', 'arguments': {}}},
                                       'response': {'result': {'isError': True, 'content': [{'text': 'PRIVATE'}]}}}]})
        self.assertTrue(result['tool_calls'][0]['budget_denied'])
        self.assertIn('evaluation_tool_budget_exhausted', json.dumps(result))
        self.assertNotIn('PRIVATE', json.dumps(result))


if __name__ == '__main__':
    unittest.main()
