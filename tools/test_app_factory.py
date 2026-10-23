#!/usr/bin/env python3
"""Regression checks for complete, failure-atomic generated site libraries."""
import subprocess
import tempfile
import unittest
import io
import json
import hashlib
import sys
from contextlib import redirect_stderr
from pathlib import Path
from unittest.mock import patch

import app_factory

class NumericRefusalTests(unittest.TestCase):
    def test_policy_changed_after_review_cannot_receive_the_review_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "v2").mkdir()
            records = [("project.zeno", "project_sha256"), ("v2/policy.json", "rules_sha256"), ("v2/schema.zcve", "schema_sha256")]
            sources = {}
            for name, key in records:
                (root / name).write_text(name)
                sources[key] = hashlib.sha256(name.encode()).hexdigest()
            app_factory.verify_review_subject({"sources":sources}, root)
            (root / "v2/policy.json").write_text("changed policy")
            with self.assertRaisesRegex(ValueError, "AppContractChangedAfterReview:v2/policy.json"):
                app_factory.verify_review_subject({"sources":sources}, root)

    def test_checked_numeric_contract_cannot_emit_an_unbound_durable_app(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            contract = root / "contract"
            (contract / "v2").mkdir(parents=True)
            (contract / "v2/policy.json").write_text(json.dumps({"schema":"zeno-fcis/template-declarative-policy/4"}))
            (contract / "ui.json").write_text("{}")
            output = root / "app"
            errors = io.StringIO()
            with patch.object(sys, "argv", ["app_factory", "--cli", "test-cli", "--contract", str(contract), "--out", str(output)]), patch.object(app_factory.subprocess, "check_output", return_value="v22.0.0"), patch.object(app_factory, "contract_review", return_value={"mode":"symbolic","status":"solver-checked"}), patch.object(app_factory, "prepare_library") as prepare, redirect_stderr(errors):
                self.assertEqual(app_factory.entrypoint(), 1)
            self.assertIn("AppNumericDurableUnsupported", errors.getvalue())
            self.assertEqual(len(errors.getvalue().splitlines()), 1)
            prepare.assert_not_called()
            self.assertFalse(output.exists())

    def test_u128_refusal_is_one_line_without_traceback(self):
        child = subprocess.CalledProcessError(1, ["checker"], stderr="Traceback (most recent call last):\nContractError U128SubjectUnsupported\nmore context\n")
        output = io.StringIO()
        with patch.object(app_factory, "main", side_effect=child), redirect_stderr(output):
            self.assertEqual(app_factory.entrypoint(), 1)
        self.assertEqual(output.getvalue(), "app build refused: U128SubjectUnsupported: the native UI binding does not support U128\n")

    def test_child_ui_refusal_cannot_produce_a_successful_build(self):
        child = subprocess.CalledProcessError(1, ["checker"], stderr="U128SubjectUnsupported")
        with patch.object(app_factory, "run", side_effect=child):
            with self.assertRaisesRegex(ValueError, "U128SubjectUnsupported"):
                app_factory.build_ui("checker")

    def test_review_enumerates_up_to_the_factory_cap(self):
        commands = []
        def review(*command, **kwargs):
            commands.append([str(part) for part in command])
            Path(command[command.index("--out") + 1]).write_text('{"mode":"exhaustive","status":"reviewed"}')
        with patch.object(app_factory, "run", side_effect=review):
            app_factory.contract_review(Path("test-cli"), Path("test-contract"))
        command = commands[0]
        self.assertIn("--max-tuples", command)
        self.assertEqual(command[command.index("--max-tuples") + 1], str(1 << 22))

    def test_symbolic_review_unknown_is_not_admitted(self):
        def review(*command, **kwargs):
            packet = Path(command[command.index("--out") + 1])
            packet.write_text('{"mode":"symbolic","status":"UNKNOWN"}')
        with patch.object(app_factory, "run", side_effect=review):
            with self.assertRaisesRegex(ValueError, "AppContractLawsUnknownOrRefuted"):
                app_factory.contract_review(Path("test-cli"), Path("test-contract"))


class LibraryPreparationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.destination = self.root / "site/apps/withdrawal-queue"
        self.root_patch = patch.object(app_factory, "ROOT", self.root)
        self.root_patch.start()
        self.addCleanup(self.root_patch.stop)

    def generate(self, *command, **kwargs):
        staged = Path(command[2])
        self.assertEqual(staged.name, self.destination.name)
        staged.mkdir()
        (staged / "src").mkdir()
        (staged / "Cargo.toml").write_text(
            '[package]\nname = "withdrawal-queue"\nversion = "0.0.0"\n'
            '\n[patch.crates-io]\nplaceholder = {}\n')
        (staged / "src/lib.rs").write_text("pub fn checked_library() {}\n")
        (staged / "Cargo.lock").write_text("temporary lock")
        (staged / "rust-toolchain.toml").write_text("temporary toolchain")

    def test_failure_publishes_nothing_and_retry_succeeds(self):
        def failed(*command, **kwargs):
            self.generate(*command, **kwargs)
            self.assertFalse(self.destination.exists())
            raise subprocess.CalledProcessError(3, command)

        with patch.object(app_factory, "run", side_effect=failed):
            with self.assertRaises(subprocess.CalledProcessError):
                app_factory.prepare_library(Path("test-cli"), "withdrawal-queue")
        self.assertFalse(self.destination.exists())
        self.assertEqual(list(self.destination.parent.iterdir()), [])
        with patch.object(app_factory, "run", side_effect=self.generate):
            app_factory.prepare_library(Path("test-cli"), "withdrawal-queue")
        self.assertTrue((self.destination / "src/lib.rs").is_file())
        self.assertNotIn("[patch.crates-io]", (self.destination / "Cargo.toml").read_text())
        self.assertFalse((self.destination / "Cargo.lock").exists())
        self.assertFalse((self.destination / "rust-toolchain.toml").exists())

    def test_existing_partial_library_is_refused_and_preserved(self):
        self.destination.mkdir(parents=True)
        (self.destination / "ui.json").write_text("retained partial output")
        before = app_factory.hashes(self.destination)
        with patch.object(app_factory, "run", side_effect=self.generate):
            with self.assertRaisesRegex(ValueError, "AppLibraryIncompleteOrChanged"):
                app_factory.prepare_library(Path("test-cli"), "withdrawal-queue")
        self.assertEqual(app_factory.hashes(self.destination), before)
        self.assertEqual(list(self.destination.parent.iterdir()), [self.destination])

    def test_manifest_alone_does_not_make_a_library_complete(self):
        with patch.object(app_factory, "run", side_effect=self.generate):
            app_factory.prepare_library(Path("test-cli"), "withdrawal-queue")
            (self.destination / "src/lib.rs").unlink()
            before = app_factory.hashes(self.destination)
            with self.assertRaisesRegex(ValueError, "AppLibraryIncompleteOrChanged"):
                app_factory.prepare_library(Path("test-cli"), "withdrawal-queue")
        self.assertEqual(app_factory.hashes(self.destination), before)

    def test_changed_library_is_refused_without_overwriting(self):
        with patch.object(app_factory, "run", side_effect=self.generate):
            app_factory.prepare_library(Path("test-cli"), "withdrawal-queue")
            (self.destination / "src/lib.rs").write_text("retained edited output")
            before = app_factory.hashes(self.destination)
            with self.assertRaisesRegex(ValueError, "AppLibraryIncompleteOrChanged"):
                app_factory.prepare_library(Path("test-cli"), "withdrawal-queue")
        self.assertEqual(app_factory.hashes(self.destination), before)

    def test_complete_library_is_checked_and_extra_build_files_are_preserved(self):
        with patch.object(app_factory, "run", side_effect=self.generate) as generate:
            app_factory.prepare_library(Path("test-cli"), "withdrawal-queue")
            extra = self.destination / "target/retained"
            extra.parent.mkdir()
            extra.write_text("build output")
            before = app_factory.hashes(self.destination)
            app_factory.prepare_library(Path("test-cli"), "withdrawal-queue")
        self.assertEqual(generate.call_count, 2)
        self.assertEqual(app_factory.hashes(self.destination), before)


if __name__ == "__main__":
    unittest.main()
