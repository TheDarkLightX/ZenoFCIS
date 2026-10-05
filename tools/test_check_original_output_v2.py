"""Gate wiring and fail-closed coverage tests; never counted as proof evidence."""
import copy
import json
import unittest
from pathlib import Path

import check_original_output_v2 as gate
from verus_coverage import require_coverage


class OutputCoverage(unittest.TestCase):
    def test_complete_executable_contracts_and_bodies(self):
        p = json.loads((gate.ROOT / gate.PROFILE).read_text())
        executable = {name for name, r in p['functions'].items() if r['mode'] == 'Exec'}
        self.assertEqual(executable, set(p['body_covered_functions']))
        self.assertTrue(all(r['requires'] == 0 and r['ensures'] > 0 for r in p['functions'].values() if r['mode'] == 'Exec'))
        for name in ['encode_atom', 'encode_record', 'encode_envelope']:
            self.assertTrue(any(x.endswith('::' + name) for x in executable))
        self.assertTrue(any('execution_v2::decision::' in x for x in executable))
        self.assertTrue(any(x.endswith('::read_signed_128') for x in executable))
        self.assertTrue(any(x.endswith('::frame') for x in executable))
        with self.assertRaises(ValueError): require_coverage('', p)

    def test_complete_actual_dependency_and_oracle_source_inventory(self):
        hashes = gate.snapshot()
        self.assertEqual(set(hashes), set(map(str, gate.SOURCES)))
        for folder in ['canonical_v2', 'evaluation', 'execution_v2']:
            self.assertLessEqual({p.relative_to(gate.ROOT) for p in (gate.ROOT / gate.FINITE / folder).rglob('*.rs')}, set(gate.UNIT_SOURCES))
        for package in ['zeno-fcis-codec', 'zeno-fcis-value']:
            self.assertIn(f'crates/{package}/src/lib.rs', hashes)
        self.assertIn(str(gate.TESTS), hashes)

    def test_controls_change_only_owned_paths_and_cover_each_boundary(self):
        rows = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(), (gate.ROOT / gate.SPEC).read_text())
        self.assertGreaterEqual(len(rows), 30)
        self.assertEqual({k for _, _, k in rows.values()}, {'proof', 'native', 'coverage'})
        for path, changed, kind in rows.values():
            self.assertIn(path, [gate.SUBJECT, gate.SPEC])
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
        for prefix in ['bool_', 'signed_', 'unsigned_', 'integer_', 'enum_', 'variant_', 'sum_', 'blob_', 'ascii_', 'atom_', 'record_', 'duplicate_', 'field_', 'envelope_', 'omit_', 'weaken_', 'equivalent_', 'uncontracted_']:
            self.assertTrue(any(n.startswith(prefix) for n in rows), prefix)

    def test_ambiguous_or_stale_mutation_anchor_refuses(self):
        with self.assertRaises(ValueError): gate.once('x x', 'x', 'y')
        with self.assertRaises(ValueError): gate.once('x', 'z', 'y')
        self.assertEqual(gate.once('x', 'x', 'y'), 'y')


if __name__ == '__main__': unittest.main()
