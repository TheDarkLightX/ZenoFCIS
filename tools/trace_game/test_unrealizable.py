import copy
import unittest
from product import description
from solve import unrealizable
from certificate_check import check


class Unrealizable(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.packet=unrealizable(description('oracle-withholding'),{})
        cls.baseline=check(cls.packet,{})

    def test_withholding_certificate(self):
        self.assertEqual(self.baseline['evidence'],'certified unrealizability')
        self.assertEqual(self.baseline['initial_rank'],2)
        self.assertIn('no controller in the declared class',self.baseline['claim'])
        root=self.packet['certificate']['root']
        self.assertEqual(next(row[2] for row in self.packet['certificate']['ranks'] if row[0]==root),1)

    def test_rank_step_must_decrease(self):
        p=copy.deepcopy(self.packet); root=p['certificate']['root']
        for row in p['certificate']['ranks']:
            if row[0]==root: row[1]=1
        with self.assertRaisesRegex(ValueError,'rank step does not decrease'): check(p,{})

    def test_strategy_must_cover_every_response(self):
        p=copy.deepcopy(self.packet); root=p['certificate']['root']
        targets={t for e,g,t in p['graph']['edges'][root] if e==1 and g=='settle'}
        p['certificate']['ranks']=[r for r in p['certificate']['ranks'] if r[0] not in targets]
        with self.assertRaisesRegex(ValueError,'every response'): check(p,{})

    def test_budget_is_unknown(self):
        self.assertEqual(unrealizable(description('oracle-withholding'),{},max_edges=1)['status'],'UNKNOWN')


if __name__=='__main__': unittest.main()
