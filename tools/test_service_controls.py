"""Regression for the review's setup panic incorrectly counted as an order kill."""
from pathlib import Path
import unittest

import check_service_controls as controls

ORDER_TEST = 'first_refusal_obeys_manifest_order'
# Minimized from the reviewer's actual failed control, including its panic site.
REVIEWER_SETUP_PANIC = """running 1 test
thread 'checked_composition::service_tests::first_refusal_obeys_manifest_order' (529) panicked at crates/zeno-fcis-contract/src/checked_composition/domain_tests.rs:19:5:
test operation failed: "genesis:post-root-mismatch"
test checked_composition::service_tests::first_refusal_obeys_manifest_order ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 147 filtered out; finished in 0.09s
"""
# The review's independent request-only mutant reached this assertion instead.
REQUEST_ORDER_FAILURE = REVIEWER_SETUP_PANIC.replace(
    'domain_tests.rs:19:5', 'service_tests.rs:68:5').replace(
    'test operation failed: "genesis:post-root-mismatch"',
    'fixed order selects counter refusal: reject:budget:200')


class ServiceControls(unittest.TestCase):
    def test_reviewer_setup_panic_is_not_the_order_assertion(self):
        self.assertFalse(controls.expected_failure(101, REVIEWER_SETUP_PANIC, ORDER_TEST),
                         'genesis setup failure must not count as the request-order witness')

    def test_reviewers_request_order_assertion_is_recognized(self):
        self.assertTrue(controls.expected_failure(101, REQUEST_ORDER_FAILURE, ORDER_TEST))

    def test_wrong_assertion_site_is_rejected(self):
        for wrong in ['service_tests.rs:69:5', 'domain_tests.rs:68:5']:
            with self.subTest(site=wrong):
                self.assertFalse(controls.expected_failure(101, REQUEST_ORDER_FAILURE.replace(
                    'service_tests.rs:68:5', wrong), ORDER_TEST))

    def test_wrong_witness_message_is_rejected(self):
        self.assertFalse(controls.expected_failure(101, REQUEST_ORDER_FAILURE.replace(
            'reject:budget:200', 'unrelated failure'), ORDER_TEST))

    def test_unrelated_or_multiple_panics_are_rejected(self):
        self.assertFalse(controls.expected_failure(101, REQUEST_ORDER_FAILURE.replace(
            "thread 'checked_composition::service_tests::first_refusal_obeys_manifest_order'",
            "thread 'another_test'"), ORDER_TEST))
        self.assertFalse(controls.expected_failure(101, REQUEST_ORDER_FAILURE * 2, ORDER_TEST))

    def test_nonfailure_or_incomplete_test_runs_are_rejected(self):
        for code in [0, 1, -6]:
            with self.subTest(code=code):
                self.assertFalse(controls.expected_failure(code, REQUEST_ORDER_FAILURE, ORDER_TEST))
        for before, after in [('running 1 test', 'running 0 tests'),
                              ('0 passed; 1 failed;', '1 passed; 1 failed;')]:
            with self.subTest(summary=after):
                self.assertFalse(controls.expected_failure(101, REQUEST_ORDER_FAILURE.replace(
                    before, after), ORDER_TEST))

    def test_ansi_formatting_does_not_hide_the_assertion(self):
        self.assertTrue(controls.expected_failure(101, '\x1b[1m' + REQUEST_ORDER_FAILURE
                                                 + '\x1b[0m', ORDER_TEST))

    def test_each_declared_site_is_an_assertion_in_its_named_test(self):
        root = Path(__file__).resolve().parents[1]
        for test, (path, line, message) in controls.ASSERTIONS.items():
            with self.subTest(test=test):
                lines = (root / path).read_text().splitlines()
                self.assertTrue(lines[line - 1].strip().startswith('assert!('))
                self.assertIn('fn ' + test + '()', '\n'.join(lines[:line]))
                self.assertTrue(message)


if __name__ == '__main__':
    unittest.main()
