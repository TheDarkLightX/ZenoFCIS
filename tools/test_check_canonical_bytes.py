"""Coverage and native byte-reader regressions; fixtures are not proof results."""
import json
import os
from pathlib import Path
import tempfile
import unittest

import check_canonical_bytes as gate
import check_verus as verifier


class CanonicalByteCoverage(unittest.TestCase):
    def test_runtime_domains_and_mutation_anchors_remain_complete(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        runtime = {name for name, record in profile["functions"].items() if record["mode"] == "Exec"}
        self.assertEqual(len(profile["functions"]), 11)
        self.assertEqual(len(runtime), 2)
        self.assertEqual(set(profile["body_covered_functions"]), runtime)
        self.assertTrue(all(record["requires"] == 0 and record["ensures"] > 0
                            for record in profile["functions"].values() if record["mode"] == "Exec"))
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                           (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(len(mutations), 14)
        self.assertEqual(sum(expected == "proof" for _, _, expected in mutations.values()), 9)
        for path, changed, expected in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
            self.assertIn(expected, ("proof", "coverage"))

    def test_simulated_success_cannot_omit_the_actual_byte_functions(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        pin.update({key: profile[key] for key in ("expected_verified", "target_functions")})
        report = {"verus": {"commit": pin["commit"], "version": pin["version"]},
                  "verification-results": {"success": True, "errors": 0, "verified": 9,
                    "encountered-error": False, "encountered-vir-error": False,
                    "is-verifying-entire-crate": True},
                  "func-details": dict.fromkeys(pin["target_functions"], {})}
        self.assertTrue(verifier.accepted(report, pin))
        for name in profile["body_covered_functions"]:
            changed = {**report, "func-details": {key: value for key, value in report["func-details"].items()
                                                  if key != name}}
            self.assertFalse(verifier.accepted(changed, pin))

    def test_all_evidence_sources_are_hashed_and_symlinks_refuse(self):
        from unittest.mock import patch
        sources = gate.snapshot()
        self.assertEqual(set(sources), set(map(str, gate.SOURCES)))
        with patch.object(Path, "is_symlink", return_value=True):
            with self.assertRaisesRegex(RuntimeError, "symbolic link"):
                gate.snapshot()


class NativeCanonicalBytes(unittest.TestCase):
    def test_actual_source_matches_independent_integer_conversions(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        with tempfile.TemporaryDirectory(prefix="zeno-byte-native-") as temporary:
            evidence = gate.native_checks(Path(temporary), pin, dict(os.environ))
        self.assertEqual(evidence["exit_code"], 0)
        self.assertIn("4 passed", evidence["test_output"])


if __name__ == "__main__":
    unittest.main()
