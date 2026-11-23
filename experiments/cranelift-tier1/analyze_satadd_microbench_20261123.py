#!/usr/bin/env python3
"""Paired x86-64 SysV native object-microbenchmark analyzer.

Blocks are bootstrap-resampled deterministically only to characterize noise on
this one host. No cross-CPU statistical generalization, instruction causality,
end-to-end Wasmtime benefit or reliable speedup is implied.
"""
from __future__ import annotations
import argparse, csv, hashlib, json, math, random, statistics
from collections import defaultdict
from pathlib import Path

def analyze(path: Path, swapped: bool = False):
    raw = path.read_bytes(); pairs = defaultdict(dict)
    for row in csv.DictReader(raw.decode().splitlines()):
        key = (row['workload'], int(row['block']))
        v = row['variant']
        if swapped:
            v = 'experimental' if v == 'baseline' else 'baseline'
        if v in pairs[key]:
            raise ValueError(f'duplicate sample: {key}, {v}')
        pairs[key][v] = int(row['elapsed_ns']) / int(row['calls'])
    rng = random.Random(20261123)
    result = {}
    for mode in ('no_overflow', 'positive_overflow', 'negative_overflow', 'mixed_random'):
        samples = [p for (name, _), p in sorted(pairs.items()) if name == mode]
        if len(samples) < 4 or any(set(x) != {'baseline', 'experimental'} for x in samples):
            raise ValueError('missing A/B pair or too few blocks')
        log_speedups = [math.log(x['baseline'] / x['experimental']) for x in samples]
        observed = math.exp(statistics.mean(log_speedups))
        boot = sorted(math.exp(statistics.mean(rng.choices(log_speedups, k=len(log_speedups)))) for _ in range(10000))
        lo, hi = boot[249], boot[9749]
        result[mode] = {
            'blocks': len(samples), 'ratio_baseline_time_divided_by_experimental_time': observed,
            'within_run_block_bootstrap95_low': lo, 'within_run_block_bootstrap95_high': hi,
            'median_ns_per_call_baseline': statistics.median(x['baseline'] for x in samples),
            'median_ns_per_call_experimental': statistics.median(x['experimental'] for x in samples),
            'candidate_faster_on_this_host_under_bootstrap_model': lo > 1.0,
            'candidate_slower_on_this_host_under_bootstrap_model': hi < 1.0,
        }
    return {'schema': 'zenofcis/tier1-satadd-native-throughput/2',
            'csv_sha256': hashlib.sha256(raw).hexdigest(),
            'inputs_swapped_for_order_control': swapped,
            'results': result,
            'scope': 'Function-pointer call throughput of exact ELF objects on a single host, not whole-application Wasm, target-independent speedup or compiler pipeline performance; resampled blocks are not independent CPU samples.'}

if __name__ == '__main__':
    ap=argparse.ArgumentParser(); ap.add_argument('csv',type=Path); ap.add_argument('--swapped',action='store_true'); ap.add_argument('--out',type=Path)
    x=ap.parse_args(); out=json.dumps(analyze(x.csv,x.swapped),indent=2,sort_keys=True)+'\n'
    if x.out: x.out.write_text(out)
    else: print(out,end='')
