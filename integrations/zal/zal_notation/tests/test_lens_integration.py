"""Public Lens entrypoints and adversarial native integration regressions."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import unittest

from zal_notation import core, lenses as L, sharing

ROOT = Path(__file__).resolve().parents[2]


class LensIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.source = L.make_source()


    def test_native_toolkit_exposes_closed_lens_actions(self):
        names = {tool['name'] for tool in sharing.toolkit()['tools']}
        self.assertTrue({'zal_lens_source', 'zal_lens_discover', 'zal_lens_apply',
                         'zal_lens_check', 'zal_lens_verify', 'zal_lens_restore'} <= names)
        source = sharing.dispatch_tool('zal_lens_source', {'contract': L.counter_contract().data()})
        self.assertEqual(source, self.source)
        app = sharing.dispatch_tool('zal_lens_apply', {'source': source, 'lens_id': 'parity-quotient', 'parameters': {}})
        receipt = sharing.dispatch_tool('zal_lens_check', {'source': source, 'application': app, 'budget': 100000})
        self.assertEqual(receipt['status'], 'checked')
        verified = sharing.dispatch_tool('zal_lens_verify', {'source': source, 'application': app, 'receipt': receipt})
        self.assertTrue(verified['matches_replay'])
        self.assertEqual(sharing.dispatch_tool('zal_lens_restore', {'source': source, 'application': app}), source)

    def test_boolean_image_is_not_equal_to_integer_image(self):
        app = L.apply(self.source, 'transformation-monoid')
        self.assertEqual(app['payload']['algebra']['elements'][0][0], 0)
        app['payload']['algebra']['elements'][0][0] = False
        self.assertEqual(L.check(self.source, app)['status'], 'unsupported')

    def test_receipt_float_and_boolean_are_not_integer_evidence(self):
        app = L.apply(self.source, 'parity-quotient')
        receipt = L.check(self.source, app)
        for number in (8.0, True):
            edited = deepcopy(receipt)
            edited['evidence']['native_result']['cases'] = number
            self.assertFalse(L.verify_receipt(self.source, app, edited))

    def test_refuted_quotient_receipt_survives_json_roundtrip(self):
        app = L.apply(self.source, 'parity-quotient', {'mapping': {'unseen': 'a', 'one': 'a', 'two': 'b', 'three': 'b'}})
        receipt = L.check(self.source, app)
        self.assertEqual(receipt['status'], 'refuted')
        core.canonical(receipt)
        self.assertTrue(L.verify_receipt(self.source, app, json.loads(json.dumps(receipt))))

    def test_public_apply_exhaustion_stays_incomplete(self):
        for lens in ('definition-expand', 'parity-quotient', 'transformation-monoid'):
            with self.assertRaises(L.finite.Incomplete):
                L.apply(self.source, lens, budget=0)
            request = {'action': 'apply', 'source': self.source, 'lens_id': lens, 'parameters': {}, 'budget': 0}
            self.assertEqual(L.dispatch(request)['status'], 'incomplete')

    def test_invalid_budget_never_runs_unbounded_work(self):
        app = L.apply(self.source, 'transformation-monoid')
        for limit in (True, -1, 1000001, '100', None, []):
            result = L.check(self.source, app, limit)
            self.assertEqual(result['status'], 'unsupported')
            self.assertEqual(result['work'], 0)

    def test_malformed_bounded_values_return_unsupported(self):
        for application in (None, [], {}, {'schema': 'zal.lens-application/1'}, {'schema': 'wrong'}):
            self.assertEqual(L.check(self.source, application)['status'], 'unsupported')
        app = L.apply(self.source, 'finite-table')
        for source in (None, [], {}, {'semantics': None}):
            self.assertEqual(L.check(source, app)['status'], 'unsupported')

    def test_non_json_source_does_not_escape_check(self):
        self.assertEqual(L.check({'x': object()}, {})['status'], 'unsupported')

    def test_bad_lens_ids_are_structured_unsupported(self):
        for lens in ([], {}, None, 'execution-tree', 'automorphism-group', 'os.system'):
            result = L.dispatch({'action': 'apply', 'source': self.source, 'lens_id': lens, 'parameters': {}})
            self.assertEqual(result['status'], 'unsupported')



    def test_full_effect_group_can_pass_for_invertible_commands(self):
        contract = L.counter_contract()
        rows = contract.table()
        for state in contract.inputs[0].values:
            rows[(state, 'reset', 'yes')] = (('ok', state, 'done'),)
        source = L.make_source(L.finite.build(contract.inputs, contract.outputs, rows, contract.state))
        app = L.apply(source, 'all-effects-group')
        receipt = L.check(source, app)
        self.assertEqual(receipt['status'], 'checked')
        self.assertEqual(receipt['evidence']['carrier_size'], 4)

    def test_all_algebra_views_declare_full_operational_loss(self):
        for lens in ('transformation-monoid', 'all-effects-group', 'unit-group', 'one-object-category'):
            app = L.apply(self.source, lens)
            for loss in ('word', 'intermediate states', 'trace timing', 'output observations'):
                self.assertIn(loss, ' '.join(app['forgets']))

    def test_fixed_sibling_import_does_not_use_ambient_prototype(self):
        code = "import sys, types; fake=types.ModuleType('prototype'); sys.modules['prototype']=fake; before=list(sys.path); from zal_notation import lenses; assert sys.modules['prototype'] is fake; assert before == sys.path; assert lenses.finite is not fake; assert lenses.make_source()"
        completed = subprocess.run([sys.executable, '-c', code], cwd=ROOT, text=True, capture_output=True)
        self.assertEqual(completed.returncode, 0, completed.stderr)



if __name__ == '__main__':
    unittest.main()
