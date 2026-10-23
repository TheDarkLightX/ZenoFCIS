"""Coverage-loop authority, refusal and native regression checks."""

import dataclasses
import unittest
from pathlib import Path
from unittest.mock import patch

import behavior2
import factory

HERE = Path(__file__).resolve().parent


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
