"""Code examples must not become links; real missing prose links still fail."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import check_library_docs as check


class LinkChecks(unittest.TestCase):
    def test_code_examples_are_ignored_and_real_links_checked(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "docs").mkdir()
            (root / "README.md").write_text(
                "```text\nFormula [0,7](C(r))\n```\n"
                "~~~json\n[not-a-link](missing.json)\n~~~\n"
                "Inline `◇[0,7](C(r))` and ``[literal](also-missing.md)``.\n"
                "[real link](docs/present.md)\n")
            (root / "docs/present.md").write_text("Present.\n")
            with patch.object(check, "ROOT", root):
                failures = []
                check.check_markdown_links(failures)
                self.assertEqual(failures, [])
                with (root / "README.md").open("a") as stream:
                    stream.write("[real missing link](docs/absent.md)\n")
                check.check_markdown_links(failures)
                self.assertEqual(len(failures), 1)
                self.assertIn("docs/absent.md", failures[0])


if __name__ == "__main__":
    unittest.main()
