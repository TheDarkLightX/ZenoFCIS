#!/usr/bin/env python3
"""Dependency-free boundary, tamper, and candidate-vocabulary controls."""
from __future__ import annotations
import random
import unittest

from install_rust_signmask_palette_28 import PALETTE, RULES, sha

MASK = (1 << 64) - 1
MIN = 1 << 63


def eval_mode(mode: str, x: int) -> int:
    if not 0 <= x <= MASK:
        raise ValueError("not a declared 64-bit bitvector")
    if mode not in PALETTE:
        raise ValueError("not an admitted candidate")
    signed_x = x if x < MIN else x - (1 << 64)
    shift = 0 if signed_x >= 0 else MASK
    if mode == "original":
        return shift ^ MIN
    if mode == "add":
        return (shift + MIN) & MASK
    return (MIN - (x >> 63)) & MASK


class PaletteTests(unittest.TestCase):
    def test_closed_palette_and_unique_rules(self):
        self.assertEqual(PALETTE, ("original", "add", "sub"))
        self.assertEqual(set(RULES), set(PALETTE))
        self.assertNotIn("rule", RULES["original"])
        for mode in PALETTE[1:]:
            self.assertEqual(RULES[mode].count("(rule "), 1)
            self.assertIn("(simplify (bxor $I64", RULES[mode])
        self.assertNotEqual(sha(RULES["add"].encode()), sha(RULES["sub"].encode()))

    def test_finite_boundaries_and_random_values(self):
        rng = random.Random(0xC127)
        values = (0, 1, 2, 63, MIN-2, MIN-1, MIN, MIN+1,
                  MASK-2, MASK-1, MASK)
        values += tuple(rng.getrandbits(64) for _ in range(500))
        for x in values:
            original = eval_mode("original", x)
            self.assertEqual(original, eval_mode("add", x))
            self.assertEqual(original, eval_mode("sub", x))
            self.assertEqual(original, MIN if x < MIN else MIN-1)

    def test_bad_type_or_unknown_candidate_refused(self):
        for x in (-1, MASK+1):
            for mode in PALETTE:
                with self.subTest(mode=mode, x=x), self.assertRaises(ValueError):
                    eval_mode(mode, x)
        for mode in ("", "and", "min", "add;sub"):
            with self.assertRaises(ValueError):
                eval_mode(mode, 0)

    def test_wrong_candidate_is_detectably_wrong(self):
        self.assertNotEqual((MIN + 1) & MASK, eval_mode("original", MIN))


if __name__ == "__main__":
    unittest.main()
