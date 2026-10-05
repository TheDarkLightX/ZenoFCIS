"""Failure controls for the composition evidence gate (no compiler invocation)."""
from pathlib import Path
import json
import shutil
import tempfile
import subprocess
import unittest

import check_composition_v2 as gate
from verus_coverage import inventory

VIR = '(Function (Fun :path composition::example) :mode Exec :typ_bounds () :params () :ret () :require () :ensure (true) :d () :body (42))'


def profile(vir=VIR):
    names = ['composition::example']
    return dict(namespace='composition::', body_covered_functions=names,
                functions=inventory(vir, 'composition::', tuple(names)))


class CompositionGate(unittest.TestCase):
    def test_negative_proof_requires_intended_semantic_failure_and_pinned_whole_crate(self):
        pin = dict(version='pinned', commit='exact')
        values = dict(success=False, errors=1, **{'encountered-vir-error': False, 'is-verifying-entire-crate': True})
        report = {'verification-results': values, 'verus': pin}
        failed = subprocess.CompletedProcess([], 1, '', 'error: postcondition not satisfied\n --> producer.rs:3:1\n')
        self.assertTrue(gate.proof_killed(failed, report, Path('producer.rs'), pin))
        for suffix in ['resource limit exceeded', 'timeout', 'unknown solver result', 'internal error', 'unsupported construct']:
            inconclusive = subprocess.CompletedProcess([], 1, '', failed.stderr+suffix)
            self.assertFalse(gate.proof_killed(inconclusive, report, Path('producer.rs'), pin))
        for key, value in [('errors', True), ('success', True), ('encountered-vir-error', True), ('is-verifying-entire-crate', False)]:
            bad = {**report, 'verification-results': {**values, key: value}}
            self.assertFalse(gate.proof_killed(failed, bad, Path('producer.rs'), pin))
        self.assertFalse(gate.proof_killed(failed, report, Path('unrelated.rs'), pin))
        self.assertFalse(gate.proof_killed(failed, report, Path('producer.rs'), dict(version='other', commit='exact')))
        self.assertFalse(gate.proof_killed(failed, None, Path('producer.rs'), pin))

    def test_coverage_rejects_changed_body_contract_inventory_and_domains(self):
        p = profile()
        self.assertEqual(len(gate.coverage(VIR, p)), 1)
        for changed in (VIR.replace('(42)', '(43)'), VIR.replace('(true)', '(false)'),
                        VIR.replace(':require ()', ':require (true)'),
                        VIR + VIR.replace('::example', '::extra')):
            with self.assertRaises(ValueError):
                gate.coverage(changed, p)
        with self.assertRaises(ValueError):
            gate.coverage(VIR[:-1], p)

    def test_all_executable_bodies_are_mandatory_even_if_profile_omits_hash(self):
        p = profile()
        p['body_covered_functions'] = []
        del p['functions']['composition::example']['body_sha256']
        with self.assertRaisesRegex(ValueError, 'every executable body'):
            gate.coverage(VIR, p)
        for changed in (VIR.replace(':require ()', ':require (false)'),
                        VIR.replace(':ensure (true)', ':ensure ()')):
            with self.assertRaises(ValueError):
                gate.coverage(changed, profile(changed))

    def test_source_closure_includes_new_imported_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            # Specimen closure is manifest driven: preserve its real registered
            # files while testing discovery of an additional imported source.
            for registered in (*gate.execution_sources(gate.ROOT),
                Path('verification/verus/authority_v2_sources.json'),
                Path('verification/verus/authority_v2_test_sources.json')):
                target = root / registered
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(gate.ROOT / registered, target)
            path = gate.OWNED/'additional.rs'
            (root/path).parent.mkdir(parents=True, exist_ok=True)
            (root/path).write_text('pub fn additional() {}')
            self.assertIn(path, gate.sources(root))
        self.assertIn(gate.TEST, gate.sources())
        self.assertIn(Path('crates/zeno-fcis-cli/templates/durable-counter/synthesized/program.zcve'), gate.sources())

    def test_control_anchors_are_unique_owned_and_semantically_classified(self):
        controls = gate.mutations()
        self.assertEqual(len(controls), 32)
        for name, (path, changed, expected, native) in controls.items():
            self.assertTrue(path == gate.OWNED.with_suffix('.rs') or path.is_relative_to(gate.OWNED), name)
            self.assertNotEqual(changed, (gate.ROOT/path).read_text(), name)
            self.assertEqual(native, expected == 'proof' and not name.startswith('record_'), name)
        self.assertEqual(sum(row[2] == 'proof' for row in controls.values()), 25)
        with self.assertRaises(ValueError):
            gate.once('x x', 'x', 'y')
        with self.assertRaises(ValueError):
            gate.once('x', 'z', 'y')


class RecordStageMigration(unittest.TestCase):
    def test_all_retired_controls_have_a_surviving_fault_or_an_explicit_removed_boundary(self):
        ledger = json.loads((gate.ROOT/'verification/verus/record-stage-migration.json').read_text())['controls']
        self.assertEqual(len(ledger), 24)
        self.assertEqual(len({row['retired_control'] for row in ledger}), 24)
        controls = gate.mutations()
        migrated = [row['composition_control'] for row in ledger if row['composition_control']]
        self.assertEqual(len(migrated), 20)
        self.assertTrue(set(migrated) <= set(controls))
        self.assertTrue(all(row['reason'] for row in ledger))
        self.assertIn('record_execution_contract', controls)
        self.assertNotIn('record_before_admission', controls)
        self.assertEqual(controls['record_skip_context'][2], 'proof')


if __name__ == '__main__':
    unittest.main()
