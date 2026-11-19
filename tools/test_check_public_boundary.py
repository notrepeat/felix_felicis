import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_public_boundary as guard


class TextFilesTests(unittest.TestCase):
    def tree(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for relative in ['docs/a.md', 'graphify-out/graph.json', 'graphify-out/cache/x.json',
                         'target/debug/out.txt', 'docs/graphify-out.md', 'nested/graphify-out/y.json']:
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('x', encoding='utf-8')
        return root

    def scanned(self, root):
        with mock.patch.object(guard, 'ROOT', root):
            return {path.relative_to(root).as_posix() for path in guard.text_files()}

    def test_skips_local_graphify_output_only_at_repository_root(self):
        scanned = self.scanned(self.tree())
        self.assertNotIn('graphify-out/graph.json', scanned)
        self.assertNotIn('graphify-out/cache/x.json', scanned)
        self.assertIn('docs/graphify-out.md', scanned)
        self.assertIn('nested/graphify-out/y.json', scanned)

    def test_still_scans_regular_files_and_skips_target(self):
        scanned = self.scanned(self.tree())
        self.assertIn('docs/a.md', scanned)
        self.assertNotIn('target/debug/out.txt', scanned)


if __name__ == '__main__':
    unittest.main()
