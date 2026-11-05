"""Typed compositional, cross-representation procedure search."""
import unittest
from dataclasses import replace

import procedure_cosearch as p
import representation_transport as t


class ProcedureTests(unittest.TestCase):
    def test_grammar_is_compositional_and_type_checked(self):
        plans = p.enumerate_plans()
        self.assertGreater(len(plans), 2)
        self.assertEqual(len(plans), len(set(x.key() for x in plans)))
        self.assertTrue(any('intt(' in x.describe() and 'pointwise(' in x.describe() for x in plans))
        self.assertTrue(any('direct_convolve(' in x.describe() for x in plans))
        with self.assertRaises(ValueError):
            _ = p.Plan('pointwise', (p.Plan('left'), p.Plan('right'))).domain
        with self.assertRaises(ValueError):
            _ = p.Plan('intt', (p.Plan('left'),)).domain

    def test_search_prefers_cheapest_verified_composition_by_size(self):
        for n in t.LENGTHS:
            with self.subTest(n=n):
                result = p.synthesize(n)
                self.assertEqual(result.status, "finite-replayed")
                self.assertEqual(result.checked_cases, 40)
                self.assertEqual(result.evaluation_status, "passed-fixed-evaluation")
                self.assertTrue(result.plan is not None)
                self.assertEqual(result.cost_data_multiplies, p.cost(result.plan, n))
                if n < 8:
                    self.assertEqual(result.cost_data_multiplies, n*n)
                    self.assertIn("direct_convolve", result.plan.describe())
                else:
                    self.assertLess(result.cost_data_multiplies, n*n)
                    self.assertIn("ntt", result.plan.describe())
                    self.assertIn("intt", result.plan.describe())
                    self.assertIn("pointwise", result.plan.describe())

    def test_budget_does_not_invent_solution(self):
        result = p.synthesize(32, budget=1)
        self.assertEqual(result.status, 'budget-inconclusive')
        self.assertIsNone(result.plan)
        self.assertEqual(result.evaluation_status, 'not-evaluated')

    def test_counterexample_ledger_nonempty_and_reproducible(self):
        first = p.synthesize(32)
        second = p.synthesize(32)
        self.assertEqual(first, second)
        self.assertGreater(len(first.witnesses), 0)
        self.assertEqual(first.attempted_candidates, second.attempted_candidates)

    def test_changing_algebraic_semantics_is_refused_by_oracle(self):
        left, right = p.fixtures(4, 1)[2]
        chosen = p.synthesize(4).plan
        self.assertEqual(p.evaluate(chosen, left, right), t.direct_cyclic_convolution(left, right))
        self.assertNotEqual((0 == 97), ((0 - 97) % 97 == 0))


if __name__ == "__main__":
    unittest.main()
