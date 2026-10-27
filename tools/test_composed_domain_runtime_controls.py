#!/usr/bin/env python3
"""Build realistic runtime mutations and require the existing tests to reject them.

Specimens are isolated copies. Source and diagnostic evidence are retained next
 to the JSON receipt. Build errors and unrelated failures never count as kills.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

import check_composed_domain as gate
import composition_domain_reference as reference
import composition_domain_soundness as soundness

NATIVE = {
    'own_successor_only': 'the_first_committed_failure_commits_its_prefix_and_ends_the_step',
    'read_only_written': 'a_read_only_component_that_changes_its_state_is_refused',
    'call_after_reject': 'resolver_never_calls_after_any_terminating_kind',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args(); args.out.parent.mkdir(parents=True, exist_ok=True)
    logs = Path(tempfile.mkdtemp(prefix='composed-native-', dir=args.out.parent)).resolve()
    specimen = logs / 'source'
    shutil.copytree(gate.ROOT, specimen, ignore=shutil.ignore_patterns('.git', 'target', '__pycache__'))
    # Never let a mutant overwrite another checkout's normal binary or cached
    # diagnostics. Even a shared target supplied by the caller gets its own
    # invocation-specific subtree for these mutation specimens.
    target_root = Path(os.environ.get('CARGO_TARGET_DIR', logs / 'target')).resolve()
    environment = dict(os.environ, CARGO_PROFILE_DEV_DEBUG='0',
        CARGO_TARGET_DIR=str(target_root / 'composed-domain-controls' / logs.name))
    command = ['cargo', '+1.97.1', 'test', '-p', 'zeno-fcis-contract', '--locked', '--offline', '-j', '2', '--lib']
    results = []

    def run(label, args):
        if args[0] == 'cargo':
            disk = subprocess.run(['df', '-B1G', '/'], capture_output=True, text=True, check=True)
            (logs / (label + '.disk')).write_text(disk.stdout)
            available = shutil.disk_usage('/').free
            if available < 6 * 1024**3:
                raise RuntimeError('root disk below 6 GiB; build stopped')
        (logs / (label + '.command.json')).write_text(json.dumps(args) + '\n')
        process = subprocess.run(args, cwd=specimen, env=environment,
            capture_output=True, text=True, timeout=600)
        (logs / (label + '.stdout')).write_text(process.stdout)
        (logs / (label + '.stderr')).write_text(process.stderr)
        return process

    try:
        sources = {p: (gate.ROOT / p).read_text() for p in gate.PROOFS}
        mutations = gate.control_sources(sources)
        for name, target in NATIVE.items():
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
        for name, target in {**NATIVE, 'nonparticipant': target}.items():
            restored = run(name + '-restored', [*command, target, '--', '--nocapture'])
            if restored.returncode != 0 or '1 passed' not in restored.stdout:
                raise RuntimeError(f'{target} did not pass after restoration')
        receipt = {'schema': 'zeno-fcis/composed-domain-runtime-controls/1', 'status': 'passed',
            'controls': results, 'logs': str(logs),
            'runtime_source_sha256': {str(p): gate.verifier.digest(gate.ROOT / p)
                for p in (gate.SUBJECT, Path('crates/zeno-fcis-contract/src/checked_composition/joint.rs'))}}
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        receipt = {'schema': 'zeno-fcis/composed-domain-runtime-controls/1', 'status': 'failed',
            'error': str(error), 'controls': results, 'logs': str(logs)}
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
    print(json.dumps(receipt))
    return int(receipt['status'] != 'passed')

if __name__ == '__main__':
    raise SystemExit(main())
