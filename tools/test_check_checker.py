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
            diagnostic + 'note: solver returned unknown\nerror: aborting due to 1 previous error\n',
            diagnostic + 'note: SMT result UNKNOWN\nerror: aborting due to 1 previous error\n',
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

    def test_type_invariant_failure_requires_the_same_precise_semantic_scope(self):
        _, _, _, source, diagnostic = self.refusal_fixture()
        diagnostic = diagnostic.replace('postcondition not satisfied',
            'constructed value may fail to meet its declared type invariant')
        terminal = 'error: aborting due to 1 previous error\n'
        self.assertTrue(self.classify(diagnostic + terminal))
        for changed in (
            diagnostic.replace(str(gate.SUBJECT), 'unrelated/' + gate.SUBJECT.name),
            diagnostic.replace(f':{gate.target_lines(source, "scan")[0]}:1',
                f':{gate.target_lines(source, "scan")[1] + 1}:1'),
            diagnostic.replace('error: ', 'error[E0308]: '),
            diagnostic + 'note: solver returned unknown\n',
            diagnostic + 'note: Resource limit (rlimit) exceeded\n',
        ):
            self.assertFalse(self.classify(changed + terminal), changed)
        for code in (0, 101, 124, -9):
            self.assertFalse(self.classify(diagnostic + terminal, exit_code=code))

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

    def test_pinned_unrank_division_diagnostics_keep_exact_failure_attribution(self):
        # Retained R8_wrong_unrank_width emitted these two diagnostics for one
        # failed tuple_at query. Derive their locations from the actual mutant.
        path, source, expected = gate.g10.mutations(gate.ROOT)['R8_wrong_unrank_width']
        self.assertEqual(path, gate.RANGES)
        self.assertEqual(expected, 'proof')
        pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        report = {
            'verus': {**pin, 'toolchain': pin['rust_toolchain']
                      + ' (overridden by environment variable RUSTUP_TOOLCHAIN)'},
            'func-details': {name: {} for name in profile['target_functions']},
            'verification-results': {'success': False, 'errors': 1, 'verified': 1281,
                'encountered-error': True, 'encountered-vir-error': False,
                'is-verifying-entire-crate': True},
        }
        start, end = gate.target_lines(source, 'tuple_at')
        lines = source.splitlines()
        division_line = next(i for i in range(start, end + 1)
                             if 'let digit = remaining % width;' in lines[i - 1])
        requires_line = next(i for i in range(start, end + 1)
                             if 'requires remaining < p * width, remaining == next * width + digit,' in lines[i - 1])
        division = (
            'error: possible division by zero\n'
            f'   --> verification/verus/../../{path}:{division_line}:21\n'
            '    |\n'
            f'{division_line} |         let digit = remaining % width;\n'
            '    |                     ^^^^^^^^^^^^^^^^^\n\n'
        )
        requires = (
            'error: requires not satisfied\n'
            f'   --> verification/verus/../../{path}:{requires_line}:26\n'
            '    |\n'
            f'{requires_line} |                 requires remaining < p * width, remaining == next * width + digit,\n'
            '    |                          ^^^^^^^^^^^^^^^^^^^^^\n\n'
        )
        diagnostic = division + requires
        summary = 'error: aborting due to 2 previous errors\n'

        def classify(stderr, record=report):
            proc = subprocess.CompletedProcess([], 1, '', stderr)
            return gate.semantic_refusal(proc, record, pin, profile, path, source,
                                         'tuple_at', Path('/tmp/checker-mutation'))

        killed, evidence = classify(diagnostic + summary)
        self.assertTrue(killed)
        self.assertEqual(evidence['reported_error_groups'], 1)
        self.assertEqual(evidence['diagnostic_count'], 2)
        self.assertEqual(evidence['terminal_error_count'], 2)
        self.assertEqual(evidence['diagnostics'], [
            {'kind': 'possible division by zero', 'line': division_line, 'column': 21},
            {'kind': 'requires not satisfied', 'line': requires_line, 'column': 26},
        ])
        unrelated_line = gate.target_lines(source, 'same_program')[0]
        for stderr in (
            division.replace(str(path), 'unrelated/' + path.name) + requires + summary,
            division + requires.replace(str(path), 'unrelated/' + path.name) + summary,
            division.replace(f':{division_line}:21', f':{unrelated_line}:21') + requires + summary,
            division + requires.replace(f':{requires_line}:26', f':{unrelated_line}:26') + summary,
            diagnostic.replace('error: possible', 'error[E0308]: possible') + summary,
            diagnostic.replace('division by zero', 'division by zero with extra text') + summary,
            diagnostic + 'error: Resource limit (rlimit) exceeded\n'
                + 'error: aborting due to 3 previous errors\n',
            diagnostic + 'note: solver returned UNKNOWN\n' + summary,
            diagnostic,
            diagnostic + 'error: aborting due to 1 previous error\n',
            diagnostic + summary + summary,
        ):
            with self.subTest(stderr=stderr):
                self.assertFalse(classify(stderr)[0])
        missing = copy.deepcopy(report)
        del missing['func-details'][profile['target_functions'][0]]
        self.assertFalse(classify(diagnostic + summary, missing)[0])

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


    def test_g10_controls_cover_every_requested_fault_and_unique_anchor(self):
        controls = gate.g10.mutations(gate.ROOT)
        self.assertEqual(len(controls), 24)
        self.assertEqual(set(controls), set(gate.g10.SEMANTIC_TARGETS)
                         | set(gate.g10.COVERAGE_TARGETS)
                         | set(gate.g10.NATIVE_TARGETS) | {'route_handbuilt_completion'})
        for name, (path, changed, kind) in controls.items():
            with self.subTest(control=name):
                self.assertNotEqual(changed, (gate.ROOT / path).read_text())
                if kind == 'proof':
                    start, end = gate.target_lines(changed, gate.g10.SEMANTIC_TARGETS[name])
                    self.assertLess(start, end)
        self.assertIn('requires false', controls['range_narrow_domain'][1])
        self.assertIn('result) || true,', controls['range_weaken_projection'][1])

    def test_handbuilt_range_completion_and_extra_constructors_are_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in gate.ROUTE_INPUTS:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((gate.ROOT / relative).read_bytes())
            original = (root / gate.CLI).read_text()
            (root / gate.CLI).write_text(gate.g10.mutations(gate.ROOT)['route_handbuilt_completion'][1])
            with self.assertRaisesRegex(ValueError, 'PreparedCheck::finish bypasses'):
                gate.check_routes(root)
            (root / gate.CLI).write_text(original + '\nfn forged() { Equivalence { } }\n')
            with self.assertRaisesRegex(ValueError, 'unexpected Equivalence construction'):
                gate.check_routes(root)

    def test_runtime_scheduler_and_controls_are_source_bound(self):
        paths = set(gate.source_paths(profile=False))
        for name in ('transform_plan.rs', 'transform_parallel.rs', 'transform_parallel_tests.rs',
                     'transform_command.rs', 'checker_process.rs', 'loop_command_intake_tests.rs'):
            self.assertIn(Path('crates/zeno-fcis-cli/src') / name, paths)
        for name in ('checker_controls.py', 'checker_native_controls.py', 'compare_checker_profiles.py'):
            self.assertIn(Path('tools') / name, paths)

    def test_candidate_controls_cannot_be_partial_or_production_acceptance(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary) / 'receipt'
            for positive, refresh in ((True, True), (False, False), (True, False)):
                with self.assertRaisesRegex(ValueError, 'candidate controls require'):
                    gate.check(Path(temporary), out, positive, refresh, True)
            receipt = {'status': 'candidate-controls-passed-review-required', 'review_accepted': False}
            with mock.patch('sys.argv', ['check_checker.py', '--out', str(out),
                                         '--refresh-coverage', '--candidate-controls']):
                with mock.patch.object(gate, 'check', return_value=receipt):
                    self.assertEqual(gate.main(), 1)
            self.assertFalse(json.loads((out / 'receipt.json').read_text())['review_accepted'])

    def test_native_mutation_requires_exact_assertion_failure_after_running_test(self):
        from checker_native_controls import native_result
        name = 'tests::expected'
        text = (f"test {name} ... FAILED\nthread '{name}' panicked at file.rs:1:1:\n"
                'assertion `left == right` failed\n'
                'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3 filtered out;\n')
        run = subprocess.CompletedProcess([], 101, text, 'error: test failed, to rerun pass --bin x')
        self.assertTrue(native_result(run, name, failure=True))
        coverage = subprocess.CompletedProcess([], 101,
            text.replace('assertion `left == right` failed', 'Coverage'), '')
        self.assertFalse(native_result(coverage, name, failure=True))
        self.assertTrue(native_result(coverage, name, failure=True, expected_panics=('Coverage',)))
        for wrong in (
            subprocess.CompletedProcess([], 101, '', 'error[E0308]: mismatched types'),
            subprocess.CompletedProcess([], 101, text.replace(name, 'tests::other'), ''),
            subprocess.CompletedProcess([], -9, text, ''),
            subprocess.CompletedProcess([], 101, text.replace('assertion `left == right` failed', 'unrelated panic'), ''),
            subprocess.CompletedProcess([], 101, text, 'error: could not compile'),
        ):
            self.assertFalse(native_result(wrong, name, failure=True))

    def test_worker_failure_custom_assertion_is_control_specific(self):
        from checker_native_controls import EXPECTED_PANICS, native_result
        name = 'transform_parallel::tests::a_failed_worker_is_inconclusive_with_incomplete_accounting'
        text = (f"test {name} ... FAILED\n"
            "thread 'finite-range-0' (2) panicked at worker.rs:1:1:\nplanted worker failure\n\n"
            f"thread '{name}' (1) panicked at test.rs:2:1:\nslot 0\n\n"
            "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 393 filtered out;\n")
        run = subprocess.CompletedProcess([], 101, text, '')
        expected = EXPECTED_PANICS['P5_ignore_worker_failure']
        self.assertTrue(native_result(run, name, failure=True, expected_panics=expected))
        self.assertFalse(native_result(run, name, failure=True))
        self.assertFalse(native_result(run, name, failure=True,
            expected_panics=EXPECTED_PANICS['P3_reverse_receivers']))
        for changed in (text.replace('slot 0', 'slot 1'), text.replace('slot 0', 'unrelated panic'),
                text.replace(f"thread '{name}'", "thread 'other_test'"),
                text.replace('1 failed;', '2 failed;')):
            self.assertFalse(native_result(subprocess.CompletedProcess([], 101, changed, ''),
                name, failure=True, expected_panics=expected))
        for code, stderr in ((0, ''), (-9, ''), (124, ''), (101, 'error: could not compile'),
                             (101, 'out of memory')):
            self.assertFalse(native_result(subprocess.CompletedProcess([], code, text, stderr),
                name, failure=True, expected_panics=expected))

    def test_pinned_native_thread_id_preserves_exact_failure_attribution(self):
        from checker_native_controls import native_result
        # Retained P1_floor_partition output from the pinned native compiler.
        # The planted floor division yields five chunks where six are required.
        name = 'transform_plan::tests::exact_reservation_one_short_and_total_partition'
        header = f"thread '{name}' (540275) panicked at "
        text = (
            f"\nrunning 1 test\ntest {name} ... FAILED\n\nfailures:\n\n"
            f"---- {name} stdout ----\n\n"
            + header + "crates/zeno-fcis-cli/src/transform_plan.rs:215:9:\n"
            "assertion `left == right` failed\n  left: 5\n right: 6\n"
            "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n"
            f"\n\nfailures:\n    {name}\n\n"
            "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; "
            "393 filtered out; finished in 0.00s\n\n"
        )
        stderr = 'error: test failed, to rerun pass `-p zeno-fcis-cli --bin zeno-fcis`\n'
        run = subprocess.CompletedProcess([], 101, text, stderr)
        self.assertTrue(native_result(run, name, failure=True))
        self.assertTrue(native_result(subprocess.CompletedProcess([], 101,
            text.replace(' (540275)', ''), stderr), name, failure=True))
        coverage = subprocess.CompletedProcess([], 101,
            text.replace('assertion `left == right` failed', 'Coverage'), stderr)
        self.assertFalse(native_result(coverage, name, failure=True))
        self.assertTrue(native_result(coverage, name, failure=True, expected_panics=('Coverage',)))
        for replacement in (
            "thread 'tests::other' (540275) panicked at ",
            f"thread '{name}' (540275 extra) panicked at ",
            f"thread '{name}' (worker) panicked at ",
            f"thread '{name}' () panicked at ",
            f"thread '{name}' (-1) panicked at ",
            f"thread '{name}' (５４０２７５) panicked at ",
            f"thread '{name}' (540275) (7) panicked at ",
        ):
            with self.subTest(header=replacement):
                changed = subprocess.CompletedProcess([], 101, text.replace(header, replacement), stderr)
                self.assertFalse(native_result(changed, name, failure=True))
        for wrong in (
            subprocess.CompletedProcess([], 101, text.replace(f'test {name} ... FAILED',
                'test tests::other ... FAILED'), stderr),
            subprocess.CompletedProcess([], 101, text.replace('assertion `left == right` failed',
                'unrelated panic'), stderr),
            subprocess.CompletedProcess([], 101, text.replace('0 passed; 1 failed',
                '0 passed; 2 failed') + "thread 'tests::other' (42) panicked at other.rs:1:1:\n"
                'assertion failed\n', stderr),
            subprocess.CompletedProcess([], 101, text.replace('0 passed; 1 failed',
                '1 passed; 0 failed'), stderr),
            subprocess.CompletedProcess([], 101, text, 'error[E0308]: mismatched types\n'),
            subprocess.CompletedProcess([], 101, text, 'error: could not compile\n'),
            subprocess.CompletedProcess([], 101, text, 'resource limit exceeded\n'),
            subprocess.CompletedProcess([], 101, text, 'out of memory\n'),
            subprocess.CompletedProcess([], 101, text, 'signal: 9\n'),
            subprocess.CompletedProcess([], 124, text, stderr),
            subprocess.CompletedProcess([], -9, text, stderr),
            subprocess.CompletedProcess([], 0, text, stderr),
        ):
            with self.subTest(run=wrong):
                self.assertFalse(native_result(wrong, name, failure=True))

    def test_profile_explanation_preserves_every_non_id_token(self):
        from compare_checker_profiles import compare
        name = 'checker::subject'
        def vir(read_id=10, value=1):
            return (f'(Function (Fun :path {name}) :mode Exec :typ_bounds () '
                    ':params () :ret () :require () :ensure ((Bool true)) :d () '
                    f':body (Block (UnfinalizedReadKind :preliminary_kind (ReadKind Copy) :id {read_id}) (Int {value})))')
        def profile(source):
            return {'namespace': 'checker::', 'body_covered_functions': [name],
                    'functions': gate.inventory(source, 'checker::', (name,))}
        base, candidate = profile(vir()), profile(vir(20))
        result = compare(base, candidate, vir(20), -10)
        self.assertEqual(result['status'], 'explained')
        self.assertFalse(result['review_accepted'])
        self.assertEqual(len(result['id_only_changes']), 1)
        self.assertEqual(compare(base, candidate, vir(20), -9)['status'], 'unexplained')
        altered = vir(20, 2)
        self.assertEqual(compare(base, profile(altered), altered, -10)['status'], 'unexplained')
        with self.assertRaises(ValueError):
            compare(base, candidate, altered, -10)

    def test_missing_reviewed_profile_is_not_positive_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary) / 'evidence'
            with mock.patch.object(gate, 'snapshot', side_effect=FileNotFoundError('unreviewed checker profile')):
                with self.assertRaises(FileNotFoundError):
                    gate.check(Path(temporary), out, True, False)
            self.assertFalse((out / 'receipt.json').exists())


