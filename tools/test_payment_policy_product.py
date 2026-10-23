"""Acceptance diagnostics shared by the independent payment reference."""
import unittest

from check_payment_policy_product import check_actions


class PaymentActionTests(unittest.TestCase):
    def test_duplicate_precedes_silent_drop_of_earlier_key(self):
        self.assertEqual(check_actions([0, 1], [1, 1], [], []), 'duplicate commit')
        self.assertEqual(check_actions([1, 0], [1, 1], [], []), 'duplicate commit')

    def test_unauthorized_precedes_duplicate(self):
        self.assertEqual(check_actions([0], [1, 1], [], []), 'unauthorized commit')

    def test_silent_drop_requires_no_other_violation(self):
        self.assertEqual(check_actions([0, 1], [1], [], []), 'silent drop')
        self.assertIsNone(check_actions([0, 1], [1], [0], []))
        self.assertIsNone(check_actions([0, 1], [1], [], [0]))


if __name__ == '__main__':
    unittest.main()
