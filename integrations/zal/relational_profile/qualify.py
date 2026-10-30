#!/usr/bin/env python3
"""Qualify the current files of the optional relational profile on this host.

The upstream validation, replay and mutation harness is not part of the
retained capsule. This driver is the integration's equivalent. It prepares a
new external runtime from the manifest-checked current files, builds the
worker with the pinned GHC, and then runs, with evidence:
- the retained upstream controls: exact counts and supplied-model
  evaluations against the independent original-formula oracle, comparisons
  against independent enumeration, malformed, resource and forged-response
  controls of the upstream shell, and source-freshness and interruption
  controls;
- compiled semantic mutants of the Haskell sources: each must build, the
  unmutated worker must give the oracle's count for its query, and the
  mutant must give a different exact count;
- forged worker responses against this integration's own wrapper
  validation, each derived from an actual response;
- replay of a completed count and comparison through the wrapper, with
  changed-request, changed-result and changed-witness controls;
- the integration's live wrapper tests and focused unit tests.

Every current input is hashed before and after; a change fails the run.
Evidence, the runtime and scratch stay in the new `--out` directory.
No result here is a proof of the Haskell implementation, and nothing grants
human semantic approval.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
REPOSITORY = HERE.parents[2]
if not __package__:
    sys.path.insert(0, str(REPOSITORY))
from integrations.zal.relational_profile import profile  # noqa: E402

SCHEMA = 'zal/relational-current-qualification/1'
UPSTREAM_TESTS = ('test_backend.py', 'test_operations.py', 'test_shell.py')
LADDER_INPUTS = ('integrations/zal/ladder.py', 'integrations/zal/ladder_en.json')
# The upstream builder's compiler invocation, with a mutant's own paths.
GHC = ['ghc', '-O1', '-Wall', '-Werror', '-Wincomplete-patterns', '-rtsopts', '-package', 'aeson']


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def inputs() -> dict[str, str]:
    """Every current profile file and the ladder files the bridge reads."""
    files = {}
    for path in sorted(HERE.rglob('*')):
        if path.is_file() and '__pycache__' not in path.parts:
            files[path.relative_to(REPOSITORY).as_posix()] = sha(path.read_bytes())
    for name in LADDER_INPUTS:
        files[name] = sha((REPOSITORY / name).read_bytes())
    return files


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


class Steps:
    """Run labelled commands, keep their logs, and refuse an unexpected exit."""

    def __init__(self, logs: Path):
        self.logs, self.records = logs, []
        logs.mkdir()

    def run(self, label, command, cwd, *, env=None, timeout=900):
        process = subprocess.run([str(part) for part in command], cwd=cwd, env=env,
                                 capture_output=True, timeout=timeout, check=False)
        (self.logs / (label + '.stdout')).write_bytes(process.stdout)
        (self.logs / (label + '.stderr')).write_bytes(process.stderr)
        self.records.append({'check': label, 'exit': process.returncode})
        require(process.returncode == 0, label + ' failed; inspect its logs')
        print(label + ': passed', flush=True)
        return process


def compile_limits():
    os.nice(10)
    resource.setrlimit(resource.RLIMIT_AS, (2 * 1024**3,) * 2)
    resource.setrlimit(resource.RLIMIT_CPU, (120,) * 2)


def mutations(runtime: Path, work: Path) -> dict:
    """Compiled wrong implementations, each with its normal and mutated count."""
    sys.path.insert(0, str(runtime))
    import oracle
    import run
    from test_backend import atom, count, mod, quant, unary, query, request

    p, s, b = 'src/Zal/Logic/Population.hs', 'src/Zal/Logic/Syntax.hs', 'src/Zal/Logic/Budget.hs'
    r = atom('R', 'x', 'y')
    cases = [
        ('always-zero', p, 'countNormal lim (normalize checked)',
         'runBudget lim (pure (0 * fromInteger (domainSize (checkedQuery checked))))', query({'op': 'true'}, 2)),
        ('missing-reverse-pair', p, '&& all (eval (not . standard)) (universal normal)', '',
         query(quant('forall', 'x', quant('forall', 'y', {'op': 'implies', 'left': r, 'right': atom('R', 'y', 'x')})), 2)),
        ('missing-diagonal', p, 'diagonal=map (bit . eval . rowBody) (rows normal)',
         'diagonal=map (\\row -> 0 * bit (eval (rowBody row))) (rows normal)', query(quant('forall', 'x', count('y', 0, r)), 1)),
        ('lost-multiplicity', b, 'total <- sumMass mass (M.findWithDefault 0 key table)', 'total <- sumMass mass 0',
         query({'op': 'true'}, 2)),
        ('strict-negated-count', p, 'observationFor (rowObligation r==Required) (rowTest r)', 'observationFor True (rowTest r)',
         query(quant('forall', 'x', unary('not', count('y', 1, r))), 2)),
        ('cap-instead-of-wrap', s, 'Just (n `mod` m)', 'Just (min (m-1) n)', query(quant('forall', 'x', mod('y', 0, 2, r)), 2)),
    ]

    def worker(binary, q):
        tested = subprocess.run([str(binary), '+RTS', '-M1536m', '-RTS'], input=run.canonical(request(q)) + b'\n',
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60,
                                preexec_fn=run.child_limits(60), check=False)
        require(tested.returncode == 0, 'a worker crash is not a semantic result')
        return json.loads(tested.stdout)

    work.mkdir()
    originals = {path.relative_to(runtime).as_posix(): path.read_text()
                 for directory in ('src', 'app') for path in sorted((runtime / directory).rglob('*.hs'))}
    baseline = runtime / '.build/zal-relational-worker'
    reports = []
    for name, path, old, new, q in cases:
        expected = oracle.count(q)
        normal = worker(baseline, q)
        require(normal.get('status') == 'ExactCount' and normal.get('value') == expected,
                'the unmutated worker disagrees with the oracle: ' + name)
        source = work / name
        for relative, text in originals.items():
            (source / relative).parent.mkdir(parents=True, exist_ok=True)
            (source / relative).write_text(text)
        require(originals[path].count(old) == 1, 'mutation anchor is not unique: ' + name)
        changed = originals[path].replace(old, new)
        (source / path).write_text(changed)
        compiled = subprocess.run([*GHC, '-i' + str(source / 'src'), '-outputdir', str(source / 'objects'),
                                   '-o', str(source / 'worker'), str(source / 'app/Main.hs'), '+RTS', '-M768m', '-RTS'],
                                  stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180,
                                  preexec_fn=compile_limits, check=False)
        (work / (name + '-build.log')).write_bytes(compiled.stdout)
        require(compiled.returncode == 0, 'a compiler failure is not a semantic kill: ' + name)
        mutated = worker(source / 'worker', q)
        require(mutated.get('status') == 'ExactCount' and mutated.get('value') != expected,
                'mutant not distinguished by a wrong exact count: ' + name)
        reports.append({'mutation': name, 'source': path, 'source_sha256': sha(changed.encode()),
                        'binary_sha256': sha((source / 'worker').read_bytes()), 'query': q,
                        'expected': expected, 'normal': normal['value'], 'mutated': mutated['value']})
        shutil.rmtree(source / 'objects')
        print(json.dumps({'mutation': name, 'expected': expected, 'mutated': mutated['value']}), flush=True)
    return {'baseline_binary_sha256': sha(baseline.read_bytes()), 'controls': reports, 'killed': len(reports)}


def run_wrapper(request, runtime, scratch):
    return profile.execute(request, str(runtime), seconds=30, scratch=str(scratch))


def forged_responses(count_request, count_answer, comparison_request, comparison_answer) -> list:
    """Forged worker responses, each derived from an actual one, must not pass the wrapper."""
    manifest, _ = profile.verify_provider()
    upstream, oracle = profile._load_vendor(manifest)
    limits = profile._limits(None, 30)
    count_payload, _, _ = profile._snapshot_request(count_request, limits, upstream)
    comparison_payload, _, _ = profile._snapshot_request(comparison_request, limits, upstream)

    def witness_left(answer):
        answer['counterexample']['left'] = not answer['counterexample']['left']

    def witness_model(answer):
        model = answer['counterexample']['model']
        name = sorted(model)[0]
        model[name] = [] if model[name] else [['0']]

    count_cases = [
        ('count for another domain size', lambda a: a.update(domain_size=str(int(a['domain_size']) + 1))),
        ('noncanonical count', lambda a: a.update(value='0' + a['value'])),
        ('count from another engine', lambda a: a.update(engine='other/1')),
        ('count with an extra field', lambda a: a.update(extra='x')),
        ('count over the work limit', lambda a: a.update(work=str(int(limits['work']) + 1))),
        ('count with another assurance', lambda a: a.update(assurance='proved')),
        ('failure carrying a value', lambda a: a.update(status='Invalid', message='x')),
        ('malformed exhaustion', lambda a: a.update(status='ResourceExhausted')),
    ]
    comparison_cases = [
        ('equivalence claimed for a refutation', lambda a: a.update(status='EquivalentAt', holds=True)),
        ('partial coverage', lambda a: a.update(enumerated_models=str(int(a['enumerated_models']) - 1))),
        ('witness side changed', witness_left),
        ('witness model changed', witness_model),
        ('comparison scope changed', lambda a: a.update(scope='some structures')),
        ('comparison for another domain size', lambda a: a.update(domain_size=str(int(a['domain_size']) + 1))),
    ]
    reports = []
    for payload, actual, cases in ((count_payload, count_answer, count_cases),
                                   (comparison_payload, comparison_answer, comparison_cases)):
        require(profile._validate_answer(payload, copy.deepcopy(actual), limits, upstream, oracle) == actual['status'],
                'the actual response must pass before its forgeries are meaningful')
        for label, change in cases:
            forged = copy.deepcopy(actual)
            change(forged)
            try:
                status = profile._validate_answer(payload, forged, limits, upstream, oracle)
            except profile.ProfileFailure as failure:
                status, detail = failure.status, failure.detail
            except (RuntimeError, ValueError, KeyError, TypeError) as error:
                status, detail = 'Error', str(error)
            else:
                detail = 'returned without a semantic result'
            require(status in ('Error', 'Invalid', 'Unsupported', 'UNKNOWN'), 'forged response admitted: ' + label)
            reports.append({'case': label, 'status': status, 'detail': str(detail)[:300]})
    return reports


def projection(result):
    if result['status'] in ('ExactCount', 'Evaluated'):
        keys = ('status', 'value', 'domain_size')
    else:
        keys = ('status', 'holds', 'vacuous', 'relation', 'domain_size', 'enumerated_models',
                'admitted_models', 'partition', 'counterexample')
    return {key: result.get(key) for key in keys}


def replay(record, runtime, scratch):
    """Execute the retained request again; compare its binding and exact result."""
    fresh = run_wrapper(record['request'], runtime, scratch)
    if fresh.get('request_sha256') != record['answer'].get('request_sha256'):
        return 'BindingMismatch'
    if 'result' not in fresh or 'result' not in record['answer']:
        return 'ReplayIncomplete'
    same = projection(fresh['result']) == projection(record['answer']['result'])
    return 'Replayed' if same else 'ReplayMismatch'


def replay_controls(records, runtime, scratch) -> list:
    count, comparison = records
    reports = []

    def control(label, record, expected):
        observed = replay(record, runtime, scratch)
        require(observed == expected, f'replay control {label}: expected {expected}, observed {observed}')
        reports.append({'case': label, 'expected': expected, 'observed': observed})

    control('count replayed', count, 'Replayed')
    control('comparison replayed', comparison, 'Replayed')
    changed = copy.deepcopy(comparison)
    changed['request']['domain_size'] = str(int(changed['request']['domain_size']) + 1)
    control('changed request', changed, 'BindingMismatch')
    changed = copy.deepcopy(count)
    changed['answer']['result']['value'] = str(int(changed['answer']['result']['value']) + 1)
    control('changed count', changed, 'ReplayMismatch')
    changed = copy.deepcopy(comparison)
    changed['answer']['result']['holds'] = True
    control('changed agreement', changed, 'ReplayMismatch')
    changed = copy.deepcopy(comparison)
    changed['answer']['result']['counterexample']['left'] = not changed['answer']['result']['counterexample']['left']
    control('changed witness', changed, 'ReplayMismatch')
    return reports


def comparison_request():
    """forall x P(x) against exists x P(x) at size 2: refuted with a witness."""
    p = {'op': 'atom', 'predicate': 'P', 'args': ['x']}
    return {'schema': 'zal/relational-comparison/1', 'logic': 'zal/c2/1',
            'signature': [{'id': 'P', 'arity': 1}], 'domain_size': '2',
            'context_id': 'current-qualification', 'assumptions': {'op': 'true'},
            'left': {'op': 'forall', 'var': 'x', 'body': p},
            'right': {'op': 'exists', 'var': 'x', 'body': p}, 'relation': 'equivalence'}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--out', type=Path, required=True, help='a new directory outside the source tree')
    args = parser.parse_args()
    out = args.out.resolve()
    require(not out.exists(), '--out must be a new directory')
    require(not out.is_relative_to(REPOSITORY), '--out must be outside the source tree')
    out.mkdir(parents=True)
    before = inputs()
    steps = Steps(out / 'logs')
    runtime, scratch = out / 'runtime', out / 'scratch'
    scratch.mkdir()
    prepared = profile.prepare_runtime(runtime)
    python = [sys.executable, '-B', '-E', '-s']
    steps.run('build', [*python, 'build.py'], runtime, timeout=300)
    for script in UPSTREAM_TESTS:
        steps.run(script.removesuffix('.py'), [*python, script], runtime)

    def evidence(name):
        return json.loads((runtime / 'evidence' / name).read_text())

    build, backend, operations, shell = (evidence(name) for name in
                                         ('build.json', 'qualification.json', 'operations.json', 'shell-controls.json'))
    killed = mutations(runtime, out / 'mutations')
    steps.records.append({'check': 'mutations', 'exit': 0})

    count_request = json.loads((HERE / 'vendor/examples/even-graph-count.json').read_text())
    counted = run_wrapper(count_request, runtime, scratch)
    require(counted['status'] == 'ExactCount' and counted['result']['value'] == '2', 'wrapper count disagrees')
    compared = run_wrapper(comparison_request(), runtime, scratch)
    require(compared['status'] == 'RefutedAt', 'wrapper comparison is not a refutation')
    forged = forged_responses(count_request, counted['result'], comparison_request(), compared['result'])
    replays = replay_controls(({'request': count_request, 'answer': counted},
                               {'request': comparison_request(), 'answer': compared}), runtime, scratch)
    print('wrapper forged responses and replay: passed', flush=True)

    environment = {**os.environ, 'ZAL_RELATIONAL_RUNTIME': str(runtime), 'ZAL_RELATIONAL_SCRATCH': str(scratch)}
    live = steps.run('test_live_provider', [*python[:-2], str(HERE / 'test_live_provider.py')], REPOSITORY, env=environment)
    require(b'skipped' not in live.stderr.lower(), 'live wrapper tests were skipped')
    steps.run('test_profile', [*python[:-2], str(HERE / 'test_profile.py')], REPOSITORY)

    worker = build['binary_sha256']
    require(backend.get('binary_sha256', worker) == worker == operations['binary_sha256']
            == shell['haskell_binary_sha256'] == counted['worker']['binary_sha256'], 'worker bindings differ')
    require(inputs() == before, 'current inputs changed during the run')
    record = {
        'schema': SCHEMA, 'profile': 'optional-experimental', 'checks': steps.records,
        'inputs_sha256': before, 'provider_manifest_sha256': prepared['provider_manifest_sha256'],
        'upstream_commit': prepared['upstream_commit'], 'ghc': build['ghc'], 'worker_sha256': worker,
        'count_comparisons': backend['count_comparisons'],
        'supplied_model_evaluations': backend['supplied_model_evaluations'],
        'comparison_cases': operations['comparisons'], 'public_evaluations': operations['evaluations'],
        'invalid_resource_cases': operations['malformed_or_resource_controls'],
        'upstream_forged_response_controls': len(operations['response_controls']),
        'source_interruption_controls': len(shell['controls']),
        'compiled_semantic_mutants_killed': killed['killed'], 'mutations': killed['controls'],
        'wrapper_forged_response_controls': forged, 'replay_controls': replays,
        'not_established': ['a proof of the Haskell implementation', 'native FCIS execution of the upstream bridge',
                            'the upstream Lean arithmetic check', 'human semantic approval'],
    }
    (out / 'qualification.json').write_text(json.dumps(record, indent=2, sort_keys=True) + '\n')
    print(json.dumps({key: value for key, value in record.items()
                      if key not in ('inputs_sha256', 'checks', 'mutations', 'wrapper_forged_response_controls',
                                     'replay_controls')}, indent=2, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
