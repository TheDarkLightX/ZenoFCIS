#!/usr/bin/env python3
"""Qualify the shared finite checker against its real metered evaluator.

An unreviewed VIR inventory is a candidate, never acceptance. The positive-only
owner lane skips mutation execution explicitly; CI runs the complete controls.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib

import check_verus as verifier
from check_finite_execution import once
from v2_proof_sources import execution_sources
from v2_native_dependencies import native_dependency_args
from verus_coverage import inventory, require_coverage

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path('verification/verus/checker.rs')
PROFILE = Path('verification/verus/checker.json')
SUBJECT = Path('crates/zeno-fcis-shell-sqlite/src/v2/equivalence/checker.rs')
SPEC = SUBJECT.with_name('checker_spec.rs')
EQUALITY = SUBJECT.with_name('checker_equality.rs')
TESTS = SUBJECT.with_name('checker_tests.rs')
API = SUBJECT.with_name('checker_api.rs')
CLI = Path('crates/zeno-fcis-cli/src/transform.rs')
SHELL = SUBJECT.parent.parent / 'equivalence.rs'
ROUTE_INPUTS = (CLI, SHELL, API, HARNESS, Path('crates/zeno-fcis-cli/src/main.rs'),
                Path('crates/zeno-fcis-cli/src/shell_v2.rs'),
                Path('crates/zeno-fcis-cli/Cargo.toml'), Path('Cargo.lock'),
                Path('release/package-set.toml'))

# Each planted operational fault owns one declared proof region. A failure in
# another function is a broken qualification run, not evidence against it.
SEMANTIC_TARGETS = {
    'skip_last_tuple': 'scan', 'repeat_carry': 'advance',
    'width_minus_one': 'domain_size', 'lose_trapping_usage': 'observe',
    'wrong_strict_limit': 'record', 'wrong_candidate_more': 'record',
    'ignore_all_results': 'scan', 'equate_all_failures': 'scan',
    'skip_carry_reset': 'advance', 'first_input_fastest': 'advance',
    'wrap_product': 'domain_size', 'overflow_before_late_empty': 'domain_size',
    'identity_before_cap': 'compare_equal', 'budget_before_mismatch': 'scan',
    'wrong_mismatch_ordinal': 'scan', 'first_output_only': 'results',
    'api_wrong_count': 'compare_with_usage',
    'api_wrong_cap': 'validate_pair',
    'api_wrong_limit': 'compare_with_usage',
}
COVERAGE_TARGETS = {
    'api_weaken_projection': ('checker::checker_api::compare_with_usage', 'ensures_sha256'),
    'weaken_final_theorem': ('checker::checker::compare_equal', 'ensures_sha256'),
    'narrow_final_domain': ('checker::checker_api::compare_with_usage', 'requires_sha256'),
    'alter_rank_spec': ('checker::checker::spec::rank', 'body_sha256'),
    'unchecked_path': ('checker::checker::unchecked_identity', 'inventory'),
}
SEMANTIC_FAILURES = frozenset((
    'assertion failed', 'postcondition not satisfied', 'precondition not satisfied',
    'requires not satisfied', 'invariant not satisfied at end of loop body',
    'loop invariant not satisfied', 'invariant not satisfied',
    'possible arithmetic underflow/overflow',
))
ERROR_HEADER = re.compile(r'^error(?:\[[A-Za-z0-9]+\])?: (.*)$', re.M)
NON_SEMANTIC = re.compile(
    r'resource limit|\brlimit\b|timed? out|\btimeout\b|out of memory|memory exhausted|'
    r'\bkilled\b|\bpanic(?:ked)?\b|internal compiler error|segmentation fault|'
    r'\bsignal\b|transport error|failed to (?:start|execute)', re.I)


def source_paths(root: Path = ROOT, *, profile: bool = True) -> tuple[Path, ...]:
    paths = set(execution_sources(root))
    paths.update(ROUTE_INPUTS)
    paths.update((HARNESS, SUBJECT, SPEC, EQUALITY, TESTS, API, CLI, SHELL, verifier.PIN,
                  Path('crates/zeno-fcis-cli/src/main.rs'),
                  Path('crates/zeno-fcis-cli/src/transform_tests.rs'),
                  Path('crates/zeno-fcis-cli/tests/transform_cli.rs'),
                  Path('crates/zeno-fcis-shell-sqlite/src/v2/upgrade.rs'),
                  Path('crates/zeno-fcis-shell-sqlite/src/v2.rs'),
                  Path('crates/zeno-fcis-shell-sqlite/tests/upgrade.rs'),
                  Path('crates/zeno-fcis-synthesis/src/finite/mod.rs'),
                  Path('crates/zeno-fcis-synthesis/src/finite_runtime.rs'),
                  Path('verification/verus/authority_v2_sources.json'),
                  Path('verification/verus/authority_v2_test_sources.json'),
                  Path('verification/verus/metered-execution.json'),
                  Path('tools/check_checker.py'), Path('tools/test_check_checker.py'),
                  Path('tools/check_verus.py'), Path('tools/check_finite_execution.py'),
                  Path('tools/v2_proof_sources.py'), Path('tools/v2_native_dependencies.py'),
                  Path('tools/verus_coverage.py'), Path('tools/atdd.py'),
                  Path('.github/workflows/verus.yml'),
                  Path('Cargo.toml'), Path('Cargo.lock'), Path('rust-toolchain.toml'),
                  Path('.cargo/config.toml')))
    for name in ('zeno-fcis-cli', 'zeno-fcis-shell-sqlite', 'zeno-fcis-synthesis', 'zeno-fcis-codec', 'zeno-fcis-value'):
        paths.add(Path('crates') / name / 'Cargo.toml')
    for directory in ('crates/zeno-fcis-codec/src', 'crates/zeno-fcis-value/src',
                      'crates/zeno-fcis-cli/tests/fixtures/transform-check'):
        paths.update(p.relative_to(root) for p in (root / directory).rglob('*') if p.is_file())
    if profile:
        paths.add(PROFILE)
    return tuple(sorted(paths))


def snapshot(root: Path = ROOT, *, profile: bool = True) -> dict[str, str]:
    paths = source_paths(root, profile=profile)
    for relative in paths:
        path = root / relative
        if not path.is_file() or any(p.is_symlink() for p in (path, *path.parents)):
            raise ValueError(f'missing or symlink checker source: {relative}')
    return {str(p): verifier.digest(root / p) for p in paths}


def check_routes(root: Path = ROOT) -> None:
    """Guard the required construction path; semantic mapping has native tests."""
    main = (root / 'crates/zeno-fcis-cli/src/main.rs').read_text()
    cli, shell = (root / CLI).read_text(), (root / SHELL).read_text()
    api = (root / API).read_text()
    harness = (root / HARNESS).read_text()
    manifest = tomllib.loads((root / 'crates/zeno-fcis-cli/Cargo.toml').read_text())
    dependency = manifest['dependencies'].get('zeno-fcis-shell-sqlite', {})
    if dependency != {'version': '=1.1.0', 'path': '../zeno-fcis-shell-sqlite'}:
        raise ValueError('CLI lacks the exact published shell library dependency')
    if 'pub(crate) use zeno_fcis_shell_sqlite::v2::equivalence::finite_checker;' not in main:
        raise ValueError('CLI no longer uses the shared library checker API')
    if '#[path = "equivalence/checker.rs"]\nmod checker;' not in shell:
        raise ValueError('SQLite no longer includes the shared checker source')
    if '#[path = "equivalence/checker_api.rs"]\npub mod finite_checker;' not in shell:
        raise ValueError('SQLite no longer exports the checked data API')
    if '#[path = "../../crates/zeno-fcis-shell-sqlite/src/v2/equivalence/checker_api.rs"]\npub mod checker_api;' not in harness:
        raise ValueError('public data API is outside the proof unit')
    if 'checker::compare_with_usage(original, candidate, cap, step_limit)' not in api:
        raise ValueError('public data API bypasses the shared comparison route')
    if 'execute_v2(' in api:
        raise ValueError('public data API contains an independent evaluator')
    shim = (root / 'crates/zeno-fcis-cli/src/shell_v2.rs').read_text()
    if '#[path' in shim or 'zeno-fcis-shell-sqlite/src/' in main:
        raise ValueError('production CLI reads sibling shell source')
    if 'pub(crate) use zeno_fcis_shell_sqlite::v2::migration;' not in shim:
        raise ValueError('migration adapter bypasses the actual shell library')
    lock = tomllib.loads((root / 'Cargo.lock').read_text())
    cli_packages = [p for p in lock['package'] if p['name'] == 'zeno-fcis-cli']
    if len(cli_packages) != 1 or 'zeno-fcis-shell-sqlite' not in cli_packages[0]['dependencies']:
        raise ValueError('locked CLI graph lacks the shell library')
    order = tomllib.loads((root / 'release/package-set.toml').read_text())['publish_order']
    if order.index('zeno-fcis-shell-sqlite') >= order.index('zeno-fcis-cli'):
        raise ValueError('publication order puts CLI before its shell dependency')
    if 'crate::finite_checker::compare_with_usage(' not in cli or 'checker::compare_equal(' not in shell:
        raise ValueError('a shipped adapter bypasses its shared comparison route')
    for name, text in [('CLI', cli), ('SQLite', shell)]:
        if 'execute_v2(' in text or 'fn advance(' in text or 'fn domain_size(' in text:
            raise ValueError(f'{name} contains an independent evaluator or enumeration implementation')


def mutations(source: str, spec: str, equality: str, api: str) -> dict[str, tuple[Path, str, str]]:
    controls = {}
    for name, before, after in (
        ('skip_last_tuple', 'if !advance(domains, &mut input) {', 'if visited == size - 1 || !advance(domains, &mut input) {'),
        ('repeat_carry', 'tuple[position] += 1;', 'tuple[position] += 0;'),
        ('width_minus_one', '(max as i128) - (min as i128) + 1', '(max as i128) - (min as i128)'),
        ('lose_trapping_usage', 'steps: usage.used(V2Resource::Step)', 'steps: 0'),
        ('wrong_strict_limit', 'if original > limit {', 'if original >= limit {'),
        ('wrong_candidate_more', 'if candidate > original {', 'if candidate >= original {'),
        ('ignore_all_results', 'if !equality::results(&left.result, &right.result) {', 'if false {'),
    ):
        controls[name] = (SUBJECT, once(source, before, after), 'proof')
    controls['equate_all_failures'] = (SUBJECT, once(source,
        'if !equality::results(&left.result, &right.result) {',
        'if !equality::results(&left.result, &right.result) && (left.result.is_ok() || right.result.is_ok()) {'), 'proof')
    controls['skip_carry_reset'] = (SUBJECT, once(source, 'tuple[position] = min;', 'tuple[position] = max;'), 'proof')
    forward = once(source,
        'position -= 1;\n        let (min, max) = domains[position].bounds();\n        if tuple[position] < max {\n            tuple[position] += 1;',
        'position -= 1;\n        let index = tuple.len().min(domains.len()) - position - 1;\n        let (min, max) = domains[index].bounds();\n        if tuple[index] < max {\n            tuple[index] += 1;')
    forward = once(forward, 'tuple[position] = min;', 'tuple[index] = min;')
    controls['first_input_fastest'] = (SUBJECT, forward, 'proof')
    controls['wrap_product'] = (SUBJECT, once(source, 'Some(n) => n.checked_mul(width),',
        'Some(n) => Some(n.wrapping_mul(width)),'), 'proof')
    controls['overflow_before_late_empty'] = (SUBJECT, once(source, 'Some(n) => n.checked_mul(width),',
        'Some(n) => match n.checked_mul(width) { Some(next) => Some(next), None => return Ok(None) },'), 'proof')
    controls['identity_before_cap'] = (SUBJECT, once(source,
        'let size = validate_pair(original, candidate, cap)?;\n    if equality::nodes',
        'let size = 1;\n    if equality::nodes'), 'proof')
    controls['budget_before_mismatch'] = (SUBJECT, once(source,
        'if !equality::results(&left.result, &right.result) {',
        'if left.steps > step_limit || right.steps > step_limit { return Err(Failure::BudgetBoundary { over_limit: [1, 1], minimum_limit: 0, usage: tally.usage() }); }\n        if !equality::results(&left.result, &right.result) {'), 'proof')
    controls['wrong_mismatch_ordinal'] = (SUBJECT, once(source, 'ordinal: visited,', 'ordinal: size - 1,'), 'proof')
    controls['first_output_only'] = (EQUALITY, once(equality,
        'while i < left.len() {', 'while i < left.len() && i < 1 {'), 'proof')
    # A removed final theorem, altered spec or extra unchecked executable path
    # must be refused by translated coverage even if its remaining proofs pass.
    # These outer entry points have no proved callers in this whole unit.
    # Weakening a private contract consumed by the public projection would
    # instead break the caller's proof, invalidating a coverage-only oracle.
    needle = 'ensures spec::equality_result(original, candidate, cap, result),'
    controls['weaken_final_theorem'] = (SUBJECT, once(source, needle, 'ensures true,'), 'coverage')
    needle = 'ensures comparison(original, candidate, cap, step_limit, result),'
    controls['narrow_final_domain'] = (API, once(api, needle,
        'requires original.inputs.len() > 0,\n    ' + needle), 'coverage')
    controls['alter_rank_spec'] = (SPEC, once(spec, 'rank(ds, xs, n - 1) * width(ds[n - 1]) + xs[n - 1] - lo(ds[n - 1])', 'rank(ds, xs, n - 1) * width(ds[n - 1]) + (xs[n - 1] - lo(ds[n - 1])) + 0'), 'coverage')
    controls['unchecked_path'] = (SUBJECT, source + '\nfn unchecked_identity(x: u64) -> u64 { x }\n', 'coverage')
    controls['api_wrong_count'] = (API, once(api, 'Ok((done.tuples(), done.usage()))', 'Ok((0, done.usage()))'), 'proof')
    controls['api_wrong_cap'] = (API, once(api, 'checker::validate_pair(original, candidate, cap)', 'checker::validate_pair(original, candidate, 0)'), 'proof')
    controls['api_wrong_limit'] = (API, once(api, 'checker::compare_with_usage(original, candidate, cap, step_limit)', 'checker::compare_with_usage(original, candidate, cap, 0)'), 'proof')
    controls['api_weaken_projection'] = (API, once(api,
        'ensures comparison(original, candidate, cap, step_limit, result),', 'ensures true,'), 'coverage')
    return controls


def proof_succeeded(report: dict, pin: dict) -> bool:
    if not isinstance(report, dict):
        return False
    result = report.get('verification-results', {})
    tool = report.get('verus', {})
    return (isinstance(result, dict) and isinstance(tool, dict)
            and tool.get('version') == pin['version']
            and tool.get('commit') == pin['commit']
            and ('rust_toolchain' not in pin
                 or str(tool.get('toolchain', '')).split(' ')[0] == pin['rust_toolchain'])
            and result.get('success') is True and result.get('errors') == 0
            and type(result.get('errors')) is int
            and type(result.get('verified')) is int and result['verified'] > 0
            and result.get('encountered-error') is False
            and result.get('encountered-vir-error') is False
            and result.get('is-verifying-entire-crate') is True)


def target_lines(source: str, name: str) -> tuple[int, int]:
    """Annotated region in the exact mutated specimen, including its contract."""
    functions = list(re.finditer(r'\bfn\s+([A-Za-z_][A-Za-z_0-9]*)\b', source))
    matches = [i for i, match in enumerate(functions) if match[1] == name]
    if len(matches) != 1:
        raise ValueError(f'not one intended function: {name}')
    index = matches[0]
    start = source.rfind('#[cfg_attr(verus_keep_ghost', 0, functions[index].start())
    if start < 0:
        raise ValueError(f'intended function lacks an annotated contract: {name}')
    end = len(source)
    if index + 1 < len(functions):
        end = functions[index + 1].start()
        attribute = source.rfind('#[cfg_attr(verus_keep_ghost', functions[index].end(), end)
        if attribute >= 0:
            end = attribute
    return source.count('\n', 0, start) + 1, source.count('\n', 0, end)


def semantic_refusal(proc, report: dict, pin: dict, profile: dict,
                     path: Path, changed: str, target: str, specimen: Path) -> tuple[bool, dict]:
    region = target_lines(changed, target)
    evidence = {'file': str(path), 'function': target, 'lines': region, 'diagnostics': []}
    if not isinstance(report, dict):
        return False, evidence
    result, tool, functions = (report.get(k, {}) for k in ('verification-results', 'verus', 'func-details'))
    if not all(isinstance(value, dict) for value in (result, tool, functions)):
        return False, evidence
    normal_failure = (proc.returncode == 1 and tool.get('version') == pin['version']
        and tool.get('commit') == pin['commit']
        and str(tool.get('toolchain', '')).split(' ')[0] == pin['rust_toolchain']
        and result.get('success') is False and result.get('encountered-error') is True
        and result.get('encountered-vir-error') is False
        and result.get('is-verifying-entire-crate') is True
        and type(result.get('errors')) is int and result['errors'] > 0
        and type(result.get('verified')) is int and result['verified'] > 0
        and set(profile['target_functions']).issubset(functions))
    if not normal_failure or NON_SEMANTIC.search(proc.stderr):
        return False, evidence
    headers = list(ERROR_HEADER.finditer(proc.stderr))
    terminal = None
    expected_path = os.path.normpath(str(specimen / path))
    for index, header in enumerate(headers):
        message = header[1]
        summary = re.fullmatch(r'aborting due to (\d+) previous errors?', message)
        if summary:
            if terminal is not None or index != len(headers) - 1:
                return False, evidence
            terminal = int(summary[1])
            continue
        if message not in SEMANTIC_FAILURES or not header[0].startswith('error: '):
            return False, evidence
        end = headers[index + 1].start() if index + 1 < len(headers) else len(proc.stderr)
        block = proc.stderr[header.end():end]
        primary = re.search(r'^\s*-->\s+(.+):(\d+):(\d+)\s*$', block, re.M)
        if primary is None:
            return False, evidence
        observed_path = Path(primary[1])
        if not observed_path.is_absolute():
            observed_path = specimen / observed_path
        line = int(primary[2])
        if os.path.normpath(str(observed_path)) != expected_path or not region[0] <= line <= region[1]:
            return False, evidence
        evidence['diagnostics'].append({'kind': message, 'line': line, 'column': int(primary[3])})
    count = len(evidence['diagnostics'])
    evidence.update(reported_error_groups=result['errors'], diagnostic_count=count,
                    terminal_error_count=terminal)
    # The pinned verifier can emit multiple diagnostics for one failed query:
    # retained attempt8 has 5 diagnostics but 4 error groups. Every group must
    # nevertheless have a diagnostic, and every diagnostic must be intended.
    return bool(count and terminal == count and result['errors'] <= count), evidence


def coverage_refusal(vir: str, profile: dict, target: str, field: str) -> tuple[bool, dict]:
    evidence = {'function': target, 'field': field, 'refusal': None}
    try:
        actual = inventory(vir, profile['namespace'], tuple(profile['body_covered_functions']))
    except ValueError as error:
        evidence['parse_error'] = str(error)
        return False, evidence
    try:
        require_coverage(vir, profile)
    except ValueError as error:
        evidence['refusal'] = str(error)
    if evidence['refusal'] is None:
        return False, evidence
    expected = profile['functions']
    if field == 'inventory':
        intended = (set(actual) == set(expected) | {target} and target not in expected
                    and actual[target]['mode'] == 'Exec'
                    and actual[target]['requires'] == 0 and actual[target]['ensures'] == 0
                    and all(actual[name] == expected[name] for name in expected))
    else:
        changed = {name for name in expected if actual.get(name) != expected[name]}
        permitted = {field}
        if field in ('requires_sha256', 'ensures_sha256'):
            permitted.add(field.removesuffix('_sha256'))
        intended = (set(actual) == set(expected) and changed == {target}
                    and actual[target].get(field) != expected[target].get(field)
                    and {key for key in set(actual[target]) | set(expected[target])
                         if actual[target].get(key) != expected[target].get(key)} <= permitted)
    return intended, evidence


def candidate_profile(vir: str, report: dict, pin: dict) -> dict:
    if not proof_succeeded(report, pin):
        raise ValueError('cannot inventory a failed or differently pinned proof')
    observed = inventory(vir, 'checker::')
    bodies = sorted(k for k, v in observed.items() if v['mode'] == 'Exec')
    return {'namespace': 'checker::', 'reviewed': False,
            'expected_verified': report['verification-results']['verified'],
            'target_functions': sorted(k for k in report['func-details'] if k.startswith('checker::')),
            'body_covered_functions': bodies,
            'functions': inventory(vir, 'checker::', tuple(bodies))}


def run_logged(command: list[str], cwd: Path, environment: dict, output: Path) -> tuple[subprocess.CompletedProcess, dict]:
    output.mkdir(parents=True, exist_ok=False)
    (output / 'command.json').write_text(json.dumps({'argv': command, 'cwd': str(cwd)}, indent=2) + '\n')
    try:
        run = verifier.run(command, cwd, environment, timeout=600)
    except subprocess.TimeoutExpired as error:
        def decoded(value):
            return value.decode(errors='replace') if isinstance(value, bytes) else value or ''
        (output / 'stdout.json').write_text(decoded(error.stdout))
        (output / 'stderr.log').write_text(decoded(error.stderr))
        (output / 'exit.json').write_text(json.dumps({'status': 'timeout', 'timeout_seconds': 600}) + '\n')
        raise
    (output / 'stdout.json').write_text(run.stdout)
    (output / 'stderr.log').write_text(run.stderr)
    (output / 'exit.json').write_text(json.dumps({'exit_code': run.returncode}) + '\n')
    try:
        report = json.loads(run.stdout)
    except json.JSONDecodeError:
        report = {}
    return run, report


def native(directory: Path, pin: dict, environment: dict) -> dict:
    version_command = ['rustc', '+' + pin['runtime_rust'], '--version', '--verbose']
    version, _ = run_logged(version_command, ROOT, environment, directory / 'compiler')
    verifier.require_success(version)
    if 'release: ' + pin['runtime_rust'] not in version.stdout.splitlines():
        raise ValueError('native compiler identity differs from the pinned runtime Rust')
    (directory / 'rustc-version.txt').write_text(version.stdout)
    args = native_dependency_args(ROOT, directory, environment)
    executable = directory / 'checker-native'
    command = ['rustc', '+' + pin['runtime_rust'], '--edition=2024', '--check-cfg',
               'cfg(verus_keep_ghost)', '--check-cfg', 'cfg(test)', *args, '--test',
               str(HARNESS), '-o', str(executable)]
    compiled, _ = run_logged(command, ROOT, environment, directory / 'native-compile')
    verifier.require_success(compiled)
    runs = {'compiler': {'command': version_command, 'identity': version.stdout},
            'compile': {'command': command, 'exit_code': compiled.returncode,
                        'executable_sha256': verifier.digest(executable)}}
    for name, command in (
        ('direct_source', [str(executable)]),
        ('cli_f3', ['cargo', '+1.97.1', 'test', '--locked', '--offline', '-p', 'zeno-fcis-cli', '--bin', 'zeno-fcis', 'transform::']),
        ('cli_process', ['cargo', '+1.97.1', 'test', '--locked', '--offline', '-p', 'zeno-fcis-cli', '--test', 'transform_cli']),
        ('shell_core', ['cargo', '+1.97.1', 'test', '--locked', '--offline', '-p', 'zeno-fcis-shell-sqlite', '--lib', 'equivalence::']),
        ('shell_upgrade', ['cargo', '+1.97.1', 'test', '--locked', '--offline', '-p', 'zeno-fcis-shell-sqlite', '--test', 'upgrade']),
    ):
        run, _ = run_logged(command, ROOT, environment, directory / name)
        verifier.require_success(run)
        runs[name] = {'command': command, 'exit_code': run.returncode, 'stdout': run.stdout}
    return runs


def check(cache: Path, out: Path, positive_only: bool, refresh: bool) -> dict:
    if out.exists():
        raise ValueError('evidence output already exists; preserve previous attempts')
    out.mkdir(parents=True)
    check_routes()
    before = snapshot(profile=not refresh)
    (out / 'source-before.json').write_text(json.dumps(before, indent=2) + '\n')
    pin = json.loads((ROOT / verifier.PIN).read_text())
    tool = verifier.prepare_tools(cache, pin, False)
    environment = {k: v for k, v in os.environ.items() if not k.startswith(('VERUS_', 'VARGO_'))
                   and k not in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')}
    environment.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'], VERUS_Z3_PATH=str(tool / 'z3'),
                       CARGO_BUILD_JOBS='1', CARGO_INCREMENTAL='0', RUST_TEST_THREADS='1')
    command = [str(tool / 'verus'), '--crate-type=lib', '--edition=2024', '--no-cheating',
               '--no-external-by-default', '--num-threads', '1', '-V', 'spinoff-all',
               '--output-json', '--log', 'vir', '--log', 'vir-option=no_span+no_type+no_fn_details']
    positive, report = run_logged([*command, '--log-dir', str(out / 'vir'), str(HARNESS)], ROOT, environment, out / 'positive')
    verifier.require_success(positive)
    if not proof_succeeded(report, pin):
        raise ValueError('complete exact-tool proof did not pass')
    vir = (out / 'vir/crate.vir').read_text()
    if refresh:
        profile = candidate_profile(vir, report, pin)
        (out / 'candidate-profile.json').write_text(json.dumps(profile, indent=2, sort_keys=True) + '\n')
        status, coverage, native_report = 'profile-review-required', {}, {}
    else:
        profile = json.loads((ROOT / PROFILE).read_text())
        if profile.get('reviewed') is not True:
            raise ValueError('checker profile has not been independently reviewed')
        if not verifier.accepted(report, {**pin, **{k: profile[k] for k in ('expected_verified', 'target_functions')}}):
            raise ValueError('proof report does not match the reviewed complete inventory')
        coverage = require_coverage(vir, profile)
        native_report = native(out, pin, environment)
        status = 'passed-positive-only' if positive_only else 'passed'
    controls = []
    if not positive_only and not refresh:
        for name, (path, changed, expected) in mutations((ROOT / SUBJECT).read_text(), (ROOT / SPEC).read_text(), (ROOT / EQUALITY).read_text(), (ROOT / API).read_text()).items():
            specimen = out / ('control-' + name)
            for relative in source_paths():
                target = specimen / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(changed) if relative == path else shutil.copyfile(ROOT / relative, target)
            specimen_before = snapshot(specimen)
            (specimen / 'source-before.json').write_text(json.dumps(specimen_before, indent=2) + '\n')
            run, record = run_logged([*command, '--log-dir', str(specimen / 'vir'), str(HARNESS)], specimen, environment, specimen / 'run')
            specimen_after = snapshot(specimen)
            (specimen / 'source-after.json').write_text(json.dumps(specimen_after, indent=2) + '\n')
            if specimen_before != specimen_after:
                raise ValueError(f'control {name} source changed during proof')
            if expected == 'coverage':
                target, field = COVERAGE_TARGETS[name]
                killed, attribution = coverage_refusal((specimen / 'vir/crate.vir').read_text(), profile, target, field)
                killed = (killed and run.returncode == 0 and proof_succeeded(record, pin)
                          and not ERROR_HEADER.search(run.stderr) and not NON_SEMANTIC.search(run.stderr))
            else:
                killed, attribution = semantic_refusal(run, record, pin, profile, path, changed,
                                                       SEMANTIC_TARGETS[name], specimen)
            classified = {'name': name, 'expected': expected, 'killed': killed,
                          'source_stable': True, 'attribution': attribution}
            (specimen / 'classification.json').write_text(json.dumps(classified, indent=2) + '\n')
            controls.append(classified)
            if not killed:
                raise ValueError(f'control {name} survived or failed for another reason')
    verifier.verify_tool_files(tool, pin)
    after = snapshot(profile=not refresh)
    (out / 'source-after.json').write_text(json.dumps(after, indent=2) + '\n')
    if before != after:
        raise ValueError('checker proof or adapter source changed during qualification')
    return {'schema': 'zeno-fcis/shared-checker-evidence/1', 'status': status,
            'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
            'working_tree_dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()),
            'source_sha256': before, 'toolchain': pin, 'verus_report': report,
            'translated_function_coverage': coverage, 'native': native_report, 'mutations': controls,
            'unproved': ['canonical byte importer and scalar adapter refinement', 'compiler correctness and binary equivalence',
                        'whole contract upgrade soundness', 'unlimited semantics of malformed raw programs',
                        'installed source archive journey is a separate release gate']}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache', type=Path, default=Path.home() / '.cache/zeno-fcis/verus')
    parser.add_argument('--out', type=Path, required=True, help='new retained evidence directory')
    parser.add_argument('--positive-only', action='store_true')
    parser.add_argument('--refresh-coverage', action='store_true', help='emit an unreviewed candidate, never acceptance')
    args = parser.parse_args()
    try:
        receipt = check(args.cache, args.out.resolve(), args.positive_only, args.refresh_coverage)
    except (OSError, ValueError, RuntimeError, KeyError, subprocess.SubprocessError) as error:
        receipt = {'schema': 'zeno-fcis/shared-checker-evidence/1', 'status': 'failed', 'error': str(error)}
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / 'receipt.json').write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'status': receipt['status'], 'out': str(args.out), 'error': receipt.get('error')}))
    return 0 if receipt['status'] in ('passed', 'passed-positive-only') else 1


if __name__ == '__main__':
    raise SystemExit(main())
