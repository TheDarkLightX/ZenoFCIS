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
import check_envelope_frame_v2
import check_schema_binding_v2
import check_decision_v2
from verus_coverage import inventory
import v2_proof_sources
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
                    target = 'finite_execution::evaluation::' + (
                        'unchecked_identity' if name == 'add_uncontracted_function' else 'evaluate_into')
                raise ValueError(f'mock reviewed coverage refusal {target}')
            return profile['functions']

        def inventory(vir, bound_profile):
            # Synthetic coverage records; the real coverage-control classifier runs.
            self.assertEqual(bound_profile, profile)
            name = vir.split(': ', 1)[1]
            self.assertEqual(mutants[name][2], 'coverage')
            actual = copy.deepcopy(profile['functions'])
            if gate is check_finite_execution:
                target = 'finite_execution::evaluation::'
                if name == 'add_uncontracted_function':
                    actual[target + 'unchecked_identity'] = {'mode': 'Exec', 'requires': 0, 'ensures': 0}
                else:
                    field = 'requires_sha256' if name == 'narrow_execution_domain' else 'ensures_sha256'
                    record = actual[target + 'evaluate_into']
                    record[field] = '0' * 64
                    record['requires' if name == 'narrow_execution_domain' else 'ensures'] = 0 if name == 'omit_execution_contract' else 1
            else:
                if gate is check_metered_execution:
                    prefix = profile['namespace'] + 'execution_v2::'
                    extra = ('add_uncontracted_work', 'uncharged_identity')
                    edits = {'execute_before_charging': ('numeric::evaluate','body_sha256'), 'omit_metered_contract': ('execute','ensures_sha256'), 'weaken_usage_contract': ('execute','ensures_sha256'), 'narrow_input_domain': ('execute','requires_sha256'), 'inject_caller_usage_argument': ('execute','signature_sha256'), 'change_charge_specification': ('spec::charge','body_sha256')}
                else:
                    prefix = profile['namespace'] + 'execution_v2::input_view::'
                    extra = ('add_uncontracted_projection_work', 'uncontracted_record_probe')
                    edits = {'parse_header_before_ingress': ('typed::project_into','body_sha256'), 'parse_field_before_read': ('typed::project_fields','body_sha256'), 'weaken_projection_contract': ('project','ensures_sha256'), 'narrow_projection_domain': ('project','requires_sha256'), 'inject_caller_usage': ('project','signature_sha256'), 'change_projection_specification': ('spec::projection','body_sha256')}
                if name == extra[0]:
                    actual[prefix + extra[1]] = {'mode': 'Exec', 'requires': 0, 'ensures': 0}
                else:
                    relative, field = edits[name]
                    actual[prefix + relative][field] = '0' * 64
            return actual

        with ExitStack() as stack:
            stack.enter_context(mock.patch.object(gate.platform, 'system', return_value='Linux'))
            stack.enter_context(mock.patch.object(gate.platform, 'machine', return_value='x86_64'))
            stack.enter_context(mock.patch.object(gate.verifier, 'prepare_tools', return_value=self.directory / 'mock-tools'))
            stack.enter_context(mock.patch.object(gate.verifier, 'verify_tool_files'))
            stack.enter_context(mock.patch.object(gate.verifier, 'run', side_effect=fake_run))
            stack.enter_context(mock.patch.object(gate.verifier, 'require_success', side_effect=require_success_after_retention))
            stack.enter_context(mock.patch.object(gate, 'require_coverage', side_effect=coverage))
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
                self.assertEqual(len(mutants), 31 if gate is check_protected_input else 25 if gate is check_finite_execution else 16)
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
                self.assertEqual(len(observed), 1 + len(mutants))
                self.assertEqual(len(entries), len(mutants))
                self.assertFalse(entries[0]['killed'])

    def test_malformed_control_retains_raw_stdout_before_json_classification(self):
        for gate in GATES:
            with self.subTest(gate=gate.__name__):
                receipt, observed, mutants = self.run_gate(gate, 'malformed')
                self.assertEqual(len(observed), 1 + len(mutants))
                self.assertEqual(observed[1][2].stdout, 'not JSON: raw mutant stdout\n')

    def test_proof_control_without_semantic_error_is_not_killed(self):
        for gate in GATES:
            with self.subTest(gate=gate.__name__):
                receipt, observed, mutants = self.run_gate(gate, 'missing-semantic')
                entries = json.loads((Path(receipt['logs']) / 'mutations.json').read_text())
                self.assertEqual(len(observed), 1 + len(mutants))
                self.assertEqual(len(entries), len(mutants))
                self.assertFalse(entries[0]['killed'])
                self.assertEqual(observed[1][2].returncode, 1)
                self.assertEqual(observed[1][2].stderr,
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

class StrictGateMutationTests(unittest.TestCase):
    def test_filtered_native_tests_keep_oracle_and_table_module_dependencies(self):
        required = {
            Path('crates/zeno-fcis-synthesis/src/finite/execution_v2/oracle/mod.rs'),
            Path('crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/ordered_tests.rs'),
            Path('crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/tests.rs'),
        }
        for gate in (check_finite_execution, check_metered_execution, check_protected_input,
                     check_envelope_frame_v2, check_schema_binding_v2, check_decision_v2):
            with self.subTest(gate=gate.__name__):
                self.assertTrue(required.issubset(set(gate.UNIT_SOURCES)))

    def test_every_copy_list_binds_both_source_registry_files(self):
        for gate in (check_finite_execution, check_metered_execution, check_protected_input,
                     check_envelope_frame_v2, check_schema_binding_v2, check_decision_v2):
            sources = set(gate.UNIT_SOURCES)
            self.assertIn(v2_proof_sources.SOURCE_MANIFEST, sources)
            self.assertIn(v2_proof_sources.TEST_MANIFEST, sources)
            bound = gate.snapshot()
            for path in v2_proof_sources.NATIVE_TEST_DATA:
                self.assertIn(path, sources)
                self.assertEqual(bound[str(path)], gate.verifier.digest(gate.ROOT/path))
            for path in (v2_proof_sources.SOURCE_MANIFEST, v2_proof_sources.TEST_MANIFEST):
                self.assertEqual(bound[str(path)], gate.verifier.digest(gate.ROOT / path))
        production = json.loads((ROOT / v2_proof_sources.SOURCE_MANIFEST).read_text())['paths']
        self.assertNotIn(str(v2_proof_sources.TEST_MANIFEST), production)


    """Synthetic diagnostics exercise admission; they are not verifier evidence."""
    def test_declared_targets_are_complete_and_reject_unrelated_failures(self):
        for gate in (check_metered_execution, check_protected_input, check_envelope_frame_v2,
                     check_schema_binding_v2, check_decision_v2):
            pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
            profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
            if gate is check_metered_execution:
                paths = (gate.SUBJECT, gate.METER, gate.SPEC, gate.RESOURCE, gate.NUMERIC)
            elif gate is check_protected_input:
                paths = (gate.SUBJECT, gate.SPEC, gate.TYPED)
            elif gate is check_schema_binding_v2:
                paths = (gate.SUBJECT, gate.ADMISSION, gate.SPEC)
            else:
                paths = (gate.SUBJECT, gate.SPEC)
            create = gate.mutations if gate is check_decision_v2 else gate.mutation_sources
            controls = create(*((gate.ROOT / p).read_text() for p in paths))
            names = {name for name, row in controls.items() if row[2] == 'proof'}
            targets = (gate.SEMANTIC_TARGETS if hasattr(gate, 'SEMANTIC_TARGETS')
                       else gate.PROOF_FUNCTIONS if hasattr(gate, 'PROOF_FUNCTIONS') else dict.fromkeys(names, 'frame'))
            self.assertEqual(set(targets), names)
            for name in names:
                path, changed, *_ = controls[name]
                target = targets[name]
                if isinstance(target, tuple):
                    self.assertEqual(path, target[0])
                    target = target[1]
                region = gate.verifier.function_lines(changed, target)
                report = {'verus': {'version': pin['version'], 'commit': pin['commit'],
                    'toolchain': pin['rust_toolchain']},
                    'func-details': dict.fromkeys(profile['target_functions'], {}),
                    'verification-results': {'success': False, 'errors': 1, 'verified': 1,
                        'encountered-error': True, 'encountered-vir-error': False,
                        'is-verifying-entire-crate': True}}
                diagnostic = f'error: postcondition not satisfied\n --> {path}:{region[0]}:1\n'
                summary = 'error: aborting due to 1 previous error\n'
                def classify(stderr, current=report):
                    return gate.verifier.semantic_refusal(
                        subprocess.CompletedProcess([], 1, '', stderr), current, pin,
                        path, gate.ROOT, region=region,
                        target_functions=profile['target_functions'])
                self.assertTrue(classify(diagnostic + summary), (gate.__name__, name))
                for wrong in (diagnostic.replace(str(path), 'unrelated.rs') + summary,
                        diagnostic.replace(f':{region[0]}:1', f':{region[1] + 1}:1') + summary,
                        diagnostic.replace('error: ', 'error[E0308]: ') + summary,
                        diagnostic + 'note: resource limit exceeded\n' + summary,
                        diagnostic + 'note: timed out\n' + summary,
                        diagnostic + 'note: out of memory\n' + summary):
                    self.assertFalse(classify(wrong), (gate.__name__, name, wrong))
                other = copy.deepcopy(report)
                other['verus']['commit'] = 'unqualified'
                self.assertFalse(classify(diagnostic + summary, other))

    def test_coverage_controls_require_the_intended_target_and_fault(self):
        cases = (
            (check_metered_execution, 'metered_execution::', 'execution_v2::',
             'execute_before_charging', 'numeric::evaluate', 'add_uncontracted_work', 'uncharged_identity'),
            (check_protected_input, 'protected_input::', 'execution_v2::input_view::',
             'parse_header_before_ingress', 'typed::project_into',
             'add_uncontracted_projection_work', 'uncontracted_record_probe'),
            (check_envelope_frame_v2, 'envelope_frame::canonical_v2::', 'envelope::',
             'change_frame_specification', 'spec::frame', 'add_uncontracted_function', 'uncontracted_frame_length'),
            (check_schema_binding_v2, 'schema_binding::canonical_v2::', 'schema::',
             'equivalent_schema_specification_change', 'spec::encoding_matches',
             'add_uncontracted_schema_function', 'uncontracted_schema_length'),
            (check_decision_v2, 'decision::', 'execution_v2::decision::',
             'resolve_before_write_charge', 'assignments', 'uncontracted_helper', 'uncontracted_decision_probe'),
        )
        def function(name, body='0', mode='Exec', ensures='((Bool true))'):
            return (f'(Function (Fun :path {name}) :mode {mode} :typ_bounds () '
                f':params () :ret () :require () :ensure {ensures} :d () :body (Int {body}))')
        for gate, namespace, suffix, control, target_suffix, add, extra_suffix in cases:
            target = namespace + suffix + target_suffix
            other = namespace + 'identity'
            extra = namespace + suffix + extra_suffix
            mode = 'Spec' if gate in (check_envelope_frame_v2, check_schema_binding_v2) else 'Exec'
            original = function(target, mode=mode) + function(other, mode='Spec')
            bodies = [target] if mode == 'Exec' else []
            profile = {'namespace': namespace, 'body_covered_functions': bodies,
                'functions': inventory(original, namespace, tuple(bodies))}
            intended = function(target, body='1', mode=mode) + function(other, mode='Spec')
            incidental = function(target, mode=mode) + function(other, body='1', mode='Spec')
            unchecked = function(extra, ensures='()')
            self.assertTrue(gate.coverage_control(intended, profile, control)[0])
            self.assertFalse(gate.coverage_control(incidental, profile, control)[0])
            self.assertFalse(gate.coverage_control(intended + unchecked, profile, control)[0])
            self.assertTrue(gate.coverage_control(original + unchecked, profile, add)[0])
            self.assertFalse(gate.coverage_control(original + function(extra), profile, add)[0])
            self.assertFalse(gate.coverage_control(original + unchecked + function(namespace + 'unexpected'), profile, add)[0])

if __name__ == '__main__':
    unittest.main(verbosity=2)
