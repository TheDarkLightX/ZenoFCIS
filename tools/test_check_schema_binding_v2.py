"""Refuse incomplete formal coverage and miswired semantic controls."""
import copy
import json
import os
import re
import subprocess
import unittest
from pathlib import Path

import check_schema_binding_v2 as gate
import verus_contract_controls as contract_controls
from test_verus_contract_controls import PIN, assert_strict_classification, report
from verus_coverage import require_coverage

# Primary error/location excerpt of the retained local schema omission run at rlimit 80.
# Diagnostic order and duplicate catalog postcondition are retained; paths are relative.
# This fixture tests classification and is not a proof result.
RETAINED_OMIT = """error: precondition not satisfied
   --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/numeric_metadata.rs:133:13

error: postcondition not satisfied
  --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/catalog.rs:83:63

error: postcondition not satisfied
   --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/catalog.rs:136:5

error: postcondition not satisfied
   --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/catalog.rs:136:5

error: postcondition not satisfied
   --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/catalog/matching.rs:345:56

error: postcondition not satisfied
   --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/ingress/original.rs:63:56

error: precondition not satisfied
   --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/compound.rs:542:60

error: aborting due to 7 previous errors
"""


class SchemaCoverage(unittest.TestCase):
    def test_every_executable_has_total_contract_and_body_guard(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        executable = {name for name, row in profile["functions"].items() if row["mode"] == "Exec"}
        self.assertEqual(executable, set(profile["body_covered_functions"]))
        self.assertTrue(all(row["requires"] == 0 and row["ensures"] > 0
            for row in profile["functions"].values() if row["mode"] == "Exec"))
        self.assertTrue(any(name.endswith("::admit") for name in executable))
        self.assertTrue(any(name.endswith("::encoding_matches") for name in executable))
        with self.assertRaises(ValueError):
            require_coverage("", profile)

    def test_all_semantic_controls_are_changed_and_bind_every_dependency(self):
        mutants = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
            (gate.ROOT / gate.ADMISSION).read_text(), (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(len(mutants), 23)
        self.assertEqual(sum(kind == "proof" for _, _, kind in mutants.values()), 18)
        self.assertEqual(sum(kind == "proof_contract" for _, _, kind in mutants.values()), 3)
        for path, changed, _ in mutants.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
        self.assertEqual(set(gate.snapshot()), set(map(str, gate.SOURCES)))
        canonical_files = {str(path.relative_to(gate.ROOT)) for path in (gate.ROOT / gate.CANONICAL).rglob("*.rs")}
        self.assertLessEqual(canonical_files, set(map(str, gate.UNIT_SOURCES)))
        self.assertIn(str(gate.PUBLIC_TEST), gate.snapshot())

    def test_specimens_copy_every_path_module_the_native_mutant_build_reads(self):
        units = set(map(str, gate.UNIT_SOURCES))
        targets = set()
        for unit in gate.UNIT_SOURCES:
            if unit.suffix == ".rs":
                for target in re.findall(r'#\[path\s*=\s*"([^"]+)"\]', (gate.ROOT / unit).read_text()):
                    targets.add(os.path.normpath(unit.parent / target))
        self.assertIn(str(gate.ORDER_FIXTURE), targets)
        self.assertLessEqual(targets, units)


    def test_proof_controls_refuse_inconclusive_and_unrelated_failures(self):
        # Synthetic protocol tests; these are not proof results.
        pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        mutants = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
            (gate.ROOT / gate.ADMISSION).read_text(), (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(set(gate.PROOF_FUNCTIONS),
            {name for name, (_, _, kind) in mutants.items() if kind == "proof"})
        report = {
            "verus": {"version": pin["version"], "commit": pin["commit"],
                      "toolchain": pin["rust_toolchain"]},
            "verification-results": {"success": False, "errors": 1, "verified": 1160,
                "encountered-error": True, "encountered-vir-error": False,
                "is-verifying-entire-crate": True},
            "func-details": {key: {} for key in profile["target_functions"]},
        }
        specimen = Path("/synthetic/schema-control")
        for name, (path, changed, kind) in mutants.items():
            if kind != "proof":
                continue
            start, end = gate.verifier.function_lines(changed, gate.PROOF_FUNCTIONS[name])
            diagnostic = f"error: postcondition not satisfied\n --> {path}:{start}:1\n"
            summary = "error: aborting due to 1 previous error\n"
            def classify(stderr, current=report, code=1):
                proc = subprocess.CompletedProcess([], code, "", stderr)
                return gate.proof_rejected(proc, current, pin, profile, name, path, changed, specimen)
            with self.subTest(name=name):
                self.assertTrue(classify(diagnostic + summary))
                loop = diagnostic.replace('postcondition not satisfied',
                                          'invariant not satisfied before loop')
                self.assertTrue(classify(loop + summary))
                self.assertFalse(classify(loop.replace('before loop', 'before loop unrelated') + summary))
                self.assertFalse(classify(loop.replace(str(path), 'unrelated.rs') + summary))
                self.assertFalse(classify(loop + 'note: timeout\n' + summary))
                for extra in ("note: Resource limit (rlimit) exceeded\n", "note: UNKNOWN\n",
                              "note: timeout\n", "note: signal 9\n",
                              "error[E0308]: mismatched types\n",
                              "error: postcondition not satisfied\n --> unrelated.rs:1:1\n"):
                    self.assertFalse(classify(diagnostic + extra + summary), extra)
                self.assertFalse(classify(diagnostic.replace(str(path), "unrelated.rs") + summary))
                self.assertFalse(classify(diagnostic.replace(f":{start}:1", f":{end + 1}:1") + summary))
                self.assertFalse(classify(diagnostic))
                for code in (0, 101, 124, -9):
                    self.assertFalse(classify(diagnostic + summary, code=code))
                for section, key, value in (("verus", "version", "other"),
                        ("verus", "commit", "other"), ("verus", "toolchain", "other"),
                        ("verification-results", "is-verifying-entire-crate", False),
                        ("verification-results", "encountered-vir-error", True),
                        ("verification-results", "errors", True)):
                    current = copy.deepcopy(report); current[section][key] = value
                    self.assertFalse(classify(diagnostic + summary, current))


    def test_getter_contract_controls_change_only_that_contract_and_classify_strictly(self):
        admission = (gate.ROOT / gate.ADMISSION).read_text()
        end = admission.index("    pub fn original(&self)")
        start = admission.rindex("    #[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
        self.assertEqual(admission[start:end], "    #[cfg_attr(verus_keep_ghost, verus_spec(result => "
                                               "ensures result@ == self.view().0,))]\n")
        mutants = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(), admission,
            (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(set(gate.CONTRACT_REJECTIONS),
            {name for name, (_, _, kind) in mutants.items() if kind == "proof_contract"})
        for name, expected in gate.CONTRACT_REJECTIONS.items():
            path, changed, _ = mutants[name]
            self.assertEqual(path, gate.ADMISSION)
            self.assertTrue(changed.startswith(admission[:start]) and changed.endswith(admission[end:]), name)
            assert_strict_classification(self, expected)
        outcome = contract_controls.contract_rejected(1, report(errors=6), RETAINED_OMIT, PIN,
            gate.CONTRACT_REJECTIONS["omit_original_getter_contract"])
        self.assertTrue(outcome["accepted"], outcome)


if __name__ == "__main__":
    unittest.main()
