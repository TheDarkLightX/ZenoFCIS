#!/usr/bin/env python3
"""Unit tests for tools/loop_judges_controls.py: its comparisons, the size envelope and the domain count.

They run no CLI. This test is LLM-generated."""
from __future__ import annotations

from pathlib import Path
import copy
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import loop_judges_controls as C  # noqa: E402


class Controls(unittest.TestCase):
    def test_question_timing_rejects_slowness_and_semantic_drift(self):
        good = {"status": "determined", "answers": 8, "tuples": 432, "determined_with_reasons": 432,
                "inconsistent": 0, "answers_recorded": 8, "gaps": "measured", "identical_repeat": True, "seconds": 0.5}
        self.assertTrue(C.question_timing_passes(good))
        self.assertTrue(C.question_timing_passes(dict(good, seconds=1.8)))
        for key, value in (("seconds", 2.5), ("status", "inconclusive"), ("answers", 7), ("tuples", 431),
                           ("determined_with_reasons", 431), ("inconsistent", 1), ("answers_recorded", 7), ("gaps", "proposal-false-alarm"),
                           ("identical_repeat", False)):
            self.assertFalse(C.question_timing_passes(dict(good, **{key: value})), key)

    def test_question_timing_compares_complete_details_and_requires_success(self):
        import tempfile
        report = {"status": "determined", "detail": {"answers": [{"status": "recorded"}] * 8, "stats": {
            "tuples": 432, "determined_with_reasons": 432, "inconsistent": 0}, "gaps": {"status": "measured"}}}
        different = copy.deepcopy(report)
        different["detail"]["answers"][0] = {"status": "disagreement"}
        for repeats, expected in (([(0, report), (0, report)], True),
                                  ([(0, different), (0, different)], False),
                                  ([(0, report), (0, different)], False),
                                  ([(1, report), (0, report)], False)):
            with tempfile.TemporaryDirectory() as scratch, patch.object(C, "run", side_effect=repeats), \
                    patch.object(C.time, "monotonic", side_effect=[0.0, 0.5]):
                row = C.question_timing_control(Path("/fixture/cli"), Path("/fixture/contract"), Path(scratch))
                self.assertEqual(row["passed"], expected)

    def test_the_self_test_passes(self):
        self.assertEqual(C.self_test(), [])

    def test_the_domain_counts_declared_values_of_each_field(self):
        fields = {"names": ["value", "action", "authorized"], "ranges": [[0, 8], [150, 151], [0, 1]],
                  "variants": {"action": {"Inc": 150, "Dec": 151}}}
        self.assertEqual(C.domain_size(fields), 9 * 2 * 2)
        # a sum type with sparse IDs counts its variants, not the span of their IDs
        sparse = {"names": ["phase"], "ranges": [[150, 160]], "variants": {"phase": {"A": 150, "B": 160}}}
        self.assertEqual(C.domain_size(sparse), 2)

    def test_the_envelope_reports_large_rows_and_keeps_wrong_verdicts(self):
        big = C.SIZE_ENVELOPE + 1
        stopped = C.enveloped("teach_back", C.failed({"instance": "x"}, {"status": "inconclusive"}, "door"), big)
        self.assertIsNone(stopped["passed"])
        self.assertTrue(stopped["result"].startswith("outside-size-envelope"))
        self.assertEqual(stopped["domain"], big)
        wrong = C.enveloped("teach_back", C.judged("teach_back", {"own": "exact", "changed": "exact",
                                                                    "exceptions": 0, "region": 4}), big)
        self.assertIs(wrong["passed"], False)
        # inside the envelope, or for other controls, nothing changes
        inside = C.enveloped("teach_back", C.failed({"instance": "y"}, {"status": "inconclusive"}, "door"),
                             C.SIZE_ENVELOPE)
        self.assertIs(inside["passed"], False)
        composed = C.enveloped("composed", C.failed({"instance": "z"}, {"status": "inconclusive"}, "door"), big)
        self.assertIs(composed["passed"], False)
        # a structural not-applicable row stays not applicable
        na = C.enveloped("disagreement", C.not_applicable({"instance": "w"}, "only the core's option"), big)
        self.assertNotIn("outside_size_envelope", na)


if __name__ == "__main__":
    unittest.main()
