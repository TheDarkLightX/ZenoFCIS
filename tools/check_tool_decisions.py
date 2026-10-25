#!/usr/bin/env python3
"""Check the production optimizer status and contract decision-space helpers.

This focused gate proves their stated contracts, tests the native bodies, and
requires two planted regressions to fail semantically. It does not qualify the
entire optimizer or contract compiler. Evidence stays in --out.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

import check_verus as verifier

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path('verification/verus/tool_decisions.rs')
STATUS = Path('crates/zeno-fcis-cli/src/optimize/status.rs')
SPACE = Path('crates/zeno-fcis-contract/src/internals/decision_space.rs')
CLASS = Path('crates/zeno-fcis-contract/src/internals/decision_class.rs')
SUBJECTS = (HARNESS, STATUS, SPACE, CLASS)
SOURCES = (*SUBJECTS, verifier.PIN, Path('rust-toolchain.toml'),
           Path('tools/check_tool_decisions.py'), Path('tools/check_verus.py'),
           *verifier.QUALIFICATION_SOURCES)
TARGETS = {
    'tool_decisions::status::classify', 'tool_decisions::status::Status::name',
    'tool_decisions::status::Status::exit_code',
    *('tool_decisions::decision_space::' + name for name in
      ('same_class', 'reasons_for', 'append_one', 'append_reasons', 'alternatives')),
}
CONTROLS = (
    ('false_no_improvement', STATUS,
     '} else if untypable && phases == 0 {\n        Status::UnsupportedOriginal',
     '} else if untypable && phases == 0 {\n        Status::NoCheckedImprovement',
     'status::tests::a_search_that_never_ran_cannot_report_no_improvement', 'classify'),
    ('mixed_reason_classes', SPACE, 'if same_class(reasons[i].1, class) {', 'if true {',
     'decision_space::tests::alternatives_never_cross_reason_classes', 'reasons_for'),
)


def snapshot() -> dict:
    return {str(p): verifier.digest(ROOT / p) for p in SOURCES}


def execute(command: list[str], cwd: Path, out: Path, name: str):
    started = time.monotonic()
    result = subprocess.run(command, cwd=cwd, env=os.environ.copy(),
                            capture_output=True, text=True, timeout=180)
    (out / f'{name}.stdout').write_text(result.stdout)
    (out / f'{name}.stderr').write_text(result.stderr)
    (out / f'{name}.json').write_text(json.dumps({
        'command': command, 'exit': result.returncode,
        'seconds': time.monotonic() - started,
    }, indent=2) + '\n')
    return result


def proof(tools: Path, cwd: Path, out: Path):
    result = execute([str(tools / 'verus'), '--crate-type=lib', '--edition=2024',
                      '--no-cheating', '--output-json', str(HARNESS)], cwd, out, 'proof')
    return result, json.loads(result.stdout)


def native(cwd: Path, out: Path, test: str | None = None):
    binary = out / 'native-tests'
    compiled = execute(['rustc', '+1.97.1', '--edition=2024', '--test', str(HARNESS),
                        '-o', str(binary)], cwd, out, 'native-build')
    if compiled.returncode:
        raise RuntimeError('native harness did not compile')
    return execute([str(binary), *([test, '--exact'] if test else [])], cwd, out, 'native-test')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--cache', type=Path, default=verifier.cache_directory())
    parser.add_argument('--install', action='store_true')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    before = snapshot()
    receipt = {'schema': 'zeno-fcis/tool-decisions-check/1', 'status': 'failed',
               'sources': before, 'controls': [], 'authority': 'none'}
    try:
        pin = json.loads((ROOT / verifier.PIN).read_text())
        tools = verifier.prepare_tools(args.cache, pin, args.install)
        result, report = proof(tools, ROOT, out)
        if (result.returncode != 0 or not verifier.proof_succeeded(report, pin)
                or not TARGETS.issubset(report.get('func-details', {}))):
            raise RuntimeError('positive proof or target coverage failed')
        receipt['verification'] = report['verification-results']
        result = native(ROOT, out)
        if result.returncode or '2 passed; 0 failed' not in result.stdout:
            raise RuntimeError('positive native tests failed')
        for name, source, anchor, replacement, test, function in CONTROLS:
            lane = out / name
            lane.mkdir()
            for path in SUBJECTS:
                target = lane / path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / path, target)
            text = (lane / source).read_text()
            if text.count(anchor) != 1:
                raise RuntimeError(f'{name}: mutation anchor changed')
            (lane / source).write_text(text.replace(anchor, replacement))
            native_result = native(lane, lane, test)
            if (native_result.returncode != 101 or f'{test} ... FAILED' not in native_result.stdout
                    or 'assertion `left == right` failed' not in native_result.stdout
                    or '0 passed; 1 failed' not in native_result.stdout):
                raise RuntimeError(f'{name}: intended native assertion did not fail')
            result, report = proof(tools, lane, lane)
            mutant = (lane / source).read_text()
            start = mutant.index('fn ' + function + '(')
            end = mutant.index('\n}', start) + 2
            region = (mutant.count('\n', 0, start) + 1, mutant.count('\n', 0, end) + 1)
            if not verifier.semantic_refusal(result, report, pin, source, lane, region=region):
                raise RuntimeError(f'{name}: intended semantic proof failure not established')
            receipt['controls'].append({'name': name, 'native_assertion': test,
                                        'semantic_refusal': True})
        if snapshot() != before:
            raise RuntimeError('source changed during the gate')
        receipt['status'] = 'passed'
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
        receipt['error'] = str(error)
    (out / 'receipt.json').write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'status': receipt['status'], 'controls': len(receipt['controls']),
                      'error': receipt.get('error')}))
    return 0 if receipt['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
