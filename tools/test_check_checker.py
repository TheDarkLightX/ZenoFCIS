"""Cheap source-routing and gate refusal tests; no proof/build is implied."""
import json
import copy
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest import mock

import check_checker as gate


class SharedCheckerGate(unittest.TestCase):
    def test_both_actual_adapters_use_the_same_source(self):
        gate.check_routes()
        shared = (gate.ROOT / gate.SUBJECT).read_text()
        self.assertEqual(shared.count('fn scan('), 1)
        self.assertEqual(shared.count('execute_v2('), 1)

    def test_routing_guard_refuses_an_added_duplicate_evaluator(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in gate.ROUTE_INPUTS:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((gate.ROOT / relative).read_bytes())
            gate.check_routes(root)
            cli = root / gate.CLI
            cli.write_text(cli.read_text() + '\nfn duplicate() { execute_v2(); }\n')
            with self.assertRaisesRegex(ValueError, 'independent evaluator'):
                gate.check_routes(root)

    def test_library_route_rejects_missing_proof_and_wrong_package_order(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in gate.ROUTE_INPUTS:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((gate.ROOT / relative).read_bytes())
            version = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['package']['version']
            for relative, before, after, reason in (
                (gate.HARNESS, 'pub mod checker_api;', '', 'outside the proof unit'),
                (Path('crates/zeno-fcis-cli/Cargo.toml'), '=' + version, '=0.0.0-invalid', 'exact published'),
                (Path('crates/zeno-fcis-cli/src/shell_v2.rs'), '//! The actual', '#[path = "sibling.rs"]\n//! The actual', 'sibling shell source'),
                (Path('release/package-set.toml'), '    "zeno-fcis-shell-sqlite",\n    "zeno-fcis-cli",', '    "zeno-fcis-cli",\n    "zeno-fcis-shell-sqlite",', 'publication order'),
            ):
                target = root / relative
                original = target.read_text()
                target.write_text(original.replace(before, after))
                with self.subTest(path=str(relative)):
                    with self.assertRaisesRegex(ValueError, reason):
                        gate.check_routes(root)
                target.write_text(original)

    def test_release_version_changes_keep_the_exact_dependency_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in gate.ROUTE_INPUTS:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((gate.ROOT / relative).read_bytes())
            workspace = root / 'Cargo.toml'
            old = tomllib.loads(workspace.read_text())['workspace']['package']['version']
            workspace.write_text(workspace.read_text().replace('version = "' + old + '"', 'version = "2.9.0-rc.1"', 1))
            with self.assertRaisesRegex(ValueError, 'exact published'):
                gate.check_routes(root)
            cli = root / 'crates/zeno-fcis-cli/Cargo.toml'
            cli.write_text(cli.read_text().replace('=' + old, '=2.9.0-rc.1'))
            gate.check_routes(root)

    def test_source_inventory_covers_shared_core_and_both_consumers(self):
        paths = set(gate.source_paths(profile=False))
        self.assertTrue({gate.HARNESS, gate.SUBJECT, gate.SPEC, gate.TESTS, gate.API, gate.CLI, gate.SHELL}.issubset(paths))
        self.assertIn(Path('crates/zeno-fcis-shell-sqlite/src/v2/upgrade.rs'), paths)
        self.assertIn(Path('Cargo.lock'), paths)
        gate.snapshot(profile=False)

    def test_receipt_fixture_changes_invalidate_the_source_snapshot(self):
        fixture = Path('crates/zeno-fcis-cli/tests/fixtures/transform-check/1/'
                       'equivalent-usage-preserved.receipt.json')
        paths = gate.source_paths(profile=False)
        self.assertIn(fixture, paths)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in paths:
                destination = root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes((gate.ROOT / relative).read_bytes())
            before = gate.snapshot(root, profile=False)
            changed = root / fixture
            changed.write_bytes(changed.read_bytes() + b'\n')
            after = gate.snapshot(root, profile=False)
            self.assertEqual(set(before), set(after))
            self.assertEqual([name for name in before if before[name] != after[name]],
                             [fixture.as_posix()])

    def test_mutation_anchors_are_unique_and_change_the_intended_source(self):
        controls = gate.mutations((gate.ROOT / gate.SUBJECT).read_text(), (gate.ROOT / gate.SPEC).read_text(), (gate.ROOT / gate.EQUALITY).read_text(), (gate.ROOT / gate.API).read_text())
        self.assertEqual(len(controls), 24)
        for path, changed, expected in controls.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
            self.assertIn(expected, ('proof', 'coverage'))
        self.assertEqual(controls['weaken_final_theorem'][2], 'coverage')
        self.assertEqual(set(controls), set(gate.SEMANTIC_TARGETS) | set(gate.COVERAGE_TARGETS))
        for name, target in gate.SEMANTIC_TARGETS.items():
            start, end = gate.target_lines(controls[name][1], target)
            self.assertLess(start, end)

    def test_order_mutant_changes_matching_domain_and_tuple_coordinates(self):
        controls = gate.mutations((gate.ROOT / gate.SUBJECT).read_text(), (gate.ROOT / gate.SPEC).read_text(), (gate.ROOT / gate.EQUALITY).read_text(), (gate.ROOT / gate.API).read_text())
        forward = controls['first_input_fastest'][1]
        self.assertIn('let index = tuple.len().min(domains.len()) - position - 1;', forward)
        for required in ('domains[index].bounds()', 'tuple[index] < max', 'tuple[index] += 1;', 'tuple[index] = min;'):
            self.assertIn(required, forward)
        for old in ('domains[position].bounds()', 'tuple[position] < max', 'tuple[position] += 1;', 'tuple[position] = min;'):
            # Other functions may read domains[position], but every tuple access
            # in the odometer must use the same forward coordinate.
            if old.startswith('tuple'):
                self.assertNotIn(old, forward)

    def test_coverage_controls_preserve_the_contract_consumed_by_the_public_wrapper(self):
        controls = gate.mutations((gate.ROOT / gate.SUBJECT).read_text(), (gate.ROOT / gate.SPEC).read_text(), (gate.ROOT / gate.EQUALITY).read_text(), (gate.ROOT / gate.API).read_text())
        private = 'ensures spec::comparison_result(original, candidate, cap, step_limit, result),'
        self.assertIn(private, controls['weaken_final_theorem'][1])
        self.assertEqual(controls['weaken_final_theorem'][0], gate.SUBJECT)
        self.assertEqual(gate.COVERAGE_TARGETS['weaken_final_theorem'],
                         ('checker::checker::compare_equal', 'ensures_sha256'))
        self.assertEqual(controls['narrow_final_domain'][0], gate.API)
        self.assertEqual(gate.COVERAGE_TARGETS['narrow_final_domain'],
                         ('checker::checker_api::compare_with_usage', 'requires_sha256'))
        self.assertIn('requires false,', controls['narrow_final_domain'][1])
        self.assertIn('ensures comparison(original, candidate, cap, step_limit, result),',
                      controls['narrow_final_domain'][1])
        self.assertIn('ensures spec::equality_result(original, candidate, cap, result) || true,',
                      controls['weaken_final_theorem'][1])
        self.assertIn('ensures comparison(original, candidate, cap, step_limit, result) || true,',
                      controls['api_weaken_projection'][1])

    def test_simulated_proof_reports_fail_closed(self):
        pin = {'version': 'pinned', 'commit': 'exact'}
        report = {'verus': pin, 'verification-results': {'success': True, 'errors': 0, 'verified': 1,
                  'encountered-error': False, 'encountered-vir-error': False, 'is-verifying-entire-crate': True}}
        self.assertTrue(gate.proof_succeeded(report, pin))
        for key, value in [('success', False), ('verified', 0), ('verified', True), ('errors', 1), ('errors', False),
                           ('encountered-error', True), ('encountered-vir-error', True), ('is-verifying-entire-crate', False)]:
            changed = {**report, 'verification-results': {**report['verification-results'], key: value}}
            self.assertFalse(gate.proof_succeeded(changed, pin))
        self.assertFalse(gate.proof_succeeded(report, {'version': 'other', 'commit': 'exact'}))
        self.assertFalse(gate.proof_succeeded([], pin))
        self.assertFalse(gate.proof_succeeded({'verification-results': []}, pin))

    def refusal_fixture(self):
        pin = {'version': 'pinned', 'commit': 'exact', 'rust_toolchain': '1.98.1-x86_64-unknown-linux-gnu'}
        profile = {'target_functions': ['checker::checker::scan']}
        report = {'verus': {**pin, 'toolchain': pin['rust_toolchain'] + ' (overridden by environment variable RUSTUP_TOOLCHAIN)'},
                  'func-details': {'checker::checker::scan': {}},
                  'verification-results': {'success': False, 'errors': 1, 'verified': 895,
                      'encountered-error': True, 'encountered-vir-error': False, 'is-verifying-entire-crate': True}}
        source = (gate.ROOT / gate.SUBJECT).read_text()
        line = gate.target_lines(source, 'scan')[0]
        diagnostic = f'error: postcondition not satisfied\n   --> verification/verus/../../{gate.SUBJECT}:{line}:1\n'
        return pin, profile, report, source, diagnostic

    def classify(self, stderr, report=None, exit_code=1):
        pin, profile, original, source, _ = self.refusal_fixture()
        proc = subprocess.CompletedProcess([], exit_code, '', stderr)
        return gate.semantic_refusal(proc, original if report is None else report, pin, profile,
                                     gate.SUBJECT, source, 'scan', Path('/tmp/checker-mutation'))[0]

    def test_only_intended_semantic_error_blocks_are_mutant_kills(self):
        _, _, report, _, diagnostic = self.refusal_fixture()
        self.assertTrue(self.classify(diagnostic + 'error: aborting due to 1 previous error\n'))
        # Verus groups failed queries, not individual emitted diagnostics.
        self.assertTrue(self.classify(diagnostic * 2 + 'error: aborting due to 2 previous errors\n'))
        rejected = (
            '', 'error: aborting due to 1 previous error\n',
            'error: resource limit exceeded\nerror: aborting due to 1 previous error\n',
            diagnostic + 'error: resource limit exceeded\nerror: aborting due to 2 previous errors\n',
            diagnostic + 'note: timed out\nerror: aborting due to 1 previous error\n',
            diagnostic.replace('postcondition not satisfied', 'internal compiler error') + 'error: aborting due to 1 previous error\n',
            diagnostic.replace('error: ', 'error[E0308]: ') + 'error: aborting due to 1 previous error\n',
            diagnostic.replace(str(gate.SUBJECT), 'unrelated/' + gate.SUBJECT.name) + 'error: aborting due to 1 previous error\n',
            diagnostic.replace('verification/verus/../../', '/outside/') + 'error: aborting due to 1 previous error\n',
            diagnostic + f'error: assertion failed\n --> {gate.SUBJECT}:1:1\nerror: aborting due to 2 previous errors\n',
            diagnostic + 'error: aborting due to 2 previous errors\n',
        )
        for stderr in rejected:
            with self.subTest(stderr=stderr):
                self.assertFalse(self.classify(stderr))
        # The retained attempt8 shape: four semantic diagnostics plus a resource
        # failure, but four grouped errors in the machine-readable report.
        mixed = copy.deepcopy(report)
        mixed['verification-results']['errors'] = 4
        self.assertFalse(self.classify(diagnostic * 4 + 'error: assert_nonlinear_by: Resource limit (rlimit) exceeded\nerror: aborting due to 5 previous errors\n', mixed))

    def test_failed_run_identity_scope_and_exit_must_be_exact(self):
        _, _, report, _, diagnostic = self.refusal_fixture()
        stderr = diagnostic + 'error: aborting due to 1 previous error\n'
        for code in (0, 101, 124, -9):
            self.assertFalse(self.classify(stderr, exit_code=code))
        changes = [('verus', 'version', 'other'), ('verus', 'commit', 'other'),
                   ('verus', 'toolchain', 'other'), ('func-details', 'checker::checker::scan', None),
                   ('verification-results', 'success', True),
                   ('verification-results', 'encountered-error', False),
                   ('verification-results', 'encountered-vir-error', True),
                   ('verification-results', 'is-verifying-entire-crate', False),
                   ('verification-results', 'verified', True),
                   ('verification-results', 'errors', True),
                   ('verification-results', 'errors', 2)]
        for group, key, value in changes:
            changed = copy.deepcopy(report)
            if value is None:
                del changed[group][key]
            else:
                changed[group][key] = value
            with self.subTest(group=group, key=key):
                self.assertFalse(self.classify(stderr, changed))

    def test_pinned_width_mutant_diagnostics_keep_exact_failure_attribution(self):
        # Retained controls08 emitted these two diagnostics for one failed
        # domain_size query. Recognize the pinned wording without accepting an
        # unrelated diagnostic, resource failure, or incomplete error summary.
        pin, _, report, source, _ = self.refusal_fixture()
        controls = gate.mutations(source, (gate.ROOT / gate.SPEC).read_text(),
                                  (gate.ROOT / gate.EQUALITY).read_text(),
                                  (gate.ROOT / gate.API).read_text())
        path, source, expected = controls['width_minus_one']
        self.assertEqual(path, gate.SUBJECT)
        self.assertEqual(expected, 'proof')
        target = 'checker::checker::domain_size'
        profile = {'target_functions': [target]}
        report['func-details'] = {target: {}}
        start, end = gate.target_lines(source, 'domain_size')
        lines = source.splitlines()
        requires_line = next(i for i in range(start, end + 1)
                             if 'requires prior > u128::MAX, width >= 1;' in lines[i - 1])
        invariant_line = next(i for i in range(start, end + 1)
                              if 'match size {' in lines[i - 1])
        requires_diagnostic = (
            f'error: requires not satisfied\n'
            f'   --> verification/verus/../../{gate.SUBJECT}:{requires_line}:49\n'
        )
        invariant_diagnostic = (
            f'error: invariant not satisfied at end of loop body\n'
            f'   --> verification/verus/../../{gate.SUBJECT}:{invariant_line}:13\n'
        )
        diagnostic = requires_diagnostic + invariant_diagnostic
        summary = 'error: aborting due to 2 previous errors\n'

        def classify(stderr):
            proc = subprocess.CompletedProcess([], 1, '', stderr)
            return gate.semantic_refusal(proc, report, pin, profile,
                                         gate.SUBJECT, source, 'domain_size',
                                         Path('/tmp/checker-mutation'))

        killed, evidence = classify(diagnostic + summary)
        self.assertTrue(killed)
        self.assertEqual(evidence['reported_error_groups'], 1)
        self.assertEqual(evidence['diagnostic_count'], 2)
        self.assertEqual(evidence['terminal_error_count'], 2)
        for stderr in (
            diagnostic.replace(f':{requires_line}:49', ':1:49') + summary,
            diagnostic.replace(f':{invariant_line}:13', ':1:13') + summary,
            requires_diagnostic.replace(str(gate.SUBJECT), 'outside/' + gate.SUBJECT.name) + invariant_diagnostic + summary,
            requires_diagnostic + invariant_diagnostic.replace(str(gate.SUBJECT), 'outside/' + gate.SUBJECT.name) + summary,
            diagnostic.replace('requires not satisfied', 'unknown verification failure') + summary,
            diagnostic.replace('loop body', 'loop body with unknown suffix') + summary,
            diagnostic.replace('error: requires', 'error[E0308]: requires') + summary,
            diagnostic + 'note: resource limit exceeded\n' + summary,
            diagnostic + 'error: aborting due to 1 previous error\n',
        ):
            with self.subTest(stderr=stderr):
                self.assertFalse(classify(stderr)[0])
        report['verification-results']['errors'] = 2
        extra = f'error: assertion failed\n   --> {gate.SUBJECT}:1:1\n'
        self.assertFalse(classify(diagnostic + extra + 'error: aborting due to 3 previous errors\n')[0])
        self.assertFalse(classify(diagnostic + 'error: resource limit exceeded\n'
                                 + 'error: aborting due to 3 previous errors\n')[0])

    def test_coverage_refusal_requires_only_the_intended_translated_delta(self):
        name = 'checker::checker::compare_with_usage'
        other = 'checker::checker::observe'
        def vir(function, ensure='true', body='0'):
            return f'(Function (Fun :path {function}) :mode Exec :typ_bounds () :params () :ret () :require () :ensure ((Bool {ensure})) :d () :body (Int {body}))'
        original = vir(name) + vir(other)
        profile = {'namespace': 'checker::', 'body_covered_functions': [name, other],
                   'functions': gate.inventory(original, 'checker::', (name, other))}
        self.assertTrue(gate.coverage_refusal(vir(name, 'false') + vir(other), profile, name, 'ensures_sha256')[0])
        for changed in ('(malformed', original, vir(name) + vir(other, 'false'),
                        vir(name, 'false') + vir(other, 'false'),
                        vir(name, 'false', '1') + vir(other)):
            self.assertFalse(gate.coverage_refusal(changed, profile, name, 'ensures_sha256')[0])
        extra = 'checker::checker::unchecked_constant'
        unchecked = vir(extra).replace(':ensure ((Bool true))', ':ensure ()')
        self.assertTrue(gate.coverage_refusal(original + unchecked, profile, extra, 'inventory')[0])
        self.assertFalse(gate.coverage_refusal(original + vir(extra), profile, extra, 'inventory')[0])
        self.assertFalse(gate.coverage_refusal(original + unchecked + vir('checker::unexpected'), profile, extra, 'inventory')[0])

    def test_local_binding_attribution_refuses_body_changes_and_capture(self):
        name = 'checker::checker::compare_equal'
        def variable(number, label='value'):
            return f'(VarIdent "{label}" (VarIdentDisambiguate RustcId {number}))'
        def vir(ids=(4, 4, 5, 5), ensure='true', parameter=1, body_value=0):
            body = ' '.join(variable(i) for i in ids)
            return (f'(Function (Fun :path {name}) :mode Exec :typ_bounds () '
                    f':params ((Param :name {variable(parameter)})) :ret () :require () '
                    f':ensure ((Bool {ensure})) :d () :body (Block {body} '
                    f'{variable(1)} (Int {body_value})))')
        original = vir()
        profile = {'namespace': 'checker::', 'body_covered_functions': [name],
                   'functions': gate.inventory(original, 'checker::', (name,))}
        changed = vir((14, 14, 15, 15), ensure='false')
        self.assertFalse(gate.coverage_refusal(changed, profile, name, 'ensures_sha256')[0])
        killed, evidence = gate.coverage_refusal(changed, profile, name, 'ensures_sha256',
                                                original_vir=original)
        self.assertTrue(killed)
        self.assertIn('binding IDs only', evidence['unchanged_structure'])
        for changed in (
            vir((14, 4, 15, 15), 'false'),   # Inconsistent references.
            vir((14, 14, 14, 14), 'false'), # Two bindings merged.
            vir((1, 1, 15, 15), 'false'),   # Local captures parameter.
            vir((14, 14, 15, 15), 'false', parameter=2),
            vir((14, 14, 15, 15), 'false', body_value=1),
            vir((14, 14, 15, 15), 'false').replace('"value"', '"other"'),
        ):
            with self.subTest(changed=changed):
                self.assertFalse(gate.coverage_refusal(changed, profile, name, 'ensures_sha256',
                                                      original_vir=original)[0])
        self.assertFalse(gate.same_noncontract_structure(vir(ensure='false'), changed,
                                                        profile, name, 'ensures_sha256'))

    def test_contract_control_allows_only_consistent_signature_and_body_binding_ids(self):
        name = 'checker::checker_api::compare_with_usage'
        def variable(number, label):
            return f'(VarIdent "{label}" (VarIdentDisambiguate RustcId {number}))'
        def vir(parameter=1, result=2, local=3, require='()', body_parameter=None):
            ref = parameter if body_parameter is None else body_parameter
            return (f'(Function (Fun :path {name}) :mode Exec :typ_bounds () '
                    f':params ((Param :name {variable(parameter, "value")})) '
                    f':ret (Param :name {variable(result, "result")}) :require {require} '
                    f':ensure ((Eq {variable(parameter, "value")} {variable(result, "result")})) '
                    f':d () :body (Block {variable(local, "value")} {variable(local, "value")} '
                    f'{variable(ref, "value")} {variable(result, "result")} (Int 0)))')
        original = vir()
        profile = {'namespace': 'checker::', 'body_covered_functions': [name],
                   'functions': gate.inventory(original, 'checker::', (name,))}
        changed = vir(11, 12, 13, '((Bool false))')
        self.assertTrue(gate.coverage_refusal(changed, profile, name, 'requires_sha256',
                                             original_vir=original)[0])
        # The raw production guard must continue to reject the same specimen.
        with self.assertRaises(ValueError):
            gate.require_coverage(changed, profile)
        for invalid in (
            vir(11, 12, 13, '((Bool false))', body_parameter=1),
            vir(11, 12, 11, '((Bool false))'),
            changed.replace('(Int 0)', '(Int 1)'),
            changed.replace(':typ_bounds ()', ':typ_bounds ((Bool true))'),
            changed.replace(':d ()', ':d ((Bool true))'),
            changed.replace(':ensure ((Eq', ':ensure ((Ne'),
        ):
            with self.subTest(invalid=invalid):
                self.assertFalse(gate.coverage_refusal(invalid, profile, name, 'requires_sha256',
                                                      original_vir=original)[0])

    def test_missing_reviewed_profile_is_not_positive_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary) / 'evidence'
            with mock.patch.object(gate, 'snapshot', side_effect=FileNotFoundError('unreviewed checker profile')):
                with self.assertRaises(FileNotFoundError):
                    gate.check(Path(temporary), out, True, False)
            self.assertFalse((out / 'receipt.json').exists())


if __name__ == '__main__':
    unittest.main()
