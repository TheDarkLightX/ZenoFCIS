"""Source-revision checks for the gate evidence recorder.

Gates, versions, and git are replaced with fakes, so these tests never run a
gate or edit the checkout.
"""

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import record_gate_evidence as recorder

REVISION = "a" * 40
TREE = "b" * 40


class Checkout:
    """A fake git view whose commit or tracked files can change mid-run."""

    def __init__(self, changes=""):
        self.revision = REVISION
        self.changes = changes

    def git(self, *args):
        if args == ("rev-parse", "HEAD"):
            return self.revision
        if args == ("rev-parse", "HEAD^{tree}"):
            return TREE if self.revision == REVISION else "c" * 40
        if args == ("status", "--porcelain", "--untracked-files=no"):
            return self.changes
        raise AssertionError(f"unexpected git call {args}")


def modify(checkout):
    checkout.changes = " M README.md"


def commit(checkout):
    checkout.revision = "d" * 40


class RecorderSourceChecks(unittest.TestCase):
    def record(self, checkout, *, during_versions=None, during_inventory=None):
        gates = []

        def gate(name, command, environment=None):
            gates.append(name)
            return {"name": name, "command": ["MOCKED"], "exit_code": 0,
                    "tests": {"passed": 1, "failed": 0, "ignored": 0}}

        def version(command):
            if during_versions:
                during_versions(checkout)
            return "mocked"

        def inventory(steps):
            if during_inventory:
                during_inventory(checkout)
            return []

        with tempfile.TemporaryDirectory(prefix="gate-evidence-") as directory:
            out = Path(directory) / "evidence.json"
            with mock.patch.object(recorder, "git", checkout.git), \
                    mock.patch.object(recorder, "run", gate), \
                    mock.patch.object(recorder, "version", version), \
                    mock.patch.object(recorder, "pinned_steps", lambda: []), \
                    mock.patch.object(recorder, "lean_trust_anchor", lambda: None), \
                    mock.patch.object(recorder, "ignored_tests", inventory), \
                    mock.patch.object(sys, "argv", ["record_gate_evidence.py", "--out", str(out)]):
                result = recorder.main()
            record = json.loads(out.read_text()) if out.exists() else None
        return result, record, gates

    def test_a_clean_run_records_its_exact_revision_and_tree(self):
        result, record, gates = self.record(Checkout())
        self.assertEqual(result, 0)
        self.assertEqual((record["revision"], record["tree"], record["status"]),
                         (REVISION, TREE, "passed"))
        self.assertIn("atdd", gates)

    def test_uncommitted_tracked_changes_are_refused_before_any_gate(self):
        result, record, gates = self.record(Checkout(changes=" M README.md"))
        self.assertEqual((result, record, gates), (2, None, []))

    def test_a_change_during_version_collection_writes_nothing(self):
        # Regression from the independent review of 562926d: the last source
        # check came before version collection, so this run published
        # `passed` with README.md modified.
        result, record, _ = self.record(Checkout(), during_versions=modify)
        self.assertEqual((result, record), (2, None))

    def test_a_change_during_the_ignored_test_inventory_writes_nothing(self):
        result, record, _ = self.record(Checkout(), during_inventory=modify)
        self.assertEqual((result, record), (2, None))

    def test_a_new_commit_during_the_run_writes_nothing(self):
        result, record, _ = self.record(Checkout(), during_versions=commit)
        self.assertEqual((result, record), (2, None))


if __name__ == "__main__":
    unittest.main()
