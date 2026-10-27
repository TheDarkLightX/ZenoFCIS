#!/usr/bin/env python3
"""Check the production state-lemma kernel, exact proof inventory and semantic controls.

The table adapter and JSON renderer are tested boundaries. This gate neither
qualifies the evaluator nor grants application or release authority.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

import check_verus as verifier
from verus_coverage import inventory

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path('verification/verus/lemma_kernel.rs')
SUBJECT = Path('crates/zeno-fcis-cli/src/contract/review/lemma_kernel.rs')
TESTS = Path('verification/verus/lemma_kernel_tests.rs')
SPEC = Path('verification/verus/finite_machine_spec.rs')
PROFILE = Path('verification/verus/lemma-kernel.json')
ADAPTER = SUBJECT.with_name('lemma.rs')
NAMESPACE = 'lemma_kernel::lemma_kernel::'
COVERAGE_NAMESPACE = 'lemma_kernel::'
UNIT = (HARNESS, SUBJECT, TESTS, SPEC)
SOURCES = (*UNIT, PROFILE, ADAPTER, verifier.PIN,
    Path('tools/check_lemma_kernel.py'), Path('tools/test_check_lemma_kernel.py'),
    Path('tools/check_verus.py'), Path('tools/verus_coverage.py'),
    *verifier.QUALIFICATION_SOURCES, Path('Cargo.toml'), Path('Cargo.lock'),
    Path('rust-toolchain.toml'), Path('.github/workflows/verus.yml'))
FORBIDDEN = re.compile(r'\b(?:assume(?:_specification)?|admit|external_body|axiom|unsafe)\b|'
    r'verifier\s*::\s*(?:external\w*|verify\s*\(|exec_allows_no_decreases_clause)|'
    r'\bdecreases\s*\*')
CONTROLS = {
    'skip_initiation': ('if !initiation {', 'if false {', 'decide', 'initiation_is_required'),
    'approve_false_genesis': ('(Verdict::False, Some(g), None, Vec::new())',
        '(Verdict::Inductive, None, None, Vec::new())', 'decide', 'initiation_is_required'),
    'noncommit_as_commit': ('if commits[ordinal] { after[ordinal] } else { state }',
        'if true { after[ordinal] } else { state }', 'reachable',
        'noncommitting_raw_successor_cannot_change_reachability'),
    'stop_search_early': ('while at < queue.len() {',
        'while at < queue.len() && at != queue.len() - 1 {', 'reachable',
        'search_must_process_last_frontier_state'),
    'admitted_counterexample': ('Some((before, input, state))',
        'Some((before, input, before))', 'decide', 'counterexample_successor_is_excluded'),
    'native_only_approval': ('    Some(Decision {',
        '    #[cfg(not(verus_keep_ghost))] let verdict = if k == 3 { Verdict::Inductive } else { verdict };\n'
        '    Some(Decision {', 'decide', 'native_only_approval_must_be_false'),
}


def snapshot() -> dict[str, str]:
    result = {}
    for p in SOURCES:
        path = ROOT / p
        if not path.is_file() or path.is_symlink():
            raise RuntimeError(f'missing or symbolic-link input: {p}')
        result[str(p)] = verifier.digest(path)
    return result


def source_controls(source: str) -> dict[str, tuple[str, str, str]]:
    # Whitespace is normalized only for the one short expression rustfmt expands.
    controls = dict(CONTROLS)
    anchor, replacement, function, test = controls['noncommit_as_commit']
    if source.count(anchor) == 0:
        anchor = 'if commits[ordinal] {\n                after[ordinal]\n            } else {\n                state\n            }'
        replacement = anchor.replace('if commits[ordinal]', 'if true')
        controls['noncommit_as_commit'] = anchor, replacement, function, test
    result = {}
    for name, (anchor, replacement, function, test) in controls.items():
        if source.count(anchor) != 1:
            raise RuntimeError(f'control anchor is not unique: {name}')
        result[name] = source.replace(anchor, replacement), function, test
    return result


def check_inventory(vir: str, profile: dict) -> dict:
    if profile.get('namespace') != COVERAGE_NAMESPACE:
        raise ValueError('unexpected kernel namespace')
    actual = inventory(vir, COVERAGE_NAMESPACE, tuple(profile['body_covered_functions']))
    if actual != profile['functions']:
        changed = sorted(k for k in set(actual) | set(profile['functions'])
            if actual.get(k) != profile['functions'].get(k))
        raise ValueError(f'kernel translated inventory/contracts changed: {changed}')
    for name in ('decide', 'valid_tables'):
        contract = actual[NAMESPACE + name]
        if contract['requires'] != 0 or contract['ensures'] == 0:
            raise ValueError(f'unconditional boundary lost: {name}')
    return actual


def positive_accepted(proc, report: dict, pin: dict, profile: dict) -> bool:
    return (proc.returncode == 0 and verifier.accepted(report, {
            **pin, 'expected_verified': profile['expected_verified'],
            'target_functions': profile['target_functions']})
        and verifier.proof_succeeded(report, pin)
        and not verifier.ERROR_HEADER.search(proc.stderr)
        and not verifier.NON_SEMANTIC.search(proc.stderr))


def function_region(source: str, name: str) -> tuple[int, int]:
    # The shared arithmetic helper stops at an inner loop's verus_spec attribute.
    # Retain its annotated start, but balance the actual executable function body.
    start, _ = verifier.function_lines(source, name)
    match = re.search(r'\bfn\s+' + re.escape(name) + r'\b', source)
    if match is None:
        raise ValueError(f'missing target function: {name}')
    body = source.index('{', match.end())
    depth = 0
    for token in re.finditer(r'//[^\n]*|"(?:\\.|[^"\\])*"|[{}]', source[body:]):
        value = token.group()
        if value == '{':
            depth += 1
        elif value == '}':
            depth -= 1
            if depth == 0:
                return start, source.count('\n', 0, body + token.end()) + 1
    raise ValueError(f'unclosed target function: {name}')


def native_failed(proc, test: str) -> bool:
    # Rust test assertions are expected; compiler failures and process crashes are not.
    return (proc.returncode == 101 and f'test tests::{test} ... FAILED' in proc.stdout
        and re.search(r'assertion(?: `[^\n]+`)? failed', proc.stdout + proc.stderr) and 'test result: FAILED. 0 passed; 1 failed;' in proc.stdout
        and not re.search(r'segmentation fault|signal:|stack overflow', proc.stdout + proc.stderr, re.I))


def run(command, cwd: Path, environment: dict, output: Path):
    proc = verifier.run(command, cwd, environment, timeout=600)
    output.with_suffix('.stdout').write_text(proc.stdout)
    output.with_suffix('.stderr').write_text(proc.stderr)
    output.with_suffix('.command.json').write_text(json.dumps({
        'command': command, 'cwd': str(cwd), 'exit': proc.returncode}, indent=2) + '\n')
    return proc


def native_command(pin: dict, root: Path, binary: Path) -> list[str]:
    return ['rustc', f"+{pin['runtime_rust']}", '--edition=2024', '--test',
        '--check-cfg', 'cfg(verus_keep_ghost)', '--check-cfg', 'cfg(test)',
        str(root / HARNESS), '-o', str(binary)]


def check(cache: Path, install: bool, directory: Path) -> dict:
    before = snapshot()
    source = (ROOT / SUBJECT).read_text()
    for path in (SUBJECT, SPEC):
        if FORBIDDEN.search(re.sub(r'//[^\n]*', '', (ROOT / path).read_text())):
            raise RuntimeError(f'kernel source has an untrusted proof escape or unsafe code: {path}')
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    configuration = verifier.check_native_configuration(source, SUBJECT,
        profile['native_configuration_exceptions'])
    verifier.check_native_configuration((ROOT / SPEC).read_text(), SPEC)
    tool_directory = verifier.prepare_tools(cache, pin, install)
    environment = {k: v for k, v in os.environ.items()
        if not k.startswith(('VERUS_', 'VARGO_')) and k not in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')}
    environment.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'],
        VERUS_Z3_PATH=str(tool_directory / 'z3'), RUST_BACKTRACE='0')
    command = [str(tool_directory / 'verus'), '--crate-type=lib', '--edition=2024',
        '--no-cheating', '--no-external-by-default', '--num-threads', '2',
        '--output-json', '--log', 'vir', '--log', 'vir-option=no_span+no_type+no_fn_details']
    positive = run([*command, '--log-dir', str(directory / 'positive'), str(ROOT / HARNESS)],
        ROOT, environment, directory / 'positive-proof')
    report = json.loads(positive.stdout)
    if not positive_accepted(positive, report, pin, profile):
        raise RuntimeError('kernel proof failed pinned complete-run/count admission')
    coverage = check_inventory((directory / 'positive/crate.vir').read_text(), profile)
    binary = directory / 'native-tests'
    verifier.require_success(run(native_command(pin, ROOT, binary), ROOT, environment, directory / 'native-build'))
    native = run([str(binary), '--nocapture'], ROOT, environment, directory / 'native-tests')
    verifier.require_success(native)
    controls = []
    for name, (changed, function, test) in source_controls(source).items():
        specimen = directory / name
        specimen.mkdir()
        for p in UNIT:
            path = specimen / p
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((ROOT / p).read_bytes())
        target = specimen / SUBJECT
        original = target.read_bytes()
        restored = False
        try:
            target.write_text(changed)
            (specimen / 'mutated-source.rs.txt').write_text(changed)
            mutant_hash = verifier.digest(target)
            executable = specimen / 'native-tests'
            build = run(native_command(pin, specimen, executable), ROOT, environment, specimen / 'native-build')
            verifier.require_success(build)
            native_control = run([str(executable), '--exact', f'tests::{test}', '--nocapture'],
                ROOT, environment, specimen / 'native-control')
            if not native_failed(native_control, test):
                raise RuntimeError(f'control lacks the intended native assertion: {name}')
            configuration_refusal = None
            try:
                verifier.check_native_configuration(changed, SUBJECT,
                    profile['native_configuration_exceptions'])
            except verifier.ConfigurationRefused as error:
                configuration_refusal = {'line': error.line, 'source': error.source_line,
                    'diagnostic': str(error)}
            if name == 'native_only_approval':
                expected_line = changed[:changed.index('    #[cfg(not(verus_keep_ghost))] let verdict')].count('\n') + 1
                if configuration_refusal is None or configuration_refusal['line'] != expected_line:
                    raise RuntimeError('native-only control lacks the intended configuration refusal')
            elif configuration_refusal is not None:
                raise RuntimeError(f'semantic control was refused by configuration guard: {name}')
            proof = run([*command, '--log-dir', str(specimen / 'proof'), str(specimen / HARNESS)],
                ROOT, environment, specimen / 'verus')
            mutant_report = json.loads(proof.stdout)
            if name == 'native_only_approval':
                # This control demonstrates why translated VIR alone is insufficient.
                if not positive_accepted(proof, mutant_report, pin, profile):
                    raise RuntimeError('native-only control no longer reproduces the translation gap')
                check_inventory((specimen / 'proof/crate.vir').read_text(), profile)
            elif not verifier.semantic_refusal(proof, mutant_report, pin, SUBJECT, specimen,
                    region=function_region(changed, function),
                    target_functions=profile['target_functions']):
                raise RuntimeError(f'control lacks the intended Verus obligation refusal: {name}')
            failure_notes = mutant_report['func-details'][NAMESPACE + function].get('failed_proof_notes', [])
            obligations = []
            headers = list(verifier.ERROR_HEADER.finditer(proof.stderr))
            for index, header in enumerate(headers):
                if header[1].startswith('aborting due to '):
                    continue
                end = headers[index+1].start() if index+1 < len(headers) else len(proof.stderr)
                primary = re.search(r'^\s*-->\s+(.+):(\d+):(\d+)\s*$',
                    proof.stderr[header.end():end], re.M)
                line = int(primary[2])
                obligations.append({'kind': header[1], 'line': line,
                    'source': changed.splitlines()[line-1].strip()})
            control = {'name': name, 'function': NAMESPACE + function, 'native_test': test,
                'expected_failure': 'configuration' if configuration_refusal else 'proof',
                'configuration_refusal': configuration_refusal,
                'native_build_exit': build.returncode, 'native_assertion_exit': native_control.returncode,
                'verus_exit': proof.returncode, 'verification_results': mutant_report['verification-results'],
                'failed_proof_notes': failure_notes, 'obligations': obligations, 'mutant_sha256': mutant_hash,
                'normal_sha256': before[str(SUBJECT)]}
        finally:
            target.write_bytes(original)
            restored = verifier.digest(target) == before[str(SUBJECT)]
        if not restored:
            raise RuntimeError(f'control did not restore the source bytes: {name}')
        control['restored_sha256'] = verifier.digest(target)
        verifier.check_native_configuration(target.read_text(), SUBJECT,
            profile['native_configuration_exceptions'])
        restored_build = run(native_command(pin, specimen, executable), ROOT, environment,
            specimen / 'restored-build')
        verifier.require_success(restored_build)
        restored_native = run([str(executable), '--exact', f'tests::{test}', '--nocapture'],
            ROOT, environment, specimen / 'restored-native')
        verifier.require_success(restored_native)
        restored_proof = run([*command, '--log-dir', str(specimen / 'restored-proof'),
            str(specimen / HARNESS)], ROOT, environment, specimen / 'restored-verus')
        restored_report = json.loads(restored_proof.stdout)
        if not positive_accepted(restored_proof, restored_report, pin, profile):
            raise RuntimeError(f'restored control did not verify: {name}')
        check_inventory((specimen / 'restored-proof/crate.vir').read_text(), profile)
        control['restored'] = {'native_build_exit': restored_build.returncode,
            'native_assertion_exit': restored_native.returncode, 'verus_exit': restored_proof.returncode,
            'verified': restored_report['verification-results']['verified']}
        controls.append(control)
        (directory / 'controls.json').write_text(json.dumps(controls, indent=2) + '\n')
        refusal = 'configuration guard' if configuration_refusal else 'Verus obligation'
        print(f'lemma kernel: {name}: native assertion and {refusal} rejected; restored checks passed', flush=True)
    verifier.verify_tool_files(tool_directory, pin)
    if before != snapshot():
        raise RuntimeError('kernel proof source changed during gate')
    return {'schema': 'zeno-fcis/lemma-kernel-evidence/1', 'status': 'passed',
        'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'working_tree_dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()),
        'source_sha256': before, 'toolchain': pin, 'verus_report': report,
        'translated_inventory': coverage, 'controls': controls, 'native_exit': native.returncode,
        'native_configuration_exceptions': configuration,
        'scope': 'validated finite tables; exact Reach/Cons/verdict; trace validity and witnesses',
        'trusted_base': ['specification and table abstraction', 'pinned Verus/vstd and Z3',
            'Rust compilation and erasure', 'slice/Vec/Option/checked_mul contracts',
            'allocator and host'],
        'unproved': ['shortest trace minimality', 'machine/table adapter', 'law evaluator and holds',
            'generic induction', 'unsigned and symbolic routes', 'JSON rendering',
            'source-to-binary correspondence', 'allocation failure and physical costs']}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache', type=Path, default=verifier.cache_directory())
    parser.add_argument('--install', action='store_true')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix='lemma-kernel-', dir=args.out.parent)).resolve()
    try:
        receipt = check(args.cache.resolve(), args.install, directory)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        receipt = {'schema': 'zeno-fcis/lemma-kernel-evidence/1', 'status': 'failed', 'error': str(error)}
    receipt['logs'] = str(directory)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
    print(f"lemma kernel: {receipt['status']}; receipt: {args.out}")
    if receipt['status'] != 'passed':
        print(receipt['error'])
        return 1
    print(f"{receipt['verus_report']['verification-results']['verified']} obligations; {len(receipt['controls'])} controls")
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
