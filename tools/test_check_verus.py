"""Admission tests for Verus reports and native tests of the shared source."""

import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import check_verus as gate


class EvidenceAdmission(unittest.TestCase):
    def test_proof_workers_preserve_defaults_and_refuse_unassigned_counts(self):
        self.assertEqual(gate.proof_threads('1', {}), '1')
        self.assertEqual(gate.proof_threads('2', {}), '2')
        for workers in range(1, 7):
            self.assertEqual(gate.proof_threads(environment={'ZENO_FCIS_VERUS_THREADS': str(workers)}), str(workers))
        for invalid in ('0', '7', '-1', '1.5', 'six', '', True, None):
            with self.assertRaises(ValueError):
                gate.proof_threads(environment={'ZENO_FCIS_VERUS_THREADS': invalid})


    def setUp(self):
        self.pin = json.loads((gate.ROOT / gate.PIN).read_text())
        # A synthetic protocol fixture; no proof result is claimed by this test.
        self.report = {
            "verification-results": {
                "success": True, "errors": 0, "verified": self.pin["expected_verified"],
                "encountered-error": False, "encountered-vir-error": False,
                "is-verifying-entire-crate": True,
            },
            "verus": {"commit": self.pin["commit"], "version": self.pin["version"]},
            "func-details": {name: {} for name in self.pin["target_functions"]},
        }

    def test_refuses_incomplete_or_failed_reports(self):
        self.assertTrue(gate.accepted(self.report, self.pin))
        for field, value in (("success", False), ("errors", 1), ("verified", 0),
                             ("encountered-error", True), ("encountered-vir-error", True),
                             ("is-verifying-entire-crate", False)):
            with self.subTest(field=field):
                changed = copy.deepcopy(self.report)
                changed["verification-results"][field] = value
                self.assertFalse(gate.accepted(changed, self.pin))
        self.assertFalse(gate.accepted({}, self.pin))

    def test_refuses_malformed_reports(self):
        for report in (None, [], "success"):
            self.assertFalse(gate.accepted(report, self.pin))
        for section in ("verification-results", "verus", "func-details"):
            changed = copy.deepcopy(self.report)
            changed[section] = None
            self.assertFalse(gate.accepted(changed, self.pin))
        changed = copy.deepcopy(self.report)
        changed["verification-results"]["errors"] = False
        self.assertFalse(gate.accepted(changed, self.pin))

    def test_refuses_wrong_tool_or_missing_target(self):
        for field in ("commit", "version"):
            changed = copy.deepcopy(self.report)
            changed["verus"][field] = "another tool"
            self.assertFalse(gate.accepted(changed, self.pin))
        changed = copy.deepcopy(self.report)
        changed["func-details"].pop(self.pin["target_functions"][0])
        self.assertFalse(gate.accepted(changed, self.pin))

    def test_tool_file_drift_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tool = root / "tool"
            tool.write_bytes(b"test tool fixture")
            pin = {"files": {"tool": gate.digest(tool)}}
            gate.verify_tool_files(root, pin)
            tool.write_bytes(b"changed tool fixture")
            with self.assertRaisesRegex(RuntimeError, "missing or changed"):
                gate.verify_tool_files(root, pin)

    def test_mutation_anchors_remain_applicable(self):
        source = (gate.ROOT / gate.SUBJECT).read_text()
        mutants = gate.mutation_sources(source)
        self.assertEqual(len(mutants), 4)
        self.assertTrue(all(changed != source for changed, _ in mutants.values()))
        with self.assertRaisesRegex(RuntimeError, "anchors changed"):
            gate.mutation_sources("")

    def test_negative_control_requires_only_intended_semantic_diagnostics(self):
        report = copy.deepcopy(self.report)
        report['verus']['toolchain'] = self.pin['rust_toolchain']
        report['verification-results'].update(success=False, errors=1, **{'encountered-error': True})
        path, cwd = Path('subject.rs'), Path('/tmp/verus-control-fixture')
        diagnostic = 'error: postcondition not satisfied\n --> subject.rs:12:1\n'
        summary = 'error: aborting due to 1 previous error\n'
        def classify(stderr, current=report, code=1):
            return gate.semantic_refusal(subprocess.CompletedProcess([], code, '', stderr),
                current, self.pin, path, cwd, region=(10, 20),
                target_functions=self.pin['target_functions'])
        self.assertTrue(classify(diagnostic + summary))
        self.assertTrue(classify(diagnostic.replace('postcondition not satisfied',
            'invariant not satisfied before loop') + summary))
        type_invariant_diagnostic = diagnostic.replace('postcondition not satisfied',
            'constructed value may fail to meet its declared type invariant')
        self.assertTrue(classify(type_invariant_diagnostic + summary))
        self.assertFalse(classify(type_invariant_diagnostic + 'note: solver returned unknown\n' + summary))
        self.assertFalse(classify(type_invariant_diagnostic.replace('subject.rs', 'unrelated.rs') + summary))
        self.assertFalse(classify(type_invariant_diagnostic.replace('error: ', 'error[E0308]: ') + summary))
        self.assertTrue(classify(diagnostic * 2 + 'error: aborting due to 2 previous errors\n'))
        for wrong in (
            diagnostic + 'note: SMT result UNKNOWN\n' + summary,
            diagnostic + 'note: solver returned unknown\n' + summary,
            diagnostic + 'note: resource limit (rlimit) exceeded\n' + summary,
            diagnostic + 'note: timeout\n' + summary,
            diagnostic + 'note: out of memory\n' + summary,
            diagnostic + 'note: signal 9\n' + summary,
            diagnostic.replace('subject.rs', 'unrelated/subject.rs') + summary,
            diagnostic.replace(':12:1', ':9:1') + summary,
            diagnostic.replace('postcondition not satisfied', 'mismatched types') + summary,
            diagnostic.replace('error: ', 'error[E0308]: ') + summary,
            diagnostic, summary, '',
            diagnostic + 'error: aborting due to 2 previous errors\n',
        ):
            with self.subTest(stderr=wrong):
                self.assertFalse(classify(wrong))
        for code in (0, 101, 124, -9):
            self.assertFalse(classify(diagnostic + summary, code=code))
        for group, key, value in (
            ('verus', 'version', 'other'), ('verus', 'commit', 'other'),
            ('verus', 'toolchain', 'other'), ('verification-results', 'verified', True),
            ('verification-results', 'errors', True), ('verification-results', 'success', True),
            ('verification-results', 'encountered-error', False),
            ('verification-results', 'encountered-vir-error', True),
            ('verification-results', 'is-verifying-entire-crate', False),
        ):
            changed = copy.deepcopy(report)
            changed[group][key] = value
            self.assertFalse(classify(diagnostic + summary, changed))
        changed = copy.deepcopy(report)
        changed['func-details'].pop(self.pin['target_functions'][0])
        self.assertFalse(classify(diagnostic + summary, changed))

    def test_function_region_includes_contract_before_trailing_rlimit(self):
        source = """#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == value,))]
#[cfg_attr(verus_keep_ghost, verifier::rlimit(30))]
pub fn intended(value: u64) -> u64 {
    value
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures true,))]
#[cfg_attr(verus_keep_ghost, verifier::rlimit(10))]
pub fn unrelated() -> u64 { 0 }
"""
        self.assertEqual(gate.function_lines(source, 'intended'), (1, 6))
        self.assertEqual(gate.function_lines(source, 'unrelated'), (7, 10))
        actual = (gate.ROOT / 'crates/zeno-fcis-synthesis/src/finite/execution_v2/decision.rs').read_text()
        start, end = gate.function_lines(actual, 'construct')
        self.assertIn('ensures', '\n'.join(actual.splitlines()[start - 1:end]))
        self.assertNotIn('fn decide', '\n'.join(actual.splitlines()[start - 1:end]))

    def test_function_region_includes_inline_postcondition_without_next_function(self):
        source = """pub fn intended(value: u64) -> u64 {
    #[cfg(verus_keep_ghost)]
    verus_proof_expr!(ensures(|result: u64| result == value));
    value
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]
pub fn unrelated() -> u64 { 0 }
"""
        self.assertEqual(gate.function_lines(source, 'intended'), (1, 5))
        actual = (gate.ROOT / 'crates/zeno-fcis-cli/src/symbolic/program/library.rs').read_text()
        start, end = gate.function_lines(actual, 'of')
        self.assertIn('result.inputs@ == program.view().0',
                      '\n'.join(actual.splitlines()[start - 1:end]))

    def test_function_region_never_borrows_another_functions_contract(self):
        before = """#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]
pub fn previous() -> u64 { 0 }
pub fn intended() -> u64 { 0 }
"""
        after = """pub fn intended() -> u64 { 0 }
pub fn following() -> u64 {
    verus_proof_expr!(ensures(|result: u64| result == 0));
    0
}
"""
        for source in (before, after):
            with self.subTest(source=source):
                with self.assertRaisesRegex(ValueError, 'lacks an annotated contract'):
                    gate.function_lines(source, 'intended')

    def test_coverage_fault_success_allows_a_deliberate_count_change(self):
        report = copy.deepcopy(self.report)
        report['verus']['toolchain'] = self.pin['rust_toolchain']
        for count in (1, self.pin['expected_verified'], self.pin['expected_verified'] + 1):
            report['verification-results']['verified'] = count
            self.assertTrue(gate.proof_succeeded(report, self.pin))
        for field, value in (
            ('verified', 0), ('verified', True), ('errors', False), ('errors', 1),
            ('success', False), ('encountered-error', True), ('encountered-vir-error', True),
            ('is-verifying-entire-crate', False),
        ):
            changed = copy.deepcopy(report)
            changed['verification-results'][field] = value
            self.assertFalse(gate.proof_succeeded(changed, self.pin))
        for field in ('version', 'commit', 'toolchain'):
            changed = copy.deepcopy(report)
            changed['verus'][field] = 'other'
            self.assertFalse(gate.proof_succeeded(changed, self.pin))


