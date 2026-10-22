"""Test-only invalidation must still execute and refuse known bad evidence."""
import copy
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import qualification_sources as sources
import check_finite_execution as finite


class SourceBindings(unittest.TestCase):
    def setUp(self):
        self.snapshot = {'production.rs': 'p', 'native-tests.rs': 't', str(sources.POLICY): 'policy'}
        self.harness = 'proof.rs'
        self.policy = {'schema': 'zeno-fcis/qualification-source-policy/1', 'harnesses': {
            self.harness: {'production': {'production.rs': 'p'}, 'tests': ['native-tests.rs'],
                           'controls_use_tests': False}}}
        self.before = self.bound(self.snapshot)

    def bound(self, snapshot, policy=None):
        result = sources.split(snapshot, self.policy if policy is None else policy, self.harness)
        result['policy_sha256'] = 'policy'
        return result

    def test_separate_tests_keep_production_binding_and_only_reexecute_test_evidence(self):
        after = self.bound({**self.snapshot, 'native-tests.rs': 'changed'})
        self.assertEqual(after['production'], self.before['production'])
        self.assertNotEqual(after['tests'], self.before['tests'])
        self.assertEqual(sources.changed_evidence(self.before, after), ('native-tests', 'tool-tests'))
        seen = []
        checks = {name: lambda name=name: seen.append(name) for name in sources.ALL_EVIDENCE}
        results = sources.run_changed(self.before, after, checks)
        self.assertEqual(tuple(results), ('native-tests', 'tool-tests'))
        self.assertEqual(seen, ['native-tests', 'tool-tests'])

    def test_known_bad_native_specimen_is_reexecuted_and_refused(self):
        after = self.bound({**self.snapshot, 'native-tests.rs': 'removed assertion'})
        def bad_native():
            with tempfile.TemporaryDirectory() as tmp, \
                 patch.object(finite, 'snapshot', return_value={}), \
                 patch.object(finite, 'native_tests', return_value=subprocess.CompletedProcess([], 101, 'test result: FAILED', '')):
                return finite.check_native_only(Path(tmp))
        with self.assertRaisesRegex(RuntimeError, 'command failed'):
            sources.run_changed(self.before, after, {'native-tests': bad_native, 'tool-tests': lambda: None})

    def test_production_changes_new_files_and_missing_files_invalidate_all(self):
        for snapshot in ({**self.snapshot, 'production.rs': 'bad body'},
                         {**self.snapshot, 'unknown-test.rs': 'new'},
                         {'production.rs': 'p', str(sources.POLICY): 'policy'}):
            after = self.bound(snapshot)
            self.assertFalse(after['separated'])
            self.assertEqual(sources.changed_evidence(self.before, after), sources.ALL_EVIDENCE)

    def test_legacy_or_changed_policy_never_creates_a_hit(self):
        for before in ({}, None, {'source_sha256': self.snapshot}):
            self.assertEqual(sources.changed_evidence(before, self.before), sources.ALL_EVIDENCE)
        for policy in ({}, None, [], {'harnesses': []},
                       {'schema': self.policy['schema'], 'harnesses': {self.harness: {'tests': [['bad']]}}}):
            self.assertFalse(self.bound(self.snapshot, policy if policy is not None else {})['separated'])
        after = copy.deepcopy(self.before); after['policy_sha256'] = 'new'
        self.assertEqual(sources.changed_evidence(self.before, after), sources.ALL_EVIDENCE)

    def test_missing_callback_refuses_without_running_partial_evidence(self):
        after = self.bound({**self.snapshot, 'native-tests.rs': 'changed'})
        with self.assertRaisesRegex(ValueError, 'missing required'):
            sources.run_changed(self.before, after, {})

    def test_mixed_native_proof_control_requires_fresh_control_evidence(self):
        policy = copy.deepcopy(self.policy)
        policy['harnesses'][self.harness]['controls_use_tests'] = True
        before = self.bound(self.snapshot, policy)
        after = self.bound({**self.snapshot, 'native-tests.rs': 'weakened native assertion'}, policy)
        self.assertEqual(sources.changed_evidence(before, after),
                         ('proof-controls', 'native-tests', 'tool-tests'))
        def surviving_control():
            raise RuntimeError('native control survived')
        with self.assertRaisesRegex(RuntimeError, 'native control survived'):
            sources.run_changed(before, after, {'proof-controls': surviving_control,
                'native-tests': lambda: None, 'tool-tests': lambda: None})

    def test_successful_native_only_evidence_does_not_claim_a_proof_or_full_qualification(self):
        with tempfile.TemporaryDirectory() as tmp, \
             patch.object(finite, 'snapshot', return_value={}), \
             patch.object(finite, 'native_tests', return_value=subprocess.CompletedProcess([], 0, 'native ok', '')):
            receipt = finite.check_native_only(Path(tmp))
        self.assertEqual(receipt['status'], 'passed-native-only')
        self.assertFalse(receipt['qualification_complete'])
        self.assertFalse(receipt['proof_performed'])
        self.assertNotIn('verus_report', receipt)


if __name__ == '__main__': unittest.main()
