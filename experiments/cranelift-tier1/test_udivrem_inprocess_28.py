#!/usr/bin/env python3
"""Fail-closed regression for the exact archived udiv/urem AOT schema."""
import unittest
from qualify_udivrem_source_palette_28 import SCHEMA as PRODUCER_SCHEMA
from qualify_udivrem_inprocess_28 import (
    SOURCE_SCHEMA, SCHEMA, EXPECTED_SOURCE_SHA256, AOT, expected_checksum, inputs,
)

class TestUdivRemCallTimingQualification(unittest.TestCase):
    def test_producer_consumer_schema_matches_pinned_receipt(self):
        expected = "zeno-fcis/tier1-udiv-urem-source-palette/1"
        self.assertEqual(PRODUCER_SCHEMA, expected)
        self.assertEqual(SOURCE_SCHEMA, expected)
        self.assertEqual(SCHEMA, "zeno-fcis/udivrem-source-palette-call-timing/1")

    def test_exact_source_and_both_aot_pins(self):
        self.assertEqual(EXPECTED_SOURCE_SHA256,
                         "47d6b0bb6bc2b2c4254ad78ac7959d3d79638b92155f34f9e7dc17237d72c61a")
        self.assertEqual(set(AOT), {"direct", "quotient_reuse"})
        self.assertTrue(all(len(x) == 64 for x in AOT.values()))
        self.assertNotEqual(AOT["direct"], AOT["quotient_reuse"])

    def test_reference_timing_inputs_and_checksum(self):
        pairs = inputs()
        self.assertEqual(len(pairs), 268)
        self.assertTrue(all(y != 0 for x, y in pairs))
        self.assertEqual(expected_checksum(10000) & ((1 << 64) - 1), expected_checksum(10000))

if __name__ == "__main__":
    unittest.main()
