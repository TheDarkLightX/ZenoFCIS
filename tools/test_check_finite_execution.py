"""Coverage refusal tests and native boundaries; fixtures are not proof results."""

import copy
import json
import os
from pathlib import Path
import tempfile
import subprocess
import unittest

import check_finite_execution as gate
import check_verus as verifier
import verus_coverage as coverage

# All three gates share these mocked diagnostic-retention regressions.
from test_negative_retention import NegativeRetentionTests, StrictGateMutationTests


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

    def test_source_identity_log_capacity_remains_bounded(self):
        # Embedded complete source arrays can exceed the old eight-MiB bound.
        comment = ";;" + "x" * (8 * 1024 * 1024) + "\n"
        self.assertEqual(coverage.parse_vir(comment + "(A)"), [["A"]])
        with self.assertRaisesRegex(ValueError, "size limit"):
            coverage.parse_vir(" " * (coverage.MAX_VIR_CHARACTERS + 1))

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
        self.assertEqual(len(runtime), 641)
        executable_names = {n for n, r in profile['functions'].items() if r['mode'] == 'Exec'}
        self.assertEqual(executable_names, set(profile['body_covered_functions']))
        self.assertTrue(all('body_sha256' in profile['functions'][n] for n in executable_names))
        self.assertIn('finite_execution::value::scalar::mul_div', executable_names)
        self.assertIn('finite_execution::value::wide::products_ge', executable_names)
        self.assertTrue(all(record["requires"] == 0 and record["ensures"] > 0 for record in runtime))
        mutations = gate.mutation_sources(*((gate.ROOT / path).read_text() for path in
                                            (gate.SUBJECT, gate.ADMISSION, gate.TYPED, gate.VALUE)))
        self.assertEqual(len(mutations), 24)
        for path, changed, expected in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
            self.assertIn(expected, ("proof", "coverage"))
        with self.assertRaisesRegex(RuntimeError, "anchor changed"):
            gate.mutation_sources("", "", "", "")

    def test_every_proof_control_names_its_actual_native_function(self):
        controls = gate.mutation_sources(*((gate.ROOT / path).read_text() for path in
                                          (gate.SUBJECT, gate.ADMISSION, gate.TYPED, gate.VALUE)))
        self.assertEqual(set(gate.SEMANTIC_TARGETS),
                         {name for name, row in controls.items() if row[2] == 'proof'})
        for name, target in gate.SEMANTIC_TARGETS.items():
            start, end = verifier.function_lines(controls[name][1], target)
            self.assertLess(start, end, name)

    def test_loop_entry_refusal_keeps_function_pin_and_operational_guards(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        controls = gate.mutation_sources(*((gate.ROOT / p).read_text() for p in
            (gate.SUBJECT, gate.ADMISSION, gate.TYPED, gate.VALUE)))
        name = 'skip_last_eager_node'
        path, changed, _ = controls[name]
        first, last = verifier.function_lines(changed, gate.SEMANTIC_TARGETS[name])
        report = {'verus': {k: pin[k] for k in ('version', 'commit')},
            'verification-results': {'success': False, 'errors': 1, 'verified': 1160,
                'encountered-error': True, 'encountered-vir-error': False,
                'is-verifying-entire-crate': True},
            'func-details': dict.fromkeys(profile['target_functions'], {})}
        report['verus']['toolchain'] = pin['rust_toolchain']
        raw = (f'error: invariant not satisfied before loop\n --> {path}:{first}:1\n'
               'error: aborting due to 1 previous error\n')
        def classify(stderr, code=1):
            proc = subprocess.CompletedProcess([], code, json.dumps(report), stderr)
            answer = gate.proof_rejected(proc, report, pin, profile, name,
                path, changed, gate.ROOT)
            self.assertEqual(proc.stderr, stderr)
            return answer
        self.assertTrue(classify(raw))
        for wrong in (raw.replace(str(path), 'unrelated.rs'),
                raw.replace(f':{first}:1', f':{last + 1}:1'),
                raw.replace('before loop', 'before loop unrelated'),
                raw + 'note: timeout\n', raw + 'note: Resource limit exceeded\n',
                raw.replace('1 previous', '2 previous')):
            self.assertFalse(classify(wrong), wrong)
        for code in (0, 101, 124, -9):
            self.assertFalse(classify(raw, code))

    def test_coverage_fault_needs_the_exact_planned_function_inventory(self):
        target = 'finite_execution::evaluation::evaluate_into'
        extra = 'finite_execution::evaluation::unchecked_identity'
        def function(name, ensure='true'):
            return (f'(Function (Fun :path {name}) :mode Exec :typ_bounds () '
                    ':params () :ret () :require () '
                    f':ensure ((Bool {ensure})) :d () :body (Int 0))')
        original = function(target)
        profile = {'namespace': 'finite_execution::', 'body_covered_functions': [target],
                   'functions': coverage.inventory(original, 'finite_execution::', (target,))}
        unchecked = function(extra).replace(':ensure ((Bool true))', ':ensure ()')
        self.assertTrue(gate.coverage_control(original + unchecked, profile,
                                              'add_uncontracted_function')[0])
        self.assertTrue(gate.coverage_control(function(target, 'false'), profile,
                                              'weaken_execution_contract')[0])
        for vir in (original, original + function(extra),
                    original + unchecked + function('finite_execution::unexpected')):
            self.assertFalse(gate.coverage_control(vir, profile, 'add_uncontracted_function')[0])
        self.assertFalse(gate.coverage_control(function(target, 'false') + unchecked,
                                              profile, 'weaken_execution_contract')[0])

    def test_intended_contract_fault_survives_an_earlier_descriptor_diagnostic(self):
        target = 'finite_execution::evaluation::evaluate_into'
        def function(ensures):
            return (f'(Function (Fun :path {target}) :mode Exec :typ_bounds () '
                ':params () :ret () :require () '
                f':ensure {ensures} :d () :body (Int 0))')
        original = function('((Bool true))')
        profile = {'namespace': 'finite_execution::', 'body_covered_functions': [target],
            'functions': coverage.inventory(original, 'finite_execution::', (target,))}
        from unittest import mock
        with mock.patch.object(gate, 'require_coverage',
                side_effect=ValueError('translated descriptor declarations changed')):
            self.assertTrue(gate.coverage_control(function('()'), profile,
                'omit_execution_contract')[0])
            self.assertTrue(gate.coverage_control(function('((Bool false))'), profile,
                'weaken_execution_contract')[0])
            self.assertFalse(gate.coverage_control(original, profile,
                'omit_execution_contract')[0])
            self.assertFalse(gate.coverage_control(original, profile,
                'weaken_execution_contract')[0])

    def test_verifier_report_requires_this_units_complete_coverage(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        pin.update({key: profile[key] for key in ("expected_verified", "target_functions")})
        report = {"verus": {"commit": pin["commit"], "version": pin["version"]},
                  "verification-results": {"success": True, "errors": 0, "verified": pin["expected_verified"],
                                           "encountered-error": False, "encountered-vir-error": False,
                                           "is-verifying-entire-crate": True},
                  "func-details": dict.fromkeys(pin["target_functions"], {})}
        self.assertTrue(verifier.accepted(report, pin))
        for field, value in (("verified", pin["expected_verified"] - 1), ("success", False), ("is-verifying-entire-crate", False)):
            changed = copy.deepcopy(report)
            changed["verification-results"][field] = value
            self.assertFalse(verifier.accepted(changed, pin))


class NativeRuntime(unittest.TestCase):
    def test_production_source_on_application_toolchain(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        with tempfile.TemporaryDirectory(prefix="zeno-finite-native-") as temporary:
            native = gate.native_tests(Path(temporary), pin, dict(os.environ))
        self.assertRegex(native.stdout, r"test result: ok\. \d+ passed; 0 failed")


if __name__ == "__main__":
    unittest.main()