class MutantAdmission(unittest.TestCase):
    """Exercise the actual gate, with external processes replaced by fixtures."""
    def run_gate(self, diagnostic=None, *, wrong_function=False):
        import os
        from unittest.mock import patch
        pin = json.loads((gate.ROOT / gate.PIN).read_text())
        controls = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text())
        self.assertEqual(set(gate.SEMANTIC_TARGETS), {n for n, (_, kind) in controls.items() if kind == 'proof'})
        for jobs in (1, 2):
            seen = []
            def fake(command, cwd, environment, **kwargs):
                if '--output-json' not in command:
                    return subprocess.CompletedProcess(command, 0, 'mock native/version', '')
                name = Path(cwd).name
                mutant = cwd != gate.ROOT
                proof = mutant and controls[name][1] == 'proof'
                report = {'verus': {k: pin[k] for k in ('version', 'commit')},
                    'func-details': dict.fromkeys(pin['target_functions'], {}),
                    'verification-results': {'success': not proof, 'errors': int(proof),
                        'verified': pin['expected_verified'] - int(mutant),
                        'encountered-error': proof, 'encountered-vir-error': False,
                        'is-verifying-entire-crate': True}}
                report['verus']['toolchain'] = pin['rust_toolchain']
                stderr = ''
                if mutant:
                    seen.append(name)
                    if not proof:
                        report['func-details'].pop('domain_bounds::finite_bounds::interval_width')
                if proof:
                    target = gate.SEMANTIC_TARGETS[name]
                    if wrong_function and name == 'wrong_interval_width':
                        target = 'bounded_product'
                    line, _ = gate.function_lines(controls[name][0], target)
                    stderr = (f'error: postcondition not satisfied\n --> {gate.SUBJECT}:{line}:1\n'
                              'error: aborting due to 1 previous error\n')
                    if diagnostic is not None and name == 'wrong_interval_width':
                        stderr = diagnostic
                return subprocess.CompletedProcess(command, int(proof), json.dumps(report), stderr)
            with self.subTest(jobs=jobs), patch.dict(os.environ, {'ZENO_FCIS_CONTROL_JOBS': str(jobs)}), \
                    patch.object(gate, 'prepare_tools', return_value=Path('/tmp/sol-q25/not-executed')), \
                    patch.object(gate, 'verify_tool_files'), patch.object(gate, 'snapshot', return_value={}), \
                    patch.object(gate, 'run', side_effect=fake), \
                    patch.object(gate.subprocess, 'check_output', return_value='fixture'):
                if diagnostic is not None or wrong_function:
                    with self.assertRaisesRegex(gate.ControlRefused, 'wrong_interval_width') as caught:
                        gate.check(Path('/tmp/sol-q25/not-executed'), False)
                    self.assertFalse(caught.exception.result['killed'])
                else:
                    self.assertEqual(gate.check(Path('/tmp/sol-q25/not-executed'), False)['status'], 'passed')
            self.assertCountEqual(seen, controls)

    def test_resource_limit_witness_is_never_a_kill(self):
        self.run_gate('error: resource limit exceeded\n')

    def test_smt_solver_unknown_witness_is_never_a_kill(self):
        self.run_gate('error: SMT solver returned UNKNOWN\n')

    def test_semantic_failure_in_another_function_is_refused(self):
        self.run_gate(wrong_function=True)

    def test_attributed_semantic_mutants_still_qualify(self):
        self.run_gate()


