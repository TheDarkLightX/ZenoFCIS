"""Refusal recovery preserves status, intake charges and existing files."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import instantiate_core

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.environ.get("ZENO_FCIS_CLI"), "set ZENO_FCIS_CLI to the built binary")
class Diagnostics(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.work = Path(self.temporary.name)

    def cli(self, *args):
        return subprocess.run([os.environ["ZENO_FCIS_CLI"], "--no-kb", *map(str, args)],
                              cwd=ROOT, capture_output=True, text=True, check=False)

    def test_prove_refusals_name_a_recovery_action(self):
        tools = self.work / "tools.json"
        tools.write_text('{"format":"zeno-fcis/tools/2","tools":[]}')
        project = ROOT / "examples/minimal/project.zeno"
        for selected_project, claim, backend, manifest, exit_code, message in [
            (self.work / "absent", "500", "cvc5", tools, 3, "run zeno-fcis check PROJECT"),
            (project, "9999", "cvc5", tools, 1, "select a declared claim ID"),
            (project, "500", "cvc5", self.work / "absent", 2, "supply a readable --tools manifest"),
            (project, "500", "cvc5", tools, 2, "configure this backend"),
            (project, "500", "lean", tools, 2, "select a backend declared by this claim"),
        ]:
            with self.subTest(message=message):
                result = self.cli("prove", selected_project, "--claim", claim, "--backend", backend, "--tools", manifest)
                self.assertEqual(result.returncode, exit_code, result.stderr)
                self.assertIn("next:", result.stderr)
                self.assertIn(message, result.stderr)
                self.assertEqual(result.stdout, "")

    def test_assessment_without_a_suite_is_a_usage_error(self):
        # This build includes kb assess, so the unavailable-command hint must not appear.
        result = self.cli("kb", "assess")
        self.assertEqual(result.returncode, 64)
        self.assertIn("--suite <SUITE>", result.stderr)
        self.assertNotIn("kb assess is unavailable in this build", result.stderr)

    def encode(self, value, out=None):
        source = self.work / "program.json"
        source.write_text(value if isinstance(value, str) else json.dumps(value))
        result = self.cli("loop", "encode", "--program", source, "--out", out or self.work / "new.bin")
        return result, json.loads(result.stdout)

    def test_encode_refusals_preserve_reason_and_intake(self):
        good = json.loads((ROOT / "crates/zeno-fcis-cli/tests/fixtures/transform-check/1/program-1.json").read_text())
        cases = [("{", "program-json-unreadable", "Fix the JSON syntax"),
                 ({}, "program-json-shape", "exactly inputs, outputs, nodes and roots")]
        for field, value, reason, action in [
            ("inputs", [{"kind": "Other"}], "program-json-domain", "Bool domains or Int domains"),
            ("inputs", [{"kind": "Int", "min": "00", "max": "4"}], "program-json-decimal", "canonical decimal strings"),
            ("inputs", [{"kind": "U128"}], "U128SubjectUnsupported", "do not narrow its meaning"),
            ("nodes", [["Other"]], "program-json-node", "admitted opcode"),
            ("roots", ["2"], "program-json-root", "JSON integer node indices"),
            ("roots", [999], "program-not-admitted", "Repair the reported admission code"),
        ]:
            value_doc = copy.deepcopy(good)
            value_doc[field] = value
            cases.append((value_doc, reason, action))
        for value, reason, action in cases:
            with self.subTest(reason=reason):
                result, report = self.encode(value)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertEqual(report["status"], "refused")
                self.assertEqual(report["detail"]["reason"], reason)
                self.assertIn(action, report["detail"]["next_step"])
                self.assertEqual(report["authority"], "none")
                self.assertEqual(report["acquisition"]["observed_intake"]["attempted_reads"], 1)
                self.assertGreater(report["acquisition"]["observed_intake"]["bytes"], 0)
                self.assertFalse((self.work / "new.bin").exists())

    def test_encode_existing_output_precedes_read_and_is_preserved(self):
        out = self.work / "existing.bin"
        out.write_bytes(b"retained")
        result, report = self.encode("{", out)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(report["status"], "output-exists")
        self.assertIn("fresh --out path", report["detail"]["next_step"])
        self.assertEqual(report["acquisition"]["observed_intake"]["attempted_reads"], 0)
        self.assertEqual(out.read_bytes(), b"retained")

    def test_encode_io_refusal_preserves_charge(self):
        result = self.cli("loop", "encode", "--program", self.work / "absent", "--out", self.work / "new.bin")
        self.assertEqual(result.returncode, 3)
        report = json.loads(result.stdout)
        self.assertEqual(report["status"], "io-error")
        self.assertIn("permissions", report["detail"]["next_step"])
        self.assertEqual(report["acquisition"]["observed_intake"]["attempted_reads"], 1)

    def test_encode_size_limit_is_retained(self):
        result, report = self.encode(" " * (1024 * 1024 + 1))
        self.assertEqual(result.returncode, 1)
        self.assertEqual(report["detail"]["reason"], "program-json-too-large")
        self.assertIn("Reduce the JSON below max_bytes", report["detail"]["next_step"])
        self.assertGreater(report["acquisition"]["observed_intake"]["bytes"], report["detail"]["max_bytes"])


class InstantiationDiagnostic(unittest.TestCase):
    def test_stale_family_refusal_names_ready_example_without_installing(self):
        with patch("sys.argv", ["instantiate_core.py", "bounded-counter", "unused", "--param", "C=2"]), \
             patch.object(instantiate_core, "instantiate", side_effect=ValueError("stale source-bound family/checker")), \
             patch("sys.stderr") as stderr:
            with self.assertRaises(SystemExit) as exit_value:
                instantiate_core.main()
        self.assertEqual(exit_value.exception.code, 1)
        message = "".join(call.args[0] for call in stderr.write.call_args_list)
        self.assertIn("core instantiation refused: stale", message)
        self.assertIn("examples/guide-counter", message)
        self.assertIn("independent regeneration and checking", message)

    def test_bad_parameters_name_discovery(self):
        with patch("sys.argv", ["instantiate_core.py", "bounded-counter", "unused", "--param", "C=oops"]), patch("sys.stderr") as stderr:
            with self.assertRaises(SystemExit) as exit_value:
                instantiate_core.main()
        self.assertEqual(exit_value.exception.code, 1)
        message = "".join(call.args[0] for call in stderr.write.call_args_list)
        self.assertIn("inspect --show", message)


if __name__ == "__main__":
    unittest.main()
