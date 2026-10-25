"""Native/API and protocol tests; simulated records are not proof results."""
import copy
import json
from pathlib import Path
import subprocess
import os
from pathlib import Path
import tempfile
import unittest
from test_negative_retention import StrictGateMutationTests

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
        self.assertEqual(len(profile["functions"]), 1609)
        # Six checked helpers for payload-free sums and reason-scoped laws joined in 2.6.
        self.assertEqual(len(runtime), 653)
        self.assertEqual(set(profile["body_covered_functions"]), runtime)
        self.assertTrue(all(record["requires"] == 0 and record["ensures"] > 0
                            for record in profile["functions"].values() if record["mode"] == "Exec"))
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
            (gate.ROOT / gate.METER).read_text(), (gate.ROOT / gate.SPEC).read_text(),
            (gate.ROOT / gate.RESOURCE).read_text(), (gate.ROOT / gate.NUMERIC).read_text())
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
        self.assertIn("test result: ok. 195 passed; 0 failed", evidence["test_output"])
        # Retain the 2.5 corpus and A63's additional reason-scope admission check.
        for test in ('independent_wide_oracle_matches_fraction_corpora',
                     'variant_oracle_rejects_unrepresentable_cardinalities_without_overflow',
                     'signed_floor_law_uses_shared_node_with_eager_usage_and_retry',
                     'reason_subset_skips_other_reasons_and_refuses_output_observations'):
            self.assertIn(test + ' ... ok', evidence['test_output'])
        self.assertIn("new_arithmetic_matches_independent_oracle_at_all_step_boundaries ... ok", evidence["test_output"])
        self.assertEqual(evidence["api_consumers"]["positive"]["exit_code"], 0)
        self.assertEqual(len(evidence["api_consumers"]), 5)
        self.assertTrue(all(record["exit_code"] != 0 for name, record in evidence["api_consumers"].items()
                            if name != "positive"))


class MeteredRefusal(unittest.TestCase):
    def setUp(self):
        self.pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        self.path = gate.NUMERIC
        self.changed = '#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\npub fn evaluate() {}\n'
        self.profile = {"namespace": "example::", "target_functions": ["example::identity"],
            "body_covered_functions": ["example::identity"],
            "functions": coverage.inventory(FUNCTION + "\n" + SPEC, "example::", ("example::identity",))}
        self.report = {"verus": {"version": self.pin["version"], "commit": self.pin["commit"],
            "toolchain": self.pin["rust_toolchain"]}, "func-details": {"example::identity": {}},
            "verification-results": {"success": False, "errors": 1, "verified": 1,
                "encountered-error": True, "encountered-vir-error": False, "is-verifying-entire-crate": True}}
        self.stderr = f'error: postcondition not satisfied\n --> {self.path}:2:1\nerror: aborting due to 1 previous error\n'

    def classify(self, stderr=None, report=None, code=1, expected="proof", vir=None, name="omit_step_charge"):
        report = self.report if report is None else report
        proc = subprocess.CompletedProcess([], code, json.dumps(report), self.stderr if stderr is None else stderr)
        return gate.classify_mutation(proc, report, name, self.path, self.changed,
            Path("/private-specimen"), self.pin, self.profile, expected=expected, vir=vir)[0]

    def test_complete_intended_refusal_and_mixed_errors(self):
        for name in ("omit_step_charge", "charge_wrong_resource"):
            with self.subTest(name=name):
                self.assertTrue(self.classify(name=name))
            for extra in ("note: Resource limit (rlimit) exceeded\n", "note: UNKNOWN\n", "note: timed out\n",
                          "error[E0308]: mismatched types\n --> unrelated.rs:2:1\n",
                          "error: assertion failed\n --> unrelated.rs:2:1\n"):
                with self.subTest(name=name, extra=extra):
                    self.assertFalse(self.classify(self.stderr + extra, name=name))
        for stderr in (self.stderr.replace(str(self.path), "unrelated.rs"),
                       self.stderr.replace(":2:1", ":9:1"), self.stderr.replace("1 previous", "2 previous")):
            self.assertFalse(self.classify(stderr))

    def test_partial_malformed_and_unpinned_reports_refuse(self):
        for report in ({}, {"verification-results": {}}):
            self.assertFalse(self.classify(report=report))
        for field, value in (("is-verifying-entire-crate", False), ("encountered-error", False),
                             ("encountered-vir-error", True), ("verified", 0)):
            report = copy.deepcopy(self.report); report["verification-results"][field] = value
            self.assertFalse(self.classify(report=report))
        report = copy.deepcopy(self.report); report["verus"]["commit"] = "wrong"
        self.assertFalse(self.classify(report=report))
        report = copy.deepcopy(self.report); report["func-details"] = {}
        self.assertFalse(self.classify(report=report))
        for code in (None, True, 0, 2, 137):
            self.assertFalse(self.classify(code=code))

    def test_coverage_requires_complete_success_and_clean_diagnostics(self):
        # Name the declared operational target; unrelated body drift is not a kill.
        target = "example::execution_v2::numeric::evaluate"
        function = FUNCTION.replace("example::identity", target)
        self.profile = {"namespace": "example::", "target_functions": [target],
            "body_covered_functions": [target],
            "functions": coverage.inventory(function + "\n" + SPEC, "example::", (target,))}
        report = copy.deepcopy(self.report)
        report["func-details"] = {target: {}}
        report["verification-results"].update(success=True, errors=0, **{"encountered-error": False})
        vir = function.replace("Constant Int 7", "Constant Int 8") + "\n" + SPEC
        name = "execute_before_charging"
        self.assertTrue(self.classify("", report, 0, "coverage", vir, name))
        unrelated = function + "\n" + SPEC.replace("Constant Int 7", "Constant Int 8")
        self.assertFalse(self.classify("", report, 0, "coverage", unrelated, name))
        for stderr in ("note: Resource limit (rlimit) exceeded\n", "note: UNKNOWN\n",
                       "error[E0308]: mismatched types\n", self.stderr):
            self.assertFalse(self.classify(stderr, report, 0, "coverage", vir, name))
        report["verification-results"]["is-verifying-entire-crate"] = False
        self.assertFalse(self.classify("", report, 0, "coverage", vir, name))



if __name__ == "__main__":
    unittest.main()
