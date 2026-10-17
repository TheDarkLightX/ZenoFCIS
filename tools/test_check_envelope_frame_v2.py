"""Coverage and control wiring; these fixtures are not proof receipts."""
import copy
import json
from pathlib import Path
import subprocess
import unittest
from test_negative_retention import StrictGateMutationTests

import check_envelope_frame_v2 as gate
import check_verus as verifier
from test_check_finite_execution import FUNCTION, SPEC
import verus_coverage as coverage
# The shared classifier's parser regression uses this gate's retained hosted diagnostic.
from test_verus_contract_controls import ContractClassifier, assert_strict_classification  # noqa: F401
from verus_coverage import require_coverage


class EnvelopeCoverage(unittest.TestCase):
    def test_each_executable_has_total_exact_contract_and_body_guard(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        executable = {name for name, row in profile["functions"].items()
                      if row["mode"] == "Exec"}
        self.assertEqual(executable, set(profile["body_covered_functions"]))
        self.assertTrue(all(row["requires"] == 0 and row["ensures"] > 0
                            for row in profile["functions"].values()
                            if row["mode"] == "Exec"))
        self.assertTrue(any(name.endswith("::frame") for name in executable))

    def test_controls_are_distinct_and_all_dependency_files_are_bound(self):
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                          (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(len(mutations), 13)
        self.assertEqual(sum(kind == "proof" for _, _, kind in mutations.values()), 8)
        self.assertEqual(sum(kind == "proof_contract" for _, _, kind in mutations.values()), 3)
        for path, changed, _ in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
        self.assertEqual(set(gate.snapshot()), set(map(str, gate.SOURCES)))
        canonical_files = {str(path.relative_to(gate.ROOT))
                           for path in (gate.ROOT / gate.SUBJECT.parent).rglob("*.rs")}
        self.assertLessEqual(canonical_files, set(map(str, gate.UNIT_SOURCES)))
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        with self.assertRaises(ValueError):
            require_coverage("", profile)

    def test_frame_contract_controls_change_only_that_contract_and_classify_strictly(self):
        source = (gate.ROOT / gate.SUBJECT).read_text()
        end = source.index("pub fn frame<")
        start = source.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
        self.assertIn("== spec::frame(bytes@, root_type, schema_hash@, max_bytes),", source[start:end])
        mutations = gate.mutation_sources(source, (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(set(gate.CONTRACT_REJECTIONS),
                         {name for name, (_, _, kind) in mutations.items() if kind == "proof_contract"})
        for name, expected in gate.CONTRACT_REJECTIONS.items():
            path, changed, _ = mutations[name]
            self.assertEqual(path, gate.SUBJECT)
            self.assertTrue(changed.startswith(source[:start]) and changed.endswith(source[end:]), name)
            self.assertNotEqual(changed, source)
            assert_strict_classification(self, expected)


class EnvelopeRefusal(unittest.TestCase):
    def setUp(self):
        self.pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        self.path = gate.SUBJECT
        self.changed = '#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\npub fn frame() {}\n'
        self.profile = {"namespace": "example::", "target_functions": ["example::identity"],
            "body_covered_functions": ["example::identity"],
            "functions": coverage.inventory(FUNCTION + "\n" + SPEC, "example::", ("example::identity",))}
        self.report = {"verus": {"version": self.pin["version"], "commit": self.pin["commit"],
            "toolchain": self.pin["rust_toolchain"]}, "func-details": {"example::identity": {}},
            "verification-results": {"success": False, "errors": 1, "verified": 1,
                "encountered-error": True, "encountered-vir-error": False, "is-verifying-entire-crate": True}}
        self.stderr = f'error: postcondition not satisfied\n --> {self.path}:2:1\nerror: aborting due to 1 previous error\n'

    def classify(self, stderr=None, report=None, code=1, expected="proof", vir=None, name="refuse_equal_size_limit"):
        report = self.report if report is None else report
        proc = subprocess.CompletedProcess([], code, json.dumps(report), self.stderr if stderr is None else stderr)
        return gate.classify_mutation(proc, report, name, self.path, self.changed,
            Path("/private-specimen"), self.pin, self.profile, expected=expected, vir=vir)[0]

    def test_complete_intended_refusal_and_mixed_errors(self):
        self.assertTrue(self.classify())
        for extra in ("note: Resource limit (rlimit) exceeded\n", "note: UNKNOWN\n", "note: timed out\n",
                      "error[E0308]: mismatched types\n --> unrelated.rs:2:1\n",
                      "error: assertion failed\n --> unrelated.rs:2:1\n"):
            with self.subTest(extra=extra):
                self.assertFalse(self.classify(self.stderr + extra))
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
        # Cover the declared frame specification, not an arbitrary synthetic body.
        spec = SPEC.replace("example::semantics", "example::envelope::spec::frame")
        self.profile = {"namespace": "example::", "target_functions": ["example::identity"],
            "body_covered_functions": ["example::identity"],
            "functions": coverage.inventory(FUNCTION + "\n" + spec, "example::", ("example::identity",))}
        report = copy.deepcopy(self.report)
        report["verification-results"].update(success=True, errors=0, **{"encountered-error": False})
        vir = FUNCTION + "\n" + spec.replace("Constant Int 7", "Constant Int 8")
        self.assertTrue(self.classify("", report, 0, "coverage", vir))
        unrelated = FUNCTION.replace("Constant Int 7", "Constant Int 8") + "\n" + spec
        self.assertFalse(self.classify("", report, 0, "coverage", unrelated))
        for stderr in ("note: Resource limit (rlimit) exceeded\n", "note: UNKNOWN\n",
                       "error[E0308]: mismatched types\n", self.stderr):
            self.assertFalse(self.classify(stderr, report, 0, "coverage", vir))
        report["verification-results"]["is-verifying-entire-crate"] = False
        self.assertFalse(self.classify("", report, 0, "coverage", vir))



if __name__ == "__main__":
    unittest.main()
