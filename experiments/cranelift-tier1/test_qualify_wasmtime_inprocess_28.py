#!/usr/bin/env python3
"""Negative controls for balanced in-process Wasmtime qualification."""
import unittest
from qualify_wasmtime_inprocess_28 import (
    parse_samples, summarize, reference, ABBA, BAAB, SCHEMA, MASK
)

LOOPS, REPS, BLOCKS = 1000, 4, 4

def fixture():
    checksum = ((reference(LOOPS) & MASK) * REPS) & MASK
    lines = [f"META,{SCHEMA},{LOOPS},{REPS},{BLOCKS}",
             "CHECK,baseline,1036,8", "CHECK,patched,1036,8"]
    for b in range(BLOCKS):
        for p, which in enumerate(ABBA if b % 2 == 0 else BAAB):
            lines.append(f"SAMPLE,{b},{p},{which},{10000 + b*50 + (0 if which == 'baseline' else 10)},{checksum}")
    return lines

class Tests(unittest.TestCase):
    def test_positive_complete_and_timing_scope(self):
        x = parse_samples("\n".join(fixture()) + "\n", LOOPS, REPS, BLOCKS)
        self.assertEqual(len(x["samples"]), 16)
        y = summarize(x["samples"], BLOCKS)
        self.assertEqual(len(y["ratios_by_block"]), 4)
        self.assertEqual(y["status"], "exploratory-on-one-virtualized-host-no-runtime-winner")

    def test_loss_duplication_and_metadata_drift(self):
        original = fixture()
        for alt in (original[:-1], original + [original[-1]],
                    ["META,unrecognized,1000,4,4", *original[1:]],
                    [original[0], "CHECK,baseline,1035,8", *original[2:]]):
            with self.subTest(n=len(alt)), self.assertRaises(ValueError):
                parse_samples("\n".join(alt), LOOPS, REPS, BLOCKS)

    def test_sample_order_and_checksum_tamper(self):
        original = fixture()
        for index, value in ((3, original[3].replace(",baseline,", ",patched,")),
                             (5, original[5].replace("SAMPLE,0,2", "SAMPLE,4,2")),
                             (3, original[3].rsplit(",", 1)[0] + ",987"),
                             (3, original[3].replace(",10000,", ",0,"))):
            changed = original.copy()
            changed[index] = value
            with self.subTest(index=index, value=value), self.assertRaises(ValueError):
                parse_samples("\n".join(changed), LOOPS, REPS, BLOCKS)

    def test_invalid_budget_rejected(self):
        for loops, reps, blocks in ((999, 4, 4), (1000, 0, 4),
                                    (1000, 4, 3), (1000, 4, 34)):
            with self.subTest(loops=loops, reps=reps, blocks=blocks), self.assertRaises(ValueError):
                parse_samples("\n".join(fixture()), loops, reps, blocks)

    def test_saturating_reference_unbounded_arithmetic(self):
        self.assertIsInstance(reference(1000), int)
        self.assertEqual(reference(1000), 501)
        self.assertEqual(reference(1002), 502)

    def test_timing_is_not_classified_as_verified_speedup(self):
        data = parse_samples("\n".join(fixture()), LOOPS, REPS, BLOCKS)
        report = summarize(data["samples"], BLOCKS)
        self.assertEqual(report["status"], "exploratory-on-one-virtualized-host-no-runtime-winner")
        self.assertGreater(report["median_ratio"], 0)

if __name__ == "__main__":
    unittest.main()
