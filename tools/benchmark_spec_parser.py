#!/usr/bin/env python3
"""Measure the same complete parsing workload against frozen release libraries.

Three paired runs alternate order. Each case has five warmups and seven samples;
reported time includes dropping the returned AST/diagnostics. No external Python
dependency is required. A changed acceptance outcome is a failure, not a speedup.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def cases(corpus):
    yield 'header', 'zeno 1; project 1 p;', 10_000, None
    for count, rounds in [(100, 400), (1000, 40)]:
        source = 'zeno 1; project 1 p;\n' + ''.join(
            f'namespace {n + 2} n{n};\n' for n in range(count))
        yield f'{count}_declarations', source, rounds, None
    chain = ' + '.join(['1'] * 200)
    yield 'arithmetic_chain_200', f'zeno 1; project 1 p; law 2 l = {chain} == 200;', 200, None
    yield 'nested_not_100', 'zeno 1; project 1 p; law 2 l = ' + 'not ' * 100 + 'true;', 200, None
    yield 'invalid_bytes_2000', '?' * 2000, 100, None
    groups = [
        ('tables', ('contract-tables',)),
        ('collections', ('collections',)),
        ('distribution', ('distribution-laws',)),
    ]
    fixtures = sorted(corpus.rglob('*.zeno'))
    for name, parts in groups:
        selected = [p for p in fixtures if all(part in p.parts for part in parts)
                    and 'target' not in p.parts]
        if selected:
            # Deterministic middle-size fixture; exact bytes and path are recorded.
            selected.sort(key=lambda p: (p.stat().st_size, p.as_posix()))
            fixture = selected[len(selected) // 2]
            source = fixture.read_text()
            yield f'extension_{name}', source, 100, fixture.relative_to(corpus).as_posix()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-lib', required=True, type=Path)
    parser.add_argument('--candidate-lib', required=True, type=Path)
    parser.add_argument('--rustc', required=True, type=Path)
    parser.add_argument('--baseline-revision', required=True)
    parser.add_argument('--candidate-revision', required=True)
    parser.add_argument('--corpus-root', type=Path, default=ROOT)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    (out / 'comparison.json').unlink(missing_ok=True)
    rows, inputs = [], []
    for name, source, rounds, fixture in cases(args.corpus_root.resolve()):
        path = out / f'{name}.zeno'
        path.write_text(source)
        rows.append(f'{name}\t{path}\t{rounds}')
        inputs.append({'case': name, 'source_sha256': digest(path), 'bytes': path.stat().st_size,
                       'iterations_per_sample': rounds, 'fixture': fixture})
    manifest = out / 'cases.tsv'
    manifest.write_text('\n'.join(rows) + '\n')
    harness = ROOT / 'tools/spec_parser_bench.rs'
    builds = {}
    for name, library in [('baseline', args.baseline_lib), ('candidate', args.candidate_lib)]:
        library = library.resolve()
        dependencies = library.parent / 'deps' if (library.parent / 'deps').is_dir() else library.parent
        command = [str(args.rustc.resolve()), '--edition=2024', '-C', 'opt-level=3',
                   '-C', 'overflow-checks=yes', '-C', 'lto=thin', '-L', f'dependency={dependencies}',
                   '--extern', f'zeno_fcis_spec={library}', str(harness), '-o', str(out / name)]
        compiled = subprocess.run(command, capture_output=True, text=True, timeout=120)
        (out / f'{name}-compile.log').write_text(compiled.stdout + compiled.stderr)
        if compiled.returncode:
            raise RuntimeError(f'benchmark compilation failed: {name}')
        builds[name] = {'command': command, 'library_sha256': digest(library),
                        'binary_sha256': digest(out / name)}
    runs = []
    for pair in range(3):
        order = ['baseline', 'candidate'] if pair % 2 == 0 else ['candidate', 'baseline']
        for name in order:
            result = subprocess.run([str(out / name), str(manifest)], capture_output=True,
                                    text=True, timeout=120)
            (out / f'pair-{pair}-{name}.jsonl').write_text(result.stdout)
            (out / f'pair-{pair}-{name}.stderr').write_text(result.stderr)
            if result.returncode:
                raise RuntimeError(f'benchmark process failed: pair {pair}, {name}, {result.returncode}')
            decoded = [json.loads(line) for line in result.stdout.splitlines()]
            runs.append({'pair': pair, 'order': order, 'implementation': name,
                         'metadata': decoded[0]['metadata'], 'cases': decoded[1:]})
    comparison = []
    for item in inputs:
        name = item['case']
        observations = {version: [next(c for c in run['cases'] if c['case'] == name)
                                 for run in runs if run['implementation'] == version]
                        for version in ['baseline', 'candidate']}
        statuses = {c['accepted'] for values in observations.values() for c in values}
        if len(statuses) != 1:
            raise RuntimeError(f'acceptance changed; cannot report a parsing speedup: {name}')
        medians = {version: statistics.median(c['median_ns'] for c in values)
                   for version, values in observations.items()}
        comparison.append({**item, 'accepted': statuses.pop(),
                           'baseline_us': medians['baseline'] / 1000,
                           'candidate_us': medians['candidate'] / 1000,
                           'candidate_over_baseline': medians['candidate'] / medians['baseline']})
    report = {'schema': 'zeno-fcis/spec-parser-benchmark/2',
              'baseline_revision': args.baseline_revision, 'candidate_revision': args.candidate_revision,
              'harness_sha256': digest(harness), 'driver_sha256': digest(Path(__file__)),
              'rustc': subprocess.run([str(args.rustc), '-vV'], capture_output=True, text=True,
                                     check=True).stdout,
              'platform': platform.platform(), 'builds': builds, 'runs': runs, 'comparison': comparison,
              'method': 'Three alternating paired runs, five warmups and seven samples per case; '
                        'median of run medians; includes parse result drop. Same release compiler '
                        'and harness. Shared host, indicative local comparison only.'}
    (out / 'comparison.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(comparison, indent=2))


if __name__ == '__main__':
    main()
