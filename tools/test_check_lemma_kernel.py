#!/usr/bin/env python3
"""Fail-closed admission tests for the source-bound lemma proof gate."""
import copy
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

import check_lemma_kernel as gate


class Admission(unittest.TestCase):
    def setUp(self):
        self.pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
        self.profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        self.report = {'verification-results': {'success': True,
            'verified': self.profile['expected_verified'], 'errors': 0,
            'encountered-error': False, 'encountered-vir-error': False,
            'is-verifying-entire-crate': True},
            'verus': {k: self.pin[k] for k in ['version', 'commit']},
            'func-details': {name: {} for name in self.profile['target_functions']}}
        self.report['verus']['toolchain'] = self.pin['rust_toolchain']
        self.proc = subprocess.CompletedProcess([], 0, '', '')

    def accepted(self):
        return gate.positive_accepted(self.proc, self.report, self.pin, self.profile)

    def test_exact_unfiltered_pinned_run(self):
        self.assertTrue(self.accepted())
        for count in [0, self.profile['expected_verified']-1, self.profile['expected_verified']+1, True]:
            self.report['verification-results']['verified'] = count
            self.assertFalse(self.accepted())

    def test_partial_or_unknown_or_changed_tool_is_not_proof(self):
        for field in ['success', 'is-verifying-entire-crate']:
            with self.subTest(field=field):
                self.report['verification-results'][field] = False
                self.assertFalse(self.accepted())
                self.report['verification-results'][field] = True
        for field in ['version', 'commit', 'toolchain']:
            original = self.report['verus'][field]
            self.report['verus'][field] = 'different'
            self.assertFalse(self.accepted())
            self.report['verus'][field] = original
        self.proc.stderr = 'error: timed out'
        self.assertFalse(self.accepted())

    def test_missing_theorem_or_error_diagnostic_refuses(self):
        missing = self.profile['target_functions'][0]
        del self.report['func-details'][missing]
        self.assertFalse(self.accepted())
        self.report['func-details'][missing] = {}
        self.proc.stderr = 'error: postcondition not satisfied'
        self.assertFalse(self.accepted())
        self.proc.stderr = ''
        self.proc.returncode = 1
        self.assertFalse(self.accepted())

    def test_native_compiler_failure_or_crash_does_not_kill_control(self):
        test = 'initiation_is_required'
        stdout = f'test tests::{test} ... FAILED\ntest result: FAILED. 0 passed; 1 failed;'
        proc = subprocess.CompletedProcess([], 101, stdout, 'assertion failed: decision.is_some()')
        self.assertTrue(gate.native_failed(proc, test))
        for code in [0, 1, -11, 134]:
            proc.returncode = code
            self.assertFalse(gate.native_failed(proc, test))
        proc.returncode = 101
        proc.stderr = 'explicit panic instead of the required assertion'
        self.assertFalse(gate.native_failed(proc, test))
        proc.stderr = 'assertion failed: decision.is_some()'
        self.assertFalse(gate.native_failed(proc, 'wrong_test'))

    def test_translated_contract_omission_and_extra_body_refuse_even_with_same_count(self):
        actual = copy.deepcopy(self.profile['functions'])
        with patch.object(gate, 'inventory', return_value=actual):
            self.assertEqual(gate.check_inventory('', self.profile), actual)
            actual[gate.NAMESPACE+'decide']['ensures_sha256'] = 'weakened'
            with self.assertRaisesRegex(ValueError, 'contracts changed'):
                gate.check_inventory('', self.profile)
        actual = copy.deepcopy(self.profile['functions'])
        actual[gate.NAMESPACE+'uncontracted'] = dict(actual[gate.NAMESPACE+'decide'])
        with patch.object(gate, 'inventory', return_value=actual):
            with self.assertRaisesRegex(ValueError, 'contracts changed'):
                gate.check_inventory('', self.profile)

    def test_controls_have_one_live_production_anchor_and_named_assertions(self):
        source = (gate.ROOT / gate.SUBJECT).read_text()
        controls = gate.source_controls(source)
        self.assertEqual(set(controls), set(gate.CONTROLS))
        tests = (gate.ROOT / gate.TESTS).read_text()
        for changed, function, test in controls.values():
            self.assertNotEqual(source, changed)
            self.assertIn('fn '+test+'()', tests)
            self.assertIn('fn '+function+'(', changed)
        with self.assertRaisesRegex(RuntimeError, 'not unique'):
            gate.source_controls(source + '\n' + source)

    def test_target_region_includes_nested_loop_obligations(self):
        source = (gate.ROOT / gate.SUBJECT).read_text()
        start, end = gate.function_region(source, 'reachable')
        lines = source.splitlines()
        self.assertIn('cfg_attr', lines[start-1])
        body = '\n'.join(lines[start-1:end])
        self.assertIn('while input < k', body)
        self.assertIn('Search {', body)
        self.assertNotIn('fn bad(', body)

    def test_no_proof_escapes(self):
        for source in ['assume(true);', 'admit();', '#[verifier::external_body]',
                       'decreases *', '#[verifier::exec_allows_no_decreases_clause]']:
            self.assertIsNotNone(gate.FORBIDDEN.search(source))


if __name__ == '__main__':
    unittest.main()
