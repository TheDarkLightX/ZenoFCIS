#!/usr/bin/env python3
"""Offline qualification of exact original-wire output from actual typed values.

Run heavy checks one at a time on a shared machine. This gate never
installs tools, refreshes coverage expectations, or grants runtime authority.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
sys.dont_write_bytecode = True
import tempfile

from v2_proof_sources import execution_sources, adjust_specimen_source_lengths
from v2_native_dependencies import native_dependency_args
import check_verus as verifier
from verus_coverage import require_coverage

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path('verification/verus/original_output.rs')
PROFILE = Path('verification/verus/original-output.json')
SUBJECT = Path('crates/zeno-fcis-synthesis/src/finite/canonical_v2/output.rs')
SPEC = SUBJECT.parent / 'output/spec.rs'
TESTS = SUBJECT.parent / 'output/tests.rs'
PUBLIC_TEST = Path('crates/zeno-fcis-synthesis/tests/v2_original_output.rs')
STAGE = Path('docs/V2_ORIGINAL_OUTPUT_STAGE.md')
FINITE = Path('crates/zeno-fcis-synthesis/src/finite')
UNIT_SOURCES = tuple(sorted({HARNESS, *(p.relative_to(ROOT) for folder in
    ['canonical_v2', 'evaluation', 'execution_v2'] for p in (ROOT / FINITE / folder).rglob('*.rs')),
    Path('crates/zeno-fcis-cli/templates/order-fulfillment/synthesized/transition.rs')}))

UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
ORACLE_SOURCES = tuple(sorted(p.relative_to(ROOT) for package in
    ['zeno-fcis-codec', 'zeno-fcis-value', 'zeno-fcis-core', 'zeno-fcis-collections', 'zeno-fcis-crypto']
    for p in (ROOT / 'crates' / package).rglob('*') if p.is_file()
    and (p.suffix == '.rs' or p.name == 'Cargo.toml')))
NATIVE_SOURCES = tuple(sorted(p.relative_to(ROOT) for p in
    (ROOT / 'crates/zeno-fcis-synthesis/src').rglob('*.rs')))
SOURCES = tuple(sorted(set((*UNIT_SOURCES, *ORACLE_SOURCES, *NATIVE_SOURCES, PROFILE, STAGE, PUBLIC_TEST, verifier.PIN,
    Path('tools/check_original_output_v2.py'), Path('tools/test_check_original_output_v2.py'),
    Path('tools/check_verus.py'), Path('tools/verus_coverage.py'), Path('Cargo.toml'),
    Path('Cargo.lock'), Path('rust-toolchain.toml'), Path('crates/zeno-fcis-synthesis/Cargo.toml')))))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_proof_sources.py"),
                                Path("tools/v2_native_dependencies.py"),
                                Path("verification/verus/authority_v2_sources.json"))))
TARGET = Path(os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))


def snapshot(root: Path = ROOT) -> dict[str, str]:
    result = {}
    for p in SOURCES:
        if (root / p).is_symlink():
            raise ValueError(f'symlink in source closure: {p}')
        result[str(p)] = verifier.digest(root / p)
    return result


def once(source: str, old: str, new: str) -> str:
    tokens = re.findall(r'\w+|[^\w\s]', old)
    matches = list(re.finditer(r'\s*'.join(re.escape(t) for t in tokens), source))
    if len(matches) != 1:
        raise ValueError(f'mutation anchor count {len(matches)}: {old}')
    m = matches[0]
    return source[:m.start()] + new + source[m.end():]


def replace_contract(source: str, function: str, replacement: str) -> str:
    end = source.index('pub fn ' + function)
    start = source.rindex('#[cfg_attr(verus_keep_ghost, verus_spec(result =>', 0, end)
    return source[:start] + replacement + source[end:]


def mutation_sources(source: str, specification: str) -> dict[str, tuple[Path, str, str]]:
    rows = {}
    for name, old, new, kind in (
        ('blob_u32_length', 'if bytes.len() > u32::MAX as usize', 'if false && bytes.len() > u32::MAX as usize', 'proof'),
        ('record_u32_count', 'if fields.len() > u32::MAX as usize', 'if false && fields.len() > u32::MAX as usize', 'proof'),
        ('envelope_u32_length', 'if payload.len() > u32::MAX as usize', 'if false && payload.len() > u32::MAX as usize', 'proof'),
        ('append_length_overflow', 'if out.len().checked_add(bytes.len()).is_none()', 'if false && out.len().checked_add(bytes.len()).is_none()', 'proof'),
        ('bool_tag', 'out.push(if v { 0x02 } else { 0x01 })', 'out.push(if v { 0x01 } else { 0x02 })', 'proof'),
        ('signed_tag', 'out.push(0x04)', 'out.push(0x03)', 'native'),
        ('unsigned_tag', 'out.push(0x03)', 'out.push(0x04)', 'native'),
        ('signed_min_conversion', 'be16(v as u128)', 'be16((v as u128) & (i128::MAX as u128))', 'proof'),
        ('integer_byte_order', '(value >> 120u32) as u8', 'value as u8', 'native'),
        ('enum_tag', 'out.push(0x07)', 'out.push(0x0a)', 'native'),
        ('enum_type_width', 'append(&mut out, &be4(type_id))?;\n            append(&mut out, &be2(variant))?;\n        }', 'append(&mut out, &be2(type_id as u16))?;\n            append(&mut out, &be2(variant))?;\n        }', 'native'),
        ('variant_byte_order', '[(value >> 8u32) as u8, value as u8]', '[value as u8, (value >> 8u32) as u8]', 'native'),
        ('sum_tag', 'out.push(0x0a)', 'out.push(0x07)', 'native'),
        ('sum_payload_flag', 'out.push(0);', 'out.push(1);', 'native'),
        ('blob_text_tags', '{ 0x06 } else { 0x05 }', '{ 0x05 } else { 0x06 }', 'native'),
        ('ascii_high_bit', 'if bytes[i] > 127', 'if bytes[i] > 254', 'native'),
        ('blob_length', 'be4(bytes.len() as u32)', 'be4(0)', 'native'),
        ('atom_exact_cap', 'if size > max_bytes { return Err(Failure::Limit); }\n    let mut out = Vec::new();\n    match value', 'if size >= max_bytes { return Err(Failure::Limit); }\n    let mut out = Vec::new();\n    match value', 'native'),
        ('record_tag', 'out.push(0x09)', 'out.push(0x08)', 'native'),
        ('record_count', 'be4(fields.len() as u32)', 'be4(0)', 'native'),
        ('duplicate_fields', 'fields[i - 1].id >= fields[i].id', 'fields[i - 1].id > fields[i].id', 'proof'),
        ('field_id', 'be2(fields[i].id)', 'be2(i as u16)', 'native'),
        ('record_header_cap', 'if max_bytes < 5', 'if max_bytes <= 5', 'native'),
        ('record_field_cap', 'max_bytes - out.len() < 2', 'max_bytes - out.len() <= 2', 'native'),
        ('envelope_magic', '[90u8, 70u8, 67u8, 73u8, 83u8, 86u8, 49u8, 0u8]', '[90u8, 70u8, 67u8, 73u8, 83u8, 86u8, 50u8, 0u8]', 'native'),
        ('envelope_root', 'be4(root)', 'be4(0)', 'native'),
        ('envelope_schema', 'append(&mut out, schema)?', 'append(&mut out, &[0; 32])?', 'native'),
        ('envelope_length', 'be4(payload.len() as u32)', 'be4(0)', 'native'),
        ('envelope_payload', 'append(&mut out, payload)?', 'append(&mut out, &[])?', 'native'),
        ('envelope_header_cap', 'payload.len().checked_add(48)', 'payload.len().checked_add(47)', 'native'),
    ):
        rows[name] = (SUBJECT, once(source, old, new), kind)
    rows['omit_unused_record_contract'] = (SUBJECT, replace_contract(source, 'encode_record', ''), 'coverage')
    rows['weaken_unused_envelope_contract'] = (SUBJECT, replace_contract(source, 'encode_envelope',
        '#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n'), 'coverage')
    rows['equivalent_spec_body'] = (SPEC, once(specification,
        'else { Ok((b@.len() + 5) as usize) }', 'else { Ok((b@.len() + 4 + 1) as usize) }'), 'coverage')
    rows['uncontracted_inventory'] = (SUBJECT, source + '\npub fn uncontracted_output_probe(n: usize) -> usize { n }\n', 'coverage')
    return rows


def run(command: list[str], cwd: Path, env: dict, directory: Path, name: str,
        timeout: int = 600) -> subprocess.CompletedProcess:
    directory.mkdir(parents=True, exist_ok=True)
    (directory / (name + '.command.json')).write_text(json.dumps(command, indent=2) + '\n')
    context = {'cwd':str(cwd), 'environment':{k:env[k] for k in
        ('RUSTUP_TOOLCHAIN', 'VERUS_Z3_PATH', 'CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS',
         'RUST_TEST_THREADS', 'CARGO_NET_OFFLINE', 'CARGO_INCREMENTAL') if k in env},
        'removed_environment':['VERUS_*', 'VARGO_*', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']}
    (directory / (name + '.context.json')).write_text(json.dumps(context, indent=2) + '\n')
    try:
        result = subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as e:
        (directory / (name + '.stdout')).write_bytes(e.stdout or b'')
        (directory / (name + '.stderr')).write_bytes(e.stderr or b'')
        (directory / (name + '.exit.json')).write_text(json.dumps({'exit_code': None, 'timeout': timeout}) + '\n')
        raise
    (directory / (name + '.stdout')).write_text(result.stdout)
    (directory / (name + '.stderr')).write_text(result.stderr)
    (directory / (name + '.exit.json')).write_text(json.dumps({'exit_code': result.returncode}) + '\n')
    return result


def native_build_command(root: Path, binary: Path, libraries: dict) -> list[str]:
    result = ['rustc', '+1.97.1', '--edition=2024', '--test', '--check-cfg', 'cfg(verus_keep_ghost)',
              '--check-cfg', 'cfg(test)', '-L', 'dependency=' + str(TARGET / 'debug/deps')]
    for name, path in sorted(libraries.items()):
        result += ['--extern', name + '=' + path]
    return [*result, str(root / HARNESS), '-o', str(binary)]


def parsed_report(result: subprocess.CompletedProcess) -> dict:
    try:
        return json.loads(result.stdout)
    except ValueError:
        return {}


def check(directory: Path, positive_only: bool = False) -> dict:
    before = snapshot()
    source = directory / 'source'
    for p in SOURCES:
        target = source / p
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((ROOT / p).read_bytes())
    (directory / 'source_sha256.json').write_text(json.dumps(before, indent=2, sort_keys=True) + '\n')
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    tools = verifier.prepare_tools(Path.home() / '.cache/zeno-fcis/verus', pin, False)
    env = {k:v for k,v in os.environ.items() if not k.startswith(('VERUS_', 'VARGO_'))
           and k not in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')}
    env.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'], VERUS_Z3_PATH=str(tools / 'z3'),
               CARGO_TARGET_DIR=str(TARGET), CARGO_BUILD_JOBS='1', RUST_TEST_THREADS='1',
               CARGO_NET_OFFLINE='true', CARGO_INCREMENTAL='0')
    command = [str(tools / 'verus'), '--crate-type=lib', '--edition=2024', '--no-cheating',
               '--no-external-by-default', '--num-threads', '2', '-V', 'spinoff-all', '--output-json', '--log', 'vir',
               '--log', 'vir-option=no_span+no_type+no_fn_details']
    proof = run([*command, '--log-dir', str(directory / 'positive/logs'), str(source / HARNESS)],
                source, env, directory / 'positive', 'verus')
    verifier.require_success(proof)
    report = parsed_report(proof)
    if not verifier.accepted(report, {**pin, 'expected_verified':profile['expected_verified'], 'target_functions':profile['target_functions']}):
        raise ValueError('positive proof lacks exact whole-harness or toolchain qualification')
    coverage = require_coverage((directory / 'positive/logs/crate.vir').read_text(), profile)
    print('Original output: whole-harness proof and strict translated inventory passed', flush=True)
    native_dependency_args(ROOT, directory / 'native', env)
    dependency_receipt = json.loads((directory / 'native/native-dependencies/result.json').read_text())
    libraries = {name: record['path'] for name, record in dependency_receipt['libraries'].items()}
    native_command = native_build_command(source, directory / 'native/tests', libraries)
    verifier.require_success(run(native_command, source, env, directory / 'native', 'build'))
    native = run([str(directory / 'native/tests'), 'output::tests', '--test-threads=1'], source, env, directory / 'native', 'tests')
    verifier.require_success(native)
    if '6 passed; 0 failed' not in native.stdout:
        raise ValueError('native output oracle corpus did not run')
    no_std = run(['rustc', '+1.97.1', '--edition=2024', '--crate-type=lib', '--emit=metadata',
        '--check-cfg', 'cfg(verus_keep_ghost)', '--check-cfg', 'cfg(test)', str(source / HARNESS), '-o', str(directory / 'native/no-std.rmeta')],
        source, env, directory / 'native', 'no-std')
    verifier.require_success(no_std)
    public_test = run(['cargo', '+1.97.1', 'test', '--locked', '--offline', '-p', 'zeno-fcis-synthesis',
        '--test', 'v2_original_output', '--', '--test-threads=1'], ROOT, env, directory / 'native', 'public-test')
    verifier.require_success(public_test)
    if '1 passed; 0 failed' not in public_test.stdout:
        raise ValueError('registered public original-output test did not run')
    cargo_no_std = run(['cargo', '+1.97.1', 'check', '--locked', '--offline', '-p', 'zeno-fcis-synthesis',
        '--lib', '--no-default-features'], ROOT, env, directory / 'native', 'cargo-no-std')
    verifier.require_success(cargo_no_std)
    clippy = run(['cargo', '+1.97.1', 'clippy', '--locked', '--offline', '-p', 'zeno-fcis-synthesis',
        '--test', 'v2_original_output', '--', '-D', 'warnings'],
        ROOT, env, directory / 'native', 'clippy')
    # Preserve a failed inherited Clippy check distinctly from this proof and
    # oracle qualification. An owned-output diagnostic is a local blocker.
    if clippy.returncode and ('canonical_v2/output' in clippy.stderr or 'tests/v2_original_output.rs' in clippy.stderr):
        raise ValueError('strict Clippy reports an owned original-output diagnostic')
    mutations = []
    if not positive_only:
        for name, (path, changed, kind) in mutation_sources((source / SUBJECT).read_text(), (source / SPEC).read_text()).items():
            specimen = directory / 'mutations' / name
            for unit in UNIT_SOURCES:
                dest = specimen / unit
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(changed.encode() if unit == path else (source / unit).read_bytes())
            adjust_specimen_source_lengths(specimen)
            refusal = None
            if kind == 'native':
                binary = specimen / 'native-tests'
                verifier.require_success(run(native_build_command(specimen, binary, libraries), specimen, env, specimen, 'build'))
                result = run([str(binary), 'output::tests', '--test-threads=1'], specimen, env, specimen, 'native')
                killed = result.returncode == 101 and 'test result: FAILED' in result.stdout and 'assertion' in result.stdout
                detail = {'native_stdout': str(specimen / 'native.stdout')}
            else:
                result = run([*command, '--log-dir', str(specimen / 'logs'), str(specimen / HARNESS)], specimen, env, specimen, 'verus')
                parsed = parsed_report(result)
                detail = parsed.get('verification-results', {})
                if kind == 'proof':
                    errors = re.findall(r'(?m)^error:.*(?:\n(?!error:|note:).*)*', result.stderr)
                    killed = (result.returncode != 0 and detail.get('errors', 0) > 0
                        and detail.get('encountered-vir-error') is False
                        and not re.search(r'(?i)resource limit|timed out|timeout', result.stderr)
                        and any('canonical_v2/output.rs' in e for e in errors))
                else:
                    if result.returncode == 0:
                        try: require_coverage((specimen / 'logs/crate.vir').read_text(), profile)
                        except ValueError as e: refusal = str(e)
                    killed = result.returncode == 0 and detail.get('success') is True and detail.get('errors') == 0 and refusal is not None
            row = {'name':name, 'kind':kind, 'killed':bool(killed), 'exit_code':result.returncode,
                   'changed_source':str(path), 'changed_sha256':verifier.digest(specimen / path),
                   'coverage_refusal':refusal, 'result':detail, 'logs':str(specimen)}
            mutations.append(row)
            (directory / 'mutations.json').write_text(json.dumps(mutations, indent=2, sort_keys=True) + '\n')
            print(f'Original output: {name}: {"caught" if killed else "FAILED"} ({kind})', flush=True)
            if not killed: raise ValueError(f'control {name} survived or failed for an unrelated reason')
    verifier.verify_tool_files(tools, pin)
    if snapshot() != before: raise ValueError('source changed during qualification')
    return {'schema':'zeno-fcis/original-output-evidence/1', 'status':'positive_only' if positive_only else 'passed',
            'source_sha256':before, 'toolchain':pin, 'verus_report':report,
            'translated_function_coverage':coverage, 'coverage_modes':dict(collections.Counter(x['mode'] for x in coverage.values())),
            'native':{'exit_code':native.returncode,'output':native.stdout,'oracle_artifacts':libraries,
                      'oracle_artifact_sha256':{k:verifier.digest(Path(p)) for k,p in libraries.items()}},
            'no_std_exit_code':no_std.returncode,
            'public_test_exit_code':public_test.returncode, 'cargo_no_std_exit_code':cargo_no_std.returncode,
            'strict_clippy':{'exit_code':clippy.returncode, 'status':'passed' if clippy.returncode == 0 else 'failed_inherited_source',
                             'stderr':str(directory / 'native/clippy.stderr')},
            'scope':'whole-harness original-output proof, strict coverage and independent native/control qualification; inherited Clippy status is separate',
            'mutations':mutations,
            'trusted_base':['reviewed wire specification and source-coverage manifest', 'pinned Verus/vstd/Z3',
                            'Rust compilers and standard allocation library', 'host platform'],
            'unproved':['legacy Value/codec implementation', 'schema hashing', 'logical/physical serialization cost',
                        'mandatory shell authority, candidate-to-output connection and root integration',
                        'external delivery, compiler and binary correspondence']}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--positive-only', action='store_true', help='development only; cannot produce passed qualification')
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix='original-output-run-', dir=args.out.resolve().parent))
    try:
        receipt = check(directory, args.positive_only)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as e:
        receipt = {'schema':'zeno-fcis/original-output-evidence/1', 'status':'failed', 'error':str(e)}
    receipt['logs'] = str(directory)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
    print(f'Original output: {receipt["status"]}; receipt {args.out}')
    if receipt['status'] == 'failed': print(receipt['error'])
    return 1 if receipt['status'] == 'failed' else 0

if __name__ == '__main__':
    raise SystemExit(main())
