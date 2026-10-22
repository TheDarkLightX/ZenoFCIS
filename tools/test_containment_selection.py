#!/usr/bin/env python3
"""Control tools keep finite-data CLI calls on POSIX when the environment selects systemd.

The CLI refuses a finite-data command whose selected backend is systemd, so each
such call passes `--containment-backend auto`. Capture acquisition and offline
parallel checks keep the environment's choice. No CLI is run.
"""
from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capture_controls  # noqa: E402
import check_transform_parallel  # noqa: E402
import check_version_space  # noqa: E402
import loop_composable_controls  # noqa: E402
import loop_judges_controls  # noqa: E402
import loop_learn_controls  # noqa: E402
import loop_strategy_controls  # noqa: E402

AUTO = ["--containment-backend", "auto"]


class Stop(Exception):
    """Ends a campaign after its first CLI call has been recorded."""


class Recorder:
    def __init__(self, stop=False):
        self.calls, self.stop = [], stop

    def __call__(self, argv, *args, **kwargs):
        self.calls.append([str(part) for part in argv])
        if self.stop:
            raise Stop
        text = kwargs.get("text", False)
        return subprocess.CompletedProcess(argv, 0, "{}" if text else b"{}", "" if text else b"")


def selects_auto(argv):
    return argv[1:3] == AUTO


class Selection(unittest.TestCase):
    def test_finite_data_helpers_pass_the_automatic_backend(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            recorder = Recorder()
            with patch("subprocess.run", recorder):
                loop_learn_controls.call(Path("cli"), ["loop", "learn", "--spec", folder / "spec.json"])
                loop_judges_controls.run(Path("cli"), "loop", "question", "--contract", folder)
                check_version_space.execute(Path("cli"), {"inputs": []}, folder)
            self.assertEqual(len(recorder.calls), 3)
            for argv in recorder.calls:
                self.assertTrue(selects_auto(argv), argv)

    def test_campaigns_pass_the_automatic_backend_from_their_first_call(self):
        for module in (loop_composable_controls, loop_strategy_controls):
            with self.subTest(module.__name__), tempfile.TemporaryDirectory() as name:
                recorder = Recorder(stop=True)
                with patch("subprocess.run", recorder), self.assertRaises(Stop):
                    module.campaign(Path("cli"), Path(name) / "work")
                self.assertTrue(selects_auto(recorder.calls[0]), recorder.calls[0])

    def test_capture_acquisition_keeps_the_environment_choice(self):
        with tempfile.TemporaryDirectory() as name:
            controls = capture_controls.Controls("cli", Path(name))
            recorder = Recorder()
            with patch("subprocess.run", recorder):
                for verb in ("open", "whole", "candidate", "status"):
                    controls.call("loop", "capture", verb, "--session", "s")
            self.assertEqual([selects_auto(argv) for argv in recorder.calls], [False, False, True, True])
            self.assertEqual([argv[1:4] for argv in recorder.calls[:2]],
                             [["loop", "capture", "open"], ["loop", "capture", "whole"]])

    def test_offline_parallel_checks_keep_the_environment_choice(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            subject = {"name": "s", "original": folder / "a.zcve", "candidate": folder / "b.zcve", "extra": ()}
            recorder = Recorder()
            with patch("subprocess.run", recorder):
                check_transform_parallel.invoke(Path("cli"), folder, subject, "serial", [])
                check_transform_parallel.invoke(Path("cli"), folder, subject, "w2",
                                                ["--offline-parallel", "--workers", "2"])
                check_transform_parallel.encode(Path("cli"), folder, "s",
                                                {"input_domains": [], "output_domains": [],
                                                 "original": {"nodes": [], "roots": []}}, "original")
            self.assertEqual([selects_auto(argv) for argv in recorder.calls], [True, False, True])

    def test_parallel_parity_ignores_only_containment_and_progress_records(self):
        serial = {"status": "equivalent", "detail": {"receipt": {"n": 1}, "receipt_path": "a"}}
        parallel = {"status": "equivalent", "detail": {"receipt": {"n": 1}, "receipt_path": "b"},
                    "backend": "systemd", "required_profile": "group-resources/1",
                    "containment": {"backend": "systemd"}, "progress": {"planned_chunks": 1}}
        comparable = check_transform_parallel.comparable
        self.assertEqual(comparable(parallel), comparable(serial))
        changed = dict(parallel, detail={"receipt": {"n": 2}, "receipt_path": "b"})
        self.assertNotEqual(comparable(changed), comparable(serial))


if __name__ == "__main__":
    unittest.main()
