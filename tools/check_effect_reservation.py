#!/usr/bin/env python3
"""Qualify the abstract effect-reservation protocol, without claiming refinement.

Requires pinned complete Verus verification, exact translated inventories, native
samples, compiled assertion controls, restored proofs, and the K=6 differential.
Production source hashes identify the comparison subject; hashes are not a
production refinement theorem. No network installation is performed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

import check_verus as verifier
from verus_coverage import require_coverage

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path('verification/verus/effect-reservation.json')
HARNESS = Path('verification/verus/effect_reservation.rs')
RUNTIME = Path('verification/verus/effect_protocol_runtime.rs')
RUNNER = Path('verification/verus/effect_protocol_runner.rs')
MODEL_SOURCES = (HARNESS, RUNTIME, RUNNER,
                 Path('verification/verus/effect_protocol.rs'),
                 Path('verification/verus/effect_protocol_examples.rs'),
                 Path('verification/verus/effect_protocol_replay.rs'))
PRODUCTION_SOURCES = tuple(Path('crates/zeno-fcis-shell/src/effects') / f'{name}.rs'
                           for name in ('handler', 'policy', 'policy_record', 'record',
                                        'policy_product', 'codec', 'driver', 'port',
                                        'program', 'registry', 'simulation'))
SOURCES = (*MODEL_SOURCES, *PRODUCTION_SOURCES, PROFILE, verifier.PIN,
           Path('tools/check_effect_reservation.py'), Path('tools/test_check_effect_reservation.py'),
           Path('tools/check_effect_protocol_traces.py'), Path('tools/check_payment_policy_product.py'),
           Path('tools/check_verus.py'), Path('tools/verus_coverage.py'), Path('tools/verus_ids.py'),
           Path('Cargo.toml'), Path('Cargo.lock'), Path('rust-toolchain.toml'))
EXECUTABLES = ('prepare', 'finish')
FORBIDDEN = re.compile(r'\b(?:assume|admit)\s*[!(]|\bexternal_body\b|'
                       r'verifier\s*::\s*external\b|\bexec_allows_no_decreases_clause\b|'
                       r'\b(?:assume_specification|external_fn_specification)\b')
CONFIGURATION = re.compile(r'#\s*!?\s*\[\s*(cfg(?:_attr)?)\s*\(')


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(root=ROOT):
    result = {}
    for relative in SOURCES:
        path = root / relative
        if path.is_symlink() or not path.is_file():
            raise ValueError(f'missing or symlinked proof source: {relative}')
        result[str(relative)] = sha256(path)
    return result


def configuration_guard(source, *, runtime=False):
    """A restricted grammar, supplemented by the exact translated body inventory."""
    if FORBIDDEN.search(source) or re.search(r'\bcfg\s*!', source):
        raise ValueError('proof escape or conditional execution macro')
    for found in CONFIGURATION.finditer(source):
        tail = source[found.end():]
        if found[1] == 'cfg':
            if not re.match(r'\s*(?:verus_keep_ghost|test)\s*\)\s*\]', tail):
                raise ValueError('unqualified conditional source')
        elif not re.match(r'\s*verus_keep_ghost\s*,\s*(?:verus_spec\s*\(|'
                          r'verus_verify\s*\)|feature\s*\(proc_macro_hygiene\))', tail):
            raise ValueError('unqualified conditional attribute')
    if runtime:
        for name in EXECUTABLES:
            start, end = function_body(source, name)
            body = source[start:end]
            for found in CONFIGURATION.finditer(body):
                tail = body[found.end():]
                if found[1] == 'cfg' and not re.match(r'\s*verus_keep_ghost\s*\)\s*\]\s*proof\s*!', tail):
                    raise ValueError('executable code conditionally absent from verification')


def function_body(source, name):
    """Conservative body boundaries; comments cannot hide an executable tail."""
    matches = list(re.finditer(r'\bpub\s+fn\s+' + re.escape(name) + r'\b', source))
    if len(matches) != 1:
        raise ValueError(f'not one executable function: {name}')
    start = source.index('{', matches[0].end())
    at, depth = start, 0
    while at < len(source):
        if source.startswith('//', at):
            end = source.find('\n', at)
            at = len(source) if end < 0 else end + 1
            continue
        if source.startswith('/*', at):
            at += 2
            comments = 1
            while comments and at < len(source):
                if source.startswith('/*', at): comments, at = comments+1, at+2
                elif source.startswith('*/', at): comments, at = comments-1, at+2
                else: at += 1
            if comments: raise ValueError('unterminated comment')
            continue
        # Current executable subjects need no text/character literals. Refuse
        # additions until reviewed instead of accepting an incomplete Rust lexer.
        if source[at] in ('"', "'"):
            raise ValueError('new literal grammar in executable subject requires review')
        if source[at] == '{': depth += 1
        if source[at] == '}':
            depth -= 1
            if depth == 0: return start, at+1
        at += 1
    raise ValueError('unterminated executable function')


def source_guard(root=ROOT):
    for relative in MODEL_SOURCES:
        configuration_guard((root / relative).read_text(), runtime=relative == RUNTIME)


def accepted(proc, report, pin, profile):
    expected = {**pin, 'expected_verified': profile['expected_verified'],
                'target_functions': profile['target_functions']}
    return (proc.returncode == 0 and verifier.accepted(report, expected)
            and verifier.proof_succeeded(report, pin)
            and not verifier.ERROR_HEADER.search(proc.stderr)
            and not verifier.NON_SEMANTIC.search(proc.stderr))


def process(command, root, environment, directory, timeout=900):
    directory.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    expanded = ['nice', '-n', '19', 'ionice', '-c3', *map(str, command)]
    timed_out = False
    try:
        proc = subprocess.run(expanded, cwd=root, env=environment, text=True,
                              capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        timed_out = True
        stdout, stderr = error.stdout or '', error.stderr or ''
        if isinstance(stdout, bytes): stdout = stdout.decode(errors='replace')
        if isinstance(stderr, bytes): stderr = stderr.decode(errors='replace')
        proc = subprocess.CompletedProcess(expanded, 124, stdout, stderr)
    (directory / 'stdout').write_text(proc.stdout)
    (directory / 'stderr').write_text(proc.stderr)
    (directory / 'process.json').write_text(json.dumps(dict(
        command=expanded, cwd=str(root), exit_code=proc.returncode,
        timed_out=timed_out, seconds=time.monotonic()-start), indent=2)+'\n')
    return proc


def json_report(proc):
    try:
        result = json.loads(proc.stdout)
    except (ValueError, TypeError):
        return {}
    return result if isinstance(result, dict) else {}


def require_success(proc):
    if proc.returncode:
        raise RuntimeError(f'command failed ({proc.returncode}): {proc.args}; inspect retained logs')


def mutations(source):
    def once(function, before, after):
        start, end = function_body(source, function)
        body = source[start:end]
        if body.count(before) != 1:
            raise ValueError(f'mutation does not have one anchor: {before}')
        return source[:start] + body.replace(before, after) + source[end:]
    start, end = function_body(source, 'prepare')
    body = source[start:end]
    veto = body.index('if !allowed {')
    if body[veto:].count('reserved: keys.clone()') != 1:
        raise ValueError('not one veto rollback anchor')
    broken_veto = source[:start] + body[:veto] + body[veto:].replace(
        'reserved: keys.clone()', 'reserved: Vec::new()', 1) + source[end:]
    return {
        'forward_twice_under_one_reservation': (
            once('finish', '(final_state, copies)', '(final_state, copies.saturating_add(copies))'),
            'finish', 'one_terminal_copy_stays_one_copy'),
        'release_after_forwarding': (
            once('finish', 'if suspended && !may_invoke',
                 'if suspended && (may_invoke || !may_invoke)'),
            'finish', 'forwarded_suspend_retains_reservation'),
        'skip_policy_on_retry': (
            once('prepare', 'if !send {', 'if !send || retry {'),
            'prepare', 'retry_consults_every_policy'),
        'rollback_releases_earlier_handler': (
            broken_veto,
            'prepare', 'veto_restores_earlier_reservations'),
    }


def proof(command, root, environment, directory, pin, profile):
    proc = process([*command, '--log-dir', directory / 'vir', HARNESS],
                   root, environment, directory)
    report = json_report(proc)
    if not accepted(proc, report, pin, profile):
        raise RuntimeError(f'whole-model proof not qualified: {directory}')
    coverage = require_coverage((directory / 'vir/crate.vir').read_text(), profile)
    return report, coverage


def native_build(root, environment, directory, output, *, runner=False):
    command = ['rustc', '+1.97.1', '--edition=2024', '--check-cfg', 'cfg(verus_keep_ghost)',
               '--check-cfg', 'cfg(test)', '-D', 'warnings']
    if not runner: command.append('--test')
    command += [str(RUNNER if runner else HARNESS), '-o', str(output)]
    proc = process(command, root, environment, directory)
    require_success(proc)


def check(out, work, payment_binary):
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    before = snapshot()
    source_guard()
    tools = verifier.prepare_tools(verifier.cache_directory(), pin, False)
    environment = {k: v for k, v in os.environ.items()
                   if not k.startswith(('VERUS_', 'VARGO_'))
                   and k not in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')}
    environment.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'], VERUS_Z3_PATH=str(tools / 'z3'),
                       CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
                       CARGO_BUILD_JOBS='4', CARGO_INCREMENTAL='0')
    command = [str(tools / 'verus'), '--crate-type=lib', '--edition=2024',
               '--no-cheating', '--no-external-by-default', '--num-threads', '2',
               '--output-json', '--log', 'vir', '--log', 'vir-option=no_span+no_type+no_fn_details']
    work.mkdir(parents=True, exist_ok=True)
    report, coverage = proof(command, ROOT, environment, work / 'positive', pin, profile)
    binary = work / 'native-tests'
    native_build(ROOT, environment, work / 'native-build', binary)
    native = process([binary], ROOT, environment, work / 'native-tests-log')
    require_success(native)
    controls = []
    original = (ROOT / RUNTIME).read_bytes()
    for name, (changed, function, test) in mutations(original.decode()).items():
        directory = work / 'controls' / name
        specimen = directory / 'source'
        for relative in MODEL_SOURCES:
            target = specimen / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, target)
        path = specimen / RUNTIME
        path.write_text(changed)
        source_guard(specimen)
        executable = directory / 'native-test'
        native_build(specimen, environment, directory / 'build', executable)
        failed = process([executable, f'runtime::tests::{test}', '--exact', '--nocapture'],
                         specimen, environment, directory / 'native-failure')
        text = failed.stdout + failed.stderr
        if not (failed.returncode == 101 and 'assertion' in text and test in text
                and 'test result: FAILED' in text):
            raise RuntimeError(f'control lacks intended native assertion failure: {name}')
        mutated = process([*command, '--log-dir', directory / 'verus/vir', HARNESS],
                          specimen, environment, directory / 'verus')
        parsed = json_report(mutated)
        region = verifier.function_lines(changed, function)
        if not verifier.semantic_refusal(mutated, parsed, pin, RUNTIME, specimen,
                region=region, target_functions=profile['target_functions']):
            raise RuntimeError(f'control lacks intended complete semantic refusal: {name}')
        changed_hash = sha256(path)
        path.write_bytes(original)
        if path.read_bytes() != original:
            raise RuntimeError(f'control not restored byte-identically: {name}')
        restored_binary = directory / 'restored-test'
        native_build(specimen, environment, directory / 'restored-build', restored_binary)
        restored = process([restored_binary, f'runtime::tests::{test}', '--exact'],
                           specimen, environment, directory / 'native-restored')
        require_success(restored)
        proof(command, specimen, environment, directory / 'verus-restored', pin, profile)
        controls.append(dict(name=name, intended_function=function, intended_test=test,
                             build=0, native=failed.returncode, verus=mutated.returncode,
                             restored_native=0, restored_verus=0, restored_byte_identically=True,
                             mutated_sha256=changed_hash, restored_sha256=sha256(path)))
        (work / 'controls.json').write_text(json.dumps(controls, indent=2)+'\n')
        print(f'caught and restored: {name}', flush=True)
    runner = work / 'abstract-runner'
    native_build(ROOT, environment, work / 'runner-build', runner, runner=True)
    differential = process(['python3', '-B', 'tools/check_effect_protocol_traces.py',
                            '--payment-binary', payment_binary, '--model-binary', runner,
                            '--out', work / 'traces/differential.json'],
                           ROOT, environment, work / 'differential', timeout=1200)
    require_success(differential)
    final_report, _ = proof(command, ROOT, environment, work / 'final-proof', pin, profile)
    verifier.verify_tool_files(tools, pin)
    if snapshot() != before:
        raise RuntimeError('source changed while the gate ran')
    receipt = dict(schema='zeno-fcis/effect-reservation-evidence/1', status='passed',
                   tiers=[1], production_refinement='not proved', source_sha256=before,
                   base_revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                   verus_report=final_report, translated_function_coverage=coverage,
                   controls=controls, trace_differential=json.loads((work / 'traces/differential.json').read_text()),
                   native_test_output=native.stdout,
                   scope='Abstract protocol only; source hashes and finite differential do not prove production refinement.',
                   trusted_base=['pinned Verus/vstd/Z3', 'Rust erasure/compiler', 'standard library and host'])
    out.write_text(json.dumps(receipt, indent=2, sort_keys=True)+'\n')
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--payment-binary', type=Path, required=True)
    args = parser.parse_args()
    out, work = args.out.resolve(), args.work.resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    try:
        receipt = check(out, work, args.payment_binary.resolve())
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        out.write_text(json.dumps(dict(status='failed', error=str(error), logs=str(work)), indent=2)+'\n')
        print(f'FAIL: {error}')
        return 1
    print(f'PASS: {receipt["verus_report"]["verification-results"]["verified"]} obligations, '
          f'{len(receipt["controls"])} compiled controls, complete K=6 differential')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
