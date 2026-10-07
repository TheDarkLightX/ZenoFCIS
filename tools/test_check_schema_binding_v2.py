"""Refuse incomplete formal coverage and miswired semantic controls."""
import json
import os
import re
import unittest

import check_schema_binding_v2 as gate
import verus_contract_controls as contract_controls
from test_verus_contract_controls import PIN, assert_strict_classification, report
from verus_coverage import require_coverage

# Excerpt of the retained hosted canonical-bytes stderr for this same admission.rs
# omission (run 37404295156). The schema harness still needs its own selected run.
HOSTED_OMIT = """error: postcondition not satisfied
  --> /home/runner/work/_temp/v2-evidence/canonical-bytes/canonical-byte-run-nt4blh0_/omit_unused_getter_contract/verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/catalog.rs:67:63
   |
67 |     #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().0,))]
   |                                                               ^^^^^^^^^^^^^^^^^^^^^^^^ failed this postcondition
...
71 |         self.checked.original()
   |         ----------------------- at the end of the function body

error: postcondition not satisfied
   --> /home/runner/work/_temp/v2-evidence/canonical-bytes/canonical-byte-run-nt4blh0_/omit_unused_getter_contract/verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/catalog.rs:120:5
    |
120 | /     (match result { Ok(bound) => Ok(bound.view()), Err(e) => Err(e) }) ==

error: postcondition not satisfied
   --> /home/runner/work/_temp/v2-evidence/canonical-bytes/canonical-byte-run-nt4blh0_/omit_unused_getter_contract/verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/catalog.rs:120:5
    |
120 | /     (match result { Ok(bound) => Ok(bound.view()), Err(e) => Err(e) }) ==

error: aborting due to 3 previous errors
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
        outcome = contract_controls.contract_rejected(1, report(errors=2), HOSTED_OMIT, PIN,
            gate.CONTRACT_REJECTIONS["omit_original_getter_contract"])
        self.assertTrue(outcome["accepted"], outcome)


if __name__ == "__main__":
    unittest.main()
