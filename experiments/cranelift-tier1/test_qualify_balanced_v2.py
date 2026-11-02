#!/usr/bin/env python3
"""Positive and deliberate tamper controls for the source-bound v2 cost catalog."""
from __future__ import annotations
import copy
import gzip
import json
import unittest
from pathlib import Path

import qualify_balanced_v2 as q

HERE = Path(__file__).resolve().parent
TIMING = HERE / "evidence/performance_balanced_v2.json.gz"
SIZE = HERE / "evidence/qualified_clif_ab.json"

class QualificationTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.balanced = json.loads(gzip.decompress(TIMING.read_bytes()))

    def test_full_historical_receipt_and_exact_object_bytes(self):
        q.check_balanced(self.balanced, HERE)
        output = q.build(SIZE, TIMING, HERE / "evidence/objects", HERE)
        self.assertEqual(output["code_size"]["candidate_symbol_bytes_saved"], 17)
        self.assertIn("unqualified", output["status"])
        self.assertIn("unknown", output["pareto_relation"])
        self.assertEqual(output["compiler_rewrite_status"],
                         "unqualified: manually different CLIF, not patched compiler")

    def test_refuse_forged_speed_status(self):
        changed = copy.deepcopy(self.balanced)
        changed["status"] = "qualified-faster"
        with self.assertRaisesRegex(ValueError, "no qualified speed"):
            q.check_balanced(changed, HERE)

    def test_refuse_forged_ratio(self):
        changed = copy.deepcopy(self.balanced)
        changed["summary"]["parallel8"]["median_balanced_candidate_over_baseline_ratio"] = 0.2
        with self.assertRaisesRegex(ValueError, "forged summary"):
            q.check_balanced(changed, HERE)

    def test_refuse_forged_per_round_ratio(self):
        changed = copy.deepcopy(self.balanced)
        changed["summary"]["parallel8"]["paired_round_ratios"][0] = 0.01
        with self.assertRaisesRegex(ValueError, "forged per-round"):
            q.check_balanced(changed, HERE)

    def test_refuse_reordered_raw_sample(self):
        changed = copy.deepcopy(self.balanced)
        rows = changed["raw_samples"]
        rows[0], rows[1] = rows[1], rows[0]
        with self.assertRaisesRegex(ValueError, "reordered position"):
            q.check_balanced(changed, HERE)

    def test_refuse_missing_row(self):
        changed = copy.deepcopy(self.balanced)
        changed["raw_samples"].pop()
        with self.assertRaisesRegex(ValueError, "exact raw sample"):
            q.check_balanced(changed, HERE)

    def test_refuse_unpinned_or_swapped_object(self):
        changed = copy.deepcopy(self.balanced)
        changed["original_object_sha256"]["satadd.o"] = q.CAND_HASH
        with self.assertRaisesRegex(ValueError, "object SHA-256"):
            q.check_balanced(changed, HERE)

    def test_refuse_bogus_source_sha(self):
        changed = copy.deepcopy(self.balanced)
        changed["benchmark_source_sha256"]["benchmark_order_balanced.c"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "benchmark source hash mismatch"):
            q.check_balanced(changed, HERE)

    def test_refuse_unpinned_cpu_claim(self):
        changed = copy.deepcopy(self.balanced)
        changed["host"]["pinned_cpu"] = 999
        with self.assertRaisesRegex(ValueError, "pinned CPU"):
            q.check_balanced(changed, HERE)

    def test_refuse_modified_receipt_bytes(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as t:
            altered = Path(t) / "timing.json"
            altered.write_text(json.dumps(self.balanced) + "\n")
            with self.assertRaisesRegex(ValueError, "historical balanced timing byte identity"):
                q.read_receipt(altered)

    def test_refuse_modified_object_bytes(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as t:
            folder = Path(t)
            for file in ("satadd.o", "satadd-shift.o"):
                (folder / file).write_bytes((HERE / "evidence/objects" / file).read_bytes())
            (folder / "satadd.o").write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "original emitted object bytes"):
                q.build(SIZE, TIMING, folder, HERE)

if __name__ == "__main__":
    unittest.main()
