#!/usr/bin/env python3
"""Build realistic runtime mutations and require the existing tests to reject them.

Specimens are isolated copies. Source and diagnostic evidence are retained next
 to the JSON receipt. Build errors and unrelated failures never count as kills.
"""
from __future__ import annotations
import argparse
import difflib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import re

import check_composed_domain as gate
import composition_domain_reference as reference
import composition_domain_soundness as soundness
from cargo_source_scope import require_build_space

NATIVE = {
    'own_successor_only': 'the_first_committed_failure_commits_its_prefix_and_ends_the_step',
    'read_only_written': 'a_read_only_component_that_changes_its_state_is_refused',
    'call_after_reject': 'resolver_never_calls_after_any_terminating_kind',
}
PRE_VETO = 'outside_cut_after_veto'
PRE_VETO_TEST = 'checked_composition::obligations_tests::outside_failure_is_checked_before_a_later_member_veto'
OBLIGATIONS = Path('crates/zeno-fcis-contract/src/checked_composition/obligations.rs')
OBLIGATION_TESTS = OBLIGATIONS.with_name('obligations_tests.rs')
MINIMUM_BUILD_SPACE = 6 * 1024**3


def run_native(command, specimen, logs, label, environment):
    if command[0] == 'cargo':
        target = Path(environment['CARGO_TARGET_DIR'])
        require_build_space(specimen, target, minimum=MINIMUM_BUILD_SPACE)
        (logs / (label + '.disk.json')).write_text(json.dumps({
            'work': str(specimen), 'target': str(target),
            'minimum_free_bytes': MINIMUM_BUILD_SPACE}) + '\n')
    (logs / (label + '.command.json')).write_text(json.dumps(command) + '\n')
    process = subprocess.run(command, cwd=specimen, env=environment,
        capture_output=True, text=True, timeout=600)
    (logs / (label + '.stdout')).write_text(process.stdout)
    (logs / (label + '.stderr')).write_text(process.stderr)
    return process


def pre_veto_mutation(source):
    start = '                if !cutters[position].is_empty() && !holds(&after) {'
    evaluation = '                match local(composition, tables, *member, states, request) {'
    end = '            }\n            if !vetoed && !holds(&after) {'
    if any(source.count(anchor) != 1 for anchor in (start, evaluation, end)):
        raise RuntimeError('pre-veto mutation anchor changed')
    left, right = source.index(start), source.index(evaluation)
    if not left < right < source.index(end):
        raise RuntimeError('pre-veto mutation order changed')
    check = source[left:right]
    return source.replace(check, '', 1).replace(end, check + end, 1)


def one_test_passed(process, target):
    return (process.returncode == 0 and 'running 1 test' in process.stdout
        and f'test {target} ... ok' in process.stdout
        and 'test result: ok. 1 passed; 0 failed;' in process.stdout)


def pre_veto_killed(process, assertion_line):
    return (process.returncode == 101 and 'running 1 test' in process.stdout
        and f'test {PRE_VETO_TEST} ... FAILED' in process.stdout
        and 'test result: FAILED. 0 passed; 1 failed;' in process.stdout
        and process.stderr.count('panicked at ') == 1
        and f'{OBLIGATION_TESTS}:{assertion_line}:' in process.stderr
        and 'assertion `left == right` failed' in process.stderr
        and re.search(r'left: Proved\s+right: Violated', process.stderr) is not None)


