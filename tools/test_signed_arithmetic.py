"""Pure D7 oracle and control construction tests; no native execution."""
import unittest
from pathlib import Path
from signed_arithmetic_reference import MIN,MAX,cases,result,vectors
import signed_arithmetic_controls as c

class SignedArithmetic(unittest.TestCase):
    def test_signs_min_and_fitting_wide_product(self):
        for row,want in [((-1,1,2),-1),((-3,2,4),-2),((-3,-2,4),1),
                         ((MIN,1,1),MIN),((MIN,-1,1),None),((MIN,-1,2),1<<126),
                         ((MIN,0,1),0),((MAX,MAX,MAX),MAX),((MIN,MIN,1),None),
                         ((1001,-25,10000),-3),((1001,25,10000),2),
                         ((0,0,0),None),((0,1,-1),None),((1,0,-1),None)]:
            self.assertEqual(result(*row),want,row)
    def test_exact_remainders_and_nonconservation(self):
        for row in cases():
            q=result(*row)
            if q is not None:
                a,b,d=row;r=a*b-q*d
                self.assertTrue(0<=r<d,row)
        self.assertEqual(result(1,1,2)+result(-1,1,2),-1)
    def test_fixture_and_independent_magnitude_model(self):
        root=Path(__file__).resolve().parents[1]
        self.assertEqual((root/'tools/test-data/signed-arithmetic/vectors.tsv').read_text(),vectors())
        self.assertTrue(c.grade(cases(),result)['passed'])
    def test_admitted_specimen_and_disabled_comparison(self):
        root=Path(__file__).resolve().parents[1];specimen=c.specimens(root)[c.NAME]
        original=(root/c.SOURCE).read_text();changed=specimen[1]
        self.assertEqual(original.count('ensures'),changed.count('ensures'))
        self.assertIn(c.FAULT,changed);self.assertNotIn(c.ANCHOR,changed)
        self.assertEqual(c.magnitude_model(*c.WITNESS,fault=True),0)
        self.assertEqual(result(*c.WITNESS),-1)
        self.assertFalse(c.grade(cases(),result,comparison=False)['passed'])
    def test_positive_controls_reject_always_refuse_and_zero(self):
        self.assertNotEqual(None,result(MAX,MAX,MAX))
        self.assertNotEqual(0,result(1001,-25,10000))
    def test_legacy_d1_width_projection_matches_unchanged_i64_oracle(self):
        from scalar_arithmetic_reference import boundary_cases,result as d1
        for case in boundary_cases():
            wide=d1(case,128)
            narrowed=wide if wide is not None and -(1<<63)<=wide<(1<<63) else None
            self.assertEqual(narrowed,d1(case),case)

if __name__=='__main__':unittest.main()
