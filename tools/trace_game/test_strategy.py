import copy
import unittest
from product import description
from solve import reactive, fixed_guard
from certificate_check import check
import test_certificate


class Strategy(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        test_certificate.Certificate.setUpClass(); cls.tables=test_certificate.Certificate.tables
        cls.packet=reactive(description('withdrawal'),cls.tables)
        check(cls.packet,cls.tables)

    def test_native_strategy(self):
        self.assertEqual(check(self.packet,self.tables)['evidence'],'certified winning strategy')

    def test_quantifier_reactive_wins_fixed_does_not(self):
        self.assertEqual(check(fixed_guard(description('quantifier'),{}),{})['claim'],'no guard in G')
        p=reactive(description('quantifier'),{})
        self.assertEqual(check(p,{})['evidence'],'certified winning strategy')
        self.assertEqual(len(p['certificate']['strategy']['rules']),2) # uniform across hidden starts

    def test_hidden_read_refused(self):
        p=reactive(description('quantifier'),{}); p['certificate']['strategy']['reads'].append('hidden')
        with self.assertRaisesRegex(ValueError,'reading hidden variable'): check(p,self.tables)

    def test_hidden_rule_field_refused(self):
        p=copy.deepcopy(self.packet); p['certificate']['strategy']['rules'][0]['hidden']=0
        with self.assertRaisesRegex(ValueError,'reading hidden variable'): check(p,self.tables)

    def test_future_observation_order_refused(self):
        p=copy.deepcopy(self.packet); p['certificate']['strategy']['order']='read outcome before responding'
        with self.assertRaisesRegex(ValueError,'observation order'): check(p,self.tables)

    def test_bad_action_policy_refused(self):
        p=copy.deepcopy(self.packet)
        for rule in p['certificate']['strategy']['rules']: rule['guard']='true'
        with self.assertRaisesRegex(ValueError,'closure'): check(p,self.tables)


if __name__=='__main__': unittest.main()
