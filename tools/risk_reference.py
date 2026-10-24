#!/usr/bin/env python3
"""Independent integer/Fraction brute force for the bounded swap-pool risk example.

This does not import the Rust evaluator or implement its dynamic program. It
lists every command string, replays its elementary pool equations, compares
all three questions, and independently replays the returned witnesses.
"""
from __future__ import annotations
import argparse
from fractions import Fraction
from itertools import product
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
EXAMPLE = ROOT / 'examples/quantitative-risk/swap-pool'

def transition(state, amount, sign):
    reserve, _, _ = state
    if amount > reserve:
        return (reserve, 0, 1), 0
    fee = sign * (1 if amount == 0 else 2)
    return (reserve - amount, fee, 0), fee

def brute(capacity, sign, horizon):
    genesis = (capacity, 0, 0)
    reached = {genesis}
    values = []
    for length in range(horizon + 1):
        for commands in product(range(4), repeat=length):
            state, value = genesis, 0
            for amount in commands:
                state, fee = transition(state, amount, sign)
                value += fee
            reached.add(state)
            values.append(value)
    return reached, min(values), max(values)

def rational(value):
    return Fraction(int(value['numerator']), int(value['denominator']))

def invoke(cli, app, args):
    result = subprocess.run([str(cli), 'contract', 'risk', str(app), *args, '--format', 'json'],
                            capture_output=True, text=True, timeout=60, check=False)
    assert result.returncode == 0, (result.returncode, result.stdout, result.stderr)
    packet = json.loads(result.stdout)
    assert packet['status'] == 'checked' and packet['authority'] == 'none'
    assert packet['claim'] and packet['checker'] and packet['sources']
    return packet['result']

def replay(run, capacity, sign, horizon):
    assert len(run) <= horizon
    state, value = (capacity, 0, 0), 0
    for raw in run:
        row = tuple(map(int, raw))
        assert len(row) == 5 and row[:3] == state and row[4] == 0
        assert 0 <= row[3] <= 3
        state, fee = transition(state, row[3], sign)
        value += fee
    return state, value

def check_count(result, domain, predicate, witness_reader):
    expected = sum(map(predicate, domain))
    assert int(result['count']) == expected
    assert int(result['total']) == len(domain)
    assert int(result['does_not_satisfy']) == len(domain) - expected
    assert rational(result['share']) == Fraction(expected, len(domain))
    for name, wanted, possible in [('satisfying_witness', True, expected),
                                    ('other_witness', False, len(domain) - expected)]:
        witness = result[name]
        if not possible:
            assert witness is None
        else:
            value = witness_reader(witness)
            assert value in domain and bool(predicate(value)) == wanted


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--scratch', type=Path, required=True)
    args = parser.parse_args()
    cli = args.cli.resolve()
    args.scratch.mkdir(parents=True, exist_ok=True)
    comparisons = 0
    summaries = []
    for capacity, sign in product(range(5), (1, -1)):
        project = (EXAMPLE / 'project.zeno').read_text().replace('Reserve in 0..=4', f'Reserve in 0..={capacity}')
        if sign < 0:
            project = project.replace('Fee in 0..=2', 'Fee in -2..=2')
        policy = json.loads((EXAMPLE / 'v2/policy.json').read_text())
        policy['genesis']['110'] = capacity
        if sign < 0:
            # This scratch model deliberately reverses the documented fee
            # rule, so omit that example-specific conformance law as well.
            documented_fee_law = (
                '// Amount above reserve commits a failure with no reserve change and zero reward; otherwise reserve\n'
                '// falls by amount and the fee is 1 for amount 0 or 2 for a positive amount.\n'
                'law 501 documented_success_outputs on commit = command.101.120 <= pre.100.110 -> '
                'post.100.110 == pre.100.110 - command.101.120 && '
                'post.100.111 == choose(command.101.120 == 0, 1, 2);\n'
            )
            assert documented_fee_law in project
            project = project.replace(documented_fee_law, '')
            assert policy['law_kinds'].pop('501') == 'FeeAndRounding'
            policy['cases'][1]['post']['111'] = '0 - choose(command.101.120 == 0, 1, 2)'
        with tempfile.TemporaryDirectory(prefix='risk-oracle-', dir=args.scratch) as temporary:
            app = Path(temporary)
            (app / 'v2').mkdir()
            (app / 'project.zeno').write_text(project)
            (app / 'v2/policy.json').write_text(json.dumps(policy))
            fees = range(3) if sign > 0 else range(-2, 3)
            inputs = list(product(range(capacity + 1), fees, range(2), range(4), [0]))
            result = invoke(cli, app, ['--where', 'post.100.112 == 1'])
            check_count(result, inputs, lambda row: row[3] > row[0], lambda w: tuple(map(int, w['input'])))
            comparisons += 1
            for horizon in range(5):
                reached, best, worst = brute(capacity, sign, horizon)
                result = invoke(cli, app, ['--reward', '610', '--steps', str(horizon)])
                for name, expected in [('best', best), ('worst', worst)]:
                    extreme = result[name]
                    assert rational(extreme['value']) == expected
                    assert extreme['length'] == len(extreme['run'])
                    _, value = replay(extreme['run'], capacity, sign, horizon)
                    assert value == expected
                    comparisons += 1
                result = invoke(cli, app, ['--states', '--steps', str(horizon), '--where', 'post.100.112 == 1'])
                check_count(result, reached, lambda s: s[2] == 1,
                            lambda w: state_witness(w, capacity, sign, horizon))
                comparisons += 1
                summaries.append({'capacity': capacity, 'sign': sign, 'K': horizon,
                                  'reachable': len(reached), 'best': best, 'worst': worst})
    # A declared pre-state with reserve 4, fee 2 and limit=true is unreachable.
    unreachable = invoke(cli, EXAMPLE, ['--where', 'pre.100.110 == 4 && pre.100.111 == 2 && pre.100.112 == 1'])
    assert int(unreachable['count']) == 4 and int(unreachable['total']) == 120
    reached, _, _ = brute(4, 1, 5)
    assert (4, 2, 1) not in reached
    comparisons += 1
    print(json.dumps({'status': 'PASS', 'comparisons': comparisons,
                      'domains': 10, 'horizons': 5, 'all_witnesses_replayed': True,
                      'unreachable_input': unreachable['satisfying_witness']['input'],
                      'cases': summaries}, indent=2))

def state_witness(witness, capacity, sign, horizon):
    state, _ = replay(witness['run'], capacity, sign, horizon)
    assert state == tuple(map(int, witness['state']))
    return state

if __name__ == '__main__':
    main()
