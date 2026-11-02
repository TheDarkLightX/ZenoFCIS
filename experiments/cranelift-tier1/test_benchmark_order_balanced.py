#!/usr/bin/env python3
"""Positive and adversarial controls for balanced timing-row validation."""
import copy
import gzip
import hashlib
import json
import unittest
from pathlib import Path

from benchmark_order_balanced import check_rows

HERE = Path(__file__).resolve().parent

class OrderBalancedTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        raw = gzip.decompress((HERE / "evidence" / "performance_balanced_v2.json.gz").read_bytes())
        if hashlib.sha256(raw).hexdigest() != "2b760abd2567fa96d2c60bd51438dcb0605cb9927d3e61a7aae4d96e860da4c8":
            raise AssertionError("pinned balanced timing receipt changed")
        cls.receipt = json.loads(raw)

    def verify(self, rows):
        return check_rows(rows, self.receipt["calls_per_sample"], self.receipt["rounds"])

    def test_executed_receipt_valid(self):
        derived = self.verify(self.receipt["raw_samples"])
        for mode in derived:
            self.assertEqual(derived[mode], self.receipt["summary"][mode])

    def test_rejects_missing_sample(self):
        rows = copy.deepcopy(self.receipt["raw_samples"])
        rows.pop()
        with self.assertRaisesRegex(ValueError, "exactly four"):
            self.verify(rows)

    def test_rejects_swapped_slot(self):
        rows = copy.deepcopy(self.receipt["raw_samples"])
        rows[0]["variant"] = "candidate" if rows[0]["variant"] == "baseline" else "baseline"
        with self.assertRaisesRegex(ValueError, "ABBA/BAAB"):
            self.verify(rows)

    def test_rejects_corrupted_checksum(self):
        rows = copy.deepcopy(self.receipt["raw_samples"])
        rows[1]["checksum"] = "12345"
        with self.assertRaisesRegex(ValueError, "function results disagree"):
            self.verify(rows)

    def test_rejects_reordered_sample(self):
        rows = copy.deepcopy(self.receipt["raw_samples"])
        rows[0], rows[1] = rows[1], rows[0]
        with self.assertRaisesRegex(ValueError, "reordered position"):
            self.verify(rows)

    def test_rejects_bogus_time(self):
        rows = copy.deepcopy(self.receipt["raw_samples"])
        rows[0]["ns"] = "-1"
        with self.assertRaisesRegex(ValueError, "invalid round"):
            self.verify(rows)

    def test_rejects_unrecognized_schema(self):
        rows = copy.deepcopy(self.receipt["raw_samples"])
        rows[0]["schema"] = "v1"
        with self.assertRaisesRegex(ValueError, "unexpected record schema"):
            self.verify(rows)

if __name__ == "__main__":
    unittest.main()
