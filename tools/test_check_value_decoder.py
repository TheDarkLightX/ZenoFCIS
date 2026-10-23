#!/usr/bin/env python3
"""Gate refusal and evidence-retention tests; synthetic reports are not proofs."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import check_value_decoder as gate
import check_verus as verifier


class DecoderGateTests(unittest.TestCase):
    def setUp(self):
        self.pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        self.profile = {"expected_verified": 7,
                        "target_functions": ["value_decoder::codec::decode_value"]}
        self.report = {
            "verus": {"version": self.pin["version"], "commit": self.pin["commit"],
                      "toolchain": self.pin["rust_toolchain"]},
            "verification-results": {"success": True, "errors": 0, "verified": 7,
                "encountered-error": False, "encountered-vir-error": False,
                "is-verifying-entire-crate": True},
            "func-details": dict.fromkeys(self.profile["target_functions"], {}),
        }

    def accepts(self, report=None, code=0, stderr=""):
        report = self.report if report is None else report
        proc = subprocess.CompletedProcess([], code, json.dumps(report), stderr)
        return gate.accepted(proc, report, self.pin, self.profile)

    def test_exact_success_is_required(self):
        self.assertTrue(self.accepts())
        for key, value in [("success", False), ("errors", 1), ("errors", False),
                           ("verified", 6), ("verified", 8), ("verified", True),
                           ("encountered-error", True), ("encountered-vir-error", True),
                           ("is-verifying-entire-crate", False)]:
            report = copy.deepcopy(self.report)
            report["verification-results"][key] = value
            self.assertFalse(self.accepts(report), (key, value))
        for key in ("version", "commit", "toolchain"):
            report = copy.deepcopy(self.report)
            report["verus"][key] = "wrong"
            self.assertFalse(self.accepts(report), key)
        report = copy.deepcopy(self.report)
        report["func-details"] = {}
        self.assertFalse(self.accepts(report))
        for code in (1, 2, -9, 137, None):
            self.assertFalse(self.accepts(code=code))
        for stderr in ("error: failed", "resource limit exceeded", "solver returned unknown",
                       "timed out", "internal compiler error", "panic"):
            self.assertFalse(self.accepts(stderr=stderr))

    def test_unqualified_or_boolean_expected_count_is_refused(self):
        for count in (0, -1, True, "7"):
            self.profile["expected_verified"] = count
            self.assertFalse(self.accepts())

    def test_malformed_report_is_refused(self):
        for value in (None, [], {}, {"verification-results": []}, {"verus": 0}):
            self.assertFalse(self.accepts(value if value is not None else []))
        for text in ("", "not json", "[]", "null"):
            proc = subprocess.CompletedProcess([], 1, text, "")
            self.assertEqual(gate.parse_report(proc), {})

    def test_four_unique_mutations_preserve_original_source(self):
        original = (gate.ROOT / gate.SUBJECT).read_bytes()
        source = original.decode()
        mutants = gate.mutation_sources(source)
        self.assertEqual(set(mutants), set(gate.SEMANTIC_TARGETS))
        self.assertEqual(len(mutants), 4)
        for name, changed in mutants.items():
            self.assertNotEqual(source, changed, name)
            start, end = verifier.function_lines(changed, gate.SEMANTIC_TARGETS[name])
            self.assertGreaterEqual(start, 1)
            self.assertGreater(end, start)
        self.assertEqual((gate.ROOT / gate.SUBJECT).read_bytes(), original)
        for source in ("no anchor", "same same"):
            with self.assertRaises(ValueError):
                gate.replace_once(source, "same", "different")

    def classify(self, name, *, code=1, extra="", unrelated=False, report=None):
        source = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text())[name]
        region = verifier.function_lines(source, gate.SEMANTIC_TARGETS[name])
        specimen = Path("/specimen")
        path = specimen / gate.SUBJECT
        if unrelated:
            path = Path("/unrelated.rs")
        stderr = f"error: postcondition not satisfied\n --> {path}:{region[0]}:1\n"
        stderr += "error: aborting due to 1 previous error\n" + extra
        if report is None:
            report = copy.deepcopy(self.report)
            report["verification-results"].update({"success": False, "errors": 1,
                                                   "encountered-error": True})
        proc = subprocess.CompletedProcess([], code, json.dumps(report), stderr)
        return gate.classify_mutation(proc, report, name, source, specimen, self.pin, self.profile)

    def test_controls_require_named_semantic_obligation(self):
        for name in gate.SEMANTIC_TARGETS:
            self.assertTrue(self.classify(name), name)
            self.assertFalse(self.classify(name, unrelated=True), name)
            for code in (0, 2, -9, 137, None):
                self.assertFalse(self.classify(name, code=code), (name, code))
            for text in ("SMT result UNKNOWN", "resource limit exceeded", "rlimit exceeded",
                         "timed out", "out of memory", "panic", "transport error",
                         "error[E0308]: mismatched types\n"):
                self.assertFalse(self.classify(name, extra=text), (name, text))
            self.assertFalse(self.classify(name, report=self.report), name)

    def test_unreviewed_application_or_standard_trust_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for path in gate.UNIT_SOURCES:
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((gate.ROOT / path).read_bytes())
            profile = {"std_contracts_sha256": verifier.digest(root / gate.STD_CONTRACTS)}
            gate.audit_trust(root, profile)
            subject = root / gate.SUBJECT
            original = subject.read_text()
            for escape in ("assume(false);", "admit();", "#[verifier::external_body]",
                           "#[verifier::external]", "pub assume_specification[f]();",
                           "#[verifier::exec_allows_no_decreases_clause]"):
                subject.write_text(original + "\n" + escape)
                with self.assertRaisesRegex(RuntimeError, "forbidden trust escape"):
                    gate.audit_trust(root, profile)
            subject.write_text(original)
            contracts = root / gate.STD_CONTRACTS
            contracts.write_text(contracts.read_text() + "\npub assume_specification[other]();\n")
            profile["std_contracts_sha256"] = verifier.digest(contracts)
            with self.assertRaisesRegex(RuntimeError, "standard-library contracts"):
                gate.audit_trust(root, profile)

    def test_timeout_retains_partial_streams_and_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            error = subprocess.TimeoutExpired(["verus"], 1, output=b"partial", stderr=b"diagnostic")
            with patch.object(verifier, "run", side_effect=error):
                result = gate.retained_run(["verus"], directory, {}, timeout=1)
            self.assertIsNone(result.returncode)
            self.assertEqual((directory / "stdout.log").read_text(), "partial")
            self.assertEqual((directory / "stderr.log").read_text(), "diagnostic")
            record = json.loads((directory / "command.json").read_text())
            self.assertTrue(record["timed_out"])
            self.assertEqual(record["command"], ["verus"])
            self.assertIsNone(record["exit_code"])


if __name__ == "__main__":
    unittest.main()
