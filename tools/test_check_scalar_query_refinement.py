#!/usr/bin/env python3
"""Fail-closed admission controls for the actual scalar proof gate."""
from copy import deepcopy
from pathlib import Path
from subprocess import CompletedProcess
import unittest
from unittest.mock import patch

import check_scalar_query_refinement as gate


class AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.pin = {"version": "pinned", "commit": "commit",
                    "rust_toolchain": "rust-toolchain"}
        self.profile = {"expected_verified": 1326, "target_functions": ["checked"]}
        self.report = {
            "verus": {"version": "pinned", "commit": "commit",
                      "toolchain": "rust-toolchain (extra version text)"},
            "verification-results": {
                "success": True, "verified": 1326, "errors": 0,
                "encountered-error": False, "encountered-vir-error": False,
                "is-verifying-entire-crate": True,
            },
            "func-details": {"checked": {}},
        }
        self.process = CompletedProcess(["verus"], 0, "", "")

    def positive(self, report=None, process=None):
        return gate.positive_accepted(process or self.process,
            self.report if report is None else report, self.pin, self.profile)

    def test_positive_requires_whole_proof_and_exact_pin_and_inventory(self):
        self.assertTrue(self.positive())
        for section, key, value in (
            ("verification-results", "is-verifying-entire-crate", False),
            ("verification-results", "errors", 1),
            ("verification-results", "errors", False),
            ("verification-results", "verified", 1325),
            ("verification-results", "success", False),
            ("verification-results", "encountered-vir-error", True),
            ("verus", "version", "other"),
            ("verus", "commit", "other"),
            ("verus", "toolchain", "other"),
        ):
            with self.subTest(key=key, value=value):
                report = deepcopy(self.report)
                report[section][key] = value
                self.assertFalse(self.positive(report))
        report = deepcopy(self.report)
        report["func-details"] = {}
        self.assertFalse(self.positive(report))
        self.assertFalse(self.positive(process=CompletedProcess([], 1, "", "")))

    def test_resource_or_frontend_diagnostics_never_become_positive(self):
        for message in ("error[E0308]: wrong type", "error: assertion failed",
                        "unknown", "reached the resource limit", "out of memory"):
            with self.subTest(message=message):
                self.assertFalse(self.positive(process=CompletedProcess([], 0, "", message)))

    def negative(self):
        report = deepcopy(self.report)
        # The pinned verifier omits success on actual semantic failures.
        del report["verification-results"]["success"]
        report["verification-results"].update(
            verified=4, errors=1, **{
                "encountered-error": True, "is-verifying-entire-crate": False
            })
        source = "crates/zeno-fcis-synthesis/src/finite/ir/program.rs"
        stderr = ("error: postcondition not satisfied\n"
                  f"  --> {source}:29:72\n"
                  "   | failed this postcondition\n"
                  "error: aborting due to 1 previous error\n")
        return report, CompletedProcess(["verus"], 1, "", stderr), Path(source)

    def test_real_targeted_failure_requires_exact_source_region_and_result(self):
        report, proc, source = self.negative()
        check = lambda p=proc, r=report, s=source, region=(27, 34): gate.target_refused(
            p, r, self.pin, s, Path("/specimen"), region)
        self.assertTrue(check())
        false_status = deepcopy(report)
        false_status["verification-results"]["success"] = False
        self.assertTrue(check(r=false_status))
        for status in (True, None, 0, "false"):
            wrong_status = deepcopy(report)
            wrong_status["verification-results"]["success"] = status
            self.assertFalse(check(r=wrong_status))
        self.assertFalse(check(region=(30, 34)))
        self.assertFalse(check(s=Path("another/owner.rs")))
        for key, value in (("errors", 2), ("errors", True), ("verified", 0),
                           ("encountered-error", False),
                           ("encountered-vir-error", True),
                           ("is-verifying-entire-crate", True)):
            changed = deepcopy(report)
            changed["verification-results"][key] = value
            self.assertFalse(check(r=changed))
        for diagnostic in ("error[E0308]: wrong type\n",
                           "error: assertion failed\n", "UNKNOWN\n",
                           "error: aborting due to 2 previous errors\n"):
            self.assertFalse(check(p=CompletedProcess([], 1, "", diagnostic + proc.stderr)))

    def test_targeted_abort_allows_only_the_pinned_numeric_warning_suffix(self):
        report, proc, source = self.negative()
        for suffix in ("; 2 warnings emitted", "; 1 warning emitted"):
            diagnostic = proc.stderr.replace("aborting due to 1 previous error",
                                             "aborting due to 1 previous error" + suffix)
            self.assertTrue(gate.target_refused(CompletedProcess([], 1, "", diagnostic),
                report, self.pin, source, Path("/specimen"), (27, 34)))
        for suffix in ("; warnings emitted", "; 2 warnings emitted; other failure",
                       "; 0 warnings emitted", "; unknown"):
            diagnostic = proc.stderr.replace("aborting due to 1 previous error",
                                             "aborting due to 1 previous error" + suffix)
            self.assertFalse(gate.target_refused(CompletedProcess([], 1, "", diagnostic),
                report, self.pin, source, Path("/specimen"), (27, 34)))

    def test_absent_or_malformed_reports_fail_closed(self):
        report, proc, source = self.negative()
        for bad in ({}, {"verus": []}, None):
            self.assertFalse(gate.target_refused(proc, bad, self.pin,
                source, Path("/specimen"), (27, 34)))

    def test_body_fault_must_preserve_every_other_translated_contract(self):
        baseline = {"target": {"mode": "Exec", "requires": 0, "ensures": 1,
                               "signature_sha256": "signature",
                               "ensures_sha256": "ensures", "body_sha256": "old"},
                    "other": {"mode": "Spec", "body_sha256": "spec"}}
        profile = {"body_covered_functions": ["target"], "functions": baseline,
                   "abstract_declarations": {"descriptor": "unchanged"}}
        changed = deepcopy(baseline)
        changed["target"]["body_sha256"] = "new"
        with patch.object(gate, "observed_inventory",
                          return_value=(changed, profile["abstract_declarations"])):
            gate.require_body_only_change("translated VIR", profile, "target")
        for name, field, value in (
            ("target", "ensures_sha256", "weakened"),
            ("target", "signature_sha256", "narrowed"),
            ("other", "body_sha256", "changed-spec"),
        ):
            bad = deepcopy(changed)
            bad[name][field] = value
            with patch.object(gate, "observed_inventory",
                              return_value=(bad, profile["abstract_declarations"])):
                with self.assertRaises(ValueError):
                    gate.require_body_only_change("translated VIR", profile, "target")
        with patch.object(gate, "observed_inventory", return_value=(baseline, {})):
            with self.assertRaises(ValueError):
                gate.require_body_only_change("translated VIR", profile, "target")


if __name__ == "__main__":
    unittest.main()

