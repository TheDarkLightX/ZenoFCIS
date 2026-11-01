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

    def _synthetic_checked_catalog(self):
        """Construct an envelope without pretending to have run the SMT solver."""
        rows = {}
        file_pins = {}
        source_items = []
        for name in probe.SOURCES:
            path = probe.SOURCE_DIR / (name + ".json")
            item = json.loads(path.read_text())
            source_items.append(item)
            file_pins[name] = {
                "path": str(path.relative_to(probe.ROOT)),
                "file_sha256": probe.sha(path.read_bytes()),
            }
            code = bytes.fromhex(item["code_hex"])
            probe.put(rows, probe.meaning(item["problem"]), name, code,
                      {"status": "equivalent", "cost": item["cost"]},
                      str(path.relative_to(probe.ROOT)))
        first, second = source_items
        return {
            "schema": probe.SCHEMA, "authority": "none",
            "zenofcis_base_commit": probe.ZENO_BASE,
            "wasmtime_upstream_commit": probe.UPSTREAM,
            "experiment_commit": probe._git_head(),
            "source_files": file_pins,
            "entries": sorted(rows.values(), key=lambda row: row["key"]),
            "comparison": {
                "metric": "stand-alone x86 machine-code bytes including ret; NOT Cranelift native CLIF bytes",
                "old": first["cost"], "new": second["cost"],
                "native_byte_difference": first["cost"]["bytes"] - second["cost"]["bytes"],
                "cranelift_emitted_baseline": "not measured by this Python program",
                "cranelift_optimization": "not attempted",
            },
            "emission": {
                "status": "emitted", "replay_status": "replayed",
                "emitted_sha256": probe.sha(bytes.fromhex(second["code_hex"])),
                "artifact_relative_path": "artifact",
            },
        }

    def test_catalog_envelope_accepts_pinned_inputs_without_claiming_smt(self):
        probe.validate_catalog_envelope(self._synthetic_checked_catalog())

    def test_catalog_envelope_refuses_omitted_baseline_and_forged_savings(self):
        import copy
        intact = self._synthetic_checked_catalog()
        cases = (
            ("entries", lambda c: c["entries"].pop(), "missing required baseline"),
            ("comparison", lambda c: c["comparison"].update(
                {"native_byte_difference": 999}), "native cost comparison"),
            ("source", lambda c: c["source_files"][probe.SOURCES[0]].update(
                {"file_sha256": "0" * 64}), "baseline input hashes"),
            ("pin", lambda c: c.update({"wasmtime_upstream_commit": "0" * 40}),
             "revision pins"),
            ("receipt", lambda c: c["entries"][0]["verification"].update(
                {"status": "unknown"}), "receipt cost or status"),
            ("emission", lambda c: c["emission"].update(
                {"emitted_sha256": "0" * 64}), "emission reference"),
        )
        for label, corrupt, error in cases:
            with self.subTest(label=label):
                altered = copy.deepcopy(intact)
                corrupt(altered)
                with self.assertRaisesRegex(ValueError, error):
                    probe.validate_catalog_envelope(altered)

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
