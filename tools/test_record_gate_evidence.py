"""Source-revision checks for the gate evidence recorder.

Gates, versions, and git are replaced with fakes, so these tests never run a
gate or edit the checkout.
"""

import json
import subprocess
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
    def record(self, checkout, *, during_versions=None, during_inventory=None, during_gate=None):
        gates = []

        def gate(name, command, environment=None):
            gates.append(name)
            if during_gate:
                during_gate(checkout)
            return {"name": name, "command": ["MOCKED"], "exit_code": 0,
                    "evidence_complete": True,
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

    def test_a_change_during_a_gate_stops_before_the_next_gate(self):
        result, record, gates = self.record(Checkout(), during_gate=modify)
        self.assertEqual((result, record, gates), (2, None, ["atdd"]))

    def test_a_new_commit_during_the_run_writes_nothing(self):
        result, record, _ = self.record(Checkout(), during_versions=commit)
        self.assertEqual((result, record), (2, None))


class CompiledInventoryChecks(unittest.TestCase):
    """Mocked binary listings exercise actual compiler-closure file handling."""

    def fixture(self, directory, sources, dependencies=None):
        root = Path(directory)
        (root / "Cargo.toml").write_text("[package]\nname = 'inventory-fixture'\n")
        executable = root / "target" / "fixture-123"
        executable.parent.mkdir()
        for name, text in sources.items():
            source = root / name
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_text(text)
        dependency_paths = dependencies if dependencies is not None else sources
        executable.with_suffix(".d").write_text(
            str(executable) + ": " + " ".join(str(root / name).replace(" ", "\\ ")
                                            for name in dependency_paths) + "\n")
        artifact = {"reason": "compiler-artifact", "manifest_path": str(root / "Cargo.toml"),
                    "profile": {"test": True}, "target": {"name": "inventory-fixture"},
                    "executable": str(executable)}
        return root, executable, json.dumps(artifact)

    def inventory(self, root, output, listing):
        with mock.patch.object(recorder, "ROOT", root), \
                mock.patch.object(recorder.subprocess, "run",
                                  return_value=subprocess.CompletedProcess([], 0, listing, "")):
            return recorder.compiled_ignored_inventory(output)

    def test_relocated_compiled_ignore_is_present_but_uncompiled_archive_is_absent(self):
        with tempfile.TemporaryDirectory(prefix="compiled-inventory-") as directory:
            root, _, output = self.fixture(directory, {
                "verification/kernel-laws/src/oracle/formal_tools/legacy_cli.rs":
                    '#[ignore = "genuine pinned Lean"]\nfn pinned_lean_cli() {}\n',
                "verification/archive.rs": '#[ignore]\nfn not_compiled() {}\n',
            }, ["verification/kernel-laws/src/oracle/formal_tools/legacy_cli.rs"])
            rows = self.inventory(root, output, "legacy_cli::pinned_lean_cli: test\n")
            self.assertEqual([row["qualified_test"] for row in rows],
                             ["legacy_cli::pinned_lean_cli"])
            self.assertEqual(rows[0]["file"],
                             "verification/kernel-laws/src/oracle/formal_tools/legacy_cli.rs")
            self.assertIsNone(recorder.ignored_tests([
                {"registered_ignored_tests": rows, "exit_code": 0, "name": "listing"}
            ])[0]["run_by"])

    def test_full_module_identity_distinguishes_two_compiled_template_helpers(self):
        with tempfile.TemporaryDirectory(prefix="compiled-inventory-") as directory:
            root, _, output = self.fixture(directory, {
                "templates/account-lockout/src/helper.rs": '#[ignore]\nfn child_process() {}\n',
                "templates/inventory-reservation/src/helper.rs": '#[ignore]\nfn child_process() {}\n',
            })
            rows = self.inventory(root, output,
                "account_lockout::tests::child_process: test\n"
                "inventory_reservation::tests::child_process: test\n")
            self.assertEqual(len(rows), 2)
            self.assertIn("account-lockout", rows[0]["file"])
            self.assertIn("inventory-reservation", rows[1]["file"])

    def test_unreadable_compiler_source_refuses_inventory(self):
        with tempfile.TemporaryDirectory(prefix="compiled-inventory-") as directory:
            root, _, output = self.fixture(directory, {}, ["missing.rs"])
            with self.assertRaises(FileNotFoundError):
                self.inventory(root, output, "tests::missing: test\n")

    def test_ambiguous_compiled_sources_refuse_inventory(self):
        with tempfile.TemporaryDirectory(prefix="compiled-inventory-") as directory:
            root, _, output = self.fixture(directory, {
                "src/first.rs": '#[ignore]\nfn child_process() {}\n',
                "src/second.rs": '#[ignore]\nfn child_process() {}\n',
            })
            with self.assertRaisesRegex(ValueError, "ambiguous or missing"):
                self.inventory(root, output, "tests::child_process: test\n")

    def test_no_cargo_artifact_refuses_inventory(self):
        with self.assertRaisesRegex(ValueError, "no compiled test binaries"):
            recorder.compiled_ignored_inventory('{"reason":"build-finished","success":true}\n')

    def test_foreign_manifest_refuses_inventory(self):
        with tempfile.TemporaryDirectory(prefix="compiled-inventory-") as directory:
            root, _, output = self.fixture(directory, {"src/lib.rs": ""})
            with mock.patch.object(recorder, "ROOT", root / "unrelated"):
                with self.assertRaises(ValueError):
                    recorder.compiled_ignored_inventory(output)

    def test_compiler_dependency_with_spaces_is_read(self):
        with tempfile.TemporaryDirectory(prefix="compiled-inventory-") as directory:
            root, executable, _ = self.fixture(directory, {"src/with space.rs": ""})
            with mock.patch.object(recorder, "ROOT", root):
                self.assertEqual(recorder.compiled_test_sources(executable),
                                 {root / "src/with space.rs"})

    def test_compiled_ignore_attribute_accepts_valid_rust_whitespace(self):
        for attribute in ['#[ignore="explicit"]', '#[ignore = "explicit"]',
                          '#[ ignore\n=\n"explicit" ]']:
            with self.subTest(attribute=attribute), \
                    tempfile.TemporaryDirectory(prefix="compiled-inventory-") as directory:
                root, _, output = self.fixture(directory, {
                    "src/lib.rs": attribute + '\nfn prerequisite() {}\n'
                })
                rows = self.inventory(root, output, "tests::prerequisite: test\n")
                self.assertEqual(rows[0]["reason"], "explicit")


class IgnoredExecutionChecks(unittest.TestCase):
    def helper(self):
        return {"test": "process_helper_original_lean_cli",
                "qualified_test": "tests::process_helper_original_lean_cli",
                "target": "zeno_fcis_formal_tools",
                "file": "crates/zeno-fcis-formal-tools/src/lib.rs",
                "reason": "spawned only by the private original CLI process regression"}

    def test_spawned_only_helper_requires_an_observed_successful_parent(self):
        helper = self.helper()
        steps = [{"name": "inventory", "exit_code": 0, "registered_ignored_tests": [helper]},
                 {"name": "private-lean-cli", "exit_code": 0, "evidence_complete": True,
                  "observed_passed_tests": ["legacy_cli::pinned_lean_cli_prove_is_process_level"]}]
        row = recorder.ignored_tests(steps)[0]
        self.assertEqual((row["run_by"], row["parent_test"]),
                         ("private-lean-cli", "legacy_cli::pinned_lean_cli_prove_is_process_level"))
        for field, value in [("exit_code", 1), ("evidence_complete", False),
                             ("observed_passed_tests", [])]:
            with self.subTest(field=field):
                changed = {**steps[1], field: value}
                self.assertIsNone(recorder.ignored_tests([steps[0], changed])[0]["run_by"])

    def test_duplicate_registered_identity_refuses_inventory(self):
        with self.assertRaisesRegex(ValueError, "duplicate compiled"):
            recorder.ignored_tests([{"exit_code": 0,
                "registered_ignored_tests": [self.helper(), self.helper()]}])

    def test_zero_test_exact_filter_is_not_a_pass(self):
        output = "test result: ok. 0 passed; 0 failed; 0 ignored; 10 filtered out\n"
        with mock.patch.object(recorder.subprocess, "run",
                               return_value=subprocess.CompletedProcess([], 0, output, "")):
            result = recorder.run("pinned", ["cargo", "test", "tests::pinned", "--", "--exact"])
        self.assertFalse(result["evidence_complete"])
        self.assertEqual(result["observed_passed_tests"], [])

    def test_one_actual_test_count_qualifies_the_exact_filter(self):
        output = "test result: ok. 1 passed; 0 failed; 0 ignored; 10 filtered out\n"
        with mock.patch.object(recorder.subprocess, "run",
                               return_value=subprocess.CompletedProcess([], 0, output, "")):
            result = recorder.run("pinned", ["cargo", "test", "tests::pinned", "--", "--exact"])
        self.assertTrue(result["evidence_complete"])
        self.assertIn("tests::pinned", result["observed_passed_tests"])

    def test_incomplete_inventory_is_not_a_clean_gate(self):
        with mock.patch.object(recorder.subprocess, "run",
                               return_value=subprocess.CompletedProcess([], 0, "", "")):
            result = recorder.run("inventory", ["cargo", "test", "--no-run", "--message-format=json"])
        self.assertFalse(result["evidence_complete"])
        self.assertIn("inventory_error", result)


if __name__ == "__main__":
    unittest.main()
