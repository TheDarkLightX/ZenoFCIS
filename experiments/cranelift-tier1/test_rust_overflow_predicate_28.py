#!/usr/bin/env python3
"""Deterministic exhaustiveness at width 8; bounded grammar and source refusals."""
import random
import unittest
from install_rust_overflow_predicate_28 import MODES, RULE, RULE_NAME, source


def as_signed(n, bits):
    mod = (1 << bits) - 1
    bit = 1 << (bits - 1)
    return (((n & mod) ^ bit) - bit)


def predicates(x, y, bits):
    mod = (1 << bits) - 1
    sign = 1 << (bits - 1)
    sumv = (x + y) & mod
    baseline = (as_signed(y, bits) < 0) ^ (as_signed(sumv, bits) < as_signed(x, bits))
    xorand = bool(((sumv ^ x) & (sumv ^ y)) & sign)
    xorandnot = bool(((sumv ^ x) & ~(x ^ y)) & sign)
    wrong = bool(((sumv ^ x) & (x ^ y)) & sign)
    return baseline, xorand, xorandnot, wrong


class PredicateTests(unittest.TestCase):
    def test_closed_palette_and_source_rule(self):
        self.assertEqual(MODES, ("baseline", "xorand"))
        self.assertEqual(RULE.count("(rule "), 1)
        self.assertEqual(RULE.count(RULE_NAME), 1)
        self.assertIn("(bxor $I8", RULE)
        self.assertIn("(iadd $I64 x y)", RULE)
        self.assertIn("(slt $I64", RULE)
        self.assertNotIn("select $I64", RULE)

    def test_exhaustive_8bit_equivalence(self):
        wrong_seen = False
        for x in range(256):
            for y in range(256):
                old, a, b, bad = predicates(x, y, 8)
                self.assertEqual(old, a)
                self.assertEqual(old, b)
                wrong_seen |= old != bad
        self.assertTrue(wrong_seen)

    def test_64bit_boundary_and_seeded_witnesses(self):
        pairs = [-(1 << 63), -(1 << 63) + 1, -1, 0, 1, (1 << 63) - 2,
                 (1 << 63) - 1]
        for x in pairs:
            for y in pairs:
                bits = ((x & ((1 << 64) - 1)), (y & ((1 << 64) - 1)))
                old, a, b, _ = predicates(*bits, 64)
                self.assertEqual(old, a)
                self.assertEqual(old, b)
        r = random.Random(0xCAFE819)
        for _ in range(5000):
            x = r.getrandbits(64)
            y = r.getrandbits(64)
            old, a, b, _ = predicates(x, y, 64)
            self.assertEqual(old, a)
            self.assertEqual(old, b)

    def test_refuse_outside_vocabulary(self):
        for bad in ("", "andnot", "prod", "xor_and"):
            with self.assertRaises(ValueError):
                source(None, bad)


if __name__ == "__main__":
    unittest.main()
