#!/usr/bin/env python3
"""Closed-vocabulary forced-correct overflow rule and cost-refusal tests."""
import unittest
from unittest.mock import patch

from install_overflow_forced_28 import (
    MODES, FORCED_RULE, NEW_NAME, RULE, RULE_NAME, source,
)
from verify_overflow_forced_veriisle_28 import FIELDS
from qualify_overflow_forced_28 import SCHEMA
from independent_recheck_overflow_forced_28 import SCHEMA_OUT


class ForcedRuleTests(unittest.TestCase):
    def test_exact_same_lhs_and_subsume_only_change(self):
        self.assertEqual(MODES, ("baseline", "unforced", "forced"))
        self.assertIn("subsume", FORCED_RULE)
        self.assertEqual(FORCED_RULE.count("(subsume "), 1)
        self.assertEqual(FORCED_RULE.count("(rule "), 1)
        self.assertEqual(FORCED_RULE.count(NEW_NAME), 1)
        self.assertNotIn(RULE_NAME + "\n", FORCED_RULE)
        self.assertEqual(FORCED_RULE.replace(NEW_NAME, RULE_NAME, 1)
                         .replace("\n  (subsume (slt $I64\n",
                                  "\n  (slt $I64\n", 1)
                         .rstrip()[:-1] + "\n", RULE)
        self.assertEqual(SCHEMA, "zeno-fcis/tier1-overflow-correct-forced-AOT/1")
        self.assertTrue(SCHEMA_OUT.startswith("zeno-fcis/"))
        self.assertIn("Verification unknown", FIELDS)

    def test_forced_rule_parens_balance_excluding_comments(self):
        body = "\n".join(line for line in FORCED_RULE.splitlines()
                         if not line.lstrip().startswith(";;"))
        depth = 0
        for token in body:
            depth += (token == "(") - (token == ")")
            self.assertGreaterEqual(depth, 0)
        self.assertEqual(depth, 0)

    def test_unknown_palette_refused_before_git_access(self):
        for bad in ("", "xorand", "exact_canary", "../main", "production"):
            with self.subTest(mode=bad), self.assertRaises(ValueError):
                source(None, bad)


if __name__ == "__main__":
    unittest.main()
