"""Guard coverage and safe API admission; fixtures are not proof results."""
import json
import os
from pathlib import Path
import tempfile
import unittest

import check_record_execution as gate
import check_verus as verifier


class RecordExecutionCoverage(unittest.TestCase):
    def test_complete_shared_closure_and_real_mutation_anchors(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        runtime = {name for name, record in profile['functions'].items() if record['mode'] == 'Exec'}
        self.assertEqual(len(profile['functions']), 182)
        self.assertEqual(len(runtime), 81)
        self.assertEqual(set(profile['body_covered_functions']), runtime)
        self.assertTrue(all(record['requires'] == 0 and record['ensures'] > 0
                            for record in profile['functions'].values() if record['mode'] == 'Exec'))
        self.assertTrue(any('canonical_v2::read_signed_128' in name for name in runtime))
        self.assertTrue(any('record_execution::execute_into' in name for name in runtime))
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                          (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(len(mutations), 24)
        self.assertEqual(sum(expected == 'proof' for _, _, expected in mutations.values()), 18)
        self.assertEqual(mutations['cached_record_before_metadata'][2], 'coverage')
        self.assertEqual(mutations['remove_typing_bridge_application'][2], 'proof')
        for path, changed, expected in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
            self.assertIn(expected, ('proof', 'coverage'))

    def test_simulated_report_cannot_omit_the_actual_record_execution_entry(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        pin.update({key: profile[key] for key in ('expected_verified', 'target_functions')})
        report = {'verus': {'commit': pin['commit'], 'version': pin['version']},
                  'verification-results': {'success': True, 'errors': 0, 'verified': 145,
                    'encountered-error': False, 'encountered-vir-error': False,
                    'is-verifying-entire-crate': True},
                  'func-details': dict.fromkeys(pin['target_functions'], {})}
        self.assertTrue(verifier.accepted(report, pin))
        for name in profile['target_functions']:
            if 'record_execution::execute_into' not in name and 'record_execution::execute' not in name:
                continue
            changed = {**report, 'func-details': {key: value for key, value in report['func-details'].items()
                                                  if key != name}}
            self.assertFalse(verifier.accepted(changed, pin))

    def test_sources_are_hashed_and_symbolic_links_refuse(self):
        from unittest.mock import patch
        self.assertEqual(set(gate.snapshot()), set(map(str, gate.SOURCES)))
        with patch.object(Path, 'is_symlink', return_value=True):
            with self.assertRaisesRegex(RuntimeError, 'symbolic link'):
                gate.snapshot()


class NativeRecordExecution(unittest.TestCase):
    def test_actual_source_and_composed_report_api_refusals(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        with tempfile.TemporaryDirectory(prefix='zeno-record-native-') as temporary:
            evidence = gate.native_checks(Path(temporary), pin, dict(os.environ))
        self.assertIn('14 passed', evidence['test_output'])
        self.assertEqual(len(evidence['api_consumers']), 7)
        self.assertEqual(evidence['api_consumers']['positive']['exit_code'], 0)
        self.assertTrue(all(record['exit_code'] != 0 for name, record in evidence['api_consumers'].items()
                            if name != 'positive'))


if __name__ == '__main__':
    unittest.main()
