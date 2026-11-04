#!/usr/bin/env python3
"""Refusal controls for bounded alignment-boundary cost experiment."""
import unittest
from qualify_wasmtime_alignment_boundary_28 import (
    CHECKS, MODES, TAGGED_RESULT, render, reference,
)
from qualify_wasmtime_frontend_28 import VECTORS, signed_sat

LO = -(1 << 63)
HI = (1 << 63) - 1

class Tests(unittest.TestCase):
    def test_source_exact_exports_and_live_op_contract(self):
        for mode in MODES:
            for count in CHECKS:
                with self.subTest(mode=mode, count=count):
                    src = render(count, mode)
                    self.assertEqual(src.count('(func $sat'), count)
                    self.assertEqual(src.count('(export "sat'), count)
                    self.assertEqual(src.count("    i64.xor\n"), count * (2 if mode == "plain" else 3))
                    self.assertEqual(src.count("    i64.const 1\n    i64.xor\n"),
                                     0 if mode == "plain" else count)
                    self.assertEqual(src, render(count, mode))
                    self.assertNotIn('export "workload"', src)

    def test_invalid_counts_and_contracts_refused(self):
        for count in (0, 2, 7, -1, 512, 4096):
            with self.subTest(count=count), self.assertRaises(ValueError):
                render(count, "plain")
        for mode in ("", "xor0", "noop", "plain; patched"):
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                render(16, mode)
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                reference(0, 1, mode)

    def test_signed_saturating_xor_reference(self):
        for x, y in VECTORS:
            self.assertEqual(reference(x, y, "plain"), signed_sat(x, y))
            self.assertEqual(reference(x, y, "xor1"), signed_sat(x, y) ^ 1)
        self.assertEqual(reference(HI, 1, "xor1"), HI - 1)
        self.assertEqual(reference(LO, -1, "xor1"), LO + 1)
        self.assertEqual(reference(-1, 0, "xor1"), -2)
        self.assertEqual(reference(0, 0, "xor1"), 1)
        self.assertTrue(TAGGED_RESULT.startswith("signed_i64_saturating_add"))

    def test_out_of_domain_rejected(self):
        for x, y in ((HI+1, 0), (0, LO-1), (HI+1, HI+1)):
            for mode in MODES:
                with self.subTest(x=x, y=y, mode=mode), self.assertRaises(ValueError):
                    reference(x, y, mode)

    def test_xor1_distinct_from_original_at_valid_boundaries(self):
        for x, y in VECTORS:
            self.assertNotEqual(reference(x, y, "plain"), reference(x, y, "xor1"))

if __name__ == "__main__":
    unittest.main()