class LoggedDeadlines(unittest.TestCase):
    def test_default_deadline_remains_bounded_and_recorded(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = subprocess.CompletedProcess(['tool'], 0, '{}', '')
            with mock.patch.object(gate.verifier, 'run', return_value=result) as run:
                gate.run_logged(['tool'], root, {}, root / 'run')
            self.assertEqual(run.call_args.kwargs['timeout'], 600)
            self.assertEqual(json.loads((root / 'run/command.json').read_text())['timeout_seconds'], 600)

    def test_extended_deadline_preserves_actual_timeout_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            error = subprocess.TimeoutExpired(['tool'], 3600, output=b'partial', stderr=b'deadline')
            with mock.patch.object(gate.verifier, 'run', side_effect=error) as run:
                with self.assertRaises(subprocess.TimeoutExpired):
                    gate.run_logged(['tool'], root, {}, root / 'run', timeout=3600)
            self.assertEqual(run.call_args.kwargs['timeout'], 3600)
            self.assertEqual(json.loads((root / 'run/exit.json').read_text()),
                             {'status': 'timeout', 'timeout_seconds': 3600})
            self.assertEqual((root / 'run/stdout.json').read_text(), 'partial')
            self.assertEqual((root / 'run/stderr.log').read_text(), 'deadline')

    def test_only_default_upgrade_suite_gets_extended_native_deadline(self):
        calls = []
        def logged(command, cwd, environment, output, *, timeout=600):
            calls.append((output.name, command, timeout))
            text = 'release: 1.97.1\n' if output.name == 'compiler' else ''
            return subprocess.CompletedProcess(command, 0, text, ''), {}
        with tempfile.TemporaryDirectory() as temporary:
            with mock.patch.object(gate, 'native_dependency_args', return_value=[]), \
                 mock.patch.object(gate, 'run_logged', side_effect=logged), \
                 mock.patch.object(gate.verifier, 'digest', return_value='0' * 64):
                gate.native(Path(temporary), {'runtime_rust': '1.97.1'}, {})
        self.assertEqual({name: timeout for name, _, timeout in calls},
                         {'compiler': 600, 'native-compile': 600, 'direct_source': 600,
                          'cli_f3': 600, 'cli_process': 600, 'shell_core': 600, 'shell_upgrade': 3600})
        upgrade = next(command for name, command, _ in calls if name == 'shell_upgrade')
        self.assertEqual(upgrade, ['cargo', '+1.97.1', 'test', '--locked', '--offline', '-p',
                                   'zeno-fcis-shell-sqlite', '--test', 'upgrade'])


if __name__ == '__main__':
    unittest.main()
