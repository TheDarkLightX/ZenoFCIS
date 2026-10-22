"""File-level lowering and native refinement over shipped finite ZAL examples."""

import dataclasses
import json
import os
from pathlib import Path
import re
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
        for name in ["order2", "context_free2", "revoked_mail2"]:
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

    def test_safety_properties_keep_their_laws_and_reject_frame(self):
        model = behavior2.parse((HERE / "examples/revoked_mail2.zal").read_text())
        self.assertEqual(behavior2.check(model)["status"], "pass-with-scope")
        files = factory.lower(model)
        laws = json.loads(files["zal-source-map.json"])["property_laws"]
        self.assertEqual(laws, {"no_new_mail_after_revoke": [540], "reads_stay_after_revoke": [541, 542]})
        self.assertIn("property_no_new_mail_after_revoke_accept on accept", files["project.zeno"])
        self.assertIn("property_reads_stay_after_revoke_reject on reject", files["project.zeno"])
        # The law for rejections must read the unchanged pre-state, never post.
        reject = next(line for line in files["project.zeno"].splitlines() if line.startswith("law 542 "))
        self.assertNotIn("post.", reject)
        self.assertEqual(factory.lower(behavior2.parse(model.render(True), "english")), files)

    def test_reachable_safety_cannot_silently_strengthen_unreachable_behavior(self):
        source = (HERE / "examples/revoked_mail2.zal").read_text()
        source = source.replace("rule revoke_access: active + revoke [true] -> accept revoked;\n", "")
        source += "rule unreachable_read: revoked + read_mail [reads < 2] -> accept revoked, reads += 1;\n"
        model = behavior2.parse(source)
        self.assertEqual(behavior2.check(model)["status"], "pass-with-scope")
        with self.assertRaisesRegex(behavior2.Refusal, "no_new_mail_after_revoke does not hold on every admissible step"):
            factory.lower(model)


def weaken_revocation(files, remove_laws):
    """A concrete contract that offers a new read from a revoked device."""
    files = dict(files)
    policy = json.loads(files["v2/policy.json"])
    policy["cases"].insert(0, {"rule": "read_revoked", "when":
        "pre.100.110 == 151 && command.101 == 120 && pre.100.170 < 2",
        "class": "Accept", "reason": None, "post": {"110": 151, "170": "pre.100.170 + 1"}, "outbox": []})
    files["v2/policy.json"] = json.dumps(policy, indent=2) + "\n"
    if remove_laws:
        files["project.zeno"] = re.sub(r"(law 54[0-2] .*? = ).*;", r"\1true;", files["project.zeno"])
    return files


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

    def test_revoked_mail_contract_refines(self):
        cli = Path(os.environ["ZAL_LOWERING_TEST_CLI"])
        model = behavior2.parse((HERE / "examples/revoked_mail2.zal").read_text())
        files = factory.lower(model)
        # The control runs this same assertion with a buildable weakened contract.
        if os.environ.get("ZAL_LOWERING_CONTROL") == "weaken-revocation":
            files = weaken_revocation(files, remove_laws=True)
        with tempfile.TemporaryDirectory() as tmp:
            contract = Path(tmp) / "contract"
            factory.write_new(files, contract)
            generated = subprocess.run([str(cli), "generate", "contract", str(contract), "--format", "json"],
                                       capture_output=True, text=True)
            self.assertEqual(generated.returncode, 0, generated.stdout + generated.stderr)
            result, code = factory.refine_check(model, contract, cli)
            print(json.dumps(result, sort_keys=True), flush=True)
            self.assertEqual((code, result["status"]), (0, "refines"), result)

    def test_weakened_revocation_is_a_native_counterexample(self):
        cli = Path(os.environ["ZAL_LOWERING_TEST_CLI"])
        model = behavior2.parse((HERE / "examples/revoked_mail2.zal").read_text())
        for remove_laws in [False, True]:
            with self.subTest(remove_laws=remove_laws), tempfile.TemporaryDirectory() as tmp:
                contract = Path(tmp) / "contract"
                factory.write_new(weaken_revocation(factory.lower(model), remove_laws), contract)
                result, code = factory.refine_check(model, contract, cli)
                self.assertEqual((code, result["status"]), (1, "counterexample"), result)
                witness = result["contract_check"]["witness"]
                self.assertEqual((witness["state"], witness["input"]), (3, 0))


if __name__ == "__main__":
    unittest.main()
