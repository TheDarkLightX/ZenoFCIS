#!/usr/bin/env python3
"""Dependency-free provenance, cost, and deliberate-tampering controls for 2.8."""
from __future__ import annotations
import copy
import gzip
import hashlib
import json
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

import qualify_pareto as q

HERE = Path(__file__).resolve().parent
SIZE = HERE / "evidence" / "qualified_clif_ab.json"
TIMING_GZ = HERE / "evidence" / "performance_full_20261102.json.gz"
EXPECTED_TIMING_SHA256 = "1d9042862ae4389760f6836b415d433faff633fce9524924d6ec5710488c0527"

class TestParetoEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.size = json.loads(SIZE.read_text())
        raw = gzip.decompress(TIMING_GZ.read_bytes())
        if hashlib.sha256(raw).hexdigest() != EXPECTED_TIMING_SHA256:
            raise AssertionError("pinned raw timing receipt changed")
        cls.raw = raw
        cls.timing = json.loads(raw)

    def test_original_receipts_admitted(self):
        q.check_size(self.size)
        q.check_perf(self.timing)
        with TemporaryDirectory() as folder:
            report = Path(folder) / "raw.json"
            report.write_bytes(self.raw)
            result = q.build(SIZE, report)
            self.assertEqual(result["size_comparison"]["symbol_byte_saving"], 17)
            self.assertEqual(result["entries"][1]["runtime_performance_status"], "unknown")
            self.assertEqual(result["automatic_compiler_optimization"], "not demonstrated")
            self.assertEqual(result["pareto_relation"],
                             "incomparable-until-application-relevant-performance-cost-is-qualified")

    def test_refuses_missing_independent_proof(self):
        altered = copy.deepcopy(self.size)
        altered["smt"]["status"] = "unknown"
        with self.assertRaisesRegex(ValueError, "independent QF_BV"):
            q.check_size(altered)

    def test_refuses_changed_cranelift_baseline(self):
        altered = copy.deepcopy(self.size)
        altered["results"]["baseline"]["object_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "baseline object"):
            q.check_size(altered)

    def test_refuses_forged_speed_certification(self):
        altered = copy.deepcopy(self.timing)
        altered["status"] = "qualified-speedup"
        with self.assertRaisesRegex(ValueError, "performance status"):
            q.check_perf(altered)

    def test_refuses_forged_timing_ratio(self):
        altered = copy.deepcopy(self.timing)
        altered["summary"]["chain_saturating"]["candidate_over_baseline_ratio"] = 0.1
        with self.assertRaisesRegex(ValueError, "recomputed timing"):
            q.check_perf(altered)

    def test_refuses_sample_removal(self):
        altered = copy.deepcopy(self.timing)
        altered["raw_samples"].pop()
        with self.assertRaisesRegex(ValueError, "full raw sample"):
            q.check_perf(altered)

    def test_refuses_unchecked_object_swap(self):
        altered = copy.deepcopy(self.timing)
        altered["original_object_sha256"]["satadd-shift.o"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "object identities"):
            q.check_perf(altered)

if __name__ == "__main__":
    unittest.main()
