"""Independent ZAL reference binding, including native build-then-refuse controls.

Set ZAL_NATIVE_CLI to a CLI built from this source to run the native checks.
ZAL_CHECK_LOGS optionally retains every build and verdict for review.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import behavior
import behavior2
import factory

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
NATIVE = os.environ.get("ZAL_NATIVE_CLI")
NAME = "zal_whole_declared_domain_matches_authority"


class BindingTests(unittest.TestCase):
    def test_policy_indexes_cannot_change_a_revision_bound_decision(self):
        model = behavior2.parse((HERE / "examples/order2.zal").read_text())
        revision = model.revision
        before = model.decide("pending", {}, "approve", {"authorized": True})
        with self.assertRaises(TypeError):
            model._index[("pending", "approve")] = ()
        with self.assertRaises(TypeError):
            model._index[("pending", "approve")][0] = model.rules[0]
        with self.assertRaises(TypeError):
            model._ranges["new_variable"] = (0, 1)
        self.assertEqual(model.revision, revision)
        self.assertEqual(model.decide("pending", {}, "approve", {"authorized": True}), before)

    def test_inputs_and_returned_values_do_not_alias_decision_policy(self):
        model = behavior2.parse((HERE / "examples/keeper2.zal").read_text())
        values, context = model.initial_values(), {"alarm": False, "request": True}
        before = (dict(values), dict(context), model.revision)
        result = model.decide(model.initial, values, "tick", context)
        expected = model.decide(model.initial, values, "tick", context)
        result["variables"].clear()
        result["class"] = "changed"
        self.assertEqual((values, context, model.revision), before)
        self.assertEqual(model.decide(model.initial, values, "tick", context), expected)
        with self.assertRaises(behavior.Refusal):
            model.decide(model.initial, values, "not_declared", context)
        self.assertEqual((values, context, model.revision), before)


@unittest.skipUnless(NATIVE, "set ZAL_NATIVE_CLI for native generated-application controls")
class NativeBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="zal-refinement-binding-")
        cls.work = Path(cls.directory.name)
        cls.logs = Path(os.environ.get("ZAL_CHECK_LOGS", cls.work / "logs"))
        cls.logs.mkdir(parents=True, exist_ok=True)
        cls.sequence = 0

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()

    def command(self, args, label, cwd=ROOT):
        result = subprocess.run(args, cwd=cwd, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=600)
        type(self).sequence += 1
        (self.logs / f"{self.sequence:02d}-{label}.log").write_text(
            json.dumps({"command": list(map(str, args)), "exit": result.returncode}) + "\n" + result.stdout)
        return result

    def succeeds(self, args, label, cwd=ROOT):
        result = self.command(args, label, cwd)
        self.assertEqual(result.returncode, 0, result.stdout[-6000:])
        return result

    def check_variant(self, example, mutation=None):
        label = example + "-" + (mutation or "normal")
        model = behavior2.parse_any((HERE / f"examples/{example}.zal").read_text())
        files = factory.lower(model)
        work = self.work / label
        contract, app = work / "contract", work / "application"
        factory.write_new(files, contract)
        witness = {"model_revision": model.revision, "mutation": mutation,
                   "reference_sha256": hashlib.sha256(files["tests/zal-authority-examples.txt"].encode()).hexdigest()}
        policy = json.loads(files["v2/policy.json"])
        if mutation == "genesis":
            expected = policy["genesis"]["110"]
            policy["genesis"]["110"] = next(x for x in json.loads(files["zal-source-map.json"])["states"].values() if x != expected)
            witness.update(expected_initial=expected, actual_initial=policy["genesis"]["110"])
        elif mutation == "unauthorized":
            rule = next(r for r in policy["cases"] if r["rule"] == "approve_order")
            rule["when"] = rule["when"].replace("context.102.160", "true")
            witness.update(state="pending", event="approve", context={"authorized": False}, expected="Reject", actual="Accept")
        (contract / "v2/policy.json").write_text(json.dumps(policy, indent=2) + "\n")
        self.succeeds([NATIVE, "generate", "contract", str(contract), "--format", "json"], label + "-generate")
        self.succeeds([NATIVE, "new", str(app), "--contract", str(contract), "--source", str(ROOT)], label + "-new")
        table = files["tests/zal-authority-examples.txt"]
        if mutation == "missing-row":
            witness["omitted_row"] = table.splitlines()[-1]
            table = "\n".join(table.splitlines()[:-1]) + "\n"
        elif mutation == "duplicate-row":
            rows = table.splitlines()
            witness.update(omitted_row=rows[-1], repeated_row=rows[0])
            table = "\n".join([*rows[:-1], rows[0]]) + "\n"
        (app / "tests/zal-authority-examples.txt").write_text(table)
        (app / "tests/zal_correspondence.rs").write_text(files["tests/zal_correspondence.rs"])
        cargo = ["cargo", "+1.97.1"]
        self.succeeds(cargo + ["generate-lockfile", "--offline"], label + "-lock", app)
        test = cargo + ["test", "--locked", "--offline", "--test", "zal_correspondence"]
        self.succeeds(test + ["--no-run"], label + "-build", app)
        result = self.command(test + ["--", NAME, "--exact", "--nocapture"], label + "-test", app)
        if mutation:
            self.assertNotEqual(result.returncode, 0, "non-refining contract/reference passed: " + label)
            self.assertIn(NAME + " ... FAILED", result.stdout)
            if mutation == "genesis":
                self.assertIn("contract genesis differs", result.stdout)
                self.assertIn("assertion", result.stdout)
            elif mutation in {"missing-row", "duplicate-row"}:
                self.assertIn("ZAL reference table changed", result.stdout)
                self.assertIn("assertion", result.stdout)
            else:
                self.assertIn("ZAL decision differs at inputs", result.stdout)
        else:
            self.assertEqual(result.returncode, 0, result.stdout[-6000:])
        witness.update(build_exit=0, test_exit=result.returncode, named_test=NAME)
        (self.logs / (label + "-witness.json")).write_text(json.dumps(witness, indent=2) + "\n")

    def test_original_contracts_refine_both_profiles(self):
        for example in ("order", "order2"):
            with self.subTest(example=example):
                self.check_variant(example)

    def test_changed_initial_state_is_refused(self):
        for example in ("order", "order2"):
            with self.subTest(example=example):
                self.check_variant(example, "genesis")

    def test_missing_reference_row_is_refused(self):
        self.check_variant("order", "missing-row")

    def test_repeated_reference_row_is_refused(self):
        self.check_variant("order", "duplicate-row")

    def test_unauthorized_approval_is_refused(self):
        self.check_variant("order", "unauthorized")


if __name__ == "__main__":
    unittest.main()
