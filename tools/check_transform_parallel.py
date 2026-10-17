#!/usr/bin/env python3
"""Check that the offline parallel checker reproduces the serial checker.

For every benchmark pair (the 32 seeded cases and the 5 withdrawal-queue
artifact pairs) and every known-answer vector of the transform checker, this
runs `transform check` serially and with --offline-parallel at 1, 2 and 4
workers and 4096, 3 and 1 tuples per range. The exit status, the report
(without the parallel-only `progress` block and the receipt path) and the
receipt bytes must be identical; a domain that needs more than 65,536 ranges at
a chunk size must instead be inconclusive (`chunk-cap`) with no receipt. The
`progress` block must show the planned ranges, one worker per range up to the
worker count, and every range of a completed run; a counterexample stops after
the window holding its first witness. Known answers must match their expected
reports and receipts. A binding tuple cap and an expired deadline must be
inconclusive and write no receipt. The result binds the CLI, the sources and
the CPU environment. Timings are local measurements on this machine, never a
guarantee; --release-timing adds repeated release-build timings with medians.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
CASES = ROOT / 'crates/zeno-fcis-cli/tests/fixtures/cases-i128.json'
ARTIFACTS = ROOT / 'crates/zeno-fcis-cli/tests/fixtures/i128-programs'
VECTORS = ROOT / 'crates/zeno-fcis-cli/tests/fixtures/transform-check/4'
ARTIFACT_PAIRS = (
    ('kernel', 'boolean-kernel-original', 'boolean-kernel-candidate'),
    ('kernel-padded', 'boolean-kernel-original', 'boolean-kernel-padded'),
    ('controller', 'retained-controller-original', 'retained-controller-candidate'),
    ('decision', 'current-decision-scalars-original', 'current-decision-scalars-candidate'),
    ('decision-padded', 'current-decision-scalars-original', 'current-decision-scalars-padded'),
)
SUBJECTS = 37
WORKERS = (1, 2, 4)
CHUNKS = (4096, 3, 1)
MAX_CHUNKS = 65_536
PARALLEL_ONLY = ('progress',)
SOURCES = ('crates', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo',
           'docs/benchmarks/cases.json',
           'docs/benchmarks/withdrawal-queue/artifacts', 'crates/zeno-fcis-cli/tests/fixtures/transform-check',
           'tools/check_transform_parallel.py')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(command):
    started = time.monotonic()
    done = subprocess.run([str(part) for part in command], capture_output=True, text=True, check=False)
    return done.returncode, done.stdout, done.stderr, time.monotonic() - started


def width(domain):
    if domain['kind'] == 'Bool':
        return 2
    require(domain['kind'] == 'Int', f'unknown domain kind {domain}')
    return int(domain['max']) - int(domain['min']) + 1


def encode(cli, work, name, case, side):
    program = {'inputs': case['input_domains'], 'outputs': case['output_domains'],
               'nodes': case[side]['nodes'], 'roots': case[side]['roots']}
    source = work / f'{name}.{side}.json'
    source.write_text(json.dumps(program))
    target = work / f'{name}.{side}.zcve'
    code, out, err, _ = run([cli, 'loop', 'encode', '--program', source, '--out', target])
    return target if code == 0 and target.is_file() else None, (out + err).strip()


def subjects(cli, work):
    """Every benchmark pair, with the domain size computed from cases.json."""
    found, unencodable = [], []
    for case in json.loads(CASES.read_text())['cases']:
        original, why_original = encode(cli, work, case['id'], case, 'original')
        candidate, why_candidate = encode(cli, work, case['id'], case, 'candidate')
        if original and candidate:
            tuples = 1
            for domain in case['input_domains']:
                tuples *= width(domain)
            found.append({'name': case['id'], 'original': original, 'candidate': candidate,
                          'extra': (), 'tuples': tuples})
        else:
            unencodable.append({'id': case['id'], 'original': why_original if not original else 'ok',
                                'candidate': why_candidate if not candidate else 'ok'})
    for name, original, candidate in ARTIFACT_PAIRS:
        found.append({'name': name, 'original': ARTIFACTS / f'{original}.zcve',
                      'candidate': ARTIFACTS / f'{candidate}.zcve', 'extra': (), 'tuples': None})
    return found, unencodable


def vectors():
    manifest = json.loads((VECTORS / 'vectors.json').read_text())
    found = []
    for vector in manifest['vectors']:
        found.append({'name': 'vector-' + vector['name'], 'original': VECTORS / vector['original'],
                      'candidate': VECTORS / vector['candidate'], 'tuples': None,
                      'extra': ('--step-limit', str(vector['step_limit']),
                                '--max-input-tuples', str(vector['max_input_tuples'])),
                      'vector': vector})
    require(len(found) == 7, f'expected 7 known-answer vectors, found {len(found)}')
    return found


def comparable(report):
    report = json.loads(json.dumps(report))
    for key in PARALLEL_ONLY:
        report.pop(key, None)
    if isinstance(report.get('detail'), dict):
        report['detail'].pop('receipt_path', None)
    return report


def invoke(cli, work, subject, label, flags, receipt=True):
    path = work / f'{subject["name"]}.{label}.receipt.json'
    command = [cli, 'transform', 'check', '--original', subject['original'],
               '--candidate', subject['candidate'], *subject['extra'], *flags]
    if receipt:
        command += ['--receipt', path]
    code, out, err, seconds = run(command)
    try:
        report = json.loads(out)
    except ValueError:
        raise ValueError(f'{subject["name"]} {label}: stdout is not one JSON document: {out!r} {err!r}')
    return {'exit': code, 'report': report, 'seconds': round(seconds, 3),
            'receipt': sha(path) if path.exists() else None}


def plans(serial):
    """Whether the run reaches range planning: admitted, within the tuple cap."""
    report = serial['report']
    if report.get('status') in ('equivalent', 'counterexample'):
        return True
    return report.get('status') == 'inconclusive' and report.get('detail', {}).get('cause') == 'budget-boundary'


def learn_tuples(known, serial):
    """Domain sizes keyed by program digest; both programs share one input ABI."""
    report, detail = serial['report'], serial['report'].get('detail', {})
    size = None
    if report.get('status') == 'equivalent':
        size = int(detail['receipt']['domain']['size'])
    elif detail.get('cause') == 'domain-too-large':
        size = int(detail['domain_size'])
    if size is not None:
        for key in ('original_sha256', 'candidate_sha256'):
            digest = report[key]
            require(known.setdefault(digest, size) == size, f'conflicting domain sizes for {digest}')


def serial_runs(cli, work, subject):
    serial = invoke(cli, work, subject, 'serial', [])
    vector = subject.get('vector')
    if vector:
        bare = invoke(cli, work, subject, 'serial-bare', [], receipt=False)
        expected = json.loads((VECTORS / vector['expected']).read_text())
        require(bare['report'] == expected, f'{subject["name"]}: serial report differs from its known answer')
        require(bare['exit'] == vector['exit'], f'{subject["name"]}: serial exit differs from its known answer')
        require(serial['exit'] == vector['exit'] and comparable(serial['report']) == comparable(expected),
                f'{subject["name"]}: serial run with a receipt differs from its known answer')
        if 'receipt' in vector:
            require(serial['receipt'] == sha(VECTORS / vector['receipt']),
                    f'{subject["name"]}: receipt differs from its known-answer bytes')
        else:
            require(serial['receipt'] is None, f'{subject["name"]}: unexpected receipt')
    return serial


def expected_completed(serial, planned, active, chunk):
    """Ranges a completed run must have finished."""
    report = serial['report']
    if report.get('status') == 'counterexample':
        # Ranges are dispatched in windows of `active` consecutive ranges; the
        # run stops after the window holding the first witness.
        window = int(report['detail']['ordinal']) // chunk // active
        return min(planned, (window + 1) * active)
    return planned


def parallel_runs(cli, work, subject, serial, tuples):
    rows, observations = [], []
    for chunk in CHUNKS:
        for workers in WORKERS:
            label = f'w{workers}-c{chunk}'
            row = invoke(cli, work, subject, label,
                         ['--offline-parallel', '--workers', str(workers), '--chunk-tuples', str(chunk)])
            name = f'{subject["name"]} at {workers} workers, chunk {chunk}'
            progress = row['report'].get('progress')
            require(isinstance(progress, dict), f'{name}: no progress block: {row["report"]}')
            scheduling = progress.get('scheduling', {})
            require(isinstance(scheduling.get('cpu_affinity'), str)
                    and isinstance(scheduling.get('nice'), int), f'{name}: missing scheduling evidence')
            cpus = set()
            for part in scheduling['cpu_affinity'].split(','):
                lo, _, hi = part.partition('-')
                cpus.update(range(int(lo), int(hi or lo) + 1))
            require(cpus and cpus <= os.sched_getaffinity(0), f'{name}: caller CPU affinity escaped')
            require(scheduling['nice'] >= os.getpriority(os.PRIO_PROCESS, 0),
                    f'{name}: caller scheduling priority escaped')
            planned_needed = None if tuples is None else -(-tuples // chunk)
            if plans(serial) and planned_needed > MAX_CHUNKS:
                # A documented operational limit, never a verdict or receipt.
                require(row['exit'] == 2 and row['report'].get('status') == 'inconclusive'
                        and row['report'].get('detail', {}).get('reason') == 'chunk-cap'
                        and row['receipt'] is None and progress['planned_chunks'] == 0,
                        f'{name}: {planned_needed} ranges must be refused as chunk-cap: {row["report"]}')
                outcome = 'chunk-cap'
            else:
                require(row['exit'] == serial['exit'], f'{name}: exit differs')
                require(comparable(row['report']) == comparable(serial['report']), f'{name}: report differs')
                require(row['receipt'] == serial['receipt'], f'{name}: receipt bytes differ')
                outcome = 'parity'
                if plans(serial):
                    planned, active = progress['planned_chunks'], progress['active_workers']
                    require(planned == planned_needed, f'{name}: planned {planned} ranges, expected {planned_needed}')
                    require(active == min(workers, planned), f'{name}: {active} active workers')
                    require(progress['accounting_complete'] is True, f'{name}: incomplete accounting')
                    completed = progress['completed_chunks']
                    require(completed == expected_completed(serial, planned, active, chunk),
                            f'{name}: completed {completed} of {planned} ranges')
                    observations.append((planned, active))
                else:
                    require(progress['planned_chunks'] == 0 and progress['active_workers'] == 0,
                            f'{name}: ranges planned for a run refused before planning')
            rows.append({'workers': workers, 'chunk_tuples': chunk, 'outcome': outcome, 'exit': row['exit'],
                         'seconds': row['seconds'], 'planned_chunks': progress['planned_chunks'],
                         'active_workers': progress['active_workers'],
                         'completed_chunks': progress['completed_chunks'],
                         'scheduling': scheduling,
                         'accounting_complete': progress['accounting_complete']})
    return rows, observations


def source_digests():
    files = {}
    for entry in SOURCES:
        path = ROOT / entry
        require(path.exists(), f'missing source {entry}')
        candidates = [path] if path.is_file() else sorted(p for p in path.rglob('*') if p.is_file())
        for item in candidates:
            if '__pycache__' not in item.parts:
                files[str(item.relative_to(ROOT))] = sha(item)
    combined = hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()
    return files, combined


def git(*args):
    done = subprocess.run(['git', *args], cwd=ROOT, capture_output=True, text=True, check=False)
    return done.stdout.strip() if done.returncode == 0 else None


def cpu_max_chain(root=Path('/sys/fs/cgroup'), group=None):
    """The caller's cpu.max limits and the tightest quota in percent, as the
    CLI's `checker_process` computes them before it starts a contained unit.

    A group without the cpu controller has no cpu.max and inherits its
    ancestors' limits. A missing group, or an unreadable or malformed limit,
    refuses with the CLI's reason.
    """
    if group is None:
        relative = next((line[3:] for line in Path('/proc/self/cgroup').read_text().splitlines()
                         if line.startswith('0::')), None)
        if relative is None:
            raise RuntimeError('caller-cpu-quota-unavailable:cgroup-v2-unavailable')
        group = root / relative.lstrip('/')
    chain, quotas = [], []
    while group != root:
        if root not in group.parents:
            raise RuntimeError('caller-cpu-quota-group-outside-root')
        if not (group / 'cgroup.procs').is_file():
            raise RuntimeError(f'caller-cpu-quota-unavailable:{group}:not a cgroup')
        try:
            text = (group / 'cpu.max').read_text()
        except FileNotFoundError:
            text = None
        if text is not None:
            fields = text.split()
            if len(fields) != 2 or not fields[1].isdigit() or int(fields[1]) == 0:
                raise RuntimeError('caller-cpu-quota-malformed')
            if fields[0] != 'max':
                if not fields[0].isdigit() or int(fields[0]) == 0:
                    raise RuntimeError('caller-cpu-quota-malformed')
                percent = int(fields[0]) * 100 // int(fields[1])
                if percent == 0:
                    raise RuntimeError('caller-cpu-quota-below-one-percent')
                quotas.append(percent)
            chain.append({'group': str(group), 'cpu.max': text.strip()})
        group = group.parent
    return chain, min(quotas) if quotas else None


def environment(cli):
    chain, quota = cpu_max_chain()
    status = Path('/proc/self/status').read_text()
    cpus = next(line.split(':', 1)[1].strip() for line in status.splitlines()
                if line.startswith('Cpus_allowed_list:'))
    nice = int(Path('/proc/self/stat').read_text().rsplit(')', 1)[1].split()[16])
    rustc = subprocess.run(['rustc', '+1.97.1', '--version'], capture_output=True, text=True, check=False)
    return {'cpu_max_chain': chain, 'effective_cpu_quota_percent': quota,
            'cpus_allowed_list': cpus, 'nice': nice, 'cli_build_profile': Path(cli).parent.name,
            'rustc': rustc.stdout.strip() or None, 'nproc': os.cpu_count()}


def release_timing(work, decision, runs):
    target = Path(os.environ.setdefault('CARGO_TARGET_DIR', str(ROOT / 'target')))
    subprocess.run(['cargo', '+1.97.1', 'build', '--release', '-p', 'zeno-fcis-cli', '--locked', '--offline'],
                   cwd=ROOT, check=True)
    cli = target / 'release/zeno-fcis'
    require(cli.is_file(), 'release CLI missing')
    timings, reference = {}, None
    for mode in ('serial', *WORKERS):
        seconds = []
        for index in range(runs):
            flags = [] if mode == 'serial' else ['--offline-parallel', '--workers', str(mode)]
            row = invoke(cli, work, decision, f'release-{mode}-{index}', flags)
            require(row['exit'] == 0 and row['report'].get('status') == 'equivalent',
                    f'release decision run failed at {mode}: {row["report"]}')
            reference = reference or row['receipt']
            require(row['receipt'] == reference, f'release receipt bytes differ at {mode}')
            seconds.append(row['seconds'])
        timings[str(mode)] = {'runs': seconds, 'median': statistics.median(seconds)}
    serial = timings['serial']['median']
    return {'cli': str(cli), 'cli_sha256': sha(cli), 'build_profile': 'release', 'runs_per_mode': runs,
            'subject': 'decision', 'timings_seconds': timings,
            'median_speedup': {str(mode): round(serial / timings[str(mode)]['median'], 2) for mode in WORKERS},
            'claim': 'local measurements on this machine and CPU grant, not a guarantee'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, help='prebuilt CLI; default builds it into CARGO_TARGET_DIR')
    parser.add_argument('--work', type=Path, help='new evidence directory; default is a temporary one')
    parser.add_argument('--release-timing', action='store_true',
                        help='also build a release CLI and time the decision subject repeatedly')
    parser.add_argument('--timing-runs', type=int, default=3, help='runs per mode for --release-timing (>= 3)')
    args = parser.parse_args()
    require(args.timing_runs >= 3, '--timing-runs must be at least 3')
    sources, combined = source_digests()
    if args.cli is None:
        os.environ.setdefault('CARGO_TARGET_DIR', str(ROOT / 'target'))
        subprocess.run(['cargo', '+1.97.1', 'build', '-p', 'zeno-fcis-cli', '--locked', '--offline'], cwd=ROOT, check=True)
        args.cli = Path(os.environ['CARGO_TARGET_DIR']) / 'debug/zeno-fcis'
    if args.work is None:
        args.work = Path(tempfile.mkdtemp(prefix='zeno-fcis-transform-parallel-')) / 'run'
    cli, work = args.cli.resolve(), args.work.resolve()
    require(cli.is_file(), 'CLI missing')
    work.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    found, unencodable = subjects(cli, work)
    # A broken encoder or unavailable containment must fail, never shrink the set.
    require(unencodable == [], f'unencodable benchmark cases: {unencodable}')
    require(len(found) == SUBJECTS, f'expected {SUBJECTS} benchmark subjects, found {len(found)}')
    everything = found + vectors()
    serial = {subject['name']: serial_runs(cli, work, subject) for subject in everything}
    known = {}
    for row in serial.values():
        learn_tuples(known, row)
    results, observed = [], []
    for subject in everything:
        row = serial[subject['name']]
        tuples = subject['tuples']
        learned = known.get(row['report'].get('original_sha256')) or known.get(row['report'].get('candidate_sha256'))
        if tuples is None:
            tuples = learned
        elif learned is not None:
            require(tuples == learned, f'{subject["name"]}: computed {tuples} tuples, receipt says {learned}')
        require(tuples is not None or not plans(row), f'{subject["name"]}: domain size unknown')
        modes, observations = parallel_runs(cli, work, subject, row, tuples)
        observed += observations
        results.append({'name': subject['name'], 'known_answer': 'vector' in subject, 'tuples': tuples,
                        'exit': row['exit'], 'status': row['report'].get('status'),
                        'receipt_sha256': row['receipt'], 'serial_seconds': row['seconds'], 'parallel': modes})
    # A regression forcing one worker or one range must not keep parity silently.
    require(any(planned > 1 and active > 1 for planned, active in observed),
            'no run used more than one range and more than one worker')
    decision = next(subject for subject in found if subject['name'] == 'decision')
    capped_subject = {**decision, 'name': 'decision-tuple-cap', 'extra': ('--max-input-tuples', '1000')}
    capped = {'serial': invoke(cli, work, capped_subject, 'serial', [])}
    for workers in WORKERS:
        capped[str(workers)] = invoke(cli, work, capped_subject, f'w{workers}',
                                      ['--offline-parallel', '--workers', str(workers)])
    for mode, row in capped.items():
        require(row['report'].get('status') == 'inconclusive' and row['receipt'] is None and row['exit'] == 2,
                f'a binding tuple cap must be inconclusive with no receipt ({mode})')
        require(comparable(row['report']) == comparable(capped['serial']['report']),
                f'tuple cap report differs ({mode})')
    deadline = []
    for workers in WORKERS:
        row = invoke(cli, work, {**decision, 'name': 'decision-deadline'}, f'w{workers}',
                     ['--offline-parallel', '--workers', str(workers), '--deadline-ms', '1'])
        # The in-process deadline passes before the supervisor's stage kill,
        # so the stop is the scheduler's own, with its progress retained.
        require(row['report'].get('status') == 'inconclusive' and row['receipt'] is None and row['exit'] == 2
                and row['report'].get('detail', {}).get('reason') == 'deadline'
                and isinstance(row['report'].get('progress'), dict),
                f'an expired deadline must be an in-process inconclusive stop with no receipt ({workers} workers): '
                f'{row["report"]}')
        deadline.append({'workers': workers, 'exit': row['exit'], 'reason': 'deadline',
                         'progress': row['report']['progress'], 'seconds': row['seconds']})
    decision_row = next(row for row in results if row['name'] == 'decision')
    debug_timing = {'serial': decision_row['serial_seconds'],
                    **{str(m['workers']): m['seconds'] for m in decision_row['parallel'] if m['chunk_tuples'] == 4096}}
    head = git('rev-parse', 'HEAD')
    report = {'schema': 'zeno-fcis/transform-parallel-parity/2', 'cli': str(cli), 'cli_sha256': sha(cli),
              'git_head': head, 'git_dirty': None if head is None else bool(git('status', '--porcelain')),
              'sources_sha256': sources, 'sources_combined_sha256': combined,
              'environment': environment(cli),
              'subjects': len(found), 'known_answer_vectors': len(everything) - len(found),
              'unencodable_cases': unencodable, 'workers': list(WORKERS), 'chunk_tuples': list(CHUNKS),
              'runs': sum(1 + len(row['parallel']) for row in results),
              'multi_range_multi_worker_runs': sum(1 for planned, active in observed if planned > 1 and active > 1),
              'results': results,
              'limits': {'tuple_cap': {mode: {'exit': row['exit'], 'status': row['report']['status']}
                                       for mode, row in capped.items()},
                         'deadline_parallel_only': deadline},
              'decision_debug_timing_single_run': {
                  'seconds': debug_timing,
                  'speedup': {str(w): round(debug_timing['serial'] / debug_timing[str(w)], 2) for w in WORKERS},
                  'claim': 'one run of the CLI under test; a local measurement, not a guarantee'},
              'claim': 'identical exit, report and receipt bytes in every mode for these subjects and known answers; '
                       'timings are local measurements'}
    if args.release_timing:
        report['release_timing'] = release_timing(work, decision, args.timing_runs)
    require(source_digests() == (sources, combined), 'source changed during build/parity/timing')
    report['source_stable'] = True
    report['tool_seconds'] = round(time.monotonic() - started, 1)
    (work / 'result.json').write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    summary = {key: report[key] for key in ('subjects', 'known_answer_vectors', 'runs',
                                            'multi_range_multi_worker_runs', 'sources_combined_sha256')}
    summary['decision_debug_speedup'] = report['decision_debug_timing_single_run']['speedup']
    if args.release_timing:
        summary['decision_release_median_speedup'] = report['release_timing']['median_speedup']
    print(json.dumps(summary, sort_keys=True))
    print(f'evidence: {work / "result.json"}')
    print('transform-parallel-parity: PASS')


if __name__ == '__main__':
    main()
