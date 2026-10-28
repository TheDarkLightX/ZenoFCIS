"""Real Haskell replay through the optional profile; requires a built runtime."""
from __future__ import annotations

import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

if not __package__:
    sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from integrations.zal.relational_profile import bridge, profile
_manifest, _ = profile.verify_provider()
_, oracle = profile._load_vendor(_manifest)

ROOT = Path(__file__).resolve().parent
RUNTIME = os.environ.get('ZAL_RELATIONAL_RUNTIME')
SCRATCH = os.environ.get('ZAL_RELATIONAL_SCRATCH')
OPT = ['-O'] if sys.flags.optimize else []


def run(request, limits=None):
    return profile.execute(request, RUNTIME, limits=limits, seconds=30, scratch=SCRATCH)


def comparison(left, right, assumptions=None, relation='equivalence'):
    return {'schema': 'zal/relational-comparison/1', 'logic': 'zal/c2/1',
            'signature': [{'id': 'P', 'arity': 1}], 'domain_size': '2',
            'context_id': 'profile-live-comparison',
            'assumptions': {'op': 'true'} if assumptions is None else assumptions,
            'left': left, 'right': right, 'relation': relation}


@unittest.skipUnless(RUNTIME and SCRATCH, 'set ZAL_RELATIONAL_RUNTIME and ZAL_RELATIONAL_SCRATCH')
class LiveProvider(unittest.TestCase):
    def test_two_actual_counts_against_independent_enumeration(self):
        for name, expected in [('owner-car-count.json', '137'), ('even-graph-count.json', '2')]:
            request = json.loads((ROOT / 'vendor/examples' / name).read_text())
            with self.subTest(name=name):
                response = run(request)
                self.assertEqual(response['status'], 'ExactCount', response)
                self.assertEqual(response['result']['value'], oracle.count(request))
                self.assertEqual(response['result']['value'], expected)

    def test_current_ladder_to_actual_supplied_model_evaluation(self):
        context = ROOT / 'vendor/examples/owner_cars_context.json'
        combined = bridge.lower_and_execute(context, 'every owner has a car.', 'intuitive', 2,
                                            RUNTIME, seconds=30, scratch=SCRATCH)
        self.assertEqual(combined['status'], 'ExactCount', combined)
        self.assertEqual(combined['answer']['result']['value'], '137')
        source = combined['lowering']
        self.assertEqual(source['status'], 'Lowered', source)
        q = source['query']
        request = {k: q[k] for k in ('logic', 'signature', 'formula', 'domain_size', 'context_id')}
        request.update(schema='zal/relational-evaluation/1',
                       model={'Owner': [['0']], 'Car': [['1']], 'Has': [['0', '1']]})
        accepted = run(request)
        self.assertEqual(accepted['status'], 'Evaluated', accepted)
        self.assertIs(accepted['result']['value'], True)
        request['model']['Has'] = []
        rejected = run(request)
        self.assertEqual(rejected['status'], 'Evaluated', rejected)
        self.assertIs(rejected['result']['value'], False)

    def test_real_refutation_implication_and_vacuity(self):
        atom = {'op': 'atom', 'predicate': 'P', 'args': ['x']}
        some = {'op': 'exists', 'var': 'x', 'body': atom}
        exactly_one = {'op': 'count', 'var': 'x', 'cmp': 'eq', 'threshold': '1', 'body': atom}
        request = comparison(some, exactly_one)
        refuted = run(request)
        self.assertEqual(refuted['status'], 'RefutedAt', refuted)
        self.assertEqual(refuted['result']['partition'],
                         {'both_true': '2', 'left_only': '1', 'right_only': '0', 'both_false': '1'})
        self.assertEqual(refuted['result']['counterexample']['model'], {'P': [['0'], ['1']]})
        implication = run(comparison(exactly_one, some, relation='implication'))
        self.assertEqual(implication['status'], 'ImpliesAt', implication)
        vacuous = run(comparison(some, exactly_one, assumptions={'op': 'false'}))
        self.assertEqual(vacuous['status'], 'VacuousAt', vacuous)
        self.assertIsNone(vacuous['result']['holds'])
        self.assertEqual(vacuous['result']['admitted_models'], '0')

    def test_actual_unsupported_and_resource_exhaustion_are_not_results(self):
        query = json.loads((ROOT / 'vendor/examples/even-graph-count.json').read_text())
        query['logic'] = 'zal/c2/1'
        unsupported = run(query)
        self.assertEqual(unsupported['status'], 'Unsupported', unsupported)
        self.assertEqual(unsupported['provider_status'], 'Unsupported')
        self.assertNotIn('result', unsupported)
        atom = {'op': 'atom', 'predicate': 'P', 'args': ['x']}
        q = comparison({'op': 'exists', 'var': 'x', 'body': atom}, {'op': 'true'})
        limits = dict(profile.DEFAULT_LIMITS, candidates='1')
        exhausted = run(q, limits)
        self.assertEqual(exhausted['status'], 'UNKNOWN', exhausted)
        self.assertEqual(exhausted['provider_status'], 'ResourceExhausted')
        self.assertNotIn('result', exhausted)
        bad = copy.deepcopy(query)
        bad['logic'] = 'zal/c2mod/1'
        bad['formula']['right']['right']['body']['modulus'] = '0'
        invalid = run(bad)
        self.assertEqual(invalid['status'], 'Invalid', invalid)
        self.assertNotIn('result', invalid)

    def test_cli_real_count_refutation_and_vacuity_exit_codes(self):
        atom = {'op': 'atom', 'predicate': 'P', 'args': ['x']}
        some = {'op': 'exists', 'var': 'x', 'body': atom}
        one = {'op': 'count', 'var': 'x', 'cmp': 'eq', 'threshold': '1', 'body': atom}
        count = json.loads((ROOT / 'vendor/examples/owner-car-count.json').read_text())
        for request, status, code in [(count, 'ExactCount', 0),
                                      (comparison(some, one), 'RefutedAt', 1),
                                      (comparison(some, one, {'op': 'false'}), 'VacuousAt', 2)]:
            result = subprocess.run([sys.executable, *OPT, '-I', '-B', str(ROOT / '__main__.py'),
                                     'run', RUNTIME, '-', '--seconds', '30', '--scratch', SCRATCH],
                                    input=json.dumps(request).encode(), stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, timeout=35, check=False)
            self.assertEqual(result.returncode, code, result.stderr.decode(errors='replace'))
            self.assertEqual(json.loads(result.stdout)['status'], status)


if __name__ == '__main__':
    unittest.main(verbosity=2)
