import copy
import unittest
from certificate_check import check
from product import description, roots, events, step, GUARDS
from solve import fixed_guard
from test_certificate import Certificate


class Guards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        Certificate.setUpClass(); cls.tables=Certificate.tables

    def test_native_fixed_guard(self):
        p=fixed_guard(description('withdrawal'),self.tables)
        self.assertEqual(p['certificate']['guard'],'actor == 0')
        self.assertEqual(check(p,self.tables)['evidence'],'certified guard')

    def test_quantifier_order(self):
        spec=description('quantifier'); state=roots(spec)[0]
        for event in range(len(events(spec,state))):
            self.assertTrue(any(not step(spec,{},state,event,g).bad for g in GUARDS))
        p=fixed_guard(spec,{})
        self.assertEqual(check(p,{})['claim'],'no guard in G')
        self.assertEqual(len(p['attempts']),3)

    def test_guard_outside_grammar(self):
        p=fixed_guard(description('withdrawal'),self.tables)
        p['certificate']['guard']='hidden == 0'
        with self.assertRaisesRegex(ValueError,'outside grammar'): check(p,self.tables)

    def test_exhaustion_subjects_cannot_differ(self):
        p=fixed_guard(description('quantifier'),{})
        p['attempts'][1]['spec']['empty']=True
        with self.assertRaisesRegex(ValueError,'subject mismatch'): check(p,{})


if __name__=='__main__': unittest.main()
