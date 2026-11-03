"""Dependency-free refusal controls; no claim of executing actual VeriISLE."""
import unittest
from verify_isle_rule_28 import parse_summary, RULE_NAME

VALID = f'''========================== Verification configuration =========================
only rule:          {RULE_NAME}
==========================
============================= Verification summary ============================
Total expansions:    2
In scope expansions: 2
Expansion errors:    0
Type instantiations: 1
Applicable:          1
Verification passed: 1
Verification failed: 0
Verification unknown: 0
===============================================================================
'''

class ParseTests(unittest.TestCase):
    def test_accepts_targeted_applicable_complete_success(self):
        self.assertEqual(parse_summary(VALID)['verification_passed'], 1)

    def test_rejects_nonzero_exit(self):
        with self.assertRaisesRegex(ValueError, 'nonzero'):
            parse_summary(VALID, returncode=1)

    def test_rejects_unknown_failures_errors_and_vacuity(self):
        edits = (('Verification unknown: 0', 'Verification unknown: 1'),
                 ('Verification failed: 0', 'Verification failed: 1'),
                 ('Expansion errors:    0', 'Expansion errors:    1'),
                 ('Applicable:          1', 'Applicable:          0'),
                 ('Verification passed: 1', 'Verification passed: 0'),
                 ('only rule:          '+RULE_NAME, 'only rule:          wrong_rule'),
                 ('Type instantiations: 1', 'Type instantiations: 0'))
        for before, after in edits:
            with self.subTest(change=after), self.assertRaises(ValueError):
                parse_summary(VALID.replace(before, after))

    def test_rejects_ambiguous_summary_and_counters(self):
        for changed in (VALID + VALID, VALID + '\nApplicable: 1\n',
                        VALID.replace('Verification summary', 'Other summary')):
            with self.subTest(), self.assertRaises(ValueError):
                parse_summary(changed)

if __name__ == '__main__':
    unittest.main()