def bound_sources(root):
    paths = set(gate.source_paths()) | {
        Path('tools/cargo_source_scope.py'), Path('tools/test_effect_control_space.py'),
        Path('tools/composition_domain_reference.py'), Path('tools/composition_domain_soundness.py')}
    return {str(path): gate.verifier.digest(root / path) for path in sorted(paths)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--only', choices=(*NATIVE, 'nonparticipant_commits', PRE_VETO))
    parser.add_argument('--work-dir', type=Path, help='directory for isolated specimens and logs')
    args = parser.parse_args(); args.out.parent.mkdir(parents=True, exist_ok=True)
    work = args.work_dir or args.out.parent
    work.mkdir(parents=True, exist_ok=True)
    logs = Path(tempfile.mkdtemp(prefix='composed-native-', dir=work)).resolve()
    source_sha256 = bound_sources(gate.ROOT)
    specimen = logs / 'source'
    shutil.copytree(gate.ROOT, specimen, ignore=shutil.ignore_patterns('.git', 'target', '__pycache__'))
    # Never let a mutant overwrite another checkout's normal binary or cached
    # diagnostics. Even a shared target supplied by the caller gets its own
    # invocation-specific subtree for these mutation specimens.
    target_root = Path(os.environ.get('CARGO_TARGET_DIR', logs / 'target')).resolve()
    environment = dict(os.environ, CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
        CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2',
        CARGO_TARGET_DIR=str(target_root / 'composed-domain-controls' / logs.name))
    command = ['cargo', '+1.97.1', 'test', '-p', 'zeno-fcis-contract', '--locked', '--offline', '-j', '2', '--lib']
    results = []

    def run(label, args):
        return run_native(args, specimen, logs, label, environment)

    try:
        if bound_sources(specimen) != source_sha256:
            raise RuntimeError('isolated source copy changed')
        sources = {p: (gate.ROOT / p).read_text() for p in gate.PROOFS}
        mutations = gate.control_sources(sources)
        for name, target in NATIVE.items():
            if args.only and args.only != name:
                continue
            path, changed, _ = mutations[name]
            (specimen / path).write_text(changed)
            try:
                built = run(name + '-build', [*command, target, '--no-run'])
                if built.returncode != 0:
                    raise RuntimeError(f'{name} did not build')
                tested = run(name + '-test', [*command, target, '--', '--exact', '--nocapture'])
                # Exact fully-qualified matching is supplied below if a short
                # filter selected no tests. A zero-test run is never a kill.
                if 'running 0 tests' in tested.stdout:
                    tested = run(name + '-test', [*command, target, '--', '--nocapture'])
                if (tested.returncode != 101 or 'test result: FAILED' not in tested.stdout
                        or '1 failed' not in tested.stdout or target not in tested.stdout
                        or ('callback ran after the terminal component' if name == 'call_after_reject' else 'assertion') not in tested.stderr):
                    raise RuntimeError(f'{name} was not caught by its named assertion')
                row = {'mutation': name, 'test': target, 'build_exit': built.returncode,
                    'test_exit': tested.returncode, 'killed': True}
                if name == 'own_successor_only':
                    built_cli = run(name + '-cli-build', ['cargo', '+1.97.1', 'build', '-p',
                        'zeno-fcis-cli', '--locked', '--offline', '-j', '2'])
                    if built_cli.returncode != 0:
                        raise RuntimeError('mutated runtime CLI did not build')
                    target_dir = Path(environment.get('CARGO_TARGET_DIR', specimen / 'target'))
                    cli = logs / 'mutated-cli'; shutil.copy2(target_dir / 'debug/zeno-fcis', cli)
                    manifest = specimen / soundness.FIXTURES.relative_to(gate.ROOT) / 'documented/lockstep-failure.json'
                    composition = reference.Composition(manifest)
                    oracle = composition.oracle(65536)
                    code, table = soundness.cli_json(str(cli), manifest, '--joint-table', 65536)
                    if not oracle['complete'] or code != 0 or table.get('status') != 'explored':
                        raise RuntimeError('mutated runtime reference comparison incomplete')
                    differences = soundness.compare_rows(soundness.canonical_rows_rust(table),
                        soundness.canonical_rows_reference(composition, oracle['rows']), False)
                    if not differences:
                        raise RuntimeError('Python reference missed the actual mutated runtime')
                    (logs / 'own-successor-reference.json').write_text(json.dumps(differences, indent=2) + '\n')
                    row['python_reference_differences'] = len(differences)
                results.append(row)
            finally:
                (specimen / path).write_text(sources[path])
        if args.only in (None, 'nonparticipant_commits'):
            path = Path('crates/zeno-fcis-contract/src/checked_composition/joint.rs')
            source = (specimen / path).read_text()
            anchor = 'if !own\n        .participates'
            if source.count(anchor) != 1: raise RuntimeError('participation anchor changed')
            (specimen / path).write_text(source.replace(anchor, 'if false && !own\n        .participates'))
            target = 'a_non_participating_component_is_skipped'
            try:
                built = run('nonparticipant-build', [*command, target, '--no-run'])
                tested = run('nonparticipant-test', [*command, target, '--', '--nocapture'])
                if built.returncode != 0 or tested.returncode != 101 or '1 failed' not in tested.stdout or 'assertion' not in tested.stderr:
                    raise RuntimeError('non-participant mutation did not fail its named assertion')
                results.append({'mutation': 'nonparticipant_commits', 'test': target,
                    'build_exit': built.returncode, 'test_exit': tested.returncode, 'killed': True})
            finally: (specimen / path).write_text(source)
        # The same targets must pass again after every specimen is restored.
        restored_targets = {name: target for name, target in NATIVE.items()
                            if args.only in (None, name)}
        if args.only in (None, 'nonparticipant_commits'):
            restored_targets['nonparticipant'] = 'a_non_participating_component_is_skipped'
        for name, target in restored_targets.items():
            restored = run(name + '-restored', [*command, target, '--', '--nocapture'])
            if restored.returncode != 0 or '1 passed' not in restored.stdout:
                raise RuntimeError(f'{target} did not pass after restoration')
        if args.only in (None, PRE_VETO):
            source = (specimen / OBLIGATIONS).read_text()
            changed = pre_veto_mutation(source)
            test_source = (specimen / OBLIGATION_TESTS).read_text()
            test_start = test_source.index('fn ' + PRE_VETO_TEST.split('::')[-1] + '(')
            assertion = 'assert_eq!(result.verdict(), Verdict::Violated, "{}", result.json());'
            assertion_line = test_source[:test_source.index(assertion, test_start)].count('\n') + 1
            test_command = [*command, PRE_VETO_TEST, '--', '--exact', '--nocapture']
            baseline = run(PRE_VETO + '-baseline', test_command)
            if not one_test_passed(baseline, PRE_VETO_TEST):
                raise RuntimeError('pre-veto baseline did not pass exactly one named test')
            patch = ''.join(difflib.unified_diff(source.splitlines(keepends=True),
                changed.splitlines(keepends=True), fromfile=str(OBLIGATIONS), tofile=str(OBLIGATIONS)))
            (logs / (PRE_VETO + '.patch')).write_text(patch)
            (specimen / OBLIGATIONS).write_text(changed)
            mutant_sha256 = gate.verifier.digest(specimen / OBLIGATIONS)
            try:
                built = run(PRE_VETO + '-build', [*command, PRE_VETO_TEST, '--no-run'])
                if built.returncode != 0:
                    raise RuntimeError('pre-veto mutant did not build')
                tested = run(PRE_VETO + '-test', test_command)
                if not pre_veto_killed(tested, assertion_line):
                    raise RuntimeError('pre-veto mutant missed its intended assertion')
            finally:
                (specimen / OBLIGATIONS).write_text(source)
            restored = run(PRE_VETO + '-restored', test_command)
            if not one_test_passed(restored, PRE_VETO_TEST):
                raise RuntimeError('pre-veto restored source did not pass exactly one named test')
            results.append({'mutation': PRE_VETO, 'test': PRE_VETO_TEST,
                'normal_exit': baseline.returncode, 'build_exit': built.returncode,
                'test_exit': tested.returncode, 'restored_exit': restored.returncode,
                'killed': True, 'assertion_line': assertion_line,
                'mutant_source_sha256': mutant_sha256,
                'patch_sha256': gate.verifier.digest(logs / (PRE_VETO + '.patch'))})
        if bound_sources(specimen) != source_sha256 or bound_sources(gate.ROOT) != source_sha256:
            raise RuntimeError('bound sources changed during qualification')
        receipt = {'schema': 'zeno-fcis/composed-domain-runtime-controls/1', 'status': 'passed',
            'controls': results, 'logs': str(logs),
            'runtime_source_sha256': {str(p): gate.verifier.digest(gate.ROOT / p)
                for p in (gate.SUBJECT, Path('crates/zeno-fcis-contract/src/checked_composition/joint.rs'))}}
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        receipt = {'schema': 'zeno-fcis/composed-domain-runtime-controls/1', 'status': 'failed',
            'error': str(error), 'controls': results, 'logs': str(logs)}
    receipt.update(source_sha256=source_sha256, source_root=str(gate.ROOT),
        specimen=str(specimen), selected_controls=[args.only] if args.only else [*NATIVE, 'nonparticipant_commits', PRE_VETO])
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
    print(json.dumps(receipt))
    return int(receipt['status'] != 'passed')

if __name__ == '__main__':
    raise SystemExit(main())
