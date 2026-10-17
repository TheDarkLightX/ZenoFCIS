"""Pure Python reference and semantic comparator controls, without native claims."""
import unittest
from pathlib import Path
from scalar_arithmetic_reference import Case, MAX, MIN, boundary_cases, result, vectors

class ArithmeticReference(unittest.TestCase):
    def test_mathematical_rounding_all_signs(self):
        expected = {(5, 2):(2,3),(-5,2):(-3,-2),(5,-2):(-3,-2),(-5,-2):(2,3),(-1,2):(-1,0)}
        for (a,b),(lo,hi) in expected.items():
            self.assertEqual(result(Case("div_floor",(a,b))),lo)
            self.assertEqual(result(Case("div_ceil",(a,b))),hi)
        for op in ("div_floor","div_ceil"):
            self.assertIsNone(result(Case(op,(MIN,-1))))
            self.assertIsNone(result(Case(op,(1,0))))

    def test_fused_domain_and_final_rounding_overflow(self):
        self.assertIsNone(result(Case("mul",(MAX,MAX))))
        self.assertEqual(result(Case("mul_div_floor",(MAX,MAX,MAX))),MAX)
        self.assertEqual(result(Case("mul_div_floor",(MAX-1,MAX-1,MAX-2))),MAX)
        self.assertIsNone(result(Case("mul_div_ceil",(MAX-1,MAX-1,MAX-2))))
        for args in ((-1,0,1),(0,-1,1),(0,0,0),(0,0,-1)):
            self.assertIsNone(result(Case("mul_div_floor",args)))

    def test_product_high_bits_and_integer_square_root_predicate(self):
        self.assertEqual(result(Case("products_ge",(1<<43,)*3+(1,1,1))),1)
        for a in range(32):
            for b in range(32):
                for s in range(32):
                    predicate = result(Case("products_ge",(a,b,s,s))) == 1 and result(Case("products_ge",(a,b,s+1,s+1))) == 0
                    self.assertEqual(predicate,s*s<=a*b<(s+1)*(s+1))

    def test_exact_fee_positive_controls_reject_stubs(self):
        cases=[Case("mul_div_ceil",(a,b,10)) for a,b in ((0,3),(1,1),(10,1),(11,1),(20,3))]
        expected=[result(c) for c in cases]
        for candidate in ([0]*5,[None]*5,[1]*5,[0,0,1,1,6]):
            self.assertNotEqual(candidate,expected)
            def grade(compare):
                self.assertFalse(compare(candidate,expected),"successful-path mutant survived")
            grade(lambda actual,wanted: actual==wanted)
            with self.assertRaisesRegex(AssertionError,"mutant survived"):
                grade(lambda actual,wanted: True)
        self.assertEqual(expected,[0,1,1,2,6])

    def test_vector_file_is_complete_and_reproducible(self):
        root=Path(__file__).resolve().parents[1]
        self.assertEqual((root/"tools/test-data/scalar-arithmetic/vectors.tsv").read_text(),vectors())
        self.assertGreater(len(boundary_cases()),1000)

    def test_rounding_truncation_swap_and_high_word_controls(self):
        checks = [(result(Case("div_floor",(-1,2))),0),
            (result(Case("div_ceil",(1,2))),0),
            (result(Case("mul_div_floor",(MAX,MAX,MAX))),((MAX*MAX)&((1<<64)-1))//MAX),
            (result(Case("mul",(MAX,2))),-2),
            (result(Case("div_floor",(1,0))),0),
            (result(Case("products_ge",(1<<43,)*3+(1,1,1))),0)]
        self.assertTrue(all(expected!=mutant for expected,mutant in checks))
        def grade(compare):
            for expected,mutant in checks:
                self.assertFalse(compare(expected,mutant),"arithmetic mutant survived")
        grade(lambda expected,mutant: expected==mutant)
        with self.assertRaisesRegex(AssertionError,"mutant survived"):
            grade(lambda expected,mutant: True)

if __name__ == "__main__":
    unittest.main()
