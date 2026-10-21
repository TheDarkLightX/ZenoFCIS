#!/usr/bin/env python3
"""Regression checks for complete, failure-atomic generated site libraries."""
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import app_factory


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
