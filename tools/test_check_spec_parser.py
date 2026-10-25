"""Fail-closed admission tests for the parser's proof evidence gate."""
import copy
from types import SimpleNamespace
import unittest

from check_spec_parser import accepted_kernel, accepted_native


class Admission(unittest.TestCase):
    def setUp(self):
        self.pin = {"version": "pin", "commit": "revision", "rust_toolchain": "rust"}
        self.kernel = {"expected_verified": 3, "target_functions": ["runtime::accept"]}
        self.proc = SimpleNamespace(returncode=0)
        self.report = {
            "verus": {"version": "pin", "commit": "revision", "toolchain": "rust"},
            "func-details": {"runtime::accept": {}},
            "verification-results": {
                "success": True, "errors": 0, "verified": 3,
                "encountered-error": False, "encountered-vir-error": False,
                "is-verifying-entire-crate": True,
            },
        }

    def test_exact_complete_observation_is_accepted(self):
        self.assertTrue(accepted_kernel(self.proc, self.report, self.pin, self.kernel))

    def test_proof_errors_partial_runs_and_missing_inventory_are_refused(self):
        for key, value in [("errors", 1), ("verified", 2), ("verified", True),
                           ("success", False), ("is-verifying-entire-crate", False),
                           ("encountered-error", True), ("encountered-vir-error", True)]:
            with self.subTest(key=key, value=value):
                report = copy.deepcopy(self.report)
                report["verification-results"][key] = value
                self.assertFalse(accepted_kernel(self.proc, report, self.pin, self.kernel))
        report = copy.deepcopy(self.report)
        report["func-details"] = {}
        self.assertFalse(accepted_kernel(self.proc, report, self.pin, self.kernel))

    def test_stale_tool_and_nonzero_exit_are_refused(self):
        for key in ["version", "commit", "toolchain"]:
            report = copy.deepcopy(self.report)
            report["verus"][key] = "different"
            self.assertFalse(accepted_kernel(self.proc, report, self.pin, self.kernel))
        self.assertFalse(accepted_kernel(SimpleNamespace(returncode=1), self.report, self.pin, self.kernel))

    def test_native_summary_uses_exact_count_and_no_skipped_tests(self):
        summary = "test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.0s\n"
        self.assertTrue(accepted_native(SimpleNamespace(returncode=0, stdout=summary), 15))
        for changed in [summary.replace("15 passed", "115 passed"),
                        summary.replace("0 ignored", "1 ignored"),
                        summary.replace("0 filtered out", "1 filtered out"),
                        summary + summary, "compiler error"]:
            self.assertFalse(accepted_native(SimpleNamespace(returncode=0, stdout=changed), 15))
        self.assertFalse(accepted_native(SimpleNamespace(returncode=1, stdout=summary), 15))


if __name__ == "__main__":
    unittest.main()
