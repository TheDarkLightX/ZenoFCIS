"""Guard coverage and safe API admission; fixtures are not proof results."""
import json
import os
from pathlib import Path
import tempfile
import unittest

import check_protected_input as gate
import check_verus as verifier


class ProtectedInputCoverage(unittest.TestCase):
    def test_complete_shared_closure_and_real_mutation_anchors(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        runtime = {name for name, record in profile['functions'].items() if record['mode'] == 'Exec'}
        self.assertEqual(len(profile['functions']), 122)
        self.assertEqual(len(runtime), 59)
        self.assertEqual(set(profile['body_covered_functions']), runtime)
        self.assertTrue(all(record['requires'] == 0 and record['ensures'] > 0
                            for record in profile['functions'].values() if record['mode'] == 'Exec'))
        self.assertTrue(any('canonical_v2::read_signed_128' in name for name in runtime))
        self.assertTrue(any('input_view::project_into' in name for name in runtime))
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                          (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(len(mutations), 30)
        self.assertEqual(sum(expected == 'proof' for _, _, expected in mutations.values()), 23)
        for name in ('parse_header_before_ingress', 'parse_field_before_read'):
            self.assertEqual(mutations[name][2], 'coverage')
        for path, changed, expected in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
            self.assertIn(expected, ('proof', 'coverage'))

    def test_simulated_report_cannot_omit_the_actual_protected_entry(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        pin.update({key: profile[key] for key in ('expected_verified', 'target_functions')})
        report = {'verus': {'commit': pin['commit'], 'version': pin['version']},
                  'verification-results': {'success': True, 'errors': 0, 'verified': 98,
                    'encountered-error': False, 'encountered-vir-error': False,
                    'is-verifying-entire-crate': True},
                  'func-details': dict.fromkeys(pin['target_functions'], {})}
        self.assertTrue(verifier.accepted(report, pin))
        for name in profile['target_functions']:
            if 'input_view::project_into' not in name and 'input_view::project' not in name:
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


class NativeProtectedInputs(unittest.TestCase):
    def test_actual_source_and_opaque_report_api_refusals(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        with tempfile.TemporaryDirectory(prefix='zeno-protected-native-') as temporary:
            evidence = gate.native_checks(Path(temporary), pin, dict(os.environ))
        self.assertIn('8 passed', evidence['test_output'])
        self.assertEqual(len(evidence['api_consumers']), 5)
        self.assertEqual(evidence['api_consumers']['positive']['exit_code'], 0)
        self.assertTrue(all(record['exit_code'] != 0 for name, record in evidence['api_consumers'].items()
                            if name != 'positive'))


if __name__ == '__main__':
    unittest.main()
