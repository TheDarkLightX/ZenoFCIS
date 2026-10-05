#!/usr/bin/env python3
"""Offline exact-source composition qualification; no catalog or shell authority."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from v2_proof_sources import execution_sources, adjust_specimen_source_lengths
from v2_native_dependencies import native_dependency_args
import check_verus as verifier
from verus_coverage import inventory, require_coverage

ROOT = Path(__file__).resolve().parents[1]
FINITE = Path('crates/zeno-fcis-synthesis/src/finite')
OWNED = FINITE / 'execution_v2/composition'
HARNESS = Path('verification/verus/composition.rs')
PROFILE = Path('verification/verus/composition.json')
TEST = Path('crates/zeno-fcis-synthesis/tests/v2_composition.rs')
CARGO_TARGET = os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target'))
TEMPLATES = Path('crates/zeno-fcis-cli/templates')
FIXED = (HARNESS, PROFILE, verifier.PIN, TEST, Path('verification/verus/record-stage-migration.json'), Path('verification/verus/account-stage-migration.json'),
         Path('tools/check_composition_v2.py'), Path('tools/test_check_composition_v2.py'),
         Path('tools/check_verus.py'), Path('tools/verus_coverage.py'),
         Path('docs/V2_COMPOSITION_STAGE.md'), Path('Cargo.toml'), Path('Cargo.lock'))


def sources(root: Path = ROOT) -> tuple[Path, ...]:
    # Include the full imported tree, native dependency crates, integration inputs,
    # registrations and tool/profile bytes. New files cannot silently escape it.
    paths = set(FIXED)
    for crate in ('zeno-fcis-synthesis', 'zeno-fcis-core', 'zeno-fcis-codec',
                  'zeno-fcis-crypto', 'zeno-fcis-value'):
        base = Path('crates') / crate
        paths.add(base / 'Cargo.toml')
        paths.update(p.relative_to(root) for p in (root/base/'src').rglob('*.rs'))
    for template in ('inventory-reservation', 'order-fulfillment', 'account-lockout', 'durable-counter'):
        paths.update(p.relative_to(root) for p in (root/TEMPLATES/template).rglob('*')
                     if p.is_file() and (p.suffix in ('.rs', '.zcve', '.zeno', '.json', '.txt', '.md')))
    if (root/'.cargo').exists():
        paths.update(p.relative_to(root) for p in (root/'.cargo').rglob('*') if p.is_file())
    paths.update(execution_sources(root))
    paths.update((Path("tools/v2_proof_sources.py"), Path("verification/verus/authority_v2_sources.json")))
    paths.add(Path("tools/v2_native_dependencies.py"))
    return tuple(sorted(paths))


def snapshot(root: Path = ROOT) -> dict[str, str]:
    result = {}
    for path in sources(root):
        if any(p.is_symlink() for p in (root/path, *(root/path).parents)):
            raise RuntimeError(f'symlink in qualification source: {path}')
        result[str(path)] = verifier.digest(root/path)
    return result


def coverage(vir: str, profile: dict) -> dict:
    observed = require_coverage(vir, profile)
    executable = {name for name, item in observed.items() if item['mode'] == 'Exec'}
    if set(profile.get('body_covered_functions', [])) != executable:
        raise ValueError('every executable body must be fingerprinted')
    return observed


def once(source: str, before: str, after: str) -> str:
    tokens = re.findall(r'\w+|[^\w\s]', before)
    pattern = r'\s*'.join(re.escape(token) for token in tokens)
    matches = list(re.finditer(pattern, source))
    if len(matches) != 1:
        raise ValueError(f'expected one mutation anchor, got {len(matches)}: {before}')
    found = matches[0]
    return source[:found.start()] + after + source[found.end():]


def mutations(root: Path = ROOT) -> dict[str, tuple[Path, str, str, bool]]:
    rows = {}
    def add(name, file, before, after, expected='proof', native=True):
        path = OWNED/file
        rows[name] = (path, once((root/path).read_text(), before, after), expected, native)
    add('sum_type_identity', 'ingress.rs', 'Some(Atom::Sum { type_id: *type_id, variant: inverse(variants, code)?, })', 'Some(Atom::Sum { type_id: 0, variant: inverse(variants, code)?, })')
    add('command_binding_source', 'producer.rs', 'Source::Command => scalar(d.command, command, b.selector)', 'Source::Command => scalar(d.command, state, b.selector)')
    add('actual_branch_output', 'producer.rs', 'inputs, atoms, code, d.branches, meter, &mut trace.attempts,', 'inputs, atoms, 0, d.branches, meter, &mut trace.attempts,')
    # S4 removed law_bridge: law-frame controls (root_read_alias, law_effect_lane) moved to the laws gate;
    # delivery destination/payload/idempotency/ordinal copies no longer exist.
    add('law_decision_usage', 'outcome.rs', 'attempts: &trace.attempts, usage: meter.used, };', 'attempts: &trace.attempts, usage: trace.ingress.unwrap_or(meter.used), };')
    add('bypass_applicable_law', 'outcome.rs', 'match result { Ok(()) => Ok(produced.candidate), Err(e) => Err(Failure::Law(e)), }', 'Ok(produced.candidate)')
    add('invented_genesis', 'outcome.rs', 'initial: producer::value(&decoded.value),', 'initial: decision::RootView::Record(&[]),')
    add('header_charge_omission', 'framed.rs', 'meter.charge(Resource::Byte, header)', 'meter.charge(Resource::Byte, 0)')
    add('all_law_literal_admission', 'admission.rs', '&& schema_validation::law_literals(d.laws)', '&& true')
    add('binary_map_boundary', 'admission.rs', '*min == 0 && *max == 1', '*min == 0 || *max == 1')
    contract = '#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.descriptor_view(),descriptor_admitted(result),))]'
    add('weaken_descriptor_contract', 'outcome.rs', contract, '#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures true,))]', 'coverage', False)
    add('narrow_descriptor_domain', 'outcome.rs', contract, '#[cfg_attr(verus_keep_ghost,verus_spec(result=>requires false,ensures result==self.descriptor_view(),descriptor_admitted(result),))]', 'coverage', False)
    path = OWNED/'outcome.rs'
    rows['uncontracted_helper'] = (path, (root/path).read_text()+'\npub fn uncontracted_composition_control() -> u64 { 42 }\n', 'coverage', False)
    path = OWNED.with_suffix('.rs')
    rows['specification_body'] = (path, once((root/path).read_text(), '(r.state@,r.command@,r.context@)', '{let original=(r.state@,r.command@,r.context@);original}'), 'coverage', False)
    path = OWNED/'framed.rs'
    source = once((root/path).read_text(), 'if let Err(e) = meter.charge(Resource::Byte, header) {', 'let interpreted = envelope::frame(bytes, binding.root, &binding.schema, binding.max_bytes);\n    if let Err(e) = meter.charge(Resource::Byte, header) {')
    source = once(source, 'match envelope::frame(bytes, binding.root, &binding.schema, binding.max_bytes) {', 'match interpreted {')
    rows['interpret_before_header_charge'] = (path, source, 'coverage', False)
    # Former raw-record driver controls now target the surviving shared stages.
    add('record_state_source', 'producer.rs', 'Source::State => scalar(d.state, state, b.selector)', 'Source::State => scalar(d.state, command, b.selector)', native=False)
    add('record_source_schema', 'admission.rs', 'Source::State => d.state,', 'Source::State => d.command,', native=False)
    add('record_domain_upper', 'admission.rs', 'a == c && b == d', 'a == c', native=False)
    add('record_domain_kind', 'admission.rs', 'b: ScalarDomain) -> bool { match (a, b) {', 'b: ScalarDomain) -> bool { if matches!(a, ScalarDomain::Bool) { return true; } match (a, b) {', native=False)
    add('record_scalar_position', 'producer.rs', 'Some(codes[i])', 'Some(codes[0])', native=False)
    add('record_unknown_binding', 'admission.rs', 'find(schema(d, b.source), b.selector)', 'find(schema(d, b.source), Selector::Field(7))', native=False)
    add('record_duplicate_binding', 'admission.rs', 'if same_binding(d.bindings[j], b)', 'if false', native=False)
    add('record_context_coverage', 'admission.rs', ' + count(d.context) as u128', '', native=False)
    add('record_context_schema', 'admission.rs', '&& ingress::valid(d.context)', '&& true', native=False)
    add('record_command_refusal', 'producer.rs', 'Err(e) => return Err(Failure::Ingress(1, e)),', 'Err(e) => return Err(Failure::Ingress(0, e)),', native=False)
    add('record_context_bytes', 'producer.rs', 'ingress::project(raw.context, d.context, 2, meter, &mut trace.reads)', 'ingress::project(raw.command, d.context, 2, meter, &mut trace.reads)', native=False)
    add('record_meter_prefix', 'producer.rs', 'let mut scratch = Vec::new();', 'meter.used.counters = [0; 8]; let mut scratch = Vec::new();', native=False)
    add('record_read_prefix', 'producer.rs', 'let state = match ingress::project', 'trace.reads.clear(); let state = match ingress::project', native=False)
    add('record_refusal_usage', 'producer.rs', 'return Err(Failure::Execution(e));', 'meter.used.counters = [0; 8]; return Err(Failure::Execution(e));', native=False)
    add('record_binding_order', 'producer.rs', 'let b = d.bindings[i];', 'let b = d.bindings[0];', native=False)
    path = OWNED/'producer.rs'
    source = (root/path).read_text()
    context = 'let context = match ingress::project(raw.context, d.context, 2, meter, &mut trace.reads) { Ok(v) => v, Err(e) => return Err(Failure::Ingress(2, e)), };'
    rows['record_skip_context'] = (path, once(source, context, 'let context = ingress::Decoded { value: ingress::Value::Record(Vec::new()), scalars: Vec::new() };'), 'proof', False)
    # record_before_admission retired in S5: produce takes the admitted BoundCore,
    # so input work before admission is not expressible.
    path = OWNED/'outcome.rs'
    source = (root/path).read_text()
    end = source.index("    pub fn execute<'a>")
    begin = source.rfind('    #[cfg_attr', 0, end)
    contract = source[begin:end]
    rows['record_execution_contract'] = (path, source[:begin]+'    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures true,))]\n'+source[end:], 'coverage', False)
    rows['record_execution_domain'] = (path, source[:begin]+contract.replace('result=>ensures', 'result=>requires false,ensures', 1)+source[end:], 'coverage', False)
    return rows


def recorded(command, root, env, directory, name, timeout=600):
    result = verifier.run(command, root, env, timeout=timeout)
    (directory/f'{name}.stdout').write_text(result.stdout)
    (directory/f'{name}.stderr').write_text(result.stderr)
    receipt = dict(command=command, cwd=str(root), exit_code=result.returncode,
                   stdout_sha256=verifier.digest(directory/f'{name}.stdout'),
                   stderr_sha256=verifier.digest(directory/f'{name}.stderr'))
    (directory/f'{name}.json').write_text(json.dumps(receipt, indent=2)+'\n')
    return result


def proof_killed(result, report, path, pin):
    """A semantic failure in the changed source, never timeout/unknown/type failure."""
    if not isinstance(report, dict):
        return False
    parsed, version = report.get('verification-results'), report.get('verus')
    if not isinstance(parsed, dict) or not isinstance(version, dict):
        return False
    semantic = re.search(r'^error: (postcondition not satisfied|assertion failed|invariant not satisfied)',
                         result.stderr, re.MULTILINE)
    inconclusive = re.search(r'resource limit|rlimit|timed out|timeout|unknown|internal error|unsupported',
                             result.stderr, re.IGNORECASE)
    return (result.returncode != 0 and parsed.get('success') is False
            and type(parsed.get('errors')) is int and parsed['errors'] > 0
            and parsed.get('encountered-vir-error') is False
            and parsed.get('is-verifying-entire-crate') is True
            and version.get('version') == pin['version'] and version.get('commit') == pin['commit']
            and semantic is not None and inconclusive is None and path.name in result.stderr)


def native(root, env, directory, filtered=False):
    binary = directory/'native-tests'
    dependencies = native_dependency_args(ROOT, directory, env)
    build = recorded(['rustc', '+1.97.1', '--edition=2024', '--test', '--check-cfg',
                      'cfg(verus_keep_ghost)', '--check-cfg', 'cfg(test)', str(HARNESS),
                      '-o', str(binary), *dependencies], root, env, directory, 'native-build')
    verifier.require_success(build)
    command = [str(binary), '--test-threads=1']
    if filtered:
        command += ['execution_v2::composition::tests']
    return recorded(command, root, env, directory, 'native')


def opacity(root, env, directory):
    library = directory/'libcomposition.rlib'
    build = recorded(['rustc', '+1.97.1', '--edition=2024', '--crate-type=rlib', '--crate-name=composition',
                      '--check-cfg', 'cfg(verus_keep_ghost)', '--check-cfg', 'cfg(test)', str(HARNESS),
                      '-o', str(library)], root, env, directory, 'opaque-library')
    verifier.require_success(build)
    cases = {
        'public_read': ('pub fn inspect(o: &c::Outcome) { let _ = (o.raw(), o.usage(), o.result()); }', None),
        'construct_core': ('pub fn invent<\'a>(d: &\'a c::Descriptor<\'a>) -> c::BoundCore<\'a> { c::BoundCore { descriptor: d } }', 'E0451'),
        'forge_report': ('pub fn forge(o: &mut c::Outcome) { o.usage = o.usage(); }', 'E0616'),
        'mutate_candidate': ('pub fn alter(c: &mut c::Candidate) { c.class = c::Class::Accept; }', 'E0616'),
        'supplied_producer': ('pub fn bypass() { let _ = c::producer::produce; }', 'E0603'),
        'mutable_descriptor': ('pub fn alter(c: &c::BoundCore) { c.descriptor().decision_output = 0; }', 'E0594'),
    }
    results = []
    for name, (source, error) in cases.items():
        path = directory/f'{name}.rs'
        path.write_text('extern crate composition as z;\nuse z::execution_v2::composition as c;\n'+source+'\n')
        result = recorded(['rustc', '+1.97.1', '--edition=2024', '--crate-type=lib', '--error-format=json',
                           '--extern', f'composition={library}', str(path), '-o', str(directory/f'{name}.rlib')],
                          root, env, directory, name)
        codes = [item.get('code', {}).get('code') for line in result.stderr.splitlines()
                 if (item := json.loads(line)).get('code') is not None]
        passed = result.returncode == 0 if error is None else result.returncode != 0 and error in codes
        results.append(dict(name=name, passed=passed, expected_code=error, actual_codes=codes))
        if not passed:
            raise RuntimeError(f'opacity control failed: {results[-1]}')
    return results


def check(out: Path, positive_only=False):
    directory = out.parent
    directory.mkdir(parents=True, exist_ok=False)
    before = snapshot()
    (directory/'source-before.json').write_text(json.dumps(before, sort_keys=True, indent=2)+'\n')
    pin = json.loads((ROOT/verifier.PIN).read_text())
    profile = json.loads((ROOT/PROFILE).read_text())
    report_pin = {**pin, 'expected_verified': profile['expected_verified'], 'target_functions': profile['target_functions']}
    tool = verifier.prepare_tools(Path.home()/'.cache/zeno-fcis/verus', pin, False)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('VERUS_', 'VARGO_'))
           and k not in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTDOC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTC_BOOTSTRAP')}
    env.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'], VERUS_Z3_PATH=str(tool/'z3'),
               CARGO_TARGET_DIR=CARGO_TARGET, CARGO_NET_OFFLINE='true', CARGO_BUILD_JOBS='1', RUST_TEST_THREADS='1')
    command = [str(tool/'verus'), '--crate-type=lib', '--edition=2024', '--no-cheating',
               '--no-external-by-default', '--num-threads', '2', "-V", "spinoff-all", '--output-json', '--triggers-mode', 'silent',
               '--log', 'vir', '--log', 'vir-option=no_span+no_type+no_fn_details']
    positive = recorded([*command, '--log-dir', str(directory/'positive'), str(HARNESS)], ROOT, env, directory, 'proof')
    verifier.require_success(positive)
    report = json.loads(positive.stdout)
    if not verifier.accepted(report, report_pin):
        raise RuntimeError('unaccepted whole-crate proof, pinned tool, or proof inventory')
    observed = coverage((directory/'positive/crate.vir').read_text(), profile)
    (directory/'coverage.json').write_text(json.dumps(observed, sort_keys=True, indent=2)+'\n')
    verifier.require_success(native(ROOT, env, directory))
    opacity_results = opacity(ROOT, env, directory)
    (directory/'opacity.json').write_text(json.dumps(opacity_results, indent=2)+'\n')
    for name, args in (
        ('integration', ['test', '-p', 'zeno-fcis-synthesis', '--test', 'v2_composition']),
        ('no-std', ['check', '-p', 'zeno-fcis-synthesis', '--no-default-features']),
        ('clippy', ['clippy', '-p', 'zeno-fcis-synthesis', '--lib', '--', '-D', 'warnings']),
    ):
        verifier.require_success(recorded(['cargo', '+1.97.1', args[0], '--locked', '--offline', *args[1:]], ROOT, env, directory, name))
    results = []
    print('Positive proof, full body coverage, native, opacity and pinned crate checks passed', flush=True)
    if not positive_only:
        for name, (path, changed, expected, native_control) in mutations().items():
            control = directory/name
            control.mkdir()
            (control/'mutation.rs').write_text(changed)
            with tempfile.TemporaryDirectory(prefix='composition-control-', dir=directory) as temporary:
                specimen = Path(temporary)
                for source in before:
                    target = specimen/source
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(ROOT/source, target)
                (specimen/path).write_text(changed)
                adjust_specimen_source_lengths(specimen)
                proof = recorded([*command, '--log-dir', str(control/'vir'), str(HARNESS)], specimen, env, control, 'proof')
                report_control = json.loads(proof.stdout)
                parsed = report_control.get('verification-results', {})
                refusal = None
                if expected == 'proof':
                    caught = proof_killed(proof, report_control, path, pin)
                else:
                    try:
                        coverage((control/'vir/crate.vir').read_text(), profile)
                    except ValueError as error:
                        refusal = str(error)
                    caught = (proof.returncode == 0 and parsed.get('success') is True
                              and parsed.get('errors') == 0 and refusal is not None
                              and parsed.get('encountered-vir-error') is False
                              and parsed.get('is-verifying-entire-crate') is True
                              and report_control.get('verus', {}).get('version') == pin['version']
                              and report_control.get('verus', {}).get('commit') == pin['commit'])
                native_status = None
                if native_control:
                    result = native(specimen, env, control, True)
                    native_status = result.returncode
                    caught = caught and result.returncode != 0 and 'test result: FAILED' in result.stdout
                item = dict(name=name, path=str(path), before_sha256=before[str(path)],
                            after_sha256=verifier.digest(control/'mutation.rs'), expected=expected,
                            killed=caught, proof_exit_code=proof.returncode, verification_results=parsed,
                            coverage_refusal=refusal, native_exit_code=native_status)
                results.append(item)
                (control/'result.json').write_text(json.dumps(item, indent=2)+'\n')
                if not caught:
                    raise RuntimeError(f'control survived or failed for an unrelated reason: {item}')
                print(f'Caught {name}: {expected}; native={native_status}', flush=True)
    if snapshot() != before:
        raise RuntimeError('source drift during qualification')
    verifier.verify_tool_files(tool, pin)
    return dict(schema='zeno-fcis/composition-v2-evidence/1', status='passed',
                revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                source_sha256=before, source_unchanged=True, verus_report=report,
                translated_function_coverage=observed, controls=results, opacity=opacity_results,
                positive_only=positive_only, complete_controls=not positive_only,
                unproved=['catalog extraction and law lowering', 'original canonical identity and policy correspondence',
                          'authority/sealing/recompute/persisted replay', 'ledger and shell effects',
                          'compiler/allocator/platform assumptions', 'global acceptance/Miri/review/CI'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True, type=Path, help='receipt inside a NEW evidence directory')
    parser.add_argument('--positive-only', action='store_true')
    args = parser.parse_args()
    out = args.out.resolve()
    if out.parent.exists():
        parser.error('evidence directory already exists; use a new directory to preserve prior receipts')
    try:
        receipt = check(out, args.positive_only)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        receipt = dict(schema='zeno-fcis/composition-v2-evidence/1', status='failed', error=str(error))
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(receipt, indent=2, sort_keys=True)+'\n')
    print(receipt['status'], flush=True)
    if receipt['status'] != 'passed':
        print(receipt['error'], flush=True)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
