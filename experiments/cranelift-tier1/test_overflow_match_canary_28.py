#!/usr/bin/env python3
"""Syntax/intent and diagnostic verdict controls (no Wasmtime build required)."""
import unittest

from install_rust_overflow_predicate_28 import RULE, RULE_NAME
from install_overflow_match_canary_28 import (
    MODES, EXACT_RULE, BROAD_RULE, source,
)
from qualify_overflow_match_canary_28 import (
    WITNESSES, FUNCTIONS, classify,
)
from independent_recheck_overflow_match_canary_28 import independent_value
from qualify_rust_idiom_wasm_28 import value

class CanaryControls(unittest.TestCase):
    def test_canaries_are_explicitly_unsound_and_isolated(self):
        self.assertEqual(MODES, ("baseline", "exact_canary", "broad_canary"))
        prefix = RULE.split("\n  (slt $I64\n       (band $I64\n", 1)[0]
        self.assertTrue(EXACT_RULE.startswith(prefix))
        self.assertEqual(EXACT_RULE.count("(rule "), 1)
        self.assertIn("(subsume (all_zero $I8))", EXACT_RULE)
        self.assertEqual(EXACT_RULE.count(RULE_NAME), 1)
        self.assertIn("(simplify (bxor $I8 a b))", BROAD_RULE)
        self.assertIn("(subsume (all_zero $I8))", BROAD_RULE)
        self.assertNotIn(RULE_NAME, BROAD_RULE)

    def test_out_of_vocabulary_rejected_before_source_read(self):
        for invalid in ("", "candidate", "exact", "../x", "prod"):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                source(None, invalid)

    def test_independent_signed_reference_and_overflow_witnesses(self):
        self.assertEqual(len(WITNESSES), 8)
        self.assertEqual(len(FUNCTIONS), 4)
        for fn in FUNCTIONS:
            for x, y in WITNESSES:
                with self.subTest(fn=fn, x=x, y=y):
                    self.assertEqual(independent_value(fn, x, y),
                                     value(fn, x, y))
        hi = (1 << 63) - 1
        lo = -(1 << 63)
        self.assertEqual(independent_value("sat_add", hi, 1), hi)
        self.assertEqual(independent_value("sat_add", lo, -1), lo)
        with self.assertRaises(ValueError):
            independent_value("nonexistent", 0, 0)

    def test_verdict_must_not_claim_match_absence_from_no_effect(self):
        def record(h, dis, b, t, errors):
            return {"sha256": h, "disassembly_sha256": dis,
                    "aot_bytes": b, "elf_text_bytes": t,
                    "witness": {"mismatch_count": errors}}
        base = record("a", "a", 100, 80, 0)
        unchanged = record("a", "a", 100, 80, 0)
        result = classify(base, unchanged, unchanged)
        self.assertEqual(result["verdict"],
            "both-canaries-native-code-identical-no-reachability-inference")
        broad_wrong = record("b", "b", 120, 96, 3)
        result = classify(base, unchanged, broad_wrong)
        self.assertEqual(result["verdict"],
            "broad-i8-xor-probe-observable-exact-probe-not-observable")
        self.assertEqual(result["canaries"]["broad_canary"]["complete_aot_delta_bytes"], -20)
        exact_wrong = record("c", "c", 140, 112, 5)
        result = classify(base, exact_wrong, broad_wrong)
        self.assertEqual(result["verdict"],
            "exact-pattern-probe-had-observable-incorrect-semantic-effect")
        self.assertEqual(result["canaries"]["exact_canary"]["observed_wrong_outputs"], 5)
        assert "speed" not in result["verdict"]

if __name__ == "__main__":
    unittest.main()
