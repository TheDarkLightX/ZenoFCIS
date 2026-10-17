#!/usr/bin/env python3
"""Retain local accounting controls, native witnesses and Lean receipts."""
import argparse
import dataclasses
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
from collections import Counter

from check_exact_value import check_transition, endpoint
from check_no_free_value import command_domain, initial_domain, valuations
from native_check import probe
from search_compounding import initialized, json_value, seeds
from spot_reference import step

APP = Path(__file__).resolve().parents[1]
ROOT = APP.parents[1]
PROOF = APP / 'proofs/ExactValue.lean'

CONTROLS = {
    'isolated_actor': 'test_isolated_actor_control',
    'reject_is_no_op': 'test_reject_is_no_op_control',
    'asset_conservation': 'test_asset_conservation_control',
    'outside_liquid_non_decreasing': 'test_outside_liquid_control',
    'positive_locked_pool': 'test_positive_locked_pool_control',
    'creation_cost': 'test_creation_cost_control',
    'fixed_positive_outside_shares': 'test_fixed_outside_shares_control',
    'reserve_per_share_product': 'test_product_control',
    'liquidity_reserves_per_share': 'test_liquidity_ratio_control',
    'sum_of_member_floor_errors': 'test_floor_error_range_control',
    'endpoint_dust': 'test_endpoint_dust_control',
    'dust_telescope_identity': 'test_dust_telescope_identity_control',
    'exact_no_free_value': 'test_exact_no_free_value_control',
    'cumulative_weighted_dust_bound': 'test_weighted_bound_control',
    'one-atom-coalition': 'test_two_member_one_atom_bound_is_false',
    'credit-unvisited-search': 'test_long_search_counts_and_all_three_measures',
    'omit-excess-measurement': 'test_long_search_counts_and_all_three_measures',
    'ignore-growing-cycle': 'test_growing_cycle_detector_control',
    'omit-timeout-record': 'test_runner_timeout_retains_measured_work',
    'wrong-lean-pin': 'test_proof_tool_and_resource_gates',
    'wrong-lean-tree': 'test_proof_tool_and_resource_gates',
    'unchecked-proof': 'test_proof_tool_and_resource_gates',
    'low-disk-proof': 'test_proof_tool_and_resource_gates',
    'changed-proof-source': 'test_proof_tool_and_resource_gates',
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(argv, out, label, *, env=None, expected=0, timeout=90):
    out.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    try:
        result = subprocess.run(list(map(str, argv)), cwd=ROOT, env=env,
                                text=True, capture_output=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired as error:
        def as_text(value):
            return value.decode(errors='replace') if isinstance(value, bytes) else value or ''
        record = {'argv': list(map(str, argv)), 'exit_code': 124, 'status': 'timeout-UNKNOWN',
                  'seconds': round(time.monotonic()-started, 3),
                  'stdout': as_text(error.stdout), 'stderr': as_text(error.stderr)}
        (out / f'{label}.command.json').write_text(json.dumps(record, indent=2)+'\n')
        raise ValueError(f'{label}: timed out; measured work retained') from error
    record = {'argv': list(map(str, argv)), 'exit_code': result.returncode,
              'seconds': round(time.monotonic()-started, 3),
              'stdout': result.stdout, 'stderr': result.stderr}
    (out / f'{label}.command.json').write_text(json.dumps(record, indent=2)+'\n')
    if result.returncode != expected:
        raise ValueError(f'{label}: expected exit {expected}, got {result.returncode}; inspect retained command')
    return record


def controls(out):
    baseline = run([sys.executable, '-m', 'unittest', 'discover', '-s', APP / 'tests', '-v'], out, 'baseline')
    observations = []
    for defect, name in CONTROLS.items():
        env = dict(os.environ, SPOT_STAGE4_DEFECT=defect, PYTHONPATH=str(APP / 'tests'))
        row = run([sys.executable, '-m', 'unittest', f'test_stage4.Stage4.{name}', '-v'],
                  out, defect, env=env, expected=1)
        if name not in row['stderr'] or 'FAIL' not in row['stderr']:
            raise ValueError(f'{defect}: named test did not turn red')
        observations.append({'defect': defect, 'named_test': name, 'exit_code': 1})
    return {'baseline_exit': baseline['exit_code'], 'planted_defects': observations}


def premises(out):
    counts = Counter()
    native_candidates = []
    covered = set()
    # Every Cartesian initial snapshot, all 50 commands and each coalition.
    # These are one-step premises, not a claim that the seeded states are reachable.
    for before in initial_domain():
        for command in itertools.chain.from_iterable(command_domain(a) for a in (0, 1)):
            result = step(before, command)
            counts['edges'] += 1
            counts['accepted' if result.reason is None else 'rejected'] += 1
            for coalition in ((0,), (1,), (0, 1)):
                if command.actor not in coalition:
                    continue
                check_transition(before, command, result, coalition)
                endpoint(before, result.post, coalition)
                counts['coalition_edge_checks'] += 1
            key = command.action, result.reason, command.direction
            if key not in covered:
                covered.add(key)
                native_candidates.append((before, command, result))
    return {'counts': dict(counts), 'counterexamples': [],
            'scope': '448 seeded states x 50 commands; local oracle contracts; not full native refinement'}, native_candidates


def witnesses(cli, out):
    records = []
    for i, seed in enumerate(seeds()):
        genesis, prefix, command = seed
        current = genesis
        for j, operation in enumerate(prefix):
            result = step(current, operation)
            if result.reason is not None:
                raise ValueError('reachable prefix refused')
            probe(current, operation, result, out / f'witness-{i}', f'prefix-{j}', cli=cli)
            current = result.post
        before = current
        result = step(before, command)
        probe(before, command, result, out / f'witness-{i}', 'edge', cli=cli)
        coalition = (0, 1) if i == 1 else (0,)
        check_transition(before, command, result, coalition)
        observation = endpoint(before, result.post, coalition)
        def residual(w):
            return tuple(r-sum(w.shares[a]*r//w.pool.supply for a in (0, 1))
                         for r in (w.pool.r0, w.pool.r1))
        records.append({'genesis': genesis, 'prefix': prefix, 'before': before,
                        'command': command, 'after': result.post, 'coalition': coalition,
                        'accounting': observation,
                        'other_account_floor_before': valuations(before, (1,))[0],
                        'other_account_floor_after': valuations(result.post, (1,))[0],
                        'locked_residual_before': residual(before),
                        'locked_residual_after': residual(result.post),
                        'minimality': 'one nonidentity operation after a reachable baseline; not a global integer-size minimum'})
    return json_value({'schema': 'spot4-native-witnesses/1', 'cli_sha256': sha(cli), 'witnesses': records})


def proof(lean_root, cli, out):
    lean = lean_root / 'bin/lean'
    version = run([lean, '--version'], out, 'lean-version')['stdout'].strip()
    if 'version 4.30.0,' not in version or 'd024af099ca4bf2c86f649261ebf59565dc8c622' not in version:
        raise ValueError('wrong Lean pin')
    inventory = run([cli, '--no-kb', 'backend', 'inventory-lean', lean_root], out, 'lean-tree', timeout=600)['stdout']
    expected = (ROOT / 'release/lean-4.30.0-tree.sha256').read_text().strip()
    if f'lean tree_sha256 {expected}' not in inventory:
        raise ValueError('Lean runtime tree differs from release pin')
    source = PROOF.read_text()
    source_identity = hashlib.sha256(source.encode()).hexdigest()
    if re.search(r'\b(sorry|admit|axiom|unsafe|native_decide|implemented_by)\b', source):
        raise ValueError('proof contains a placeholder or unchecked mechanism')
    def compile_file(path, label, expected_exit=0):
        # The brief's low-disk refusal also applies to proof builds.
        disk = run(['df', '-B1G', '/'], out, label+'-disk')['stdout']
        if shutil.disk_usage('/').free < 6*1024**3:
            raise ValueError('below 6 GiB: stop builds')
        return run([lean, '-t', '0', path], out, label, expected=expected_exit)
    snapshot = out / 'ExactValue.checked.lean'
    snapshot.write_text(source)
    checked = compile_file(snapshot, 'lean-positive')
    if 'sorryAx' in checked['stdout'] or 'error:' in checked['stdout']:
        raise ValueError('untrusted or failed proof')
    mutations = {
        'remove-swap-k': ('(a.supply*a.x)*(a.supply*a.y) ≤ (b.supply*b.x)*(b.supply*b.y) → PoolStep a b', 'True → PoolStep a b'),
        'remove-mint-bound': ('m*a.x ≤ u → m*a.y ≤ v → PoolStep a b', 'True → m*a.y ≤ v → PoolStep a b'),
        'remove-conservation': ('conservation0 : total0 a = total0 b', 'conservation0 : True'),
        'remove-isolation': ('isolation0 : a.outside0 ≤ b.outside0', 'isolation0 : True'),
        'remove-positive-lock': ('(hq : 0 < q) (a b : State)', '(hq : 0 ≤ q) (a b : State)'),
        'coalition-bound-one': ('error before - error after < (before.length : Rat) := by', 'error before - error after < 1 := by'),
        'pool-member-floors': ('(xs.map fun x => (x.floor : Rat)).sum', '(xs.sum.floor : Rat)'),
    }
    negatives = []
    for name, (old, new) in mutations.items():
        if old not in source:
            raise ValueError(f'proof mutation anchor absent: {name}')
        target = out / f'{name}.lean'
        target.write_text(source.replace(old, new, 1))
        row = compile_file(target, name, 1)
        if 'error:' not in row['stdout']:
            raise ValueError(f'{name}: no named Lean error')
        negatives.append({'defect': name, 'check': 'Lean kernel elaboration', 'exit_code': 1})
    if sha(PROOF) != source_identity:
        raise ValueError('proof source changed during checks; rerun')
    return {'schema': 'spot4-lean-proof/1', 'source_sha256': source_identity,
            'lean_binary_sha256': sha(lean), 'lean_version': version,
            'runtime_tree_sha256': expected, 'trust_level': 0,
            'axioms_output': checked['stdout'], 'planted_defects': negatives,
            'level': 'KernelChecked algebraic contracts and rounding; full executable refinement open'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('controls', 'premises', 'witnesses', 'proof'))
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--cli', type=Path)
    parser.add_argument('--lean-root', type=Path)
    args = parser.parse_args()
    producer_identity = sha(Path(__file__))
    args.out.mkdir(parents=True, exist_ok=True)
    if args.mode in ('witnesses', 'proof') and args.cli is None:
        parser.error('--cli required')
    if args.mode == 'controls':
        result = controls(args.out)
    elif args.mode == 'premises':
        result, candidates = premises(args.out)
        if args.cli:
            for i, (before, command, decision) in enumerate(candidates):
                probe(before, command, decision, args.out / 'native', str(i), cli=args.cli)
            result['native_representatives_matched'] = len(candidates)
            result['cli_sha256'] = sha(args.cli)
    elif args.mode == 'witnesses':
        result = witnesses(args.cli, args.out)
    else:
        if args.lean_root is None:
            parser.error('--lean-root required')
        result = proof(args.lean_root, args.cli, args.out)
    result['tool_source_sha256'] = producer_identity
    (args.out / 'summary.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({k: v for k, v in result.items() if k not in ('witnesses', 'axioms_output')}))


if __name__ == '__main__':
    main()
