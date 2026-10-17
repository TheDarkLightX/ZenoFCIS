"""Pure comparator tests; no native compiler or interpreter is launched."""
import unittest
import tempfile
from pathlib import Path
from unittest.mock import patch
from check_scalar_arithmetic import execute, grade
from scalar_arithmetic_reference import Case
from scalar_arithmetic_controls import replace_once

class ConformanceAdmission(unittest.TestCase):
    def test_real_positives_and_semantic_negatives_are_both_required(self):
        self.assertEqual(grade([1,None],[1,None],{"wrong_round":([0],[1])}),{"wrong_round":True})
        for positive,expected,mutants in [([0],[1],{"wrong":([0],[1])}),([1],[1],{}),([1],[1],{"survivor":([1],[1])})]:
            with self.assertRaises(ValueError):grade(positive,expected,mutants)

    def test_javascript_driver_keeps_the_line_separator_escaped(self):
        with tempfile.TemporaryDirectory(prefix="scalar-driver-") as name:
            folder = Path(name)
            subject = folder / "subject.mjs"
            subject.write_text("export function transition(input) { return input; }\n")
            with patch("check_scalar_arithmetic.run", return_value="-1\n"):
                actual = execute(subject, (Case("div_floor", (-1, 2)),),
                                 "unused-rustc", "unused-node", folder, "sample")
            self.assertEqual(actual, [-1])
            driver = (folder / "subject-driver.mjs").read_text()
            self.assertIn(r".split('\n')", driver)
            self.assertNotIn(".split('\n')", driver)

    def test_native_fault_anchors_survive_the_shared_source_format(self):
        root = Path(__file__).resolve().parents[1]
        scalar = (root / "crates/zeno-fcis-synthesis/src/finite/value/scalar.rs").read_text()
        wide = (root / "crates/zeno-fcis-synthesis/src/finite/value/wide.rs").read_text()
        emitted_helpers = wide + scalar
        for anchor, replacement in [
            ("if b == 0 { return None; }", "if b == 0 { return Some(0); }"),
            ("a.checked_mul(b)", "Some(a.wrapping_mul(b))"),
        ]:
            with self.subTest(anchor=anchor):
                changed = replace_once(emitted_helpers, anchor, replacement)
                self.assertNotEqual(changed, emitted_helpers)

    def test_disabled_comparison_fails_the_same_negative_suite(self):
        mutants={"wrong_round":([0],[1]),"always_refuse":([None],[6])}
        self.assertTrue(all(grade([1],[1],mutants).values()))
        with self.assertRaisesRegex(ValueError,"semantic emitted-code mutant survived"):
            grade([1],[1],mutants,enabled=False)

if __name__=="__main__":unittest.main()
