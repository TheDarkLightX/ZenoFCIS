"""Admission tests for Verus reports and native tests of the shared source."""

import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import check_verus as gate


class EvidenceAdmission(unittest.TestCase):
    def setUp(self):
        self.pin = json.loads((gate.ROOT / gate.PIN).read_text())
        # A synthetic protocol fixture; no proof result is claimed by this test.
        self.report = {
            "verification-results": {
                "success": True, "errors": 0, "verified": self.pin["expected_verified"],
                "encountered-error": False, "encountered-vir-error": False,
                "is-verifying-entire-crate": True,
            },
            "verus": {"commit": self.pin["commit"], "version": self.pin["version"]},
            "func-details": {name: {} for name in self.pin["target_functions"]},
        }

    def test_refuses_incomplete_or_failed_reports(self):
        self.assertTrue(gate.accepted(self.report, self.pin))
        for field, value in (("success", False), ("errors", 1), ("verified", 0),
                             ("encountered-error", True), ("encountered-vir-error", True),
                             ("is-verifying-entire-crate", False)):
            with self.subTest(field=field):
                changed = copy.deepcopy(self.report)
                changed["verification-results"][field] = value
                self.assertFalse(gate.accepted(changed, self.pin))
        self.assertFalse(gate.accepted({}, self.pin))

    def test_refuses_malformed_reports(self):
        for report in (None, [], "success"):
            self.assertFalse(gate.accepted(report, self.pin))
        for section in ("verification-results", "verus", "func-details"):
            changed = copy.deepcopy(self.report)
            changed[section] = None
            self.assertFalse(gate.accepted(changed, self.pin))
        changed = copy.deepcopy(self.report)
        changed["verification-results"]["errors"] = False
        self.assertFalse(gate.accepted(changed, self.pin))

    def test_refuses_wrong_tool_or_missing_target(self):
        for field in ("commit", "version"):
            changed = copy.deepcopy(self.report)
            changed["verus"][field] = "another tool"
            self.assertFalse(gate.accepted(changed, self.pin))
        changed = copy.deepcopy(self.report)
        changed["func-details"].pop(self.pin["target_functions"][0])
        self.assertFalse(gate.accepted(changed, self.pin))

    def test_tool_file_drift_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tool = root / "tool"
            tool.write_bytes(b"test tool fixture")
            pin = {"files": {"tool": gate.digest(tool)}}
            gate.verify_tool_files(root, pin)
            tool.write_bytes(b"changed tool fixture")
            with self.assertRaisesRegex(RuntimeError, "missing or changed"):
                gate.verify_tool_files(root, pin)

    def test_mutation_anchors_remain_applicable(self):
        source = (gate.ROOT / gate.SUBJECT).read_text()
        mutants = gate.mutation_sources(source)
        self.assertEqual(len(mutants), 4)
        self.assertTrue(all(changed != source for changed, _ in mutants.values()))
        with self.assertRaisesRegex(RuntimeError, "anchors changed"):
            gate.mutation_sources("")


class NativeRuntime(unittest.TestCase):
    def test_machine_boundaries_on_the_runtime_toolchain(self):
        with tempfile.TemporaryDirectory(prefix="zeno-verus-native-") as temporary:
            executable = Path(temporary) / "tests"
            command = ["rustc", "+1.97.1", "--edition=2024", "--test",
                       "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)",
                       str(gate.ROOT / gate.HARNESS), "-o", str(executable)]
            subprocess.run(command, cwd=gate.ROOT, check=True, timeout=60)
            subprocess.run([str(executable)], cwd=gate.ROOT, check=True, timeout=60)


if __name__ == "__main__":
    unittest.main()
