#!/usr/bin/env python3
"""Build each payment-policy mutant, require its named assertion, restore and pass."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from cargo_source_scope import source_environment

ROOT = Path(__file__).resolve().parents[1]
EFFECTS = 'crates/zeno-fcis-shell/src/effects/'
PREFIX = 'effects::policy_product::tests::'
CONTROLS = [
    ('composed-suspend-rollback', 'effects::handler_tests::policy_composed_suspend_retains_forwarded_reservation',
     'composed Suspend erased a forwarded reservation', EFFECTS+'handler.rs',
     ' && invocation == Invocation::NotInvoked', '',
     'Fault(Simulation): outer [6], inner [0] forwards once then suspends; outer [0], inner [0] retry must refuse and leave one carried copy.'),
    ('runtime-skipped', 'runtime_policy_blocks_hidden_success', 'double payment after hidden success',
     EFFECTS+'handler.rs',
     'let (next, check) = policy_record::admit(&previous, operation);',
     'let (next, check) = (previous.clone(), super::policy_record::Check::default());',
     'ReadOutbox exact; Send hidden success; Clock/Random/Wait exact; second Send success; two payments of invoice 1.'),
    ('set-instead-of-multiset', 'multiset_checker_rejects_two_identical_commits', 'two commits of one key accepted',
     EFFECTS+'policy_product.rs',
     'let count = committed.iter().filter(|actual| *actual == key).count();',
     'let count = usize::from(committed.iter().any(|actual| actual == key));',
     'Authorized=[key]; committed=[key,key]; distinct-key count is one but occurrence count is two.'),
    ('silent-drop-accepted', 'checker_rejects_silent_drop', 'authorized send ended in no state',
     EFFECTS+'policy_product.rs',
     'if count == 0 && !refused.contains(key) && !pending.contains(key) {',
     'if false && count == 0 && !refused.contains(key) && !pending.contains(key) {',
     'Authorized=[key]; committed=[], refused=[], pending=[]; the invoice has no terminal or incomplete state.'),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.work.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_NET_OFFLINE='true', CARGO_PROFILE_DEV_DEBUG='0',
               CARGO_PROFILE_TEST_DEBUG='0', CARGO_BUILD_JOBS='4')
    if 'CARGO_TARGET_DIR' not in env:
        raise SystemExit('set a bounded CARGO_TARGET_DIR')
    cargo = ['nice', '-n', '19', 'ionice', '-c3', 'cargo', '+1.97.1', 'test', '--locked', '--offline',
             '-p', 'zeno-fcis-shell', '--lib']
    rows = []
    with tempfile.TemporaryDirectory(prefix='payment-policy-controls-', dir=args.work) as temp:
        copy = Path(temp)/'source'
        shutil.copytree(ROOT, copy, ignore=shutil.ignore_patterns('.git', 'target', 'node_modules', '__pycache__'))
        def run(extra):
            return subprocess.run(cargo+extra, cwd=copy, env=source_environment(copy, env),
                                  text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=900)
        for name, test, marker, relative, before, after, witness in CONTROLS:
            named = test if test.startswith('effects::') else PREFIX+test
            check = [named, '--', '--exact', '--nocapture']
            baseline = run(check)
            args.out.with_name(name+'-normal.log').write_text(baseline.stdout)
            if baseline.returncode or '1 passed' not in baseline.stdout:
                raise RuntimeError(f'{name}: baseline failed')
            source = copy/relative
            original_bytes = source.read_bytes()
            original = original_bytes.decode()
            if original.count(before) != 1:
                raise RuntimeError(f'{name}: mutation anchor count {original.count(before)}')
            source.write_text(original.replace(before, after))
            build = run(['--no-run'])
            args.out.with_name(name+'-build.log').write_text(build.stdout)
            if build.returncode:
                raise RuntimeError(f'{name}: build failed, not a kill')
            mutated = run(check)
            args.out.with_name(name+'-mutated.log').write_text(mutated.stdout)
            if (mutated.returncode != 101 or 'test result: FAILED.' not in mutated.stdout
                    or marker not in mutated.stdout or 'assertion' not in mutated.stdout):
                raise RuntimeError(f'{name}: did not fail its named assertion')
            source.write_bytes(original_bytes)
            if source.read_bytes() != original_bytes:
                raise RuntimeError(f'{name}: restoration changed source bytes')
            restored = run(check)
            args.out.with_name(name+'-restored.log').write_text(restored.stdout)
            if restored.returncode or '1 passed' not in restored.stdout:
                raise RuntimeError(f'{name}: restored baseline failed')
            row = dict(control=name, check=named, normal=baseline.returncode,
                       build=build.returncode, mutated=mutated.returncode, restored=restored.returncode,
                       source_sha256=hashlib.sha256(original_bytes).hexdigest(),
                       restored_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
                       witness=witness, verdict='killed-by-named-assertion')
            rows.append(row)
            print(json.dumps(row), flush=True)
    args.out.write_text(json.dumps(rows, indent=2)+'\n')
    print('PASS: four mutants built, failed named assertions, then byte-identical restored baselines passed')


if __name__ == '__main__':
    main()
