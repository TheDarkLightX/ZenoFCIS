#!/usr/bin/env python3
"""Read the acceptance registrations without running ATDD or any CLI."""
import ast
from contextlib import redirect_stderr, redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import check_version_space

ROOT = Path(__file__).resolve().parents[1]


def commands(name):
    tree = ast.parse((ROOT / "tools/atdd.py").read_text())
    registry = next(n.value for n in tree.body if isinstance(n, ast.AnnAssign)
                    and isinstance(n.target, ast.Name) and n.target.id == "SCENARIOS")
    for key, value in zip(registry.keys, registry.values):
        if ast.literal_eval(key) == name:
            return [tuple(ast.literal_eval(item) if isinstance(item, ast.Constant) else "<bound path>"
                          for item in command.elts) for command in value.args[1].elts]
    return []


class AcceptanceBindings(unittest.TestCase):
    def test_final_checker_controls_are_reachable(self):
        bound = commands("loop-version-space")
        for name in ("final_checker_rejects_a_lying_singleton",
                     "final_law_checker_rejects_a_lying_singleton",
                     "final_transform_checker_rejects_a_lying_singleton"):
            self.assertTrue(any(c[:3] == ("cargo", "+1.97.1", "test") and "--bin" in c
                                and "zeno-fcis" in c and name in c for c in bound), name)

    def test_real_workspace_and_owner_campaign_are_reachable(self):
        owner = commands("loop-version-space-owner")
        semantic = commands("loop-version-space-semantics")
        self.assertTrue(any("integrations/zal/check_loop_versions.py" in c and "--cli" in c for c in owner))
        self.assertTrue(any("tools/check_version_space.py" in c and "--cli" in c and "--self-test" not in c for c in semantic))
        for bound in (owner, semantic):
            self.assertTrue(any(c[:3] == ("cargo", "+1.97.1", "build") and "zeno-fcis-cli" in c for c in bound))


class EvidenceOutput(unittest.TestCase):
    def test_atdd_stdout_retains_evidence_and_can_be_repeated(self):
        report = {"authority": "none", "rows": [{"case": "mocked-output"}], "failures": [], "applicable_rows": 1}
        with patch.object(check_version_space, "controls", return_value=report) as mocked:
            for _ in range(2):
                out = io.StringIO()
                with redirect_stdout(out):
                    self.assertEqual(check_version_space.main(["--cli", "fake-cli"]), 0)
                self.assertIn('"mocked-output"', out.getvalue())
                self.assertEqual(mocked.call_args.args[0], Path("fake-cli"))
                self.assertFalse(mocked.call_args.args[1].exists(), "owned temporary input is discarded")

    def test_existing_evidence_is_preserved_before_any_campaign(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "evidence.json"
            output.write_text(json.dumps({"retained": True}))
            with patch.object(check_version_space, "controls") as mocked, redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    check_version_space.main(["--cli", "fake-cli", "--out", str(output)])
                self.assertEqual(error.exception.code, 2)
                mocked.assert_not_called()
            self.assertEqual(json.loads(output.read_text()), {"retained": True})


if __name__ == "__main__":
    unittest.main()
