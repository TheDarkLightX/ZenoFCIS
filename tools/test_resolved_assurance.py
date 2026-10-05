"""Focused refusal/delegation regressions and independent retained guardrails."""

from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import check_assurance as assurance
from resolved_purity import ResolvedChecker, ResolutionError


def report(status="clean", findings=None, unreadable=None):
    return {"schema": "zeno-fcis/purity-report/2", "status": status,
            "resolution": {"active_files": ["input.rs"], "skipped_cfg": []},
            "findings": findings or [], "unreadable": unreadable or []}


class SemanticDelegation(unittest.TestCase):
    def test_actual_checker_is_required(self):
        checker = ResolvedChecker()
        with self.assertRaises(ResolutionError):
            checker.check([Path("input.rs")])

    def test_nonresolved_or_inconsistent_process_results_refuse(self):
        checker = ResolvedChecker()
        checker.binary = Path("fixture-only-unused-binary")
        for value, code in [(report(), 1), (dict(report(), schema="old-scanner"), 0),
                            (dict(report(), resolution=None), 0), (report("unknown"), 0)]:
            with self.subTest(value=value, code=code), patch("resolved_purity.subprocess.run") as run:
                run.return_value = subprocess.CompletedProcess([], code, json.dumps(value), "")
                with self.assertRaises(ResolutionError):
                    checker.check([Path("input.rs")])

    def test_missing_or_malformed_report_refuses(self):
        checker = ResolvedChecker()
        checker.binary = Path("fixture-only-unused-binary")
        with patch("resolved_purity.subprocess.run") as run:
            run.return_value = subprocess.CompletedProcess([], 0, "not json", "")
            with self.assertRaises(ResolutionError):
                checker.check([Path("input.rs")])

    def test_assurance_checks_the_complete_semantic_set_in_one_call(self):
        calls = []

        class FixtureChecker:
            def check(self, paths):
                calls.append(paths)
                return report()

        with patch.object(Path, "is_dir", return_value=True):
            self.assertEqual(assurance.check_semantic_sources(FixtureChecker()), [])
        self.assertEqual(len(calls), 1)
        self.assertEqual([path.name for path in calls[0]], list(assurance.SEMANTIC_CRATES))

    def test_resolution_refusal_cannot_become_assurance_success(self):
        class RefusingChecker:
            def check(self, paths):
                raise ResolutionError("unsupported expansion")

        with patch.object(Path, "is_dir", return_value=True):
            self.assertTrue(assurance.check_semantic_sources(RefusingChecker()))
        self.assertTrue(assurance.semantic_report_failures(report("violations")))

    def test_float_warning_and_unreadability_remain_failures(self):
        finding = {"file": "input.rs", "line": 1, "column": 1, "severity": "warning",
                   "rule": "floating-point", "subject": "f64"}
        self.assertTrue(assurance.semantic_report_failures(report(findings=[finding])))
        self.assertTrue(assurance.semantic_report_failures(
            report("unreadable", unreadable=[{"file": "missing.rs", "message": "absent"}])))


class RetainedGuardrails(unittest.TestCase):
    def test_wire_allocation_guard_rejects_raw_count(self):
        requirement = (r"Vec::with_capacity\(initial_collection_capacity\(count,cursor\.remaining\(\),1,?\)\?\)",)
        self.assertEqual(assurance.check_decoder_region_text("fixture",
            "Vec::with_capacity(initial_collection_capacity(count, cursor.remaining(), 1)?)", requirement), [])
        self.assertTrue(assurance.check_decoder_region_text("mutant", "Vec::with_capacity(count)", requirement))

    def test_workflow_guard_rejects_tag_and_excess_permissions(self):
        self.assertTrue(assurance.check_workflow_text(assurance.PAGES_WORKFLOW,
            "  uses: actions/deploy-pages@v5\n"))
        self.assertTrue(assurance.check_workflow_text(assurance.PAGES_WORKFLOW,
            "permissions:\n  pages: write\n  id-token: write\n  contents: write\n"))

    def test_exact_external_pin_and_dependency_ring_guards_survive(self):
        with tempfile.TemporaryDirectory(prefix="zeno-guardrail-test-") as directory:
            root = Path(directory)
            manifest = root / "Cargo.toml"
            with patch.object(assurance, "ROOT", root):
                manifest.write_text("[package]\nname='zeno-fcis-core'\n[dependencies]\nexternal='1.0'\n")
                self.assertTrue(assurance.check_external_dependency_pins(manifest))
                manifest.write_text("[package]\nname='zeno-fcis-core'\n[dependencies]\nexternal='=1.0'\n")
                self.assertEqual(assurance.check_external_dependency_pins(manifest), [])
                manifest.write_text("[package]\nname='zeno-fcis-core'\n[dependencies]\nzeno-fcis-authority='=0.0.0'\n")
                self.assertTrue(assurance.check_dependency_ring(manifest))


class RepairBoundaryInterpretation(unittest.TestCase):
    def test_package_failure_does_not_require_an_active_source(self):
        finding = {"file": "Cargo.toml", "line": 0, "column": 0, "severity": "error",
                   "rule": "resolution-target", "subject": "no established source root"}
        refused = report("violations", findings=[finding])
        refused["resolution"]["active_files"] = []
        failures = assurance.semantic_report_failures(refused)
        self.assertEqual(len(failures), 1)
        self.assertIn("resolution-target", failures[0])

    def test_each_repair_refusal_remains_blocking_in_assurance(self):
        for rule in ("resolution-macro", "resolution-target", "resolution-manifest",
                     "resolution-attribute", "address", "resolution-unresolved"):
            with self.subTest(rule=rule):
                finding = {"file": "input.rs", "line": 1, "column": 1,
                           "severity": "error", "rule": rule, "subject": "unsupported boundary"}
                self.assertTrue(assurance.semantic_report_failures(
                    report("violations", findings=[finding])))


if __name__ == "__main__":
    unittest.main()
