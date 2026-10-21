#!/usr/bin/env python3
"""Execute the shell-effects stage-1 controls in an isolated source copy.

Each named check passes on the unmodified tree first. Each mutant must then
build and fail its named check with the stated marker; a build failure, a
signal or an unrelated failure is not a kill. The purity control runs the
repository purity checker instead of a test. No network, dependency change
or edit to the original source tree is used.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = 'crates/zeno-fcis-shell/src/effects/registry.rs'
PROGRAM = 'crates/zeno-fcis-shell/src/effects/program.rs'
DRIVER = 'crates/zeno-fcis-shell/src/effects/driver.rs'
ADAPTER = 'crates/zeno-fcis-shell-sqlite/src/effects.rs'
SHELL = ['-p', 'zeno-fcis-shell', '--lib']
SQLITE = ['-p', 'zeno-fcis-shell-sqlite', '--test', 'effects']


def signature_block(text: str, port: str) -> str:
    """The complete `Signature { .. },` entry of one port."""
    at = text.index(f'        port: Port::{port},\n')
    start = text.rindex('    Signature {\n', 0, at)
    end = text.index('\n    },\n', at) + len('\n    },\n')
    return text[start:end]


# (name, cargo target, named check, markers, behavioural witness, edits)
CONTROLS = [
    ('missing-port', SHELL, 'effects::tests::port_registry',
     ['left: Err(Missing(Acknowledge))'],
     'The registry loses the Acknowledge signature: the exhaustiveness check names the missing port.',
     [(REGISTRY, lambda text: signature_block(text, 'Acknowledge'), '')]),
    ('forged-subject', SQLITE, 'publication_binding',
     ['forged subject bound'],
     'The adapter stops comparing the rebuilt subject: data with one flipped subject byte binds to a publication.',
     [(ADAPTER, 'PublicationOutcome::Commit(publication) if publication.subject() == data.subject => {',
       'PublicationOutcome::Commit(publication) => {')]),
    ('wrong-store', SHELL, 'effects::tests::wrong_store_response',
     ['a refused response advanced the program: Ok(())'],
     "The driver stops comparing store identities: a lost-reply answer from another store resumes the commit program.",
     [(DRIVER, '(Some(got), Some(expected)) if got.identity != expected.identity => {',
       '(Some(got), Some(expected)) if false && got.identity != expected.identity => {')]),
    ('duplicate-response', SHELL, 'effects::tests::duplicate_response',
     ['a duplicated response was admitted'],
     'The driver stops binding ordinals: a repeated Busy answer to a commit resumes the retried commit and spends a retry.',
     [(DRIVER, 'if Some(got.id) == self.answered {\n            return Err(Refusal::Duplicate);',
       'if false && Some(got.id) == self.answered {\n            return Err(Refusal::Duplicate);'),
      (DRIVER, 'if got.id.ordinal < binding.id.ordinal {', 'if false && got.id.ordinal < binding.id.ordinal {'),
      (DRIVER, 'if got.id.ordinal != binding.id.ordinal {', 'if false && got.id.ordinal != binding.id.ordinal {')]),
    ('substituted-source', SHELL, 'effects::tests::step_conformance',
     ['program source bound: false', 'absent key keeps the original guard'],
     'A substituted step function retries unversioned whenever recovery reads history, even when the key is absent: '
     'the bound source digest no longer matches and the absent-key case requests Commit(Unversioned).',
     [(PROGRAM, '''                    let mode = if contains(&history, &continuation.invocation) {
                        CommitMode::Unversioned
                    } else {
                        continuation.invocation.mode
                    };''', '''                    let _ = (&history, contains);
                    let mode = CommitMode::Unversioned;''')]),
    ('ambient-clock', None, 'purity',
     ['error[clock]'],
     'A step function reads std::time::Instant: the purity checker reports a clock violation in program.rs.',
     [(PROGRAM, '\nfn ordered(', '\n#[allow(dead_code)]\nfn elapsed() -> bool {\n    std::time::Instant::now().elapsed().as_nanos() > 0\n}\n\nfn ordered(')]),
]


def replace(text: str, before, after: str) -> str:
    anchor = before(text) if callable(before) else before
    found = text.count(anchor)
    if found != 1:
        raise RuntimeError(f'mutation anchor count {found}, wanted 1: {anchor[:80]!r}')
    return text.replace(anchor, after)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, help='write the control rows here, logs beside it')
    parser.add_argument('--cli', type=Path, required=True, help='built zeno-fcis binary for the purity control')
    parser.add_argument('--only', choices=[control[0] for control in CONTROLS])
    args = parser.parse_args()
    env = dict(os.environ, CARGO_NET_OFFLINE='true')
    env.setdefault('CARGO_TARGET_DIR', str(ROOT / 'target' / 'shell-effects-controls'))
    cargo = ['cargo', '+1.97.1', 'test', '--locked', '--offline']

    def run(root: Path, argv: list[str]) -> subprocess.CompletedProcess[str]:
        if shutil.disk_usage('/').free < 8 * 1024**3:
            raise RuntimeError('root free space below 8 GiB')
        return subprocess.run(argv, cwd=root, env=env, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True, timeout=900)

    def check(root: Path, target, name: str) -> subprocess.CompletedProcess[str]:
        if target is None:
            return run(root, [str(args.cli), 'purity', '--no-kb', 'crates/zeno-fcis-shell'])
        return run(root, cargo + target + [name, '--', '--exact'])

    selected = [control for control in CONTROLS if args.only in (None, control[0])]
    for name, target, test, *_ in selected:
        baseline = check(ROOT, target, test)
        passed = 'purity: clean' in baseline.stdout if target is None else ' 1 passed' in baseline.stdout
        if baseline.returncode or not passed:
            raise SystemExit(f'BASELINE FAILED {name} ({test}):\n{baseline.stdout}')
    rows = []
    with tempfile.TemporaryDirectory(prefix='shell-effects-controls-') as temp:
        copy = Path(temp) / 'source'
        shutil.copytree(ROOT, copy, ignore=shutil.ignore_patterns('.git', 'target', 'node_modules', '__pycache__'))
        for name, target, test, markers, witness, edits in selected:
            original = {path: (copy / path).read_text() for path, _, _ in edits}
            changed = dict(original)
            for path, before, after in edits:
                changed[path] = replace(changed[path], before, after)
            for path, text in changed.items():
                (copy / path).write_text(text)
            start = time.monotonic()
            package = target[:2] if target else ['-p', 'zeno-fcis-shell']
            build = run(copy, ['cargo', '+1.97.1', 'test', '--locked', '--offline', '--no-run'] + package
                        + (target[2:] if target else ['--lib']))
            if build.returncode:
                raise SystemExit(f'BUILD FAILURE (not a kill) {name}:\n{build.stdout}')
            result = check(copy, target, test)
            if target is None:
                killed = 'purity: violations' in result.stdout
            else:
                killed = result.returncode == 101 and 'test result: FAILED.' in result.stdout
            missing = [marker for marker in markers if marker not in result.stdout]
            if not killed or missing:
                raise SystemExit(f'NOT KILLED for the named reason {name} (missing {missing}):\n{result.stdout}')
            row = dict(control=name, check=test, witness=witness, build_exit=build.returncode,
                       check_exit=result.returncode, result='killed',
                       seconds=round(time.monotonic() - start, 3))
            rows.append(row)
            print(json.dumps(row), flush=True)
            if args.out:
                args.out.parent.mkdir(parents=True, exist_ok=True)
                args.out.with_name(f'{args.out.stem}-{name}.log').write_text(build.stdout + '\n' + result.stdout)
            for path, text in original.items():
                (copy / path).write_text(text)
    if args.out:
        args.out.write_text(json.dumps(rows, indent=2) + '\n')
    print(f'PASS: baselines and {len(rows)} source controls killed for their named reasons')


if __name__ == '__main__':
    main()
