"""Guard coverage and safe API admission; fixtures are not proof results."""
import copy
import json
import os
from pathlib import Path
import tempfile
import subprocess
import unittest
from test_negative_retention import StrictGateMutationTests

import check_protected_input as gate
import check_verus as verifier


class ProtectedInputCoverage(unittest.TestCase):
    def test_complete_shared_closure_and_real_mutation_anchors(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        runtime = {name for name, record in profile['functions'].items() if record['mode'] == 'Exec'}
        self.assertEqual(len(profile['functions']), 1609)
        # Six checked helpers for payload-free sums and reason-scoped laws joined in 2.6.
        self.assertEqual(len(runtime), 653)
        self.assertEqual(set(profile['body_covered_functions']), runtime)
        self.assertTrue(all(record['requires'] == 0 and record['ensures'] > 0
                            for record in profile['functions'].values() if record['mode'] == 'Exec'))
        self.assertTrue(any('canonical_v2::read_signed_128' in name for name in runtime))
        self.assertTrue(any('input_view::project_into' in name for name in runtime))
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                          (gate.ROOT / gate.SPEC).read_text(),
                                          (gate.ROOT / gate.TYPED).read_text())
        self.assertEqual(len(mutations), 31)
        self.assertEqual(sum(expected == 'proof' for _, _, expected in mutations.values()), 24)
        for name in ('parse_header_before_ingress', 'parse_field_before_read'):
            self.assertEqual(mutations[name][2], 'coverage')
        for path, changed, expected in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
            self.assertIn(expected, ('proof', 'coverage'))

    def test_simulated_report_cannot_omit_the_actual_protected_entry(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        pin.update({key: profile[key] for key in ('expected_verified', 'target_functions')})
        report = {'verus': {'commit': pin['commit'], 'version': pin['version']},
                  'verification-results': {'success': True, 'errors': 0, 'verified': profile['expected_verified'],
                    'encountered-error': False, 'encountered-vir-error': False,
                    'is-verifying-entire-crate': True},
                  'func-details': dict.fromkeys(pin['target_functions'], {})}
        self.assertTrue(verifier.accepted(report, pin))
        for name in profile['target_functions']:
            if 'input_view::project_into' not in name and 'input_view::project' not in name:
                continue
            changed = {**report, 'func-details': {key: value for key, value in report['func-details'].items()
                                                  if key != name}}
            self.assertFalse(verifier.accepted(changed, pin))

    def test_sources_are_hashed_and_symbolic_links_refuse(self):
        from unittest.mock import patch
        self.assertEqual(set(gate.snapshot()), set(map(str, gate.SOURCES)))
        with patch.object(Path, 'is_symlink', return_value=True):
            with self.assertRaisesRegex(RuntimeError, 'symbolic link'):
                gate.snapshot()


class SemanticProtectedRefusals(unittest.TestCase):
    """Synthetic diagnostic admission checks; these are not proof receipts."""
    def setUp(self):
        self.pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        self.profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        self.mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                              (gate.ROOT / gate.SPEC).read_text(),
                                              (gate.ROOT / gate.TYPED).read_text())
        self.report = {
            'verus': {key: self.pin[key] for key in ('commit', 'version')},
            'verification-results': {'success': False, 'errors': 1, 'verified': 1160,
                                    'encountered-error': True, 'encountered-vir-error': False,
                                    'is-verifying-entire-crate': True},
            'func-details': dict.fromkeys(self.profile['target_functions'], {}),
        }
        self.report['verus']['toolchain'] = self.pin['rust_toolchain']

    def process(self, name, header='assertion failed', line=None, source=None):
        path, changed, _ = self.mutations[name]
        region = verifier.function_lines(changed, gate.PROOF_FUNCTIONS[name])
        stderr = (f'error: {header}\n --> {source or path}:{line or region[0]}:1\n'
                  'error: aborting due to 1 previous error\n')
        return subprocess.CompletedProcess(['synthetic-verus'], 1, '', stderr)

    def refused(self, name, proc, report=None):
        path, changed, _ = self.mutations[name]
        return gate.semantic_refusal(name, proc, report if report is not None else self.report,
                                     self.pin, path, changed, gate.ROOT,
                                     self.profile['target_functions'])

    def test_exact_function_map_and_complete_intended_refusals(self):
        proof_names = {name for name, (_, _, kind) in self.mutations.items() if kind == 'proof'}
        self.assertEqual(set(gate.PROOF_FUNCTIONS), proof_names)
        self.assertEqual(len(proof_names), 24)
        self.assertEqual(len(self.mutations), 31)
        for name in proof_names:
            with self.subTest(name=name):
                self.assertTrue(self.refused(name, self.process(name)))

    def test_only_exact_before_loop_header_aliases_and_raw_is_preserved(self):
        name = 'ignore_negative_code_bound'
        proc = self.process(name, 'invariant not satisfied before loop')
        raw = proc.stderr
        self.assertTrue(self.refused(name, proc))
        self.assertEqual(proc.stderr, raw)
        for header in ('invariant not satisfied before loop with unknown cause',
                       'before loop invariant failure', 'invariant not satisfied before loop '):
            self.assertFalse(self.refused(name, self.process(name, header)))
        self.assertFalse(self.refused(name, self.process(name, source='unrelated.rs')))
        self.assertFalse(self.refused(name, self.process(name, line=1)))

    def test_resource_compiler_and_unrelated_primary_failures_refuse(self):
        name = 'omit_protected_read_charge'
        for failure in ('Resource limit (rlimit) exceeded', 'unknown', 'timed out',
                        'out of memory', 'process killed', 'transport error'):
            proc = self.process(name)
            proc.stderr = failure + '\n' + proc.stderr
            with self.subTest(failure=failure):
                self.assertFalse(self.refused(name, proc))
        for header, source, line in (('cannot find value `missing` in this scope', gate.TYPED, 135),
                                     ('assertion failed', Path('unrelated.rs'), 135),
                                     ('assertion failed', gate.TYPED, 207)):
            proc = self.process(name)
            proc.stderr = proc.stderr.replace('error: aborting due to 1 previous error',
                f'error: {header}\n --> {source}:{line}:1\n'
                'error: aborting due to 2 previous errors')
            self.assertFalse(self.refused(name, proc))
        proc = self.process(name)
        proc.stderr = proc.stderr.replace('error: assertion failed', 'error[E0425]: cannot find value')
        self.assertFalse(self.refused(name, proc))

    def test_complete_pinned_report_and_exit_are_required(self):
        name = 'omit_protected_read_charge'
        for key, value in (('is-verifying-entire-crate', False), ('verified', 0),
                           ('errors', True), ('encountered-vir-error', True),
                           ('encountered-error', False), ('success', True)):
            report = copy.deepcopy(self.report)
            report['verification-results'][key] = value
            self.assertFalse(self.refused(name, self.process(name), report))
        for key in ('commit', 'version', 'toolchain'):
            report = copy.deepcopy(self.report)
            report['verus'][key] = 'wrong'
            self.assertFalse(self.refused(name, self.process(name), report))
        report = copy.deepcopy(self.report)
        report['func-details'].pop(next(iter(report['func-details'])))
        self.assertFalse(self.refused(name, self.process(name), report))
        for code in (0, 2, 124, -9):
            proc = self.process(name)
            proc.returncode = code
            self.assertFalse(self.refused(name, proc))
        proc = self.process(name)
        proc.stderr = proc.stderr.replace('error: aborting due to 1 previous error\n', '')
        self.assertFalse(self.refused(name, proc))