class CoverageAdmission(unittest.TestCase):
    """Reviewer matrix: 15 unqualified cases refuse; 3 intended omissions pass."""
    def run_case(self, case, jobs):
        import os
        from unittest.mock import patch
        pin = json.loads((gate.ROOT/gate.PIN).read_text())
        positive = {'verus':{k:pin[k] for k in ('version','commit','rust_toolchain')},
            'func-details':dict.fromkeys(pin['target_functions'], {}),
            'verification-results':{'success':True,'verified':3,'errors':0,
                'encountered-error':False,'encountered-vir-error':False,'is-verifying-entire-crate':True}}
        positive['verus']['toolchain'] = positive['verus'].pop('rust_toolchain')
        controls = gate.mutation_sources((gate.ROOT/gate.SUBJECT).read_text())
        seen = []
        def fake(command, cwd, environment, **kwargs):
            if '--output-json' not in command:
                return subprocess.CompletedProcess(command,0,'mock native/version','')
            report = copy.deepcopy(positive)
            name = Path(cwd).name
            stderr = ''
            if cwd != gate.ROOT:
                seen.append(name)
                if controls[name][1] == 'proof':
                    report['verification-results'].update(success=False, errors=1, verified=2,
                        **{'encountered-error':True})
                    line, _ = gate.function_lines(controls[name][0], gate.SEMANTIC_TARGETS[name])
                    stderr = (f'error: postcondition not satisfied\n --> {gate.SUBJECT}:{line}:1\n'
                              'error: aborting due to 1 previous error\n')
                else:
                    if case in ('valid_coverage','unknown_diagnostic','resource_limit_diagnostic'):
                        report['verification-results']['verified'] = 2
                        report['func-details'].pop('domain_bounds::finite_bounds::interval_width')
                    if case == 'wrong_version_no_coverage_delta':
                        report['verus']['version'] = 'unqualified-version'
                    elif case == 'incomplete_crate_no_coverage_delta':
                        report['verification-results']['is-verifying-entire-crate'] = False
                    elif case == 'reported_error_no_coverage_delta':
                        report['verification-results'].update(errors=1, **{'encountered-error':True})
                        stderr = 'error: unrelated verifier failure\n'
                    elif case == 'unknown_diagnostic':
                        stderr = 'error: SMT solver returned UNKNOWN\n'
                    elif case == 'resource_limit_diagnostic':
                        stderr = 'error: resource limit exceeded\n'
            return subprocess.CompletedProcess(command, int(not report['verification-results']['success']), json.dumps(report), stderr)
        with patch.dict(os.environ, {'ZENO_FCIS_CONTROL_JOBS':str(jobs)}), \
                patch.object(gate,'prepare_tools',return_value=Path('/tmp/sol-q25/not-executed')), \
                patch.object(gate,'verify_tool_files'), patch.object(gate,'snapshot',return_value={}), \
                patch.object(gate,'run',side_effect=fake), \
                patch.object(gate.subprocess,'check_output',return_value='fixture'):
            if case == 'valid_coverage':
                receipt = gate.check(Path('/tmp/sol-q25/not-executed'),False)
                self.assertTrue(all(row['killed'] for row in receipt['mutations']))
            else:
                with self.assertRaises(gate.ControlRefused) as caught:
                    gate.check(Path('/tmp/sol-q25/not-executed'),False)
                self.assertEqual(caught.exception.result['name'],'omit_interval_contract')
                self.assertFalse(caught.exception.result['killed'])
        self.assertCountEqual(seen, controls)

    def test_count_alone_or_another_missing_obligation_is_refused(self):
        pin = json.loads((gate.ROOT/gate.PIN).read_text())
        positive = {'verus':{k:pin[k] for k in ('version','commit')},
            'func-details':dict.fromkeys(pin['target_functions'], {}),
            'verification-results':{'success':True,'verified':3,'errors':0,
                'encountered-error':False,'encountered-vir-error':False,'is-verifying-entire-crate':True}}
        positive['verus']['toolchain'] = pin['rust_toolchain']
        omitted = 'domain_bounds::finite_bounds::interval_width'
        valid = copy.deepcopy(positive)
        valid['verification-results']['verified'] = 2
        valid['func-details'].pop(omitted)
        proc = subprocess.CompletedProcess([],0,'','')
        self.assertTrue(gate.coverage_refusal(proc,valid,pin,'omit_interval_contract',positive))
        for missing in (None, 'domain_bounds::finite_bounds::bounded_product'):
            bad = copy.deepcopy(positive)
            bad['verification-results']['verified'] = 2
            if missing is not None:bad['func-details'].pop(missing)
            self.assertFalse(gate.coverage_refusal(proc,bad,pin,'omit_interval_contract',positive))
        for count in (True,0,1,3,4):
            bad = copy.deepcopy(valid)
            bad['verification-results']['verified'] = count
            self.assertFalse(gate.coverage_refusal(proc,bad,pin,'omit_interval_contract',positive))
        extra = copy.deepcopy(valid)
        extra['func-details']['unexpected::obligation'] = {}
        self.assertFalse(gate.coverage_refusal(proc,extra,pin,'omit_interval_contract',positive))
        self.assertFalse(gate.coverage_refusal(proc,valid,pin,'unrecognized_mutant',positive))


