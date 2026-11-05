#!/usr/bin/env python3
"""Nonexecuting source parser checks; actual CLIF checks require pinned CI."""
import unittest
from diagnose_rust_clif_shape_28 import TOKENS, describe_clif


class TokenDiagnostics(unittest.TestCase):
    def test_distinguish_code_tokens_from_comments_and_substrings(self):
        report = describe_clif(
            "v1 = icmp slt v0, v2\n"
            "v3 = select v1, v0, v2\n"
            "; select, ushr, brif are comments\n"
            "v4 = ushr v3, v5\n"
            "v5 = iadd v4, v3 ; select comment\n"
            "never_select v0\n")
        counts = report["token_counts"]
        self.assertEqual(counts["select"], 1)
        self.assertEqual(counts["icmp"], 1)
        self.assertEqual(counts["ushr"], 1)
        self.assertEqual(counts["iadd"], 1)
        self.assertEqual(counts["brif"], 0)
        self.assertEqual(set(counts), set(TOKENS))
        self.assertEqual(len(report["sample_instructions"]), 4)

    def test_empty_ir_is_refused(self):
        for src in ("", "\n\t   "):
            with self.assertRaisesRegex(ValueError, "empty emitted CLIF"):
                describe_clif(src)

    def test_excerpts_are_bounded_and_line_numbers_explicit(self):
        src = "\n".join(f"v{i} = iadd v0, v1" for i in range(100))
        record = describe_clif(src)
        self.assertEqual(record["token_counts"]["iadd"], 100)
        self.assertEqual(len(record["sample_instructions"]), 25)
        self.assertEqual(record["sample_instructions"][0]["line"], 1)
        self.assertEqual(record["sample_instructions"][-1]["line"], 25)

    def test_result_does_not_claim_isle_application(self):
        record = describe_clif("v2 = select v0, v1, v3\n")
        self.assertIn("not-ISLE-match", record["scope"])
        self.assertEqual(record["token_counts"]["select"], 1)


if __name__ == "__main__":
    unittest.main()
