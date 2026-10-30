"""Planted controls for the qualification driver's own gates; no worker is built."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import sys
import unittest
from unittest import mock

if not __package__:
    sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from integrations.zal.relational_profile import profile, qualify

ROOT = Path(__file__).resolve().parent
COUNT = json.loads((ROOT / 'vendor/examples/even-graph-count.json').read_text())
COUNTED = {'status': 'ExactCount', 'value': '2', 'domain_size': COUNT['domain_size'],
           'engine': 'population-definitions/1', 'work': '1', 'peak_entries': '1', 'peak_bits': '1',
           'assurance': 'experimental; source-level proof open'}
REFUTED = {'status': 'RefutedAt', 'holds': False, 'domain_size': '2', 'enumerated_models': '4',
           'counterexample': {'left': False, 'right': True, 'model': {'P': [['0']]}}}


def admit_everything(payload, answer, *rest):
    return answer['status']


class ForgedResponses(unittest.TestCase):
    def test_a_wrapper_that_admits_forgeries_fails_the_run(self):
        with mock.patch.object(profile, '_validate_answer', side_effect=admit_everything):
            with self.assertRaisesRegex(RuntimeError, 'forged response admitted'):
                qualify.forged_responses(COUNT, COUNTED, qualify.comparison_request(), REFUTED)

    def test_a_refused_actual_response_makes_the_forgeries_meaningless(self):
        def refuse(*arguments):
            raise profile.ProfileFailure('Error', 'refused')
        with mock.patch.object(profile, '_validate_answer', side_effect=refuse):
            with self.assertRaises(profile.ProfileFailure):
                qualify.forged_responses(COUNT, COUNTED, qualify.comparison_request(), REFUTED)


class Replay(unittest.TestCase):
    def replay(self, fresh, record):
        with mock.patch.object(qualify, 'run_wrapper', return_value=fresh):
            return qualify.replay(record, None, None)

    def test_binding_and_exact_result_are_both_compared(self):
        answer = {'request_sha256': 'a', 'result': dict(COUNTED)}
        record = {'request': COUNT, 'answer': answer}
        self.assertEqual(self.replay(copy.deepcopy(answer), record), 'Replayed')
        changed = copy.deepcopy(answer)
        changed['result']['value'] = '3'
        self.assertEqual(self.replay(changed, record), 'ReplayMismatch')
        rebound = copy.deepcopy(answer)
        rebound['request_sha256'] = 'b'
        self.assertEqual(self.replay(rebound, record), 'BindingMismatch')
        self.assertEqual(self.replay({'request_sha256': 'a', 'status': 'UNKNOWN'}, record), 'ReplayIncomplete')

    def test_a_witness_change_is_a_mismatch(self):
        answer = {'request_sha256': 'a', 'result': copy.deepcopy(REFUTED)}
        changed = copy.deepcopy(answer)
        changed['result']['counterexample']['left'] = True
        self.assertEqual(self.replay(changed, {'request': {}, 'answer': answer}), 'ReplayMismatch')


class Inputs(unittest.TestCase):
    def test_inputs_cover_the_profile_and_the_ladder_it_reads(self):
        names = set(qualify.inputs())
        for name in ('integrations/zal/relational_profile/profile.py',
                     'integrations/zal/relational_profile/vendor/src/Zal/Logic/Population.hs',
                     'integrations/zal/relational_profile/qualify.py', *qualify.LADDER_INPUTS):
            self.assertIn(name, names)


if __name__ == '__main__':
    unittest.main(verbosity=2)