class ProtectedInputCommandPolicy(unittest.TestCase):
    def test_only_read_charge_control_changes_actual_verifier_command(self):
        """Exercise check() dispatch with synthetic reports and no solver/native calls."""
        from contextlib import ExitStack, redirect_stdout
        from io import StringIO
        from unittest.mock import patch

        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                          (gate.ROOT / gate.SPEC).read_text(),
                                          (gate.ROOT / gate.TYPED).read_text())
        positive_report = {
            'verus': {key: pin[key] for key in ('commit', 'version')},
            'verification-results': {
                'success': True, 'errors': 0, 'verified': profile['expected_verified'],
                'encountered-error': False, 'encountered-vir-error': False,
                'is-verifying-entire-crate': True,
            },
            'func-details': dict.fromkeys(profile['target_functions'], {}),
        }
        positive_report['verus']['toolchain'] = pin['rust_toolchain']
        calls = {}

        def simulated_run(command, cwd, environment):
            name = 'positive' if cwd == gate.ROOT else cwd.name
            self.assertNotIn(name, calls)
            calls[name] = list(command)
            logs = Path(command[command.index('--log-dir') + 1])
            logs.mkdir(parents=True)
            (logs / 'crate.vir').write_text(name)
            report = copy.deepcopy(positive_report)
            stderr, code = '', 0
            if name != 'positive' and mutations[name][2] == 'proof':
                path, changed, _ = mutations[name]
                line = verifier.function_lines(changed, gate.PROOF_FUNCTIONS[name])[0]
                report['verification-results'].update(
                    {'success': False, 'errors': 1, 'encountered-error': True})
                stderr = (f'error: assertion failed\n --> {path}:{line}:1\n'
                          'error: aborting due to 1 previous error\n')
                code = 1
            return subprocess.CompletedProcess(command, code, json.dumps(report), stderr)

        def simulated_coverage(vir, _profile):
            if vir != 'positive':
                raise ValueError('synthetic coverage refusal')
            return {'synthetic': True}

        with tempfile.TemporaryDirectory(prefix='protected-command-policy-') as temporary, ExitStack() as stack:
            directory = Path(temporary)
            stack.enter_context(patch.object(gate.platform, 'system', return_value='Linux'))
            stack.enter_context(patch.object(gate.platform, 'machine', return_value='x86_64'))
            stack.enter_context(patch.object(gate, 'snapshot', return_value={'fixture': 'synthetic'}))
            stack.enter_context(patch.object(gate, 'UNIT_SOURCES',
                                              (gate.HARNESS, gate.SUBJECT, gate.SPEC, gate.TYPED)))
            stack.enter_context(patch.object(verifier, 'prepare_tools', return_value=directory / 'tools'))
            stack.enter_context(patch.object(verifier, 'verify_tool_files'))
            stack.enter_context(patch.object(verifier, 'run', side_effect=simulated_run))
            stack.enter_context(patch.object(gate, 'native_checks', return_value={'synthetic': True}))
            stack.enter_context(patch.object(gate, 'require_coverage', side_effect=simulated_coverage))
            # This test isolates command dispatch; StrictGateMutationTests exercises
            # actual coverage attribution independently with parsed records.
            stack.enter_context(patch.object(gate, 'coverage_control',
                                              return_value=(True, 'synthetic coverage refusal')))
            stack.enter_context(patch.object(gate.subprocess, 'check_output', return_value='synthetic'))
            with redirect_stdout(StringIO()):
                receipt = gate.check(directory / 'cache', False, directory)

            self.assertEqual(receipt['status'], 'passed')
            self.assertEqual(set(calls), {'positive', *mutations})
            baseline = calls['positive'][:-3]
            self.assertNotIn('--multiple-errors', baseline)
            self.assertNotIn('--rlimit', baseline)
            for name, command in calls.items():
                with self.subTest(name=name):
                    actual = command[:-3]
                    if name == 'omit_protected_read_charge':
                        self.assertEqual(actual[1:5], ['--multiple-errors', '0', '--rlimit', '160'])
                        actual = actual[:1] + actual[5:]
                    self.assertEqual(actual, baseline)
                    self.assertEqual(command[-3], '--log-dir')
                    self.assertEqual(command[-1], str(gate.HARNESS))
                    log_dir = directory / ('positive' if name == 'positive' else name + '/logs')
                    self.assertEqual(command[-2], str(log_dir))


class NativeProtectedInputs(unittest.TestCase):
    def test_actual_source_and_opaque_report_api_refusals(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        with tempfile.TemporaryDirectory(prefix='zeno-protected-native-') as temporary:
            evidence = gate.native_checks(Path(temporary), pin, dict(os.environ))
        self.assertIn('test result: ok. 195 passed; 0 failed', evidence['test_output'])
        # Exact 2.5 corpus: retain the three additional independent checks.
        for test in ('independent_wide_oracle_matches_fraction_corpora',
                     'variant_oracle_rejects_unrepresentable_cardinalities_without_overflow',
                     'signed_floor_law_uses_shared_node_with_eager_usage_and_retry'):
            self.assertIn(test + ' ... ok', evidence['test_output'])
        self.assertEqual(len(evidence['api_consumers']), 5)
        self.assertEqual(evidence['api_consumers']['positive']['exit_code'], 0)
        self.assertTrue(all(record['exit_code'] != 0 for name, record in evidence['api_consumers'].items()
                            if name != 'positive'))


if __name__ == '__main__':
    unittest.main()
