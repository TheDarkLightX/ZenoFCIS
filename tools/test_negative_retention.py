"""Mocked orchestration regression tests; fixtures are not proof results."""
from __future__ import annotations

from contextlib import ExitStack, redirect_stdout
import copy
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import check_finite_execution
import check_metered_execution
import check_protected_input

ROOT = Path(__file__).resolve().parents[1]
GATES = (check_finite_execution, check_metered_execution, check_protected_input)


class NegativeRetentionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='retention-mock-')
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)

    def mutations(self, gate):
        if gate.__name__ == 'check_finite_execution':
            paths = (gate.SUBJECT, gate.ADMISSION, gate.TYPED, gate.VALUE)
        elif gate.__name__ == 'check_metered_execution':
            paths = (gate.SUBJECT, gate.METER, gate.SPEC, gate.RESOURCE, gate.NUMERIC)
        else:
            paths = (gate.SUBJECT, gate.SPEC, gate.TYPED)
        return gate.mutation_sources(*((gate.ROOT / path).read_text() for path in paths))

    def run_gate(self, gate, mode='passed', *, output=None, cli=True):
        self.assertEqual(gate.ROOT, ROOT)
        mutants = self.mutations(gate)
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
        before = gate.snapshot()
        positive_report = {
            'verus': {'commit': pin['commit'], 'version': pin['version'],
                      'toolchain': pin['rust_toolchain']},
            'verification-results': {'success': True, 'errors': 0,
                'verified': profile['expected_verified'], 'encountered-error': False,
                'encountered-vir-error': False, 'is-verifying-entire-crate': True},
            'func-details': dict.fromkeys(profile['target_functions'], {}),
        }
        observed = []
        first_name = next(iter(mutants))

        def fake_run(command, cwd, environment, *args, **kwargs):
            if '--version' in command:
                return subprocess.CompletedProcess(command, 0, 'mock version\n', '')
            self.assertIn('--output-json', command)
            log_dir = Path(command[command.index('--log-dir') + 1])
            log_dir.mkdir(parents=True)
            positive = log_dir.name == 'positive'
            if positive:
                location = log_dir.parent
                report = copy.deepcopy(positive_report)
                text, stderr, returncode = json.dumps(report) + '\n', 'positive mock stderr\n', 0
                vir = 'positive translated-source fixture'
                self.assertEqual(cwd, gate.ROOT)
                if mode == 'positive-refusal':
                    text, stderr, returncode = 'raw positive refusal\n', 'raw positive failure details\n', 1
            else:
                name = Path(cwd).name
                path, changed, expected = mutants[name]
                self.assertEqual((Path(cwd) / path).read_text(), changed)
                self.assertEqual(command[-1], str(gate.HARNESS))
                location = Path(cwd)
                report = copy.deepcopy(positive_report)
                report['verification-results'].update(success=expected != 'proof',
                    errors=int(expected == 'proof'), **{'encountered-error': expected == 'proof'})
                text = json.dumps(report) + '\n'
                stderr, returncode = f'raw mock diagnostic for {name}\n', int(expected == 'proof')
                if expected == 'proof':
                    if gate is check_finite_execution:
                        target = gate.SEMANTIC_TARGETS[name]
                    elif gate is check_metered_execution:
                        target_path, target = gate.SEMANTIC_TARGETS[name]
                        self.assertEqual(path, target_path)
                    else:
                        target = gate.PROOF_FUNCTIONS[name]
                    line, _ = gate.verifier.function_lines(changed, target)
                    stderr += (f'error: postcondition not satisfied\n --> {path}:{line}:1\n'
                               'error: aborting due to 1 previous error\n')
                vir = f'coverage-refusal fixture: {name}'
                if name == first_name and mode == 'missing-semantic':
                    self.assertEqual(expected, 'proof')
                    stderr = f'raw mock diagnostic for {name}\n'
                if name == first_name and mode == 'survivor':
                    result = {'success': True, 'errors': 0, 'encountered-vir-error': False}
                    text, returncode = json.dumps({'verification-results': result}) + '\n', 0
                if name == first_name and mode == 'malformed':
                    text, stderr, returncode = 'not JSON: raw mutant stdout\n', 'raw mutant parse-failure details\n', 1
            (log_dir / 'crate.vir').write_text(vir)
            process = subprocess.CompletedProcess(command, returncode, text, stderr)
            observed.append((location, 'positive' if positive else 'verus', process, str(cwd)))
            return process

        original_require_success = gate.verifier.require_success
        def require_success_after_retention(process):
            if '--output-json' in process.args:
                location, prefix, _, cwd = observed[-1]
                self.assertEqual((location / f'{prefix}.json').read_text(), process.stdout)
                self.assertEqual((location / f'{prefix}.stderr').read_text(), process.stderr)
                self.assertEqual(json.loads((location / f'{prefix}-process.json').read_text()),
                    {'command': process.args, 'cwd': cwd, 'returncode': process.returncode})
            return original_require_success(process)

        def coverage(vir, bound_profile):
            self.assertEqual(bound_profile, profile)
            if vir.startswith('coverage-refusal'):
                location, prefix, process, cwd = observed[-1]
                self.assertTrue((location / f'{prefix}-process.json').is_file())
                self.assertEqual((location / f'{prefix}.stderr').read_text(), process.stderr)
                target = ''
                if gate is check_finite_execution:
                    name = vir.split(': ', 1)[1]
                    target = profile['namespace'] + (
                        'unchecked_identity' if name == 'add_uncontracted_function' else 'evaluate_into')
                raise ValueError(f'mock reviewed coverage refusal {target}')
            return profile['functions']

        def inventory(vir, bound_profile):
            # Synthetic coverage records; the real coverage-control classifier runs.
            self.assertEqual(bound_profile, profile)
            name = vir.split(': ', 1)[1]
            self.assertEqual(mutants[name][2], 'coverage')
            actual = copy.deepcopy(profile['functions'])
            target = profile['namespace']
            if name == 'add_uncontracted_function':
                actual[target + 'unchecked_identity'] = {'mode': 'Exec', 'requires': 0, 'ensures': 0}
            else:
                field = 'requires_sha256' if name == 'narrow_execution_domain' else 'ensures_sha256'
                actual[target + 'evaluate_into'][field] = '0' * 64
            return actual

        with ExitStack() as stack:
            stack.enter_context(mock.patch.object(gate.platform, 'system', return_value='Linux'))
            stack.enter_context(mock.patch.object(gate.platform, 'machine', return_value='x86_64'))
            stack.enter_context(mock.patch.object(gate.verifier, 'prepare_tools', return_value=self.directory / 'mock-tools'))
            stack.enter_context(mock.patch.object(gate.verifier, 'verify_tool_files'))
            stack.enter_context(mock.patch.object(gate.verifier, 'run', side_effect=fake_run))
            stack.enter_context(mock.patch.object(gate.verifier, 'require_success', side_effect=require_success_after_retention))
            stack.enter_context(mock.patch.object(gate, 'require_coverage', side_effect=coverage))
            if gate is check_finite_execution:
                stack.enter_context(mock.patch.object(gate, 'profile_inventory', side_effect=inventory))
            # Copy only the source fixtures used by the fake verifier.
            # Compilation and native execution remain explicitly mocked.
            stack.enter_context(mock.patch.object(gate, 'UNIT_SOURCES',
                tuple(dict.fromkeys((gate.HARNESS, *(value[0] for value in mutants.values()))))))
            native_name = 'native_tests' if gate.__name__ == 'check_finite_execution' else 'native_checks'
            native = (subprocess.CompletedProcess([], 0, 'mock native output\n', '')
                      if native_name == 'native_tests' else {'test_output': 'mock native output', 'api_consumers': {}})
            stack.enter_context(mock.patch.object(gate, native_name, return_value=native))
            if hasattr(gate, 'adjust_specimen_source_lengths'):
                stack.enter_context(mock.patch.object(gate, 'adjust_specimen_source_lengths'))
            def git_query(command, **kwargs):
                self.assertEqual(kwargs.get('cwd'), ROOT)
                if command == ['git', 'rev-parse', 'HEAD']:
                    return 'a' * 40 + '\n'
                if command == ['git', 'status', '--porcelain']:
                    return ''
                raise AssertionError(f'unexpected subprocess command: {command!r}')

            stack.enter_context(mock.patch.object(gate.subprocess, 'check_output', side_effect=git_query))
            stack.enter_context(mock.patch.object(gate.subprocess, 'run',
                side_effect=AssertionError('real subprocesses are forbidden in this test')))
            stack.enter_context(mock.patch.object(gate.subprocess, 'Popen',
                side_effect=AssertionError('real subprocesses are forbidden in this test')))
            stack.enter_context(redirect_stdout(io.StringIO()))
            if cli:
                output = output or self.directory / gate.__name__ / f'{mode}.json'
                stack.enter_context(mock.patch.object(sys, 'argv',
                    [gate.__file__, '--cache', str(self.directory / 'unused-cache'), '--out', str(output)]))
                status = gate.main()
                receipt = json.loads(output.read_text())
                self.assertEqual(status, 0 if mode == 'passed' else 1)
                self.assertEqual(receipt['status'], 'passed' if mode == 'passed' else 'failed')
                logs = Path(receipt['logs'])
                self.assertTrue(logs.is_dir())
                self.assertEqual(logs.parent, output.parent)
                self.assertEqual(json.loads((logs / 'source_sha256.json').read_text()), before)
                for location, prefix, process, cwd in observed:
                    self.assertEqual((location / f'{prefix}.json').read_text(), process.stdout)
                    self.assertEqual((location / f'{prefix}.stderr').read_text(), process.stderr)
                    self.assertEqual(json.loads((location / f'{prefix}-process.json').read_text()),
                        {'command': process.args, 'cwd': cwd, 'returncode': process.returncode})
                return receipt, observed, mutants
            receipt = gate.check(self.directory / 'unused-cache', False)
            self.assertFalse(observed[0][0].exists())
            return receipt, observed, mutants

    def test_success_retains_every_control_and_exact_raw_diagnostics(self):
        for gate in GATES:
            with self.subTest(gate=gate.__name__):
                receipt, observed, mutants = self.run_gate(gate)
                self.assertEqual(len(mutants), 31 if gate is check_protected_input else 16)
                self.assertEqual(len(observed), 1 + len(mutants))
                self.assertEqual([entry['name'] for entry in receipt['mutations']], list(mutants))
                self.assertTrue(all(entry['killed'] for entry in receipt['mutations']))
                self.assertEqual(json.loads((Path(receipt['logs']) / 'mutations.json').read_text()), receipt['mutations'])
                self.assertTrue(all(Path(entry['logs']).is_dir() for entry in receipt['mutations']))

    def test_positive_refusal_retains_output_before_success_classification(self):
        for gate in GATES:
            with self.subTest(gate=gate.__name__):
                receipt, observed, mutants = self.run_gate(gate, 'positive-refusal')
                self.assertEqual(len(observed), 1)

    def test_surviving_control_retains_raw_diagnostics_and_partial_index(self):
        for gate in GATES:
            with self.subTest(gate=gate.__name__):
                receipt, observed, mutants = self.run_gate(gate, 'survivor')
                entries = json.loads((Path(receipt['logs']) / 'mutations.json').read_text())
                self.assertEqual(len(observed), 2)
                self.assertEqual(len(entries), 1)
                self.assertFalse(entries[0]['killed'])

    def test_malformed_control_retains_raw_stdout_before_json_classification(self):
        for gate in GATES:
            with self.subTest(gate=gate.__name__):
                receipt, observed, mutants = self.run_gate(gate, 'malformed')
                self.assertEqual(len(observed), 2)
                self.assertEqual(observed[-1][2].stdout, 'not JSON: raw mutant stdout\n')

    def test_proof_control_without_semantic_error_is_not_killed(self):
        for gate in GATES:
            with self.subTest(gate=gate.__name__):
                receipt, observed, mutants = self.run_gate(gate, 'missing-semantic')
                entries = json.loads((Path(receipt['logs']) / 'mutations.json').read_text())
                self.assertEqual(len(observed), 2)
                self.assertEqual(len(entries), 1)
                self.assertFalse(entries[0]['killed'])
                self.assertEqual(observed[-1][2].returncode, 1)
                self.assertEqual(observed[-1][2].stderr,
                                 f'raw mock diagnostic for {next(iter(mutants))}\n')

    def test_default_check_still_cleans_its_temporary_directory(self):
        for gate in GATES:
            with self.subTest(gate=gate.__name__):
                receipt, observed, mutants = self.run_gate(gate, cli=False)
                self.assertEqual(len(receipt['mutations']), len(mutants))

    def test_cli_runs_create_different_owned_directories_beside_receipts(self):
        for gate in GATES:
            with self.subTest(gate=gate.__name__):
                output = self.directory / gate.__name__ / 'refused.json'
                first, _, _ = self.run_gate(gate, 'positive-refusal', output=output)
                second, _, _ = self.run_gate(gate, 'positive-refusal', output=output)
                self.assertNotEqual(first['logs'], second['logs'])
                self.assertTrue(Path(first['logs']).is_dir())
                self.assertTrue(Path(second['logs']).is_dir())

if __name__ == '__main__':
    unittest.main(verbosity=2)
