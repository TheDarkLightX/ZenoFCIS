"""Fail-closed checks for the proof-report acceptance boundary."""

import unittest

import check_lean_proofs as check


class ProofReports(unittest.TestCase):
    source = "theorem example_proof : True := by trivial\n#print axioms example_proof\n"

    def test_accepts_permitted_axioms_and_axiom_free_reports(self):
        for report in (
            "'N.example_proof' does not depend on any axioms",
            "'N.example_proof' depends on axioms: [propext, Quot.sound, Classical.choice]",
        ):
            check.check_output(self.source, report)

    def test_refuses_missing_duplicate_or_wrong_theorem_reports(self):
        good = "'N.example_proof' does not depend on any axioms"
        for report in ("", good + "\n" + good, good.replace("example_proof", "different")):
            with self.subTest(report=report), self.assertRaises(ValueError):
                check.check_output(self.source, report)

    def test_refuses_placeholders_unexpected_axioms_and_diagnostics(self):
        for source, report in (
            (self.source.replace("trivial", "sorry"), ""),
            (self.source.replace("trivial", "admit"), ""),
            (self.source, "'N.example_proof' depends on axioms: [sorryAx]"),
            (self.source, "'N.example_proof' depends on axioms: [unprovedClaim]"),
            (self.source, "file:1:1: error: failed"),
            (self.source, "file:1:1: warning: declaration uses a placeholder"),
        ):
            with self.subTest(report=report), self.assertRaises(ValueError):
                check.check_output(source, report)


if __name__ == "__main__":
    unittest.main()
