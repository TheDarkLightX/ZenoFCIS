#!/usr/bin/env python3
"""Negative controls for the isolated Wasmtime frontend A/B."""
import unittest
from qualify_wasmtime_frontend_28 import (
    WAT, MIN, MAX, VECTORS, ABBA, BAAB, signed_sat, loop_oracle, numeric_result,
    digest, time_exploratory,
)
from pathlib import Path
class Tests(unittest.TestCase):
    def test_fixture_and_edge_grid(self):
        self.assertTrue(WAT.is_file())
        self.assertGreater(len(WAT.read_bytes()), 500)
        self.assertTrue(len(VECTORS) >= 10)
        self.assertEqual(len(digest(WAT.read_bytes())), 64)
    def test_mathematical_saturation(self):
        self.assertEqual(signed_sat(MAX, 1), MAX)
        self.assertEqual(signed_sat(MIN, -1), MIN)
        self.assertEqual(signed_sat(MAX, MIN), -1)
        self.assertEqual(signed_sat(-2, 5), 3)
    def test_loop_oracle(self):
        self.assertEqual(loop_oracle(0), 1)
        self.assertNotEqual(loop_oracle(1), loop_oracle(2))
        self.assertEqual(loop_oracle(3), signed_sat(loop_oracle(2), -9223372036854775799))
    def test_stdout_refuses_ambiguity_or_out_of_range(self):
        self.assertEqual(numeric_result(" -12\n"), -12)
        for s in ("", "1\n2", "result 1", str(MAX+1), str(MIN-1)):
            with self.subTest(s=s), self.assertRaises(ValueError):
                numeric_result(s)
    def test_balanced_comparison_orders(self):
        self.assertEqual(ABBA, ("baseline", "patched", "patched", "baseline"))
        self.assertEqual(BAAB, ("patched", "baseline", "baseline", "patched"))
        self.assertEqual(ABBA.count("patched"), 2)
    def test_budget_rejection(self):
        with self.assertRaises(ValueError):
            loop_oracle(10000001)
        with self.assertRaises(ValueError):
            time_exploratory({}, Path("."), 0, 2000000)
if __name__ == "__main__":
    unittest.main()
