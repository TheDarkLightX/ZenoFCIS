"""Fail-closed acceptance tests for the guarded-law gate itself."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import check_guarded_laws_v2 as gate
from verus_coverage import inventory, require_coverage

# Verbatim retained stderr of the legacy-form change_declared_law_order (hosted run
# 37404295156 at rlimit 10, and the rlimit 40 rerun in
# claude-selected-evidence/guarded-laws-changed-20261006T070311Z): exhausted inside
# evaluate_into. It stays an inconclusive refusal, never a kill; this original
# specimen is now declared native and is never proof-run here.
LEGACY_LAW_ORDER_SHA256 = '55021af79009d390af1935f979047fd591c938ac0e3f238c33f3045f07647702'
# Withdrawn inconclusive symbolic retargets, never kills: laws[0].id (7ce5b1d5…a11d)
# mixed a postcondition error with loop exhaustion at rlimit 10 and 40; laws[i].id ^ 1
# (claude-selected-evidence/guarded-laws-final-identity) only exhausted at rlimit 10.
WITHDRAWN_XOR_SHA256 = '789a2e2c61e4a6a4b72073efa92c7562e4f037cb6c761d5e37fe4affe37a87f4'
EXHAUSTED = """error: while loop: Resource limit (rlimit) exceeded; consider rerunning with --profile for more details
   --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/laws.rs:199:5
    |
