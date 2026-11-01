"""Pure, solver-independent regression tests for the Tier-1 catalog envelope."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("tier1_satadd_probe", HERE / "probe.py")
assert spec and spec.loader
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class CatalogContractTests(unittest.TestCase):
    def _entries(self):
        root = HERE.parents[1] / "test-data/machine-code/kernels"
        a = json.loads((root / "signed-saturating-add-x86.json").read_text())
        b = json.loads((root / "signed-saturating-add-adc-x86.json").read_text())
        return a, b

    def test_declared_semantics_match_but_grammars_differ(self):
        old, new = self._entries()
        self.assertEqual(probe.meaning(old["problem"]), probe.meaning(new["problem"]))
        self.assertNotEqual(old["problem"]["grammar"], new["problem"]["grammar"])
        self.assertEqual(old["problem"]["schema"], "zeno-fcis/machine-problem/2")
        self.assertEqual(old["problem"]["target"], "x86_64-sysv-register/2")

    def test_exact_raw_cost_and_pinned_hashes(self):
        old, new = self._entries()
        raw_old = bytes.fromhex(old["code_hex"])
        raw_new = bytes.fromhex(new["code_hex"])
        self.assertEqual(old["cost"]["bytes"], len(raw_old))
        self.assertEqual(new["cost"]["bytes"], len(raw_new))
        self.assertEqual((len(raw_old), len(raw_new)), (31, 22))
        self.assertEqual(probe.sha(raw_old), "214c51e666af6d4b9370537a3a66f1a40035c3bbb0b0d3ed3430b721afd25c15")
        self.assertEqual(probe.sha(raw_new), "823ee5322b076a7b0a04997c0b1404e33645d58b29822a5d76cd58efad4b6425")
        self.assertEqual(old["route"], "proposal")
        self.assertEqual(new["route"], "proposal")

    def test_semantic_key_ignores_search_settings_not_meaning(self):
        old, _ = self._entries()
        problem = old["problem"]
        different_search = {**problem, "grammar": [], "max_instructions": 0,
                            "objective": "instructions"}
        self.assertEqual(probe.meaning(problem), probe.meaning(different_search))
        changed_spec = {**problem, "outputs": [["input", "x"]]}
        self.assertNotEqual(probe.meaning(problem), probe.meaning(changed_spec))
        self.assertNotEqual(probe.key(probe.meaning(problem), b"abc"),
                            probe.key(probe.meaning(problem), b"abd"))

    def test_records_deduplicate_same_contract_and_bytes(self):
        old, _ = self._entries()
        semantic = probe.meaning(old["problem"])
        checked = {"status": "equivalent", "cost": {"bytes": 2, "instructions": 1}}
        rows = {}
        probe.put(rows, semantic, "first", bytes([0x90, 0xC3]), checked, "first.json")
        probe.put(rows, semantic, "second", bytes([0x90, 0xC3]), checked, "second.json")
        self.assertEqual(len(rows), 1)
        self.assertEqual(len(next(iter(rows.values()))["provenance"]), 2)
        forged = {"status": "equivalent", "cost": {"bytes": 1, "instructions": 1}}
        with self.assertRaisesRegex(ValueError, "cost disagrees"):
            probe.put(rows, semantic, "forged", bytes([0x90, 0xC3]), forged, "third.json")
        with self.assertRaisesRegex(ValueError, "unverified"):
            probe.put(rows, semantic, "unknown", bytes([0x90, 0xC3]),
                      {"status": "inconclusive"}, "unknown.json")

    def test_explicit_poison_and_abi_nonclaims(self):
        old, _ = self._entries()
        semantic = probe.meaning(old["problem"])
        self.assertIn("no CLIF poison modeling", semantic["poison_or_undefined"])
        self.assertEqual(semantic["word_bits"], 64)
        self.assertIn("saturated", semantic["overflow"])
        with self.assertRaisesRegex(ValueError, "only profile-2"):
            probe.meaning({**old["problem"], "schema": "zeno-fcis/machine-problem/1"})


if __name__ == "__main__":
    unittest.main()
