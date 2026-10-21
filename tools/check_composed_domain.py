#!/usr/bin/env python3
"""Pinned shared-source composed-domain proof and targeted semantic controls.

The theorem is conditional on declared tables and model bindings. The runtime
resolver and concrete state application are included without algorithm adapters.
An unreviewed profile candidate is never accepted as completed verification.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

import check_verus as verifier
from check_checked_assembly import counts, function_region
from verus_coverage import inventory

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path('verification/verus/composed-domain.json')
HARNESS = Path('verification/verus/composed_domain.rs')
SUBJECT = Path('crates/zeno-fcis-contract/src/checked_composition/resolve.rs')
SPEC = SUBJECT.with_name('resolve_spec.rs')
THEOREM = Path('verification/verus/composed_domain_theorem.rs')
PROOFS = (SUBJECT, SPEC, THEOREM)
NAMESPACE = 'composed_domain::'
SEMANTIC_FAILURES = verifier.SEMANTIC_FAILURES | {
    'Call to non-static function fails to satisfy `callee.requires(args)`'}
MODULES = ('resolve', 'theorem')
FORBIDDEN = re.compile(r'\b(?:assume(?:_specification)?|admit|unsafe)\b|\bexternal[A-Za-z_]*\b|verifier\s*::\s*(?:external|axiom|rlimit)')
CONTROLS = {
    'short_prefix': (SUBJECT, 'end: End::CommittedFailure(at),\n                    committed: at + 1,',
        'end: End::CommittedFailure(at),\n                    committed: at,', 'resolve'),
    'own_successor_only': (SUBJECT,
        'if !rollback && at < resolution.committed && owns[at] {',
        'if !rollback && at < resolution.committed && owns[at] && match resolution.end { End::CommittedFailure(last) => at == last, _ => true } {', 'apply'),
    'read_only_written': (SUBJECT,
        'if !rollback && at < resolution.committed && owns[at] {',
        'if !rollback && at < resolution.committed {', 'apply'),
    'call_after_reject': (SUBJECT,
        'Local::Reject => {\n                return Resolution',
        'Local::Reject => {\n                if at + 1 < n { let _ = decide(at + 1); }\n                return Resolution', 'resolve'),
    'outside_failure_cut_removed': (THEOREM,
        '|| outside_cut(m, k, decisions(m, pre, r), cut))\n        ==>',
        '|| false)\n        ==>', 'coupling_invariants_preserved'),
    'read_only_invariant_removed': (THEOREM,
        '&& (m.invariant)(i, state(m, pre, i)))\n}',
        '&& true)\n}', 'delivered_inputs_keep_domains_assumptions_and_invariants'),
}


def source_paths():
    paths = {HARNESS, *PROOFS, PROFILE, verifier.PIN, Path('tools/check_composed_domain.py'),
        Path('tools/test_check_composed_domain.py'), Path('tools/check_verus.py'),
        Path('tools/check_checked_assembly.py'), Path('tools/verus_coverage.py'),
        Path('tools/v2_proof_sources.py'), Path('tools/test_composed_domain_runtime_controls.py'),
        Path('crates/zeno-fcis-contract/src/checked_composition.rs'),
        Path('Cargo.toml'), Path('Cargo.lock'), Path('rust-toolchain.toml')}
    paths.update(p.relative_to(ROOT) for p in (ROOT / SUBJECT.parent).glob('*.rs'))
    return tuple(sorted(paths))


def snapshot():
    result = {}
    for relative in source_paths():
        path = ROOT / relative
        if not path.is_file() or any(p.is_symlink() for p in (path, *path.parents)):
            raise RuntimeError(f'missing or symbolic-link source: {relative}')
        result[str(relative)] = verifier.digest(path)
    return result


def forbidden_tokens(sources):
    return [f'{path}:{i}' for path, source in sources.items()
        for i, line in enumerate(source.splitlines(), 1)
        if FORBIDDEN.search(line) and not line.strip().startswith('//!')]


def control_sources(sources):
    result = {}
    for name, (path, anchor, replacement, target) in CONTROLS.items():
        if sources[path].count(anchor) != 1:
            raise RuntimeError(f'control anchor changed: {name}')
        result[name] = (path, sources[path].replace(anchor, replacement), target)
    return result


def positive_accepted(process, report, pin, profile):
    return (process.returncode == 0 and verifier.proof_succeeded(report, pin)
        and verifier.accepted(report, {**pin, 'expected_verified': profile['expected_verified'],
            'target_functions': profile['target_functions']})
        and not verifier.ERROR_HEADER.search(process.stderr)
        and not verifier.NON_SEMANTIC.search(process.stderr))


def targeted_refusal(process, report, pin, profile, path, specimen, region):
    """Require semantic diagnostics only at the stated failed obligation.

    Several assertions can fail within that one function. Missing tools,
    parse errors, timeouts, unrelated failures, and incomplete runs fail closed.
    """
    if not isinstance(report, dict):
        return False
    result, tool = report.get('verification-results', {}), report.get('verus', {})
    if (process.returncode != 1 or tool.get('version') != pin['version']
        or tool.get('commit') != pin['commit']
        or str(tool.get('toolchain', '')).split(' ')[0] != pin['rust_toolchain']
        or ('success' in result and result['success'] is not False)
        or result.get('encountered-error') is not True
        or result.get('encountered-vir-error') is not False
        or result.get('is-verifying-entire-crate') is not False
        or type(result.get('errors')) is not int or result['errors'] != 1
        or type(result.get('verified')) is not int or result['verified'] != profile['expected_verified'] - 1
        or verifier.NON_SEMANTIC.search(process.stderr)):
        return False
    errors, terminal = 0, None
    headers = list(verifier.ERROR_HEADER.finditer(process.stderr))
    for i, header in enumerate(headers):
        summary = re.fullmatch(r'aborting due to (\d+) previous errors?(?:; \d+ warnings? emitted)?', header[1])
        if summary:
            if i != len(headers) - 1:
                return False
            terminal = int(summary[1]); continue
        if header[1] not in SEMANTIC_FAILURES:
            return False
        end = headers[i + 1].start() if i + 1 < len(headers) else len(process.stderr)
        block = process.stderr[header.end():end]
        location = re.search(r'^\s*-->\s+(.+):(\d+):(\d+)\s*$', block, re.M)
        if location is None:
            return False
        observed = Path(location[1])
        actual = observed if observed.is_absolute() else specimen / observed
        if (os.path.normpath(str(actual)) != os.path.normpath(str(specimen / path))
                or not region[0] <= int(location[2]) <= region[1]):
            return False
        errors += 1
    return errors > 0 and terminal == errors and result['errors'] <= errors


def observed_profile(vir):
    return dict(sorted(inventory(vir, NAMESPACE,
        (NAMESPACE + 'resolve::resolve', NAMESPACE + 'resolve::apply')).items()))


def require_reviewed_profile(profile):
    if profile.get('reviewed') is not True:
        raise ValueError('profile requires independent technical review')


def check(cache, install, logs, emit_profile=None):
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    before = snapshot()
    sources = {p: (ROOT / p).read_text() for p in PROOFS}
    if forbidden_tokens({HARNESS: (ROOT / HARNESS).read_text(), **sources}):
        raise RuntimeError('trusted declaration in the proved source closure')
    controls = control_sources(sources)
    qualified = verifier.prepare_tools(cache, pin, install)
    environment = {k: v for k, v in os.environ.items()
        if not k.startswith(('VERUS_', 'VARGO_')) and k not in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')}
    environment.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'], VERUS_Z3_PATH=str(qualified / 'z3'))
    command = [str(qualified / 'verus'), '--crate-type=lib', '--edition=2024', '--no-cheating',
        '--no-external-by-default', '--num-threads', verifier.proof_threads('2'), '-V', 'spinoff-all',
        '--output-json', '--log', 'vir', '--log', 'vir-option=no_span+no_type+no_fn_details']

    def run(label, cwd, restricted=False):
        folder = logs / label; folder.mkdir()
        args = [*command, '--log-dir', str(folder / 'vir'), str(HARNESS)]
        if restricted:
            args.extend(a for m in MODULES for a in ('--verify-module', m))
        (folder / 'command.json').write_text(json.dumps({'command': args, 'cwd': str(cwd)}, indent=2) + '\n')
        process = subprocess.run(args, cwd=cwd, env=environment, capture_output=True, text=True, timeout=600)
        (folder / 'stdout').write_text(process.stdout); (folder / 'stderr').write_text(process.stderr)
        (folder / 'process.json').write_text(json.dumps({'exit_code': process.returncode}) + '\n')
        return process, json.loads(process.stdout), folder

    positive, report, folder = run('positive', ROOT)
    functions = observed_profile((folder / 'vir/crate.vir').read_text())
    if emit_profile is not None:
        if not verifier.proof_succeeded(report, pin) or positive.returncode != 0:
            raise RuntimeError('failed proof cannot emit a candidate profile')
        candidate = {'schema': 'zeno-fcis/composed-domain-profile/1', 'reviewed': False,
            'namespace': NAMESPACE, 'expected_verified': report['verification-results']['verified'],
            'functions': functions, 'inventory': counts(functions),
            'body_covered_functions': [NAMESPACE + 'resolve::resolve', NAMESPACE + 'resolve::apply'],
            'target_functions': sorted(report['func-details'])}
        if emit_profile.resolve() == (ROOT / PROFILE).resolve():
            raise ValueError('candidate must not overwrite the reviewed profile')
        verifier.verify_tool_files(qualified, pin)
        if before != snapshot():
            raise RuntimeError('source drift during candidate verification')
        with emit_profile.open('x') as stream:
            json.dump(candidate, stream, indent=2, sort_keys=True); stream.write('\n')
        return {'schema': 'zeno-fcis/composed-domain-evidence/1', 'status': 'profile-review-required',
            'review_accepted': False, 'all_controls': False, 'source_sha256': before, 'verus_report': report}
    require_reviewed_profile(profile)
    if not positive_accepted(positive, report, pin, profile):
        raise RuntimeError('unfiltered proof failed pinned exact admission')
    if functions != profile['functions'] or counts(functions) != profile['inventory']:
        raise RuntimeError('translated specification, contracts or bodies differ from reviewed profile')
    results = []
    for name, (path, changed, target) in controls.items():
        specimen = logs / (name + '-source')
        for relative in before:
            destination = specimen / relative; destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, destination)
        (specimen / path).write_text(changed)
        process, parsed, _ = run(name, specimen, True)
        # A function region includes its attributes and postcondition, because
        # a failing postcondition points at the contract rather than the body.
        region = verifier.function_lines(changed, target) if path == SUBJECT else function_region(changed, target)
        killed = targeted_refusal(process, parsed, pin, profile, path, specimen, region)
        results.append({'name': name, 'file': str(path), 'intended_function': target,
            'killed': killed, 'exit_code': process.returncode,
            'verification_results': parsed.get('verification-results', {})})
        if not killed:
            raise RuntimeError(f'control {name} survived or failed outside {target}')
        print(f'Composed domain: {name} fails {target}', flush=True)
    verifier.verify_tool_files(qualified, pin)
    if before != snapshot():
        raise RuntimeError('source drift during verification')
    return {'schema': 'zeno-fcis/composed-domain-evidence/1', 'status': 'passed',
        'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'working_tree_dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()),
        'source_sha256': before, 'toolchain': pin, 'exit_code': positive.returncode,
        'verus_report': report, 'inventory': profile['inventory'], 'controls': results,
        'all_controls': all(item['killed'] for item in results),
        'scope': 'conditional abstract finite-history theorem; shared resolver and concrete state application',
        'tested_premises': ['Authority decisions', 'manifest parser', 'formula lowering',
            'planner and enumeration coverage', 'member simulation and outside-cut coverage',
            'runtime local decisions to model inputs and result payload mapping']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache', type=Path, default=verifier.cache_directory())
    parser.add_argument('--install', action='store_true')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--emit-profile', type=Path)
    args = parser.parse_args(); args.out.parent.mkdir(parents=True, exist_ok=True)
    logs = Path(tempfile.mkdtemp(prefix='composed-domain-', dir=args.out.parent)).resolve()
    try:
        receipt = check(args.cache.resolve(), args.install, logs, args.emit_profile)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        receipt = {'schema': 'zeno-fcis/composed-domain-evidence/1', 'status': 'failed', 'error': str(error)}
    receipt['logs'] = str(logs)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
    print(f"Composed domain: {receipt['status']}; receipt: {args.out}", flush=True)
    if receipt['status'] == 'failed': print(receipt['error'])
    return int(receipt['status'] not in ('passed', 'profile-review-required'))

if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
