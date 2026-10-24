"""Admission tests for the effect proof gate. Synthetic reports are not proofs."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import check_effect_reservation as gate
import check_verus as verifier
from verus_coverage import require_coverage


class EffectProofAdmission(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        cls.profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        cls.runtime = (gate.ROOT / gate.RUNTIME).read_text()

    def report(self):
        return {'verus': {k: self.pin[k] for k in ('version', 'commit', 'rust_toolchain')},
                'verification-results': dict(success=True, verified=self.profile['expected_verified'],
                    errors=0, **{'encountered-error': False, 'encountered-vir-error': False,
                                 'is-verifying-entire-crate': True}),
                'func-details': dict.fromkeys(self.profile['target_functions'], {})}

    def positive(self):
        report = self.report()
        report['verus']['toolchain'] = report['verus'].pop('rust_toolchain')
        return report

    def test_positive_admission_fixture(self):
        self.assertTrue(gate.accepted(subprocess.CompletedProcess([], 0, '', ''),
                                     self.positive(), self.pin, self.profile))

    def test_count_type_count_value_and_complete_scope_are_mandatory(self):
        for field, value in (('verified', True), ('verified', self.profile['expected_verified']+1),
                             ('verified', self.profile['expected_verified']-1), ('errors', True),
                             ('success', False), ('is-verifying-entire-crate', False),
                             ('encountered-error', True), ('encountered-vir-error', True)):
            report = self.positive()
            report['verification-results'][field] = value
            self.assertFalse(gate.accepted(subprocess.CompletedProcess([], 0, '', ''),
                                          report, self.pin, self.profile), (field, value))

    def test_pinned_tools_and_each_named_obligation_are_mandatory(self):
        for field in ('version', 'commit', 'toolchain'):
            report = self.positive()
            report['verus'][field] = 'other'
            self.assertFalse(gate.accepted(subprocess.CompletedProcess([], 0, '', ''),
                                          report, self.pin, self.profile))
        for name in self.profile['target_functions']:
            report = self.positive()
            del report['func-details'][name]
            self.assertFalse(gate.accepted(subprocess.CompletedProcess([], 0, '', ''),
                                          report, self.pin, self.profile), name)

    def test_nonsemantic_output_and_process_failure_cannot_pass(self):
        for status, stderr in ((1, ''), (124, ''), (-9, ''), (0, 'error: assertion failed'),
                               (0, 'solver timeout'), (0, 'resource limit reached'),
                               (0, 'internal compiler error'), (0, 'transport error')):
            self.assertFalse(gate.accepted(subprocess.CompletedProcess([], status, '', stderr),
                                          self.positive(), self.pin, self.profile))

    def test_missing_or_malformed_json_never_passes(self):
        for text in ('', '[]', 'true', 'null', '{', 'PASS'):
            parsed = gate.json_report(subprocess.CompletedProcess([], 0, text, ''))
            self.assertFalse(gate.accepted(subprocess.CompletedProcess([], 0, text, ''),
                                          parsed, self.pin, self.profile))

    def test_the_complete_source_is_in_the_restricted_configuration_grammar(self):
        gate.source_guard()
        self.assertEqual({name.rsplit('::', 1)[-1] for name in self.profile['body_covered_functions']},
                         set(gate.EXECUTABLES))
        functions = self.profile['functions']
        runtime = {name for name, entry in functions.items() if entry['mode'] == 'Exec'}
        self.assertEqual(runtime, set(self.profile['body_covered_functions']))
        self.assertTrue(all(functions[name]['requires'] == 0 and functions[name]['ensures'] > 0
                            for name in runtime))

    def test_conditional_native_code_is_refused_in_every_spelling(self):
        for fragment in ('#[cfg(not(verus_keep_ghost))] let hidden = 1;',
                         '#[cfg(all(not(verus_keep_ghost), target_os="linux"))] let hidden = 1;',
                         '#[cfg_attr(not(verus_keep_ghost), allow(dead_code))] let hidden = 1;',
                         '#[cfg(test)] let hidden = 1;',
                         '#[cfg(verus_keep_ghost)] let hidden = 1;',
                         '// }\n #[cfg(verus_keep_ghost)] let hidden = 1;',
                         '/* } /* nested */ } */ #[cfg(verus_keep_ghost)] let hidden = 1;',
                         'if cfg!(not(verus_keep_ghost)) { return unreachable!(); }'):
            altered = self.runtime.replace('let _ = retry;', fragment + '\n    let _ = retry;')
            with self.assertRaises(ValueError, msg=fragment):
                gate.configuration_guard(altered, runtime=True)

    def test_trusted_application_bodies_and_termination_exemptions_refuse(self):
        for fragment in ('assume(true);', 'admit();', '#[verifier::external_body]',
                         '#[verifier::external]', '#[verifier::exec_allows_no_decreases_clause]',
                         'assume_specification [custom_function] () -> bool;'):
            with self.assertRaises(ValueError):
                gate.configuration_guard(self.runtime + fragment, runtime=True)

    def test_controls_have_one_anchor_and_one_intended_function(self):
        controls = gate.mutations(self.runtime)
        self.assertEqual(len(controls), 4)
        self.assertEqual({function for _, function, _ in controls.values()}, {'prepare', 'finish'})
        for changed, function, test in controls.values():
            self.assertNotEqual(changed, self.runtime)
            self.assertIn(f'fn {test}()', changed)
            self.assertGreater(verifier.function_lines(changed, function)[0], 0)
            gate.configuration_guard(changed, runtime=True)
        with self.assertRaises(ValueError):
            gate.mutations(self.runtime.replace('if !send {', 'if send == false {'))

    def test_source_inventory_includes_all_semantics_and_comparison_inputs(self):
        sources = gate.snapshot()
        self.assertTrue(set(map(str, gate.MODEL_SOURCES)).issubset(sources))
        self.assertTrue(set(map(str, gate.PRODUCTION_SOURCES)).issubset(sources))
        self.assertIn('tools/check_payment_policy_product.py', sources)
        self.assertIn(str(gate.PROFILE), sources)
        self.assertIn(str(verifier.PIN), sources)

    def test_source_symlinks_refuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'actual').write_text('unchanged')
            (root / 'alias').symlink_to(root / 'actual')
            with patch.object(gate, 'SOURCES', (Path('alias'),)):
                with self.assertRaises(ValueError): gate.snapshot(root)

    def test_specimen_restoration_survives_an_unrelated_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'subject.rs'
            original = b'original\r\nbytes\r\n'
            path.write_bytes(original)
            with self.assertRaisesRegex(RuntimeError, 'unrelated'):
                with gate.changed_source(path, 'mutant\n'):
                    self.assertEqual(path.read_text(), 'mutant\n')
                    raise RuntimeError('unrelated')
            self.assertEqual(path.read_bytes(), original)
            with gate.changed_source(path, 'mutant\n'):
                self.assertEqual(path.read_text(), 'mutant\n')
            self.assertEqual(path.read_bytes(), original)

    def test_product_identity_binds_every_ordered_source_byte(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            directory = root / 'crates/zeno-fcis-shell/src/effects'
            directory.mkdir(parents=True)
            for name in gate.PRODUCT_FILES: (directory / f'{name}.rs').write_text(name)
            expected = gate.product_identity(root)
            for name in gate.PRODUCT_FILES:
                path = directory / f'{name}.rs'
                path.write_text(name + ' changed')
                self.assertNotEqual(gate.product_identity(root), expected)
                path.write_text(name)
            self.assertEqual(gate.product_identity(root), expected)

    def test_semantic_refusal_must_point_to_the_intended_function(self):
        report = self.positive()
        report['verification-results'].update(success=False, verified=1, errors=1,
                                              **{'encountered-error': True})
        start, end = verifier.function_lines(self.runtime, 'finish')
        stderr = (f'error: postcondition not satisfied\n --> {gate.RUNTIME}:{start}:1\n'
                  'error: aborting due to 1 previous error\n')
        proc = subprocess.CompletedProcess([], 1, json.dumps(report), stderr)
        self.assertTrue(verifier.semantic_refusal(proc, report, self.pin, gate.RUNTIME, gate.ROOT,
                                                 region=(start, end)))
        for replacement in ('error: expected expression', 'error: solver timeout',
                            'error: internal compiler error'):
            proc = subprocess.CompletedProcess([], 1, json.dumps(report),
                                               stderr.replace('error: postcondition not satisfied', replacement))
            self.assertFalse(verifier.semantic_refusal(proc, report, self.pin, gate.RUNTIME, gate.ROOT,
                                                      region=(start, end)))
        proc = subprocess.CompletedProcess([], 1, json.dumps(report),
                                           stderr.replace(f':{start}:1', ':1:1'))
        self.assertFalse(verifier.semantic_refusal(proc, report, self.pin, gate.RUNTIME, gate.ROOT,
                                                  region=(start, end)))


if __name__ == '__main__':
    unittest.main()
