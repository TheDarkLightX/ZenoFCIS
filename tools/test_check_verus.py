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
