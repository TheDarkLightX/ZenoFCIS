"""Coverage-loop authority, refusal and native regression checks."""

import dataclasses
import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import behavior2
import factory
import loop_coverage as coverage

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


class CoverageVerdictTests(unittest.TestCase):
    def setUp(self):
        self.before_packet = json.loads((HERE / "test-data/coverage/before.json").read_text())
        self.after_packet = json.loads((HERE / "test-data/coverage/after.json").read_text())
        self.project = (ROOT / "examples/guide-counter/project.zeno").read_text()
        self.policy = json.loads((ROOT / "examples/guide-counter/v2/policy.json").read_text())
        self.before = coverage.snapshot(self.before_packet, self.project, self.policy)

    def assess(self, packet, code=0):
        return coverage.assess(packet, code, self.before, self.project, self.policy)

    def test_native_progress_is_required_for_kept(self):
        verdict, detail = self.assess(self.after_packet)
        self.assertEqual(verdict, "kept")
        self.assertEqual((detail["gaps_closed"], detail["mutants_rejected"]), (4, 6))
        verdict, _ = self.assess(self.before_packet)
        self.assertEqual(verdict, "rejected", "a law closing nothing was kept")

    def test_planted_disagreement_overrides_apparent_progress(self):
        planted = copy.deepcopy(self.after_packet)
        planted["gaps"]["status"] = "proposal-false-alarm"
        planted["gaps"]["false_alarm"] = {"law": 9000, "input": [0, 150, 1], "contract": {"class": "Accept"}}
        verdict, detail = self.assess(planted, 1)
        self.assertEqual(verdict, "disagreeing", "binary disagreement promoted to kept")
        self.assertEqual(detail["witness"], planted["gaps"]["false_alarm"])

    def test_unclassified_witnesses_survive(self):
        with patch.object(coverage, "DIMENSION_RULES", ()):
            rows = coverage.witnesses(coverage.measurement(self.before_packet), self.before_packet,
                                      self.project, self.policy)
        self.assertEqual(len(rows), 12, "unclassified witnesses were dropped")
        self.assertTrue(all(w["dimensions"] == ["unclassified"] for w in rows))

    def test_budget_never_becomes_a_measured_result(self):
        with self.assertRaises(coverage.JudgeStop) as raised:
            self.assess({"gaps": {"status": "inconclusive", "reason": "work-over-limit"}}, 2)
        self.assertEqual(raised.exception.code, 2)

    def test_input_packets_are_not_changed_by_assessment(self):
        original = copy.deepcopy(self.after_packet)
        self.assess(self.after_packet)
        self.assertEqual(original, self.after_packet)

    def test_disagreement_is_present_in_json_and_reading(self):
        candidate = coverage.Candidate("false", "proposed", ("law 9010 bad on any = false;",),
                                       ("authority",), "false")
        def review(judge, laws=()):
            if not laws:
                return 0, self.before_packet, "before.json"
            bad = {"gaps": {"status": "proposal-false-alarm", "false_alarm": {"law": 9010, "input": [0, 150, 1]}}}
            return 1, bad, "bad.json"
        with tempfile.TemporaryDirectory() as tmp:
            options = type("Options", (), {"directory": ROOT / "examples/guide-counter", "out": Path(tmp) / "report",
                                           "proposals": None, "zal": None, "max_work": None, "rounds": 3})()
            binary = Path(tmp) / "binary"
            binary.write_text("fixture")
            with patch.dict(os.environ, {"ZENO_FCIS_BIN": str(binary)}), \
                 patch.object(coverage, "proposals", return_value=[candidate]), \
                 patch.object(coverage, "synthesize", return_value=[]), \
                 patch.object(coverage.Judge, "review", review):
                self.assertEqual(coverage.run(options), 1)
            report = json.loads((options.out / "coverage.json").read_text())
            self.assertEqual(len(report["rounds"][0]["disagreeing"]), 1, "disagreement silently dropped")
            self.assertIn("Disagreement witness:", (options.out / "COVERAGE.md").read_text())


class ProposalLoweringTests(unittest.TestCase):
    def test_export_and_proposals_share_expression_lowering(self):
        model = behavior2.parse((HERE / "examples/context_free2.zal").read_text())
        kind, formula = model.invariants[0]
        proposed = factory.profile2_proposal(model, "invariant", kind, formula, 501)
        self.assertIn(proposed[0], factory.lower_machine(model)["project.zeno"])

    def test_false_proposal_is_lowered_without_a_python_verdict(self):
        model = behavior2.parse((HERE / "examples/context_free2.zal").read_text())
        with patch.object(behavior2, "check", side_effect=AssertionError("Python verdict forbidden")):
            laws = factory.profile2_proposal(model, "require", "false_proposal", behavior2.Expr("false"), 9000)
        self.assertEqual(len(laws), 2)
        self.assertTrue(all("= false;" in law for law in laws))


if __name__ == "__main__":
    unittest.main()
