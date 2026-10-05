"""Coverage and control wiring; these fixtures are not proof receipts."""
import json
import unittest

import check_envelope_frame_v2 as gate
from verus_coverage import require_coverage


class EnvelopeCoverage(unittest.TestCase):
    def test_each_executable_has_total_exact_contract_and_body_guard(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        executable = {name for name, row in profile["functions"].items()
                      if row["mode"] == "Exec"}
        self.assertEqual(executable, set(profile["body_covered_functions"]))
        self.assertTrue(all(row["requires"] == 0 and row["ensures"] > 0
                            for row in profile["functions"].values()
                            if row["mode"] == "Exec"))
        self.assertTrue(any(name.endswith("::frame") for name in executable))

    def test_controls_are_distinct_and_all_dependency_files_are_bound(self):
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                          (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(len(mutations), 13)
        self.assertEqual(sum(kind == "proof" for _, _, kind in mutations.values()), 8)
        for path, changed, _ in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
        self.assertEqual(set(gate.snapshot()), set(map(str, gate.SOURCES)))
        canonical_files = {str(path.relative_to(gate.ROOT))
                           for path in (gate.ROOT / gate.SUBJECT.parent).rglob("*.rs")}
        self.assertLessEqual(canonical_files, set(map(str, gate.UNIT_SOURCES)))
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        with self.assertRaises(ValueError):
            require_coverage("", profile)


if __name__ == "__main__":
    unittest.main()
