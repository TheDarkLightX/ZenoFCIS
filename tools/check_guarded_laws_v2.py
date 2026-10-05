#!/usr/bin/env python3
"""Offline actual-source guarded law proof, raw coverage and independent oracles.

Run under the fleet heavy lock. Profiles are refreshed only explicitly, from a
successful whole-source proof. No installation, source-list edits or coverage
normalization is performed. Receipts never authorize execution or publication.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

import check_authority_v2 as authority
import check_laws_v2 as legacy
from v2_native_dependencies import native_dependency_args
import check_verus as verifier
from check_finite_execution import once
from verus_coverage import inventory, require_coverage

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path('verification/verus/guarded_laws.rs')
PROFILE = Path('verification/verus/guarded-laws.json')
TEST = Path('crates/zeno-fcis-synthesis/tests/v2_guarded_laws.rs')
LAW = legacy.SUBJECT
PREDICATE = legacy.DIRECTORY / 'predicate.rs'
SPEC = legacy.DIRECTORY / 'spec.rs'
METADATA = authority.METADATA
NAMESPACE = 'guarded_laws::'
ORDINAL_TEST = 'execution_v2::laws::tests::explicit_delivery_ordinals_preserve_gaps_nonzero_starts_and_maximum'


def sources():
    return list(dict.fromkeys([*authority.sources(), HARNESS, PROFILE, TEST,
        legacy.DIRECTORY / 'tests.rs', Path('tools/check_laws_v2.py'),
        Path('tools/check_guarded_laws_v2.py'), Path('tools/test_check_guarded_laws_v2.py'),
        Path('docs/V2_GUARDED_LAWS_STAGE.md'),
        Path('crates/zeno-fcis-cli/templates/inventory-reservation/synthesized/program.zcve')]))


def snapshot(skip_profile=False):
    paths = [p for p in sources() if not (skip_profile and p == PROFILE)]
    if any((ROOT / p).is_symlink() or not (ROOT / p).is_file() for p in paths):
        raise ValueError('missing or symlink guarded-law source')
    return {str(p): verifier.digest(ROOT / p) for p in paths}


def controls():
    """Retain every legacy runtime mutation; declare the ordinal native oracle."""
    targets = {
        'skip_applicable_law': 'applies', 'skip_mandatory_genesis': 'applies',
        'omit_required_initial_family': 'metadata', 'ignore_required_law': 'metadata',
        'allow_duplicate_law': 'metadata', 'change_declared_law_order': 'evaluate_into',
        'reset_shared_meter': 'evaluate_into', 'shrink_delivery_ordinals': 'deliveries_valid',
        'wrong_original_source': 'observe', 'wrong_complete_candidate': 'observe',
        'alter_class_observation': 'observe', 'alter_reason_observation': 'observe',
        'genesis_fixture_substitution': 'observe', 'wrong_outbox_payload': 'observe',
        'wrong_scalar_root_source': 'observe', 'root_read_alias': 'selector_id', 'law_effect_lane': 'effect_entry', 'alias_scalar_root_with_record_field': 'view_field',
        'false_success': 'evaluate', 'omit_step_charge': 'node', 'omit_read_charge': 'node',
        'signed_arithmetic_substitution': 'binary', 'wrong_division_rounding': 'divide',
    }
    coverage_targets = {
        'observe_before_read_permission': ('laws::predicate::node', 'body_sha256'),
        'weaken_entry_contract': ('laws::evaluate', 'ensures_sha256'),
        'narrow_entry_domain': ('laws::evaluate', 'requires_sha256'),
        'uncontracted_executable': ('laws::uncontracted_law_probe', 'inventory'),
        'changed_specification': ('laws::spec::magnitude', 'body_sha256'),
    }
    result = {}
    for name, (path, changed, kind) in legacy.mutation_sources().items():
        target, field = coverage_targets[name] if kind == 'coverage' else (targets[name], None)
        result[name] = dict(path=path, changed=changed, kind=kind, target=target, field=field, native=False)
    # A fixed independent semantic oracle, never a fallback after proof failure.
    # Historical ordinal SMT resource failures remain inconclusive evidence.
    result['shrink_delivery_ordinals'].update(kind='native', native=True)
    predicate = (ROOT / PREDICATE).read_text()
    metadata = (ROOT / METADATA).read_text()
    additions = [
        ('guard_direction', PREDICATE, 'Atom::Bool(false) => return Ok(default),\n                Atom::Bool(true) => observation,',
            'Atom::Bool(true) => return Ok(default),\n                Atom::Bool(false) => observation,', 'node', True),
        ('guard_nonprior_access', PREDICATE, 'if guard >= index {', 'if false {', 'node', False),
        ('guard_wrong_node', PREDICATE, 'match at(values, guard)? {', 'match at(values, 0)? {', 'node', True),
        ('false_guard_reads', PREDICATE, 'Atom::Bool(false) => return Ok(default),', 'Atom::Bool(false) => observation,', 'node', True),
        ('wrong_fallback_value', PREDICATE, 'Atom::Bool(false) => return Ok(default),', 'Atom::Bool(false) => return Ok(Atom::Bool(true)),', 'node', True),
        ('active_fallback_swallows_errors', PREDICATE, 'Atom::Bool(true) => observation,', 'Atom::Bool(true) => return Ok(default),', 'node', True),
        ('active_undefined_success', PREDICATE, 'None => Err(Failure::Undefined),', 'None => Ok(Atom::Bool(true)),', 'node', True),
        ('active_budget_success', PREDICATE, 'match permission {\n        Err(e) => Err(Failure::Budget(e)),', 'match permission {\n        Err(_) => Ok(Atom::Bool(true)),', 'node', True),
        ('denied_read_trace', PREDICATE, 'permitted: permission.is_ok(),', 'permitted: true,', 'node', True),
        ('invalid_default_text', PREDICATE, 'if bytes[i] >= 128 {', 'if false {', 'default_text_valid', True),
        ('skip_inactive_default_admission', PREDICATE, 'Atom::Text(bytes) => default_text_valid(bytes),', 'Atom::Text(_) => true,', 'shape', True),
        ('canonical_guarded_opcode', METADATA, 'parts.push(Part::Word(12));', 'parts.push(Part::Word(11));', 'law_op', True),
        ('canonical_guard_index', METADATA, 'parts.push(Part::Word(*guard as u128));', 'parts.push(Part::Word(0));', 'law_op', True),
        ('canonical_guarded_default', METADATA, 'law_atom(parts,default);', 'law_atom(parts,&laws::Atom::Bool(true));', 'law_op', True),
        ('canonical_default_type', METADATA, 'laws::Atom::Text(v)=>{parts.push(Part::Word(6));', 'laws::Atom::Text(v)=>{parts.push(Part::Word(5));', 'law_atom', True),
    ]
    for name, path, before, after, target, native in additions:
        text = predicate if path == PREDICATE else metadata
        result[name] = dict(path=path, changed=once(text, before, after), kind='proof',
            target=target, field=None, native=native)
    return result


def target_lines(source, name):
    """Exact annotated function region, for matching proof diagnostic source spans."""
    found = list(re.finditer(r'\bfn\s+([A-Za-z_][A-Za-z_0-9]*)\b', source))
    hits = [i for i, match in enumerate(found) if match[1] == name]
    if len(hits) != 1:
        raise ValueError(f'not one intended executable target: {name}')
    i = hits[0]
    start = source.rfind('#[cfg_attr(verus_keep_ghost', 0, found[i].start())
    end = found[i + 1].start() if i + 1 < len(found) else len(source)
    if i + 1 < len(found):
        attr = source.rfind('#[cfg_attr(verus_keep_ghost', found[i].end(), end)
        if attr >= 0:
            end = attr
    return (source.count('\n', 0, max(0, start)) + 1, source.count('\n', 0, end) + 1)


def semantic_refusal(proc, report, control):
    result = report.get('verification-results', {})
    lines = target_lines(control['changed'], control['target'])
    # Only actual proof failures count; compiler/VIR/resource failures do not.
    failure = bool(re.search(r'error: (?:postcondition not satisfied|assertion failed|invariant not satisfied|precondition not satisfied)', proc.stderr))
    locations = [int(n) for n in re.findall(re.escape(control['path'].name) + r':(\d+):\d+', proc.stderr)]
    matched = [n for n in locations if lines[0] <= n <= lines[1]]
    accepted = (proc.returncode != 0 and result.get('success') is False
        and type(result.get('errors')) is int and result['errors'] > 0
        and result.get('encountered-vir-error') is False and failure and bool(matched)
        and not re.search(r'resource limit|timed out|out of memory|error\[E\d+\]', proc.stderr, re.I))
    return accepted, {'file': str(control['path']), 'function': control['target'],
                      'lines': lines, 'matched_error_lines': matched}


def coverage_refusal(vir, profile, control):
    error = None
    try:
        require_coverage(vir, profile)
    except ValueError as failure:
        error = str(failure)
    actual = inventory(vir, profile['namespace'], tuple(profile['body_covered_functions']))
    target = NAMESPACE + 'execution_v2::' + control['target']
    field = control['field']
    if field == 'inventory':
        matched = target in actual and target not in profile['functions']
    else:
        matched = target in actual and target in profile['functions'] and actual[target][field] != profile['functions'][target][field]
        if control['target'] == 'laws::predicate::node':
            matched = matched and all(actual[target][key] == profile['functions'][target][key]
                for key in ('signature_sha256', 'requires_sha256', 'ensures_sha256'))
    matched = matched and error is not None and target in error
    return matched, error, {'function': target, 'field': field, 'intended_refusal': matched}


def native_ordinal_refusal(baseline, proc):
    failures = re.findall(r'^test (\S+) \.\.\. FAILED$', proc.stdout, re.M)
    intended = {'function': 'laws::frame::deliveries_valid', 'test': ORDINAL_TEST,
        'expected_actual': 'Err(Frame)', 'expected_required': 'Ok(())'}
    killed = (baseline.returncode == 0 and f'test {ORDINAL_TEST} ... ok' in baseline.stdout
        and proc.returncode == 101 and failures == [ORDINAL_TEST]
        and 'running 1 test' in proc.stdout and '0 passed; 1 failed; 0 ignored;' in proc.stdout
        and 'left: Err(Frame)' in proc.stdout and 'right: Ok(())' in proc.stdout
        and re.search(r'laws/tests\.rs:\d+:\d+', proc.stdout) is not None)
    return killed, intended


def native_commands(out, env, cwd, name, filtered=False, exact=None):
    rust = ['rustc', '+1.97.1', '--edition=2024', '--check-cfg', 'cfg(verus_keep_ghost)', '--check-cfg', 'cfg(test)']
    binary = out / 'native-current'
    native_env = {**env, 'CARGO_MANIFEST_DIR': str(cwd / 'crates/zeno-fcis-synthesis')}
    rust += native_dependency_args(ROOT, out / (name + '.deps'), env)
    build = saved_run([*rust, '--test', str(HARNESS), '-o', str(binary)], cwd, native_env, out, name + '.build', 240)
    verifier.require_success(build)
    command = [str(binary), '--test-threads=1']
    if exact:
        command.extend(['--exact', exact])
    elif filtered:
        command.append('guarded')
    return saved_run(command, cwd, native_env, out, name + '.native', 240)


def custody_checks(out, env):
    # Exact four external-consumer refusals retained from the original laws gate.
    specimens = {
        'private_meter': ('fn main(){let _=laws::execution_v2::meter::new;}', 'E0603'),
        'private_composition': ('fn main(){let _=laws::execution_v2::laws::evaluate_into;}', 'E0603'),
        'forge_success': ('fn main(){let _=laws::execution_v2::laws::Outcome{result:Ok(()),usage:panic!(),diagnostics:vec![],reads:vec![]};}', 'E0451'),
        'forge_usage': ('fn main(){let _=laws::execution_v2::Usage{counters:[0;8]};}', 'E0451'),
    }
    results = {}
    for name, (source, error) in specimens.items():
        path = out / (name + '.rs')
        path.write_text(source)
        proc = saved_run(['rustc', '+1.97.1', '--edition=2024', '--extern',
            'laws=' + str(out / 'libguarded_laws.rlib'), str(path), '-o', str(out / name)],
            ROOT, env, out, name, 240)
        if proc.returncode == 0 or ('error[' + error + ']') not in proc.stderr:
            raise ValueError(f'custody refusal missing intended diagnostic: {name}')
        results[name] = {'exit': proc.returncode, 'expected_error': error,
            'source_sha256': verifier.digest(path)}
    return results


def saved_run(command, cwd, env, out, name, timeout=1200):
    """Retain output even on timeout; timeout is evidence of an incomplete check."""
    print('Running ' + name, flush=True)
    try:
        proc = verifier.run(command, cwd, env, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout or b''
        stderr = error.stderr or b''
        (out / (name + '.stdout')).write_bytes(stdout if isinstance(stdout, bytes) else stdout.encode())
        (out / (name + '.stderr')).write_bytes(stderr if isinstance(stderr, bytes) else stderr.encode())
        (out / (name + '.command.json')).write_text(json.dumps({'command': command, 'cwd': str(cwd), 'timeout': timeout, 'exit': None}) + '\n')
        raise
    (out / (name + '.stdout')).write_text(proc.stdout)
    (out / (name + '.stderr')).write_text(proc.stderr)
    (out / (name + '.command.json')).write_text(json.dumps({'command': command, 'cwd': str(cwd), 'exit': proc.returncode}) + '\n')
    return proc


def check(out, refresh=False, positive_only=False, selected=None):
    out.mkdir(parents=True, exist_ok=True)
    generation = authority.check_generated_sources()
    initial = snapshot(True)
    pin = json.loads((ROOT / verifier.PIN).read_text())
    tool = Path.home() / '.cache/zeno-fcis/verus' / pin['version'] / 'qualified'
    verifier.verify_tool_files(tool, pin)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('VERUS_', 'VARGO_')) and k not in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')}
    env.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'], VERUS_Z3_PATH=str(tool / 'z3'),
        CARGO_BUILD_JOBS='1', CARGO_INCREMENTAL='0', RUST_TEST_THREADS='1', CARGO_NET_OFFLINE='true',
        CARGO_TARGET_DIR=os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))
    command = [str(tool / 'verus'), '--crate-type=lib', '--edition=2024', '--no-cheating',
        '--no-external-by-default', '--num-threads', '2', '--output-json', '--triggers-mode', 'silent',
        '-V', 'spinoff-all', '--log', 'vir', '--log', 'vir-option=no_span+no_type+no_fn_details']
    positive = saved_run([*command, '--log-dir', str(out / 'positive'), str(HARNESS)], ROOT, env, out, 'positive')
    verifier.require_success(positive)
    report = json.loads(positive.stdout)
    vr = report['verification-results']
    if not vr.get('is-verifying-entire-crate') or not vr.get('success') or vr.get('errors') != 0:
        raise ValueError('whole actual-source proof failed')
    vir = (out / 'positive/crate.vir').read_text()
    if refresh:
        raw = inventory(vir, NAMESPACE)
        bodies = [name for name, record in raw.items() if record['mode'] == 'Exec']
        profile = dict(namespace=NAMESPACE, functions=inventory(vir, NAMESPACE, tuple(bodies)),
            body_covered_functions=bodies, expected_verified=vr['verified'],
            target_functions=sorted(name for name in report['func-details'] if name.startswith(NAMESPACE)))
        (ROOT / PROFILE).write_text(json.dumps(profile, indent=2, sort_keys=True) + '\n')
    profile = json.loads((ROOT / PROFILE).read_text())
    coverage = require_coverage(vir, profile)
    if set(profile['body_covered_functions']) != {n for n, v in coverage.items() if v['mode'] == 'Exec'}:
        raise ValueError('every runtime body must be covered')
    if not verifier.accepted(report, {**pin, 'expected_verified': profile['expected_verified'], 'target_functions': profile['target_functions']}):
        raise ValueError('inexact pinned report')
    before = snapshot()
    native = native_commands(out, env, ROOT, 'positive')
    verifier.require_success(native)
    no_std = saved_run(['rustc', '+1.97.1', '--edition=2024', '--check-cfg', 'cfg(verus_keep_ghost)',
        '--check-cfg', 'cfg(test)', '--crate-type=rlib', str(HARNESS), '-o', str(out / 'libguarded_laws.rlib')], ROOT, env, out, 'no-std', 240)
    verifier.require_success(no_std)
    custody = custody_checks(out, env)
    results = []
    if not positive_only:
        mutations = controls()
        if selected:
            if set(selected) - set(mutations):
                raise ValueError('unknown mutation selection')
            mutations = {name: mutations[name] for name in selected}
        for name, control in mutations.items():
            specimen = out / 'specimens' / name
            if specimen.exists():
                raise ValueError('use a fresh evidence directory; never overwrite prior specimens')
            for path in sources():
                dest = specimen / path
                dest.parent.mkdir(parents=True, exist_ok=True)
                if path == control['path']:
                    dest.write_text(control['changed'])
                else:
                    shutil.copyfile(ROOT / path, dest)
            adjustments = authority.adjust_specimen_source_lengths(specimen)
            proc = None
            parsed = None
            n = None
            refusal = None
            if control['kind'] == 'native':
                n = native_commands(out, env, specimen, name, exact=ORDINAL_TEST)
                killed, intended = native_ordinal_refusal(native, n)
            else:
                proc = saved_run([*command, '--log-dir', str(out / name), str(HARNESS)], specimen, env, out, name)
                parsed = json.loads(proc.stdout)
                if control['kind'] == 'proof':
                    killed, intended = semantic_refusal(proc, parsed, control)
                else:
                    matched, refusal, intended = coverage_refusal((out / name / 'crate.vir').read_text(), profile, control)
                    r = parsed['verification-results']
                    killed = proc.returncode == 0 and r.get('success') is True and r.get('errors') == 0 and matched
                if control['native']:
                    n = native_commands(out, env, specimen, name, True)
                    killed = killed and n.returncode != 0 and 'test result: FAILED' in n.stdout
            failed_tests = [] if n is None else re.findall(r'^test (\S+) \.\.\. FAILED$', n.stdout, re.M)
            if n is not None:
                killed = killed and bool(failed_tests)
            row = {'name': name, 'expected': control['kind'], 'killed': killed,
                'proof_exit': None if proc is None else proc.returncode,
                'verification_results': None if parsed is None else parsed['verification-results'],
                'intended_target': intended, 'coverage_refusal': refusal,
                'source_array_length_adjustments': adjustments, 'native_exit': None if n is None else n.returncode,
                'native_failed_tests': failed_tests, 'mutated_path': str(control['path']),
                'mutated_sha256': verifier.digest(specimen / control['path']),
                'generated_sha256': verifier.digest(specimen / authority.EVALUATOR)}
            results.append(row)
            (out / 'control-progress.json').write_text(json.dumps({'source_sha256': before, 'controls': results}, indent=2, sort_keys=True) + '\n')
            if not killed:
                raise RuntimeError(f'control survived or unrelated failure: {name}; inspect retained target and logs')
            print(f'Caught {name} ({control["kind"]})', flush=True)
    if initial != snapshot(True) or before != snapshot():
        raise ValueError('source drift during guarded-law checks')
    verifier.verify_tool_files(tool, pin)
    return {'schema': 'zeno-fcis/guarded-laws-evidence/1', 'status': 'passed', 'source_sha256': before,
        'source_generation': generation, 'verus_report': report, 'translated_function_coverage': coverage,
        'proof_exit': positive.returncode, 'native_exit': native.returncode, 'no_std_exit': no_std.returncode,
        'custody_refusals': custody,
        'controls': results, 'all_controls': not positive_only and not selected,
        'selected_controls': selected, 'raw_coverage': True,
        'nonclaims': ['No template migration, V2 release or runtime authorization',
            'No physical cost or external shell proof; compiler/vstd/Verus/Z3/platform remain trusted']}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--refresh-coverage', action='store_true')
    p.add_argument('--positive-only', action='store_true')
    p.add_argument('--controls', nargs='+')
    args = p.parse_args()
    if args.positive_only and args.controls:
        p.error('positive-only cannot select controls')
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    try:
        receipt = check(out, args.refresh_coverage, args.positive_only, args.controls)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        receipt = {'schema': 'zeno-fcis/guarded-laws-evidence/1', 'status': 'failed', 'error': str(error)}
    (out / 'receipt.json').write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
    print(receipt['status'], flush=True)
    if receipt['status'] != 'passed':
        print(receipt['error'], flush=True)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
