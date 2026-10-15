"""Negative controls for accidental publication of internal documents."""

from pathlib import Path
import subprocess
import tempfile
import unittest

from check_public_docs import check


class PublicDocsTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        (self.root / "docs").mkdir()
        (self.root / "release").mkdir()
        (self.root / "README.md").write_text("# Public library\n")
        (self.root / "docs/API.md").write_text("# Public API\n")
        self.inventory = self.root / "release/public-documents.txt"
        self.inventory.write_text("README.md\ndocs/API.md\n")

    def test_reviewed_document_set_passes(self):
        self.assertEqual(check(self.root), [])

    def test_internal_status_added_to_git_is_refused(self):
        (self.root / "docs/internal-status.md").write_text("Unpublished results")
        subprocess.run(["git", "add", "docs/internal-status.md"], cwd=self.root, check=True)
        self.assertTrue(any("unreviewed" in item for item in check(self.root)))

    def test_workspace_link_in_an_approved_document_is_refused(self):
        (self.root / "docs/API.md").write_text("Reference: https://chatgpt.com/space/private-record")
        self.assertTrue(any("workspace link" in item for item in check(self.root)))

    def test_missing_approved_document_is_refused(self):
        (self.root / "docs/API.md").unlink()
        self.assertTrue(any("missing or nonregular" in item for item in check(self.root)))

    def test_inventory_cannot_escape_the_repository(self):
        self.inventory.write_text("../private.md\n")
        self.assertTrue(any("invalid document inventory path" in item for item in check(self.root)))

    def test_unlisted_published_document_without_document_suffix_is_refused(self):
        (self.root / "llms.txt").write_text("# Agent index\n")
        self.assertIn("unreviewed public document: llms.txt", check(self.root))
        self.inventory.write_text("README.md\ndocs/API.md\nllms.txt\n")
        self.assertEqual(check(self.root), [])

    def test_workspace_link_in_the_site_page_is_refused(self):
        page = self.root / "site/public/index.html"
        page.parent.mkdir(parents=True)
        page.write_text('<a href="https://chatgpt.com/share/private-record">notes</a>\n')
        self.inventory.write_text("README.md\ndocs/API.md\nsite/public/index.html\n")
        self.assertTrue(any("workspace link" in item for item in check(self.root)))

    def test_document_symlink_is_refused(self):
        (self.root / "docs/API.md").unlink()
        (self.root / "docs/API.md").symlink_to(self.root / "README.md")
        self.assertTrue(any("nonregular" in item for item in check(self.root)))


if __name__ == "__main__":
    unittest.main()
