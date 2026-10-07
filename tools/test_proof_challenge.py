"""Check fixture admission and refusal classification; no fake proof checker."""

import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import time
import unittest
from unittest import mock

import check_proof_challenge as pilot


class FrozenFixtureTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name) / "fixtures"
        shutil.copytree(pilot.FIXTURES, self.root)

    def test_reviewed_complete_fixture_set(self):
        self.assertEqual(set(pilot.load_fixtures(self.root)), pilot.EXPECTED_FILES)

    def test_changed_statement_is_refused_before_execution(self):
        (self.root / "Challenge.lean").write_text("theorem changed : True := True.intro\n")
        with self.assertRaisesRegex(pilot.ExperimentError, "frozen fixture changed"):
            pilot.load_fixtures(self.root)

    def test_changed_allowlist_is_refused_before_execution(self):
        config = json.loads((self.root / "config.json").read_text())
        config["permitted_axioms"] = ["sorryAx"]
        (self.root / "config.json").write_text(json.dumps(config))
        with self.assertRaisesRegex(pilot.ExperimentError, "frozen fixture changed"):
            pilot.load_fixtures(self.root)

    def test_rewriting_manifest_cannot_approve_a_new_fixture(self):
        path = self.root / "manifest.json"
        manifest = json.loads(path.read_text())
        del manifest["Challenge.lean"]
        path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(pilot.ExperimentError, "reviewed manifest changed"):
            pilot.load_fixtures(self.root)

    def test_omission_is_refused_even_under_a_new_manifest_digest(self):
        path = self.root / "manifest.json"
        manifest = json.loads(path.read_text())
        del manifest["Challenge.lean"]
        path.write_text(json.dumps(manifest))
        with mock.patch.object(pilot, "MANIFEST_SHA256", hashlib.sha256(path.read_bytes()).hexdigest()):
            with self.assertRaisesRegex(pilot.ExperimentError, "incomplete fixture manifest"):
                pilot.load_fixtures(self.root)

    def test_missing_case_is_refused(self):
        (self.root / "cases/valid.lean").unlink()
        with self.assertRaisesRegex(pilot.ExperimentError, "unexpected or missing case"):
            pilot.load_fixtures(self.root)

    def test_extra_case_is_refused(self):
        (self.root / "cases/unreviewed.lean").write_text("#eval IO.println 1\n")
        with self.assertRaisesRegex(pilot.ExperimentError, "unexpected or missing case"):
            pilot.load_fixtures(self.root)

    def test_symlink_fixture_is_refused(self):
        case = self.root / "cases/valid.lean"
        case.unlink()
        case.symlink_to(self.root / "Challenge.lean")
        with self.assertRaisesRegex(pilot.ExperimentError, "regular file"):
            pilot.load_fixtures(self.root)

    def test_symlink_manifest_is_refused(self):
        manifest = self.root / "manifest.json"
        target = self.root / "saved-manifest.json"
        manifest.rename(target)
        manifest.symlink_to(target)
        with self.assertRaisesRegex(pilot.ExperimentError, "regular file"):
            pilot.load_fixtures(self.root)

    def test_symlink_case_directory_is_refused(self):
        cases = self.root / "cases"
        target = self.root / "saved-cases"
        cases.rename(target)
        cases.symlink_to(target)
        with self.assertRaisesRegex(pilot.ExperimentError, "case directory"):
            pilot.load_fixtures(self.root)


class ResultTests(unittest.TestCase):
    def test_valid_requires_zero_exit_and_success_marker(self):
        pilot.judge_result(0, pilot.SUCCESS + "\n", None)
        for status, text in ((1, pilot.SUCCESS), (0, ""), (0, pilot.SUCCESS + "\n" + pilot.SUCCESS)):
            with self.subTest(status=status, text=text), self.assertRaises(pilot.ExperimentError):
                pilot.judge_result(status, text, None)

    def test_negative_requires_its_stated_reason(self):
        reason = pilot.CASES["forbidden-axiom"]
        pilot.judge_result(1, reason, reason)
        for status, text in ((0, reason), (1, "missing tool"), (1, "timeout"),
                             (1, reason + "\n" + pilot.SUCCESS)):
            with self.subTest(status=status, text=text), self.assertRaises(pilot.ExperimentError):
                pilot.judge_result(status, text, reason)

    def test_fresh_project_uses_snapshot_and_has_no_cached_artifacts(self):
        snapshot = pilot.load_fixtures()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "project"
            pilot.make_project(path, snapshot, "alternate-proof")
            self.assertEqual((path / "Solution.lean").read_bytes(), snapshot["cases/alternate-proof.lean"])
            self.assertFalse((path / ".lake").exists())

    def test_timeout_kills_descendants_and_is_not_a_negative_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "late-output"
            child = "import pathlib,time;pathlib.Path('started').touch();time.sleep(0.4);pathlib.Path('late-output').touch()"
            parent = f"import subprocess,sys,time;subprocess.Popen([sys.executable,'-c',{child!r}]);time.sleep(10)"
            with self.assertRaisesRegex(pilot.ExperimentError, "timeout"):
                pilot.run([sys.executable, "-c", parent], root, {}, root / "timeout.log", timeout=0.2)
            self.assertTrue((root / "started").exists(), "cleanup control never started its child")
            time.sleep(0.5)
            self.assertFalse(marker.exists(), "timed-out child escaped process cleanup")


if __name__ == "__main__":
    unittest.main(verbosity=2)
