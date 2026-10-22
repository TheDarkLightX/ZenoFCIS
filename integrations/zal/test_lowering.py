"""File-level lowering and native refinement over shipped finite ZAL examples."""

import dataclasses
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import behavior2
import factory


HERE = Path(__file__).resolve().parent
RUN = [sys.executable, "-B", str(HERE / "run.py")]


class LoweringTests(unittest.TestCase):
    def test_cli_reuses_lowering_for_both_surfaces_and_empty_context(self):
        for name in ["order2", "context_free2"]:
            model = behavior2.parse((HERE / f"examples/{name}.zal").read_text())
            for english in [False, True]:
                with self.subTest(name=name, english=english), tempfile.TemporaryDirectory() as tmp:
                    work = Path(tmp)
                    source = work / "input.zal"
                    source.write_text(model.render(english))
                    output = work / "contract"
                    checked = subprocess.run(RUN + ["lower", str(source), "--syntax", "english" if english else "symbolic",
                                                    "--out", str(output)], capture_output=True, text=True)
                    self.assertEqual(checked.returncode, 0, checked.stderr)
                    self.assertEqual(json.loads(checked.stdout)["revision"], model.revision)
                    expected = factory.lower(model)
                    self.assertEqual({str(p.relative_to(output)): p.read_text() for p in output.rglob("*") if p.is_file()}, expected)
                    self.assertIn("type 102 context Observations;", expected["project.zeno"])
                    if name == "context_free2":
                        self.assertIn("field 160 102 unit 106;", expected["project.zeno"])
                        self.assertEqual(json.loads(expected["zal-source-map.json"])["context_fields"], {})
                    refused = subprocess.run(RUN + ["lower", str(source), "--syntax", "english" if english else "symbolic",
                                                    "--out", str(output)], capture_output=True, text=True)
                    self.assertEqual(refused.returncode, 2)
                    self.assertIn("absent or an empty directory", refused.stderr)

    def test_lowering_owns_outputs_and_preserves_model(self):
        model = behavior2.parse((HERE / "examples/context_free2.zal").read_text())
        before = (model.data(), model.revision, model.render())
        files = factory.lower(model)
        files["project.zeno"] = "changed"
        self.assertEqual((model.data(), model.revision, model.render()), before)
        self.assertNotEqual(factory.lower(model)["project.zeno"], "changed")
        with self.assertRaises(dataclasses.FrozenInstanceError):
            model.initial = "running"


@unittest.skipUnless(os.environ.get("ZAL_LOWERING_TEST_CLI"), "set ZAL_LOWERING_TEST_CLI to the built pinned CLI")
class NativeLoweringTests(unittest.TestCase):
    def test_examples_generate_and_refine(self):
        cli = Path(os.environ["ZAL_LOWERING_TEST_CLI"])
        for name in ["order2", "context_free2"]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmp:
                model = behavior2.parse((HERE / f"examples/{name}.zal").read_text())
                contract = Path(tmp) / "contract"
                factory.write_new(factory.lower(model), contract)
                generated = subprocess.run([str(cli), "generate", "contract", str(contract), "--format", "json"],
                                           capture_output=True, text=True)
                self.assertEqual(generated.returncode, 0, generated.stdout + generated.stderr)
                checked = subprocess.run(RUN + ["refine-check", str(HERE / f"examples/{name}.zal"), str(contract),
                                                "--cli", str(cli)], capture_output=True, text=True)
                self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
                result = json.loads(checked.stdout)
                self.assertEqual(result["status"], "refines")
                self.assertGreater(result["contract_check"]["checked_rows"], 0)
                self.assertEqual(result["authority"], "none")


if __name__ == "__main__":
    unittest.main()