for _jobs in (1,2,4):
    for _case in ('wrong_version_no_coverage_delta','incomplete_crate_no_coverage_delta',
                  'reported_error_no_coverage_delta','unknown_diagnostic',
                  'resource_limit_diagnostic','valid_coverage'):
        def _test(self, case=_case, jobs=_jobs):
            self.run_case(case,jobs)
        setattr(CoverageAdmission, f'test_{_case}_jobs_{_jobs}', _test)


class NativeRuntime(unittest.TestCase):
    def test_machine_boundaries_on_the_runtime_toolchain(self):
        with tempfile.TemporaryDirectory(prefix="zeno-verus-native-") as temporary:
            executable = Path(temporary) / "tests"
            command = ["rustc", "+1.97.1", "--edition=2024", "--test",
                       "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)",
                       str(gate.ROOT / gate.HARNESS), "-o", str(executable)]
            subprocess.run(command, cwd=gate.ROOT, check=True, timeout=60)
            subprocess.run([str(executable)], cwd=gate.ROOT, check=True, timeout=60)


class InstalledCacheOverride(unittest.TestCase):
    def test_override_keeps_tool_hash_checks_and_explicit_cache_selection(self):
        import os
        import hashlib
        from unittest import mock
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            pin = {"version": "synthetic", "files": {"verus": hashlib.sha256(b"pinned").hexdigest()}}
            qualified = cache / "synthetic/qualified"
            qualified.mkdir(parents=True)
            (qualified / "verus").write_bytes(b"pinned")
            with mock.patch.dict(os.environ, {"ZENO_FCIS_VERUS_CACHE": str(cache)}):
                self.assertEqual(gate.cache_directory(), cache)
                self.assertEqual(gate.prepare_tools(gate.cache_directory(), pin, False), qualified)
                (qualified / "verus").write_bytes(b"altered")
                with self.assertRaises(RuntimeError):
                    gate.prepare_tools(gate.cache_directory(), pin, False)
                with self.assertRaises(RuntimeError):
                    gate.prepare_tools(cache / "explicit-missing", pin, False)

if __name__ == "__main__":
    unittest.main()
