"""The factory tools report to Zeno Commons only when ZENO_FCIS_COMMONS is set, and reporting never changes
their results. Run: python3 -m unittest tools/test_commons_hooks.py"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
sys.path.insert(0, str(ROOT / "integrations" / "commons"))
sys.path.insert(0, str(ROOT / "integrations" / "zal"))

import atdd  # noqa: E402
from commons import Commons  # noqa: E402


class HookTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.store = str(Path(self.tmp.name) / "store")

    def tearDown(self):
        self.tmp.cleanup()

    def events(self, prefix):
        reader = Commons(self.store, "reader")
        try:
            return reader.events(kind_prefix=prefix)
        finally:
            reader.close()

    def scenario(self, *commands):
        return mock.patch.dict(atdd.SCENARIOS, {"hook-probe": atdd.SCENARIOS[next(iter(atdd.SCENARIOS))]._replace(
            title="hook probe", commands=commands)} if hasattr(atdd.SCENARIOS[next(iter(atdd.SCENARIOS))], "_replace")
            else {"hook-probe": type("S", (), {"title": "hook probe", "commands": commands})()})

    def test_acceptance_scenarios_report_pass_and_fail_with_the_failing_command(self):
        ok = ("python3", "-c", "print(1)")
        bad = ("python3", "-c", "raise SystemExit(3)")
        with mock.patch.dict(os.environ, {"ZENO_FCIS_COMMONS": self.store}), self.scenario(ok):
            atdd.run_scenario("hook-probe")
        with mock.patch.dict(os.environ, {"ZENO_FCIS_COMMONS": self.store}), self.scenario(ok, bad):
            with self.assertRaises(subprocess.CalledProcessError):
                atdd.run_scenario("hook-probe")  # the failure still propagates unchanged
        kinds = [(e["kind"], e["subject"]) for e in self.events("factory.atdd.")]
        self.assertEqual(kinds, [("factory.atdd.scenario-passed", "hook-probe"),
                                 ("factory.atdd.scenario-failed", "hook-probe")])
        failed = self.events("factory.atdd.scenario-failed")[0]["body"]
        self.assertIn("SystemExit(3)", failed["command"])
        self.assertEqual(failed["error"], "CalledProcessError")

    def test_without_a_store_nothing_is_written_and_results_are_unchanged(self):
        env = {k: v for k, v in os.environ.items() if k != "ZENO_FCIS_COMMONS"}
        with mock.patch.dict(os.environ, env, clear=True), self.scenario(("python3", "-c", "print(1)")):
            atdd.run_scenario("hook-probe")
        self.assertFalse(Path(self.store).exists())

    def test_an_unwritable_store_never_changes_a_tool_result(self):
        with mock.patch.dict(os.environ, {"ZENO_FCIS_COMMONS": "/proc/no-such-store"}), \
                self.scenario(("python3", "-c", "print(1)")):
            atdd.run_scenario("hook-probe")  # still passes; a warning goes to stderr

    def test_tool_entry_points_report_refusals_and_exit_as_before(self):
        env = {**os.environ, "ZENO_FCIS_COMMONS": self.store}
        missing = str(Path(self.tmp.name) / "no-such-contract")
        for tool, part in (("zenoui.py", "zenoui"), ("app_factory.py", "app-factory")):
            with_store = subprocess.run([sys.executable, str(ROOT / "tools" / tool), missing, "--out",
                                         str(Path(self.tmp.name) / "out")], env=env, capture_output=True, text=True)
            without = subprocess.run([sys.executable, str(ROOT / "tools" / tool), missing, "--out",
                                      str(Path(self.tmp.name) / "out")],
                                     env={k: v for k, v in env.items() if k != "ZENO_FCIS_COMMONS"},
                                     capture_output=True, text=True)
            self.assertEqual(with_store.returncode, without.returncode)
            self.assertNotEqual(with_store.returncode, 0)
            self.assertTrue(self.events(f"factory.{part}."), tool)

    def test_loop_judgements_report_the_subject_revision(self):
        import loop_workspace
        state = {"subject": {"revision": "a" * 64, "tag": "counter"}}
        events = [{"author": "model", "act": {"kind": "candidate"}},
                  {"author": "judge", "act": {"kind": "refuted"}},
                  {"author": "judge", "act": {"kind": "accepted"}}]
        with mock.patch.dict(os.environ, {"ZENO_FCIS_COMMONS": self.store}):
            loop_workspace._report_judgements(state, "model", events)
        [event] = self.events("factory.loop.judged")
        self.assertEqual(event["subject"], "a" * 64)
        self.assertEqual(event["body"]["verdicts"], ["refuted", "accepted"])


if __name__ == "__main__":
    unittest.main()
