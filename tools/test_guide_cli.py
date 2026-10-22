"""Public guide and help contracts through the installed binary."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


@unittest.skipUnless(os.environ.get("ZENO_FCIS_CLI"), "set ZENO_FCIS_CLI")
class GuideCli(unittest.TestCase):
    def call(self, *args, cwd=None):
        env = {key: value for key, value in os.environ.items() if key not in ("ZENO_FCIS_KB", "ZENO_FCIS_TELEMETRY")}
        return subprocess.run([os.environ["ZENO_FCIS_CLI"], "--no-kb", *args],
            cwd=cwd, env=env, capture_output=True, text=True, check=False)

    def test_loop_help_groups_every_exposed_command_without_renaming(self):
        report = json.loads(self.call("describe", "loop").stdout)
        commands = {item["name"] for item in report["command"]["subcommands"]}
        help_text = self.call("loop", "--help").stdout.split("Options:", 1)[0]
        displayed = re.findall(r"^  ([a-z][a-z-]+) +", help_text, re.M)
        self.assertEqual(set(displayed), commands)
        self.assertEqual(len(displayed), len(set(displayed)))
        for purpose in ("Learn from examples", "Complete and repair", "Optimize", "Check restated rules", "Ask where laws are open", "Architecture"):
            self.assertIn(purpose + ":", help_text)

    def test_every_loop_option_has_nonempty_help(self):
        result = self.call("describe", "loop")
        self.assertEqual(result.returncode, 0, result.stderr)

        def visit(command):
            for argument in command["arguments"]:
                with self.subTest(command=command["path"], option=argument["id"]):
                    self.assertTrue((argument["help"] or "").strip(),
                                    "Every loop option must explain its purpose")
            for child in command["subcommands"]:
                visit(child)

        visit(json.loads(result.stdout)["command"])

    def test_loop_summaries_fit_on_one_line(self):
        result = self.call("describe", "loop")
        self.assertEqual(result.returncode, 0, result.stderr)

        def visit(command):
            with self.subTest(command=command["path"]):
                summary = command["about"] or ""
                self.assertTrue(summary.strip())
                self.assertNotIn("\n", summary)
                self.assertLess(len(summary), 100)
            for child in command["subcommands"]:
                visit(child)

        visit(json.loads(result.stdout)["command"])

    def test_guide_is_available_without_any_workspace(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.call("guide", "list", cwd=directory)
            self.assertEqual(result.returncode, 0)
            report = json.loads(result.stdout)
            self.assertEqual(report["schema"], "zeno-fcis/guide/1")
            self.assertEqual(report["authority"], "none")
            self.assertEqual(len(report["cards"]), 19)
            self.assertIn("contract-risk", {c["name"] for c in report["cards"]})
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_unknown_card_has_actionable_refusal(self):
        result = self.call("guide", "show", "missing-card")
        self.assertEqual(result.returncode, 64)
        self.assertIn("guide list", json.loads(result.stdout)["next"])


if __name__ == "__main__":
    unittest.main()
