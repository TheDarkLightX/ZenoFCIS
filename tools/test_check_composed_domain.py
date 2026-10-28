"""Admission, mutation routing and statement coverage tests for the proof gate."""
import copy
import json
from pathlib import Path
import subprocess
import unittest
import check_composed_domain as gate


class SourceCoverage(unittest.TestCase):
    def test_checkpoint_controls_are_distinct_and_target_the_bridge(self):
        names = ('wrong_cut_index', 'post_member_checkpoint', 'next_member_must_continue')
        sources = {p: (gate.ROOT / p).read_text() for p in gate.PROOFS}
        controls = gate.control_sources(sources)
        specimens = []
        for name in names:
            path, changed, target = controls[name]
            self.assertEqual(path, gate.THEOREM)
            self.assertEqual(target, 'outside_cut_has_checkpoint')
            specimens.append(changed)
        self.assertEqual(len(set(specimens)), 3)

    def test_no_proof_escape_and_every_control_has_exact_anchor(self):
        sources = {p: (gate.ROOT / p).read_text() for p in gate.PROOFS}
        self.assertEqual(gate.forbidden_tokens(sources), [])
        controls = gate.control_sources(sources)
        self.assertEqual(set(controls), set(gate.CONTROLS))
        for name, (path, changed, target) in controls.items():
            self.assertNotEqual(changed, sources[path], name)
            self.assertLess(*gate.function_region(changed, target))
        for escape in ('assume(false);', 'admit();', '#[verifier::external_body]', 'pub assume_specification[foo]();',
                '#[verifier::external_type_specification]', '#[verifier::rlimit(100)]', 'unsafe {}'):
            changed = dict(sources); changed[gate.THEOREM] += '\n' + escape
            self.assertTrue(gate.forbidden_tokens(changed))
        for name, (path, anchor, _, _) in gate.CONTROLS.items():
            changed = dict(sources); changed[path] = sources[path].replace(anchor, '')
            with self.assertRaisesRegex(RuntimeError, 'anchor changed'):
                gate.control_sources(changed)

    def test_runtime_and_proof_share_both_implementations(self):
        harness = (gate.ROOT / gate.HARNESS).read_text()
        joint = (gate.ROOT / 'crates/zeno-fcis-contract/src/checked_composition/joint.rs').read_text()
        self.assertIn('checked_composition/resolve.rs', harness)
        self.assertIn('resolve::resolve(', joint)
        self.assertIn('resolve::apply(', joint)
        self.assertIn(gate.SUBJECT, gate.source_paths())
        self.assertIn(gate.SPEC, gate.source_paths())
        self.assertIn(gate.CUTS, gate.source_paths())
        self.assertIn('checked_composition/coupling_cuts.rs', harness)
        obligations = (gate.ROOT / 'crates/zeno-fcis-contract/src/checked_composition/obligations.rs').read_text()
        self.assertIn('coupling_cuts::outside_failure_cutters(', obligations)
        for name in ('coupling_cuts::interval_cutters', 'coupling_cuts::outside_failure_cutters'):
            self.assertIn(gate.NAMESPACE + name, gate.BODY_FUNCTIONS)

    def test_profile_requires_review_and_binds_bodies_and_statement(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        self.assertEqual(profile['namespace'], gate.NAMESPACE)
        self.assertEqual(gate.counts(profile['functions']), profile['inventory'])
        for name in ('resolve::resolve', 'resolve::apply'):
            record = profile['functions'][gate.NAMESPACE + name]
            self.assertEqual(record['mode'], 'Exec')
            self.assertIn('body_sha256', record)
            self.assertGreater(record['ensures'], 0)
        for name in ('theorem::joint_induction', 'theorem::couple_obligation', 'theorem::assume_obligation'):
            self.assertIn(gate.NAMESPACE + name, profile['functions'])
        gate.require_reviewed_profile(profile)
        for flag in (None, False, 0, 1, 'true'):
            with self.assertRaisesRegex(ValueError, 'independent technical review'):
                gate.require_reviewed_profile({'reviewed': flag})


class Admission(unittest.TestCase):
    def setUp(self):
        self.pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
        self.profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        self.report = {'verus': {'version': self.pin['version'], 'commit': self.pin['commit'],
            'toolchain': self.pin['rust_toolchain']}, 'verification-results': {
            'success': True, 'errors': 0, 'verified': self.profile['expected_verified'],
            'encountered-error': False, 'encountered-vir-error': False,
            'is-verifying-entire-crate': True}, 'func-details': dict.fromkeys(self.profile['target_functions'], {})}

    def test_incomplete_or_wrong_pinned_success_never_passes(self):
        accepted = lambda report, code=0, err='': gate.positive_accepted(
            subprocess.CompletedProcess([], code, '', err), report, self.pin, self.profile)
        self.assertTrue(accepted(self.report))
        for field, value in (('verified', 0), ('errors', 1), ('success', False),
                ('is-verifying-entire-crate', False)):
            changed = copy.deepcopy(self.report); changed['verification-results'][field] = value
            self.assertFalse(accepted(changed))
        for err in ('error: assertion failed', 'note: timeout', 'resource limit exceeded'):
            self.assertFalse(accepted(self.report, err=err))
        self.assertFalse(accepted(self.report, code=1))
        changed = copy.deepcopy(self.report); changed['verus']['commit'] = 'other'
        self.assertFalse(accepted(changed))

    def test_mutation_diagnostics_must_be_semantic_and_targeted(self):
        root = Path('/specimen')
        report = copy.deepcopy(self.report)
        report['verification-results'].update(success=False, errors=1, verified=self.profile['expected_verified'] - 1,
            **{'encountered-error': True, 'is-verifying-entire-crate': False})
        target = gate.THEOREM
        stderr = f'error: assertion failed\n  --> {target}:10:4\nerror: aborting due to 1 previous error\n'
        proc = subprocess.CompletedProcess([], 1, '', stderr)
        self.assertTrue(gate.targeted_refusal(proc, report, self.pin, self.profile, target, root, (8, 15)))
        for changed in (stderr.replace('assertion failed', 'cannot find function'),
                stderr.replace(':10:4', ':99:4'), stderr + 'timeout\n'):
            proc = subprocess.CompletedProcess([], 1, '', changed)
            self.assertFalse(gate.targeted_refusal(proc, report, self.pin, self.profile, target, root, (8, 15)))

if __name__ == '__main__':
    unittest.main()
