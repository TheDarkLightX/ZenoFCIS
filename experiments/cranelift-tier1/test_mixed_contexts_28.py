#!/usr/bin/env python3
"""Dependency-free contracts, finite oracle comparisons and refusal controls."""
from __future__ import annotations

import random
import unittest
from unittest.mock import patch

import qualify_wasmtime_scaling_28 as scaling
from qualify_wasmtime_mixed_contexts_28 import (
    CASES, digest, reference, render, sat_body, selected_indices, shape, salt,
    to_signed,
)
from independent_recheck_mixed_contexts_28 import independent_reference

LO, HI = -(1 << 63), (1 << 63) - 1


class MixedContextTests(unittest.TestCase):
    def test_exact_declared_export_count_and_contexts(self):
        for case, (count, control_only) in CASES.items():
            with self.subTest(case=case):
                src = render(case)
                self.assertTrue(src.startswith("(module\n"))
                self.assertTrue(src.endswith(")\n"))
                self.assertEqual(src.count("  (func "), count)
                self.assertEqual(src.count('(export "sat'), count)
                self.assertEqual(len(selected_indices(case)),
                                 8 if control_only else 16)
                self.assertEqual(set(shape(case, i) for i in range(count)),
                                 set(range(4, 8) if control_only else range(8)))
                self.assertEqual(src, render(case))
                for index in selected_indices(case):
                    self.assertEqual(src.count(f'(export "sat{index}")'), 1)
                if control_only:
                    self.assertNotIn("(local $overflow i32)", src)
                else:
                    self.assertEqual(src.count("(local $overflow i32)"),
                                     count // 2)

    def test_closed_case_and_bounded_index(self):
        for name in ("", "mixed33", "../mixed32", "control32"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                render(name)
        for case, (count, _) in CASES.items():
            for i in (-1, count, count + 100):
                with self.subTest(case=case, index=i), self.assertRaises(ValueError):
                    shape(case, i)

    def test_base_fixture_is_pinned(self):
        self.assertIn("local.set $overflow", sat_body())
        with patch.object(scaling, "SOURCE_SHA256", "0" * 64):
            with self.assertRaisesRegex(ValueError, "fixture drifted"):
                render("mixed32")

    def test_oracles_agree_on_boundaries_and_seeded_pairs(self):
        rng = random.Random(0xC125)
        pairs = [
            (x, y) for x in (LO, LO + 1, -1, 0, 1, HI - 1, HI)
            for y in (LO, LO + 1, -1, 0, 1, HI - 1, HI)
        ]
        pairs += [(rng.randrange(LO, HI + 1), rng.randrange(LO, HI + 1))
                  for _ in range(192)]
        for case in CASES:
            for index in selected_indices(case):
                for x, y in pairs:
                    with self.subTest(case=case, index=index, x=x, y=y):
                        actual = reference(case, index, x, y)
                        other = independent_reference(case, index, x, y)
                        self.assertEqual(actual, other)
                        self.assertTrue(LO <= actual <= HI)

    def test_refuse_out_of_domain(self):
        for case in CASES:
            for pair in ((LO - 1, 0), (HI + 1, 0), (0, LO - 1),
                         (0, HI + 1)):
                with self.subTest(case=case, pair=pair):
                    with self.assertRaises(ValueError):
                        reference(case, 0, *pair)
                    with self.assertRaises(ValueError):
                        independent_reference(case, 0, *pair)

    def test_known_total_signed_wrap_and_clamps(self):
        self.assertEqual(to_signed(1 << 63), LO)
        self.assertEqual(to_signed((1 << 64) + 2), 2)
        self.assertEqual(reference("mixed32", 0, HI, 1), HI)
        self.assertEqual(reference("mixed32", 0, LO, -1), LO)
        self.assertEqual(reference("mixed32", 1, 0, 0), salt(1))
        self.assertEqual(reference("mixed32", 4, -1, 0), to_signed((1 << 64) - 1 + salt(4)))

    def test_read_source_hash_is_content_based(self):
        script = scaling.SOURCE_WAT.read_bytes()
        self.assertEqual(digest(script), scaling.SOURCE_SHA256)


if __name__ == "__main__":
    unittest.main()
