"""Coverage refusal tests and native boundaries; fixtures are not proof results."""

import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import check_finite_execution as gate
import check_verus as verifier
import verus_coverage as coverage


FUNCTION = """(Function (Fun :path example::identity) :mode Exec :typ_bounds ()
 :params () :ret () :require () :ensure ((> Const (Constant Bool true)))
 :d () :body (> Block () (> Const (Constant Int 7))))"""
SPEC = FUNCTION.replace("example::identity", "example::semantics").replace(":mode Exec", ":mode Spec")


class TranslatedCoverage(unittest.TestCase):
    def setUp(self):
        self.source = FUNCTION + "\n" + SPEC
        self.profile = {"namespace": "example::", "functions": coverage.inventory(self.source, "example::")}

    def test_reads_complete_records_comments_and_quoted_parentheses(self):
        self.assertEqual(coverage.require_coverage(self.source, self.profile), self.profile["functions"])
        self.assertEqual(coverage.parse_vir(';; ignored (\n(A "(;) \\"quoted\\"" (B))'),
                         [["A", '"(;) \\"quoted\\""', ["B"]]])
        for changed in ("(Function", ")", '"unfinished', "garbage", self.source + "("):
            with self.subTest(changed=changed):
                with self.assertRaises(ValueError):
                    coverage.parse_vir(changed)

    def test_refuses_omitted_weakened_or_narrowed_contracts(self):
        for changed in (
            FUNCTION.replace(":ensure ((> Const (Constant Bool true)))", ":ensure ()"),
            FUNCTION.replace("Constant Bool true", "Constant Bool false"),
            FUNCTION.replace(":require ()", ":require ((> Const (Constant Bool true)))"),
        ):
            with self.subTest(changed=changed):
                with self.assertRaisesRegex(ValueError, "contracts or specifications changed"):
                    coverage.require_coverage(changed + "\n" + SPEC, self.profile)
        # Even a matching manifest must not authorize a runtime precondition.
        narrowed = FUNCTION.replace(":require ()", ":require ((> Const (Constant Bool true)))")
        profile = {"namespace": "example::", "functions": coverage.inventory(narrowed, "example::")}
        with self.assertRaisesRegex(ValueError, "narrowed domain"):
            coverage.require_coverage(narrowed, profile)

    def test_refuses_missing_added_duplicate_or_bodyless_functions(self):
        for changed in (SPEC, self.source + FUNCTION.replace("example::identity", "example::extra")):
            with self.assertRaisesRegex(ValueError, "inventory changed"):
                coverage.require_coverage(changed, self.profile)
        with self.assertRaisesRegex(ValueError, "duplicate VIR function"):
            coverage.inventory(FUNCTION + FUNCTION, "example::")
        for changed in (FUNCTION.replace(":body (> Block () (> Const (Constant Int 7)))", ":body None"),
                        FUNCTION.replace(":d ()", ":mode Exec :d ()"),
                        FUNCTION.replace(":d ()", ":unknown () :d ()")):
            with self.assertRaises(ValueError):
                coverage.inventory(changed, "example::")

    def test_refuses_changed_mathematical_specification_or_malformed_profile(self):
        with self.assertRaisesRegex(ValueError, "contracts or specifications changed"):
            coverage.require_coverage(FUNCTION + SPEC.replace("Constant Int 7", "Constant Int 8"), self.profile)
        with self.assertRaisesRegex(ValueError, "contracts or specifications changed"):
            coverage.require_coverage(FUNCTION.replace(":ret ()", ":ret (Param :mode Exec)") + SPEC,
                                      self.profile)
        for profile in (None, {}, {"namespace": "", "functions": {}},
                        {"namespace": "example::", "functions": []}):
            with self.assertRaisesRegex(ValueError, "malformed"):
                coverage.require_coverage(self.source, profile)

    def test_runtime_inventory_and_mutation_anchors_are_explicit(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        runtime = [record for record in profile["functions"].values() if record["mode"] == "Exec"]
        self.assertEqual(len(runtime), 20)
        self.assertTrue(all(record["requires"] == 0 and record["ensures"] > 0 for record in runtime))
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                           (gate.ROOT / gate.ADMISSION).read_text())
        self.assertEqual(len(mutations), 16)
        for path, changed, expected in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
            self.assertIn(expected, ("proof", "coverage"))
        with self.assertRaisesRegex(RuntimeError, "anchor changed"):
            gate.mutation_sources("", "")

    def test_verifier_report_requires_this_units_complete_coverage(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        pin.update({key: profile[key] for key in ("expected_verified", "target_functions")})
        report = {"verus": {"commit": pin["commit"], "version": pin["version"]},
                  "verification-results": {"success": True, "errors": 0, "verified": 34,
                                           "encountered-error": False, "encountered-vir-error": False,
                                           "is-verifying-entire-crate": True},
                  "func-details": dict.fromkeys(pin["target_functions"], {})}
        self.assertTrue(verifier.accepted(report, pin))
        for field, value in (("verified", 33), ("success", False), ("is-verifying-entire-crate", False)):
            changed = copy.deepcopy(report)
            changed["verification-results"][field] = value
            self.assertFalse(verifier.accepted(changed, pin))


class NativeRuntime(unittest.TestCase):
    def test_production_source_on_application_toolchain(self):
        with tempfile.TemporaryDirectory(prefix="zeno-finite-native-") as temporary:
            executable = Path(temporary) / "tests"
            command = ["rustc", "+1.97.1", "--edition=2024", "--test",
                       "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)",
                       str(gate.ROOT / gate.HARNESS), "-o", str(executable)]
            subprocess.run(command, cwd=gate.ROOT, check=True, timeout=60)
            subprocess.run([str(executable)], cwd=gate.ROOT, check=True, timeout=60)


if __name__ == "__main__":
    unittest.main()
