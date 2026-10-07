"""Native/API and protocol tests; simulated records are not proof results."""
import json
import os
from pathlib import Path
import tempfile
import unittest

import check_metered_execution as gate
import check_verus as verifier
from test_check_finite_execution import FUNCTION, SPEC
import verus_coverage as coverage


class MeteredCoverage(unittest.TestCase):
    def setUp(self):
        self.source = FUNCTION + "\n" + SPEC
        self.profile = {"namespace": "example::", "body_covered_functions": ["example::identity"],
                        "functions": coverage.inventory(self.source, "example::", ("example::identity",))}

    def test_operational_body_changes_are_refused_without_changing_contracts(self):
        self.assertEqual(coverage.require_coverage(self.source, self.profile), self.profile["functions"])
        changed = FUNCTION.replace("Constant Int 7", "Constant Int 8") + "\n" + SPEC
        with self.assertRaisesRegex(ValueError, "contracts or specifications changed"):
            coverage.require_coverage(changed, self.profile)
        # The original extensional profile intentionally permits body changes.
        extensional = {"namespace": "example::", "functions": coverage.inventory(self.source, "example::")}
        coverage.require_coverage(changed, extensional)

    def test_operational_coverage_is_strict_and_cannot_silently_drop_a_body(self):
        for body_functions in (None, "example::identity", ["example::identity"] * 2,
                               ["example::missing"], ["example::semantics"]):
            profile = {**self.profile, "body_covered_functions": body_functions}
            with self.assertRaisesRegex(ValueError, "operational body coverage"):
                coverage.require_coverage(self.source, profile)
        with self.assertRaisesRegex(ValueError, "contracts or specifications changed"):
            coverage.require_coverage(self.source, {**self.profile, "body_covered_functions": []})

    def test_reviewed_inventory_and_mutation_anchors_are_complete(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        runtime = {name for name, record in profile["functions"].items() if record["mode"] == "Exec"}
        self.assertEqual(len(profile["functions"]), 1085)
        self.assertEqual(len(runtime), 468)
        self.assertEqual(set(profile["body_covered_functions"]), runtime)
        self.assertTrue(all(record["requires"] == 0 and record["ensures"] > 0
                            for record in profile["functions"].values() if record["mode"] == "Exec"))
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
            (gate.ROOT / gate.METER).read_text(), (gate.ROOT / gate.SPEC).read_text(),
            (gate.ROOT / gate.RESOURCE).read_text())
        self.assertEqual(len(mutations), 16)
        self.assertEqual(mutations["execute_before_charging"][2], "coverage")
        self.assertEqual(mutations["inject_caller_usage_argument"][2], "coverage")
        for path, changed, expected in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
            self.assertIn(expected, ("proof", "coverage"))

    def test_simulated_report_requires_the_exact_unit(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        pin.update({key: profile[key] for key in ("expected_verified", "target_functions")})
        report = {"verus": {"commit": pin["commit"], "version": pin["version"]},
                  "verification-results": {"success": True, "errors": 0, "verified": profile["expected_verified"],
                    "encountered-error": False, "encountered-vir-error": False, "is-verifying-entire-crate": True},
                  "func-details": dict.fromkeys(pin["target_functions"], {})}
        self.assertTrue(verifier.accepted(report, pin))
        report["func-details"].pop(pin["target_functions"][-1])
        self.assertFalse(verifier.accepted(report, pin))


class NativeMeteredExecution(unittest.TestCase):
    def test_actual_source_and_external_api_refusals(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        with tempfile.TemporaryDirectory(prefix="zeno-metered-native-") as temporary:
            # The full gate already owns this name for its positive VIR logs.
            (Path(temporary) / "positive").mkdir()
            evidence = gate.native_checks(Path(temporary), pin, dict(os.environ))
        self.assertIn("test result: ok. 128 passed; 0 failed", evidence["test_output"])
        self.assertEqual(evidence["api_consumers"]["positive"]["exit_code"], 0)
        self.assertEqual(len(evidence["api_consumers"]), 5)
        self.assertTrue(all(record["exit_code"] != 0 for name, record in evidence["api_consumers"].items()
                            if name != "positive"))


if __name__ == "__main__":
    unittest.main()
