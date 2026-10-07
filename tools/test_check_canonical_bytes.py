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
        self.assertEqual(len(profile["functions"]), 171)
        self.assertEqual(len(runtime), 61)
        self.assertIn("canonical_bytes::canonical_v2::read_big_endian", runtime)
        self.assertIn("canonical_bytes::canonical_v2::read_signed_128", runtime)
        self.assertEqual(set(profile["body_covered_functions"]), runtime)
        self.assertTrue(all(record["requires"] == 0 and record["ensures"] > 0
                            for record in profile["functions"].values() if record["mode"] == "Exec"))
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                           (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(len(mutations), 17)
        self.assertEqual(sum(expected == "proof" for _, _, expected in mutations.values()), 9)
        self.assertEqual(sum(expected == "proof_contract" for _, _, expected in mutations.values()), 3)
        for path, changed, expected in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
            self.assertIn(expected, ("proof", "proof_contract", "coverage"))

    def test_unused_getter_controls_change_only_the_envelope_original_contract(self):
        envelope = (gate.ROOT / gate.ENVELOPE).read_text()
        self.assertEqual(envelope.count("pub fn original(&self)"), 1)
        getter = envelope.index("    pub fn original(&self)")
        contract = ("    #[cfg_attr(verus_keep_ghost, verus_spec(result =>\n"
                    "        ensures result@ == self.view().0,\n    ))]\n")
        start = getter - len(contract)
        self.assertEqual(envelope[start:getter], contract)
        payload_getter = ("    #[cfg_attr(verus_keep_ghost, verus_spec(result =>\n"
                          "        ensures result@ == self.view().1,\n    ))]\n    pub fn bytes(&self)")
        self.assertEqual(envelope.count(payload_getter), 1)
        self.assertGreater(envelope.index(payload_getter), getter)
        replacements = {
            "omit_unused_getter_contract": "",
            "weaken_unused_getter_contract":
                "    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n",
            "narrow_unused_getter_domain": ("    #[cfg_attr(verus_keep_ghost, verus_spec(result =>\n"
                                            "        requires self.view().0.len() > 0,\n"
                                            "        ensures result@ == self.view().0,\n    ))]\n"),
        }
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                           (gate.ROOT / gate.SPEC).read_text())
        for name, replacement in replacements.items():
            path, changed, expected = mutations[name]
            self.assertEqual((path, expected), (gate.ENVELOPE, "coverage"))
            # Only the original getter's contract differs; its body, the
            # payload getter and every other function keep their exact text.
            self.assertEqual(changed, envelope[:start] + replacement + envelope[getter:])
            self.assertEqual(changed.count(payload_getter), 1)
        # Catalog proofs call the schema getter, so no control may target it.
        self.assertNotIn(gate.SUBJECT.parent / "schema/admission.rs",
                         {path for path, _, _ in mutations.values()})

    def test_simulated_success_cannot_omit_the_actual_byte_functions(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        pin.update({key: profile[key] for key in ("expected_verified", "target_functions")})
        report = {"verus": {"commit": pin["commit"], "version": pin["version"]},
                  "verification-results": {"success": True, "errors": 0, "verified": profile["expected_verified"],
                    "encountered-error": False, "encountered-vir-error": False,
                    "is-verifying-entire-crate": True},
                  "func-details": dict.fromkeys(pin["target_functions"], {})}
        self.assertTrue(verifier.accepted(report, pin))
        # Verus reports associated methods by type name; VIR inventories use
        # internal impl identifiers. Test omissions in the actual report names.
        for name in profile["target_functions"]:
            changed = {**report, "func-details": {key: value for key, value in report["func-details"].items()
                                                  if key != name}}
            self.assertFalse(verifier.accepted(changed, pin))

    def test_all_evidence_sources_are_hashed_and_symlinks_refuse(self):
        from unittest.mock import patch
        sources = gate.snapshot()
        self.assertEqual(set(sources), set(map(str, gate.SOURCES)))
        canonical_files = {str(path.relative_to(gate.ROOT))
                           for path in (gate.ROOT / gate.SUBJECT.parent).rglob("*.rs")}
        self.assertLessEqual(canonical_files, set(map(str, gate.UNIT_SOURCES)))
        self.assertIn("verification/verus/canonical_bytes_tests.rs", set(map(str, gate.UNIT_SOURCES)))
        with patch.object(Path, "is_symlink", return_value=True):
            with self.assertRaisesRegex(RuntimeError, "symbolic link"):
                gate.snapshot()


class NativeCanonicalBytes(unittest.TestCase):
    def test_actual_source_matches_independent_integer_conversions(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        with tempfile.TemporaryDirectory(prefix="zeno-byte-native-") as temporary:
            evidence = gate.native_checks(Path(temporary), pin, dict(os.environ))
        self.assertEqual(evidence["exit_code"], 0)
        for name in ("every_two_byte_integer_preserves_byte_order",
                     "widths_offsets_and_every_truncation_match_std",
                     "every_sign_bit_boundary_and_both_signed_extremes_match_std",
                     "invalid_offsets_and_widths_never_wrap_or_read_outside_input"):
            self.assertIn(f"tests::{name} ... ok", evidence["test_output"])


if __name__ == "__main__":
    unittest.main()
