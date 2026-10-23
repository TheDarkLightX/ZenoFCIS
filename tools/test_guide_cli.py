"""Public guide and help contracts through the installed binary."""
import json
import hashlib
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "crates/zeno-fcis-cli/guide-card-inventory.json"
HELP_SNAPSHOTS = ROOT / "crates/zeno-fcis-cli/guide-help-snapshots.json"


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
            self.assertEqual(len(report["cards"]), 29)
            self.assertEqual({c["name"] for c in report["cards"]},
                             {c["name"] for c in json.loads(INVENTORY.read_text())["cards"]})
            self.assertIn("contract-risk", {c["name"] for c in report["cards"]})
            self.assertIn("contract-probe", {card["name"] for card in report["cards"]})
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_guide_inventory_matches_generated_source(self):
        inventory = json.loads(INVENTORY.read_text())
        self.assertEqual(inventory["schema"], "zeno-fcis/guide-card-inventory/1")
        actual = json.loads(self.call("guide", "list").stdout)["cards"]
        self.assertEqual([c["name"] for c in actual], [c["name"] for c in inventory["cards"]])
        for row in inventory["cards"]:
            with self.subTest(card=row["name"]):
                source = (ROOT / "docs/loops" / (row["name"] + ".md")).read_bytes()
                self.assertEqual(hashlib.sha256(source).hexdigest(), row["sha256"])
                shown = self.call("guide", "show", row["name"])
                self.assertEqual(shown.returncode, 0, shown.stderr)
                self.assertEqual(shown.stdout.encode(), source)

    def test_generated_help_snapshots_match_the_compiled_commands(self):
        snapshots = json.loads(HELP_SNAPSHOTS.read_text())
        self.assertEqual(snapshots["schema"], "zeno-fcis/guide-help-snapshots/1")
        commands = {command for row in json.loads(INVENTORY.read_text())["cards"] for command in row["commands"]}
        self.assertEqual(set(snapshots["help"]), commands)
        for command, expected in snapshots["help"].items():
            with self.subTest(command=command):
                result = self.call(*shlex.split(command), "--help")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, expected)

    def test_owner_questions_are_discoverable(self):
        result = self.call("guide", "find", "owner questions")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["cards"][0]["name"], "loop-question")

    def test_contract_question_ignores_specification_limits(self):
        args = ("loop", "question", "--contract", "examples/guide-counter")
        baseline = self.call(*args)
        result = self.call(*args, "--max-work", "0", "--max-domain", "0",
                           "--max-meanings", "0", "--max-cells", "0", "--max-conflict-subsets", "0")
        self.assertEqual(baseline.returncode, 0, baseline.stderr)
        self.assertEqual(result.returncode, 0, result.stderr)
        before, after = json.loads(baseline.stdout), json.loads(result.stdout)
        self.assertEqual(after["status"], before["status"])
        self.assertEqual(after["detail"], before["detail"])

    def test_capture_unavailable_backend_has_the_documented_refusal_schema(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            abi = work / "abi.json"
            reference = work / "reference.py"
            abi.write_text('{"inputs":[],"outputs":[]}')
            reference.write_text('def reference(point):\n    return []\n')
            session = work / "capture"
            result = self.call("--containment-backend", "posix", "loop", "capture", "open",
                               "--spec", str(abi), "--reference", str(reference), "--session", str(session))
            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["schema"], "zeno-fcis/black-box-capture/1")
            self.assertEqual(report["status"], "refused")
            self.assertEqual(report["detail"]["reason"], "capture-containment-unavailable")
            self.assertFalse(report["containment"]["final"]["application_work_started"])
            self.assertFalse(session.exists())

    def test_unknown_card_has_actionable_refusal(self):
        result = self.call("guide", "show", "missing-card")
        self.assertEqual(result.returncode, 64)
        self.assertIn("guide list", json.loads(result.stdout)["next"])


if __name__ == "__main__":
    unittest.main()
