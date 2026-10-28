"""Actual command/replay boundary, including negative evidence and stale subjects."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import observations as O

HERE = Path(__file__).resolve().parent
EXAMPLES = HERE / "examples" / "observations"


class ObservationCLI(unittest.TestCase):
    def invoke(self, mode, stem, profile, out, *extra):
        result = subprocess.run([sys.executable, "-B", str(HERE / "observations.py"), mode,
                                 str(EXAMPLES / (stem + ".zal")), str(EXAMPLES / profile),
                                 "--out", str(out), *extra], text=True, capture_output=True)
        self.assertFalse(result.stderr, result.stderr)
        return result.returncode, json.loads(result.stdout)

    def test_both_positive_and_negative_modes_replay(self):
        cases = [("game", "delivery", "delivery.partial.json", "UNREALIZABLE", 1),
                 ("game", "delivery_idempotent", "delivery_idempotent.partial.json", "CHECKED", 0),
                 ("relational", "relational_private", "relational_private.json", "CHECKED", 0),
                 ("relational", "relational_delayed_leak", "relational_delayed_leak.json", "COUNTEREXAMPLE", 1)]
        with tempfile.TemporaryDirectory() as temp:
            for i, (mode, stem, profile, status, exit_code) in enumerate(cases):
                out = Path(temp) / str(i)
                with self.subTest(mode=mode, stem=stem):
                    code, result = self.invoke(mode, stem, profile, out)
                    self.assertEqual(code, exit_code)
                    self.assertEqual(result["status"], status)
                    code, replay = self.invoke("verify", stem, profile, out)
                    self.assertEqual(code, exit_code)
                    self.assertEqual(result, replay)

    def test_unknown_retains_diagnostic_but_is_not_certificate(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "unknown"
            code, result = self.invoke("game", "delivery", "delivery.partial.json", out, "--max-work", "0")
            self.assertEqual((code, result["status"]), (2, "UNKNOWN"))
            self.assertTrue((out / "binding.json").is_file())
            code, result = self.invoke("verify", "delivery", "delivery.partial.json", out)
            self.assertEqual(code, 2)
            self.assertIn("not completion evidence", result["reason"])

    def test_artifact_and_source_drift_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "evidence"
            self.assertEqual(self.invoke("game", "delivery", "delivery.partial.json", out)[0], 1)
            original = (out / "result.json").read_bytes()
            (out / "result.json").write_bytes(original + b" ")
            with self.assertRaisesRegex(ValueError, "artifact drift"):
                O.verify(out, EXAMPLES / "delivery.zal", EXAMPLES / "delivery.partial.json")
            (out / "result.json").write_bytes(original)
            identities = O.identities()
            changed = copy.deepcopy(identities)
            changed[next(iter(changed))] = "0" * 64
            with patch.object(O, "identities", return_value=changed), self.assertRaisesRegex(ValueError, "source drift"):
                O.verify(out, EXAMPLES / "delivery.zal", EXAMPLES / "delivery.partial.json")

    def test_rehash_cannot_legitimize_false_verdict(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "evidence"
            self.invoke("game", "delivery", "delivery.partial.json", out)
            result = O.decode((out / "result.json").read_bytes())
            result["status"] = "CHECKED"
            raw = O.encode(result)
            (out / "result.json").write_bytes(raw)
            binding = O.decode((out / "binding.json").read_bytes())
            binding["artifacts"]["result.json"] = O.sha(raw)
            (out / "binding.json").write_bytes(O.encode(binding))
            with self.assertRaisesRegex(ValueError, "verdict mismatch"):
                O.verify(out, EXAMPLES / "delivery.zal", EXAMPLES / "delivery.partial.json")

    def test_expected_subject_and_fresh_output(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "evidence"
            self.invoke("game", "delivery", "delivery.partial.json", out)
            before = (out / "result.json").read_bytes()
            self.assertEqual(self.invoke("game", "delivery", "delivery.partial.json", out)[0], 2)
            self.assertEqual((out / "result.json").read_bytes(), before)
            with self.assertRaisesRegex(ValueError, "subject mismatch"):
                O.verify(out, EXAMPLES / "delivery.zal", EXAMPLES / "delivery.perfect.json")

    def test_json_types_and_symlink_outputs(self):
        for raw in (b'{"a": 1, "a": 2}', b'{"a": 1.0}', b'{"a": NaN}'):
            with self.assertRaises(ValueError):
                O.decode(raw)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            target = root / "real"
            target.mkdir()
            link = root / "link"
            link.symlink_to(target, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "symbolic-link"):
                O.safe_output(link / "new")
        with self.assertRaisesRegex(ValueError, "outside source"):
            O.safe_output(HERE / "new-evidence")

    def test_binding_metadata_cannot_grant_authority(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "evidence"
            self.invoke("relational", "relational_private", "relational_private.json", out)
            binding = O.decode((out / "binding.json").read_bytes())
            binding["authority"] = "production"
            (out / "binding.json").write_bytes(O.encode(binding))
            with self.assertRaisesRegex(ValueError, "identity"):
                O.verify(out, EXAMPLES / "relational_private.zal", EXAMPLES / "relational_private.json")


if __name__ == "__main__":
    unittest.main()
