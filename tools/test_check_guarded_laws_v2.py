"""Fail-closed acceptance tests for the guarded-law gate itself."""
import copy
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import check_guarded_laws_v2 as gate
from verus_coverage import inventory, require_coverage


class GuardedLawCoverage(unittest.TestCase):
    def test_whole_registered_closure_has_exact_total_contracts_and_raw_bodies(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        functions = profile['functions']
        runtime = {name for name, record in functions.items() if record['mode'] == 'Exec'}
        self.assertEqual(profile['namespace'], 'guarded_laws::')
        self.assertEqual(set(profile['body_covered_functions']), runtime)
        self.assertTrue(all(record['requires'] == 0 and record['ensures'] > 0 for name, record in functions.items() if name in runtime))
        self.assertTrue(all('body_sha256' in record for record in functions.values() if record['mode'] in ('Exec', 'Spec')))
        for suffix in ['laws::predicate::node', 'laws::predicate::shape', 'laws::predicate::default_text_valid',
            'laws::evaluate_into', 'laws::evaluate', 'laws::frame::observe',
            'authority::metadata::law_op', 'authority::metadata::law_atom',
            'authority::policy::policy_bytes',
            'composition::outcome::transition_run', 'composition::outcome::genesis_run']:
            self.assertTrue(any(name.endswith(suffix) for name in runtime), suffix)
        for suffix in ['laws::spec::node', 'laws::spec::read_observation', 'laws::spec::default_valid', 'laws::spec::default_text_valid',
            'authority::metadata_spec::law_op', 'authority::evaluator::EVALUATOR']:
            self.assertTrue(any(name.endswith(suffix) for name in functions), suffix)

    def test_all_legacy_controls_remain_and_new_controls_have_precise_targets(self):
        controls = gate.controls()
        old = gate.legacy.mutation_sources()
        self.assertEqual(len(old), 28)
        self.assertEqual(len(controls), 43)
        self.assertEqual(sum(c['kind'] == 'proof' for c in controls.values()), 37)
        self.assertEqual(sum(c['kind'] == 'coverage' for c in controls.values()), 5)
        self.assertEqual([n for n,c in controls.items() if c['kind'] == 'native'], ['shrink_delivery_ordinals'])
        for name, (path, changed, kind) in old.items():
            self.assertEqual((controls[name]['path'], controls[name]['changed']), (path, changed))
            self.assertEqual(controls[name]['kind'], 'native' if name == 'shrink_delivery_ordinals' else kind)
        for control in controls.values():
            self.assertNotEqual(control['changed'], (gate.ROOT / control['path']).read_text())
            if control['kind'] == 'proof':
                first, last = gate.target_lines(control['changed'], control['target'])
                self.assertGreater(last, first)
            elif control['kind'] == 'coverage':
                self.assertIn(control['field'], ('body_sha256', 'ensures_sha256', 'requires_sha256', 'inventory'))

    def test_native_ordinal_control_requires_exact_independent_before_and_after(self):
        baseline = SimpleNamespace(returncode=0, stdout=f'test {gate.ORDINAL_TEST} ... ok\n')
        output = (f'running 1 test\ntest {gate.ORDINAL_TEST} ... FAILED\n'
            'laws/tests.rs:799:9: assertion left == right failed\n'
            '  left: Err(Frame)\n right: Ok(())\n'
            'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured\n')
        negative = SimpleNamespace(returncode=101, stdout=output)
        self.assertTrue(gate.native_ordinal_refusal(baseline, negative)[0])
        self.assertFalse(gate.native_ordinal_refusal(SimpleNamespace(returncode=1, stdout=baseline.stdout), negative)[0])
        self.assertFalse(gate.native_ordinal_refusal(baseline, SimpleNamespace(returncode=0, stdout=output))[0])
        for bad in [output.replace(gate.ORDINAL_TEST, 'unrelated_test'),
            output.replace('Err(Frame)', 'Err(Undefined)'), output.replace('Ok(())', 'Err(Budget)'),
            output.replace('0 passed; 1 failed;', '1 passed; 1 failed;'), 'resource limit exceeded']:
            self.assertFalse(gate.native_ordinal_refusal(baseline, SimpleNamespace(returncode=101, stdout=bad))[0])

    def test_compile_resource_and_unrelated_failures_cannot_be_semantic_kills(self):
        c = gate.controls()['guard_direction']
        first, _ = gate.target_lines(c['changed'], c['target'])
        diagnostic = f'error: postcondition not satisfied\n --> {c["path"]}:{first}:1\n'
        proc = SimpleNamespace(returncode=1, stderr=diagnostic)
        report = {'verification-results': {'success': False, 'errors': 1, 'encountered-vir-error': False}}
        self.assertTrue(gate.semantic_refusal(proc, report, c)[0])
        for extra in ['error[E0004]: non-exhaustive pattern', 'resource limit exceeded', 'timed out', 'out of memory']:
            self.assertFalse(gate.semantic_refusal(SimpleNamespace(returncode=1, stderr=diagnostic + extra), report, c)[0])
        for result in [dict(success=True, errors=0, **{'encountered-vir-error': False}),
            dict(success=False, errors=1, **{'encountered-vir-error': True})]:
            self.assertFalse(gate.semantic_refusal(proc, {'verification-results': result}, c)[0])
        self.assertFalse(gate.semantic_refusal(SimpleNamespace(returncode=1, stderr='error: postcondition not satisfied\n --> unrelated.rs:1:1'), report, c)[0])

    def test_raw_coverage_refuses_operational_identifier_change_and_omission(self):
        # This is a parser/gate test, not a claimed Verus semantic specimen.
        vir = '(Function (Fun :path guarded_laws::execution_v2::laws::predicate::node) :mode Exec :typ_bounds () :params () :ret () :require () :ensure ((Bool true)) :d () :body (ReadAttempt 41))'
        name = 'guarded_laws::execution_v2::laws::predicate::node'
        profile = {'namespace': gate.NAMESPACE, 'functions': inventory(vir, gate.NAMESPACE, (name,)), 'body_covered_functions': [name]}
        require_coverage(vir, profile)
        c = {'target': 'laws::predicate::node', 'field': 'body_sha256'}
        self.assertTrue(gate.coverage_refusal(vir.replace('ReadAttempt 41', 'ReadAttempt 42'), profile, c)[0])
        weakened = vir.replace(':ensure ((Bool true))', ':ensure ()')
        with self.assertRaises(ValueError):
            require_coverage(weakened, profile)
        wrong = copy.deepcopy(profile)
        wrong['body_covered_functions'] = []
        with self.assertRaises(ValueError):
            require_coverage(vir, wrong)

    def test_source_manifest_includes_actual_metadata_getter_test_and_original_graph(self):
        paths = set(gate.sources())
        self.assertTrue({gate.HARNESS, gate.PROFILE, gate.TEST, gate.PREDICATE,
            gate.METADATA, gate.authority.EVALUATOR}.issubset(paths))
        self.assertIn(Path('crates/zeno-fcis-cli/templates/inventory-reservation/synthesized/program.zcve'), paths)
        self.assertEqual(set(gate.snapshot()), set(map(str, paths)))
        with patch.object(Path, 'is_symlink', return_value=True):
            with self.assertRaisesRegex(ValueError, 'symlink'):
                gate.snapshot()


if __name__ == '__main__':
    unittest.main()