199 |     while i < laws.len() {
    |     ^^^^^^^^^^^^^^^^^^^^

error: aborting due to 1 previous error
"""


class GuardedLawCoverage(unittest.TestCase):
    def test_whole_registered_closure_has_exact_total_contracts_and_raw_bodies(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        functions = profile['functions']
        runtime = {name for name, record in functions.items() if record['mode'] == 'Exec'}
        self.assertEqual(profile['namespace'], 'guarded_laws::')
        self.assertEqual(set(profile['body_covered_functions']), runtime)
        self.assertTrue(all(record['requires'] == 0 and record['ensures'] > 0 for name, record in functions.items() if name in runtime))
        self.assertTrue(all('body_sha256' in record for record in functions.values() if record['mode'] in ('Exec', 'Spec')))
        for suffix in ['laws::predicate::node', 'laws::predicate::shape', 'laws::predicate::default_text_valid',
            'laws::evaluate_into', 'laws::evaluate', 'laws::frame::observe',
            'authority::metadata::law_op', 'authority::metadata::law_atom',
            'authority::policy::policy_bytes',
            'composition::outcome::transition_run', 'composition::outcome::genesis_run']:
            self.assertTrue(any(name.endswith(suffix) for name in runtime), suffix)
        for suffix in ['laws::spec::node', 'laws::spec::read_observation', 'laws::spec::default_valid', 'laws::spec::default_text_valid',
            'authority::metadata_spec::law_op', 'authority::evaluator::EVALUATOR']:
            self.assertTrue(any(name.endswith(suffix) for name in functions), suffix)

    def test_all_legacy_controls_remain_and_new_controls_have_precise_targets(self):
        controls = gate.controls()
        old = gate.legacy.mutation_sources()
        self.assertEqual(len(old), 28)
        self.assertEqual(len(controls), 43)
        self.assertEqual(list(controls), [*old, 'guard_direction', 'guard_nonprior_access', 'guard_wrong_node',
            'false_guard_reads', 'wrong_fallback_value', 'active_fallback_swallows_errors',
            'active_undefined_success', 'active_budget_success', 'denied_read_trace', 'invalid_default_text',
            'skip_inactive_default_admission', 'canonical_guarded_opcode', 'canonical_guard_index',
            'canonical_guarded_default', 'canonical_default_type'])
        self.assertEqual(sum(c['kind'] == 'proof' for c in controls.values()), 36)
        self.assertEqual(sum(c['kind'] == 'coverage' for c in controls.values()), 5)
        native = {n for n, c in controls.items() if c['kind'] == 'native'}
        self.assertEqual(native, {'shrink_delivery_ordinals', 'change_declared_law_order'})
        self.assertEqual(set(gate.NATIVE_ORACLES), native)
        for name, (path, changed, kind) in old.items():
            self.assertEqual((controls[name]['path'], controls[name]['changed']), (path, changed))
            self.assertEqual(controls[name]['kind'], 'native' if name in native else kind)
        for control in controls.values():
            self.assertNotEqual(control['changed'], (gate.ROOT / control['path']).read_text())
            if control['kind'] == 'proof':
                first, last = gate.target_lines(control['changed'], control['target'])
                self.assertGreater(last, first)
            elif control['kind'] == 'coverage':
                self.assertIn(control['field'], ('body_sha256', 'ensures_sha256', 'requires_sha256', 'inventory'))

    def test_native_ordinal_control_requires_exact_independent_before_and_after(self):
        baseline = SimpleNamespace(returncode=0, stdout=f'test {gate.ORDINAL_TEST} ... ok\n')
        output = (f'running 1 test\ntest {gate.ORDINAL_TEST} ... FAILED\n'
            'laws/tests.rs:799:9: assertion left == right failed\n'
            '  left: Err(Frame)\n right: Ok(())\n'
            'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured\n')
        negative = SimpleNamespace(returncode=101, stdout=output)
        self.assertTrue(gate.native_ordinal_refusal(baseline, negative)[0])
        self.assertFalse(gate.native_ordinal_refusal(SimpleNamespace(returncode=1, stdout=baseline.stdout), negative)[0])
        self.assertFalse(gate.native_ordinal_refusal(baseline, SimpleNamespace(returncode=0, stdout=output))[0])
        for bad in [output.replace(gate.ORDINAL_TEST, 'unrelated_test'),
            output.replace('Err(Frame)', 'Err(Undefined)'), output.replace('Ok(())', 'Err(Budget)'),
            output.replace('0 passed; 1 failed;', '1 passed; 1 failed;'), 'resource limit exceeded']:
            self.assertFalse(gate.native_ordinal_refusal(baseline, SimpleNamespace(returncode=101, stdout=bad))[0])

    def test_compile_resource_and_unrelated_failures_cannot_be_semantic_kills(self):
        c = gate.controls()['guard_direction']
        first, _ = gate.target_lines(c['changed'], c['target'])
        diagnostic = f'error: postcondition not satisfied\n --> {c["path"]}:{first}:1\n'
        proc = SimpleNamespace(returncode=1, stderr=diagnostic)
        report = {'verification-results': {'success': False, 'errors': 1, 'encountered-vir-error': False}}
        self.assertTrue(gate.semantic_refusal(proc, report, c)[0])
        for extra in ['error[E0004]: non-exhaustive pattern', 'resource limit exceeded', 'timed out', 'out of memory']:
            self.assertFalse(gate.semantic_refusal(SimpleNamespace(returncode=1, stderr=diagnostic + extra), report, c)[0])
        for result in [dict(success=True, errors=0, **{'encountered-vir-error': False}),
            dict(success=False, errors=1, **{'encountered-vir-error': True})]:
            self.assertFalse(gate.semantic_refusal(proc, {'verification-results': result}, c)[0])
        self.assertFalse(gate.semantic_refusal(SimpleNamespace(returncode=1, stderr='error: postcondition not satisfied\n --> unrelated.rs:1:1'), report, c)[0])

    def test_law_order_control_is_the_original_specimen_declared_native_before_any_run(self):
        c = gate.controls()['change_declared_law_order']
        legacy = gate.legacy.mutation_sources()['change_declared_law_order'][1]
        self.assertEqual(c['changed'], legacy)
        self.assertEqual(hashlib.sha256(c['changed'].encode()).hexdigest(), LEGACY_LAW_ORDER_SHA256)
        self.assertNotEqual(hashlib.sha256(c['changed'].encode()).hexdigest(), WITHDRAWN_XOR_SHA256)
        self.assertEqual((c['path'], c['kind'], c['target'], c['native']), (gate.LAW, 'native', 'evaluate_into', True))
        self.assertNotIn('obligations', c)
        self.assertEqual(gate.NATIVE_ORACLES['change_declared_law_order'],
                         (gate.LAW_ORDER_TEST, gate.native_law_order_refusal))
        # The ordinal oracle is retained unchanged and selected only by its own name.
        self.assertEqual(gate.NATIVE_ORACLES['shrink_delivery_ordinals'], (gate.ORDINAL_TEST, gate.native_ordinal_refusal))
        # The exact test is a real test in the public file the harness includes.
        test = gate.LAW_ORDER_TEST.removeprefix('public_guarded_laws::')
        source = (gate.ROOT / gate.TEST).read_text()
        self.assertEqual(source.count(f'#[test]\nfn {test}() {{'), 1)
        self.assertIn('#[path = "../../crates/zeno-fcis-synthesis/tests/v2_guarded_laws.rs"]\nmod public_guarded_laws;',
                      (gate.ROOT / gate.HARNESS).read_text())
        self.assertEqual(source.count(gate.LAW_ORDER_MESSAGE), 1)

    def test_every_proof_run_records_the_default_budget_10(self):
        self.assertEqual(gate.RLIMIT, 10)
        base = ['verus', '--output-json']
        for name in ['positive', *(n for n, c in gate.controls().items() if c['kind'] != 'native')]:
            self.assertEqual(gate.proof_command(base, Path('logs') / name),
                             [*base, '--rlimit', '10', '--log-dir', str(Path('logs') / name), str(gate.HARNESS)], name)
        # The retained command record carries the exact argv, including the budget.
        argv = gate.proof_command(base, Path('logs'))
        finished = subprocess.CompletedProcess(argv, 1, '{}', EXHAUSTED)
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(gate.verifier, 'run', return_value=finished) as run:
            gate.saved_run(argv, Path(temporary), {}, Path(temporary), 'change_declared_law_order')
            record = json.loads((Path(temporary) / 'change_declared_law_order.command.json').read_text())
        self.assertEqual(run.call_args.args[0], argv)
        self.assertEqual((record['command'], record['exit']), (argv, 1))
        self.assertEqual(record['command'][record['command'].index('--rlimit') + 1], '10')

    def test_evaluate_into_exhaustion_unintended_or_misplaced_failure_is_never_a_semantic_kill(self):
        c = gate.controls()['reset_shared_meter']
        self.assertEqual((c['kind'], c['target']), ('proof', 'evaluate_into'))
        first, last = gate.target_lines(c['changed'], c['target'])
        report = {'verification-results': {'success': False, 'errors': 1, 'encountered-vir-error': False}}
        path = 'verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/laws.rs'
        semantic = f'error: postcondition not satisfied\n   --> {path}:{first + 1}:5\n'
        self.assertTrue(gate.semantic_refusal(SimpleNamespace(returncode=1, stderr=semantic), report, c)[0])
        self.assertFalse(gate.semantic_refusal(SimpleNamespace(returncode=1, stderr=EXHAUSTED), report, c)[0])
        for inconclusive in (EXHAUSTED, 'note: verification timed out\n', 'memory allocation failed: out of memory\n',
                             'error[E0308]: mismatched types\n'):
            mixed = SimpleNamespace(returncode=1, stderr=semantic + inconclusive)
            self.assertFalse(gate.semantic_refusal(mixed, report, c)[0], inconclusive)
        vir = {'verification-results': {'success': False, 'errors': 1, 'encountered-vir-error': True}}
        self.assertFalse(gate.semantic_refusal(SimpleNamespace(returncode=1, stderr=semantic), vir, c)[0])
        for misplaced in (f'error: postcondition not satisfied\n   --> {path}:{last + 1}:5\n',
                          'error: postcondition not satisfied\n   --> crates/x/src/unrelated.rs:199:5\n'):
            self.assertFalse(gate.semantic_refusal(SimpleNamespace(returncode=1, stderr=misplaced), report, c)[0])
        # The retained obligations filter still narrows a control that declares one.
        other = f'error: precondition not satisfied\n   --> {path}:{first + 1}:21\n'
        self.assertTrue(gate.semantic_refusal(SimpleNamespace(returncode=1, stderr=other), report, c)[0])
        narrowed = {**c, 'obligations': ('postcondition not satisfied',)}
        self.assertFalse(gate.semantic_refusal(SimpleNamespace(returncode=1, stderr=other), report, narrowed)[0])

    def law_order_output(self, line=None):
        if line is None:
            source = (gate.ROOT / gate.TEST).read_text()
            line = source.count('\n', 0, source.rfind('assert_eq!(', 0, source.index(gate.LAW_ORDER_MESSAGE))) + 1
        name = gate.LAW_ORDER_TEST
        return (f'\nrunning 1 test\ntest {name} ... FAILED\n\nfailures:\n\n---- {name} stdout ----\n\n'
            f"thread '{name}' panicked at verification/verus/../../crates/zeno-fcis-synthesis/tests/v2_guarded_laws.rs:{line}:9:\n"
            f'assertion `left == right` failed: {gate.LAW_ORDER_MESSAGE}\n'
            '  left: (None, [Diagnostic { id: 1, verdict: Satisfied }, Diagnostic { id: 2, verdict: Skipped }, '
            'Diagnostic { id: 3, verdict: Skipped }, Diagnostic { id: 4, verdict: Satisfied }, '
            'Diagnostic { id: 5, verdict: Skipped }, Diagnostic { id: 60, verdict: Satisfied }])\n'
            ' right: (Some(Law(Violated)), [Diagnostic { id: 1, verdict: Satisfied }, Diagnostic { id: 2, verdict: Skipped }, '
            'Diagnostic { id: 3, verdict: Skipped }, Diagnostic { id: 4, verdict: Satisfied }, '
            'Diagnostic { id: 5, verdict: Skipped }, Diagnostic { id: 60, verdict: Refused(Violated) }])\n'
            'note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n\n'
            f'failures:\n    {name}\n\n'
            'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s\n\n')

    def test_native_law_order_control_requires_exact_baseline_and_public_assertion_refusal(self):
        name = gate.LAW_ORDER_TEST
        baseline = SimpleNamespace(returncode=0, stdout=f'running 9 tests\ntest {gate.ORDINAL_TEST} ... ok\ntest {name} ... ok\n')
        output = self.law_order_output()
        mutant = SimpleNamespace(returncode=101, stdout=output)
        killed, intended = gate.native_law_order_refusal(baseline, mutant)
        self.assertTrue(killed)
        self.assertEqual((intended['function'], intended['test']), ('laws::evaluate_into', name))
        self.assertRegex(intended['assertion'], r'^crates/zeno-fcis-synthesis/tests/v2_guarded_laws\.rs:\d+$')
        # Baseline must have exited 0 with this exact test present and passed.
        for bad in [SimpleNamespace(returncode=101, stdout=baseline.stdout),
                    SimpleNamespace(returncode=0, stdout=baseline.stdout.replace(f'test {name} ... ok\n', '')),
                    SimpleNamespace(returncode=0, stdout=baseline.stdout.replace(f'{name} ... ok', f'{name} ... ignored')),
                    SimpleNamespace(returncode=0, stdout=baseline.stdout.replace(f'{name} ... ok', f'{name}_other ... ok'))]:
            self.assertFalse(gate.native_law_order_refusal(bad, mutant)[0])
        # The mutant run must exit 101 (a crash, signal, success or other code refuses).
        for code in (0, 1, -11, -9, 124):
            self.assertFalse(gate.native_law_order_refusal(baseline, SimpleNamespace(returncode=code, stdout=output))[0], code)
        diagnostic = '  left: (None, [Diagnostic { id: 1'
        for bad in [
            # Compiler errors, crashes and timeouts leave no exact test report.
            'error[E0308]: mismatched types\nerror: aborting due to 1 previous error\n',
            "thread 'main' has overflowed its stack\nfatal runtime error: stack overflow\n",
            'test timed out\n',
            # Absent, ignored or wrong-named test.
            '\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out\n',
            output.replace(f'test {name} ... FAILED', f'test {name} ... ignored').replace('0 passed; 1 failed; 0 ignored;', '0 passed; 0 failed; 1 ignored;'),
            output.replace(name, name + '_other'),
            output.replace(name, gate.ORDINAL_TEST),
            # Unrelated or multiple failures.
            output.replace('running 1 test\n', f'running 2 tests\ntest {gate.ORDINAL_TEST} ... FAILED\n').replace(
                '0 passed; 1 failed;', '0 passed; 2 failed;'),
            output.replace('0 passed; 1 failed;', '1 passed; 1 failed;'),
            output + f"thread '{name}' panicked at verification/verus/../../crates/zeno-fcis-synthesis/tests/v2_guarded_laws.rs:1:1:\n",
            # Wrong semantic assertion diagnostic, message, location or values.
            output.replace(gate.LAW_ORDER_MESSAGE, 'law-order descriptor must bind'),
            output.replace('assertion `left == right` failed: ' + gate.LAW_ORDER_MESSAGE, 'law-order descriptor must bind'),
            output.replace(diagnostic, '  left: (Some(Law(Undefined)), [Diagnostic { id: 1'),
            output.replace('id: 60, verdict: Satisfied }])\n right', 'id: 60, verdict: Refused(Undefined) }])\n right'),
            output.replace('id: 4, verdict: Satisfied }, Diagnostic { id: 5, verdict: Skipped }, Diagnostic { id: 60, verdict: Satisfied',
                           'id: 4, verdict: Refused(Violated)'),
            output.replace(' right: (Some(Law(Violated))', ' right: (Some(Law(Undefined))'),
            self.law_order_output(line=1),
            output.replace('tests/v2_guarded_laws.rs', 'src/finite/execution_v2/laws/tests.rs'),
            # The retained ordinal refusal is not a law-order kill.
            (f'running 1 test\ntest {name} ... FAILED\nlaws/tests.rs:799:9: assertion left == right failed\n'
             '  left: Err(Frame)\n right: Ok(())\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured\n'),
            'resource limit exceeded',
        ]:
            self.assertFalse(gate.native_law_order_refusal(baseline, SimpleNamespace(returncode=101, stdout=bad))[0], bad[:120])
        # Neither oracle accepts the other's exact refusal.
        self.assertFalse(gate.native_ordinal_refusal(baseline, mutant)[0])

    def test_native_build_failure_and_timeout_raise_instead_of_classifying(self):
        failed = subprocess.CompletedProcess(['rustc'], 1, '', 'error[E0425]: cannot find value\n')
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(gate, 'native_dependency_args', return_value=[]), \
             patch.object(gate.verifier, 'run', return_value=failed) as run:
            with self.assertRaises(RuntimeError):
                gate.native_commands(Path(temporary), {}, Path(temporary), 'change_declared_law_order', exact=gate.LAW_ORDER_TEST)
            self.assertEqual(run.call_count, 1)  # the test binary is never run after a failed build
        timeout = subprocess.TimeoutExpired(['native-current'], 240, output=b'running 1 test\n', stderr=b'')
        with tempfile.TemporaryDirectory() as temporary, patch.object(gate.verifier, 'run', side_effect=timeout):
            with self.assertRaises(subprocess.TimeoutExpired):
                gate.saved_run(['native-current'], Path(temporary), {}, Path(temporary), 'change_declared_law_order.native', 240)
            record = json.loads((Path(temporary) / 'change_declared_law_order.native.command.json').read_text())
        self.assertIsNone(record['exit'])

    def test_native_law_order_run_uses_the_exact_test_argv(self):
        built = subprocess.CompletedProcess(['rustc'], 0, '', '')
        with tempfile.TemporaryDirectory() as temporary, \
             patch.object(gate, 'native_dependency_args', return_value=[]), \
             patch.object(gate.verifier, 'run', side_effect=lambda command, *a, **k: subprocess.CompletedProcess(command, 101, '', '') if command[0] != 'rustc' else built):
            n = gate.native_commands(Path(temporary), {}, Path(temporary), 'change_declared_law_order', exact=gate.LAW_ORDER_TEST)
        self.assertEqual(n.args, [str(Path(temporary) / 'native-current'), '--test-threads=1', '--exact', gate.LAW_ORDER_TEST])
        self.assertEqual(n.returncode, 101)

    def test_raw_coverage_refuses_operational_identifier_change_and_omission(self):
        # This is a parser/gate test, not a claimed Verus semantic specimen.
        vir = '(Function (Fun :path guarded_laws::execution_v2::laws::predicate::node) :mode Exec :typ_bounds () :params () :ret () :require () :ensure ((Bool true)) :d () :body (ReadAttempt 41))'
        name = 'guarded_laws::execution_v2::laws::predicate::node'
        profile = {'namespace': gate.NAMESPACE, 'functions': inventory(vir, gate.NAMESPACE, (name,)), 'body_covered_functions': [name]}
        require_coverage(vir, profile)
        c = {'target': 'laws::predicate::node', 'field': 'body_sha256'}
        self.assertTrue(gate.coverage_refusal(vir.replace('ReadAttempt 41', 'ReadAttempt 42'), profile, c)[0])
        weakened = vir.replace(':ensure ((Bool true))', ':ensure ()')
        with self.assertRaises(ValueError):
            require_coverage(weakened, profile)
        wrong = copy.deepcopy(profile)
        wrong['body_covered_functions'] = []
        with self.assertRaises(ValueError):
            require_coverage(vir, wrong)

    def test_source_manifest_includes_actual_metadata_getter_test_and_original_graph(self):
        paths = set(gate.sources())
        self.assertTrue({gate.HARNESS, gate.PROFILE, gate.TEST, gate.PREDICATE,
            gate.METADATA, gate.authority.EVALUATOR}.issubset(paths))
        self.assertIn(Path('crates/zeno-fcis-cli/templates/inventory-reservation/synthesized/program.zcve'), paths)
        self.assertEqual(set(gate.snapshot()), set(map(str, paths)))
        with patch.object(Path, 'is_symlink', return_value=True):
            with self.assertRaisesRegex(ValueError, 'symlink'):
                gate.snapshot()


if __name__ == '__main__':
    unittest.main()
