#!/usr/bin/env python3
"""Bounded native exploit checks; observations never authorize publication.

The existing integer oracle predicts full decisions. A separate Fraction-based
checker checks accepted arithmetic and cash/LP accounting at the same states.
All campaign mutation is invocation-local, caller-owned diagnostic scratch.
"""
from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import asdict
from fractions import Fraction
import hashlib
import json
import math
from pathlib import Path
import random

from check_exact_value import check_transition, endpoint
from native_check import probe
from search_compounding import seeds
from spot_reference import Command, J, K, KINDS, N, U, World, step

if not __debug__:
    raise RuntimeError('run the diagnostic checker with Python assertions enabled')

APP = Path(__file__).resolve().parents[1]
ROOT = APP.parents[1]


def amounts(world, members=(0, 1)):
    """Cash, exact LP wealth and separately quoted LP wealth, in raw atoms."""
    cash = tuple(sum(world.balances[4+2*a+i] for a in members) for i in (0, 1))
    if world.pool is None:
        return cash, tuple(map(Fraction, cash)), cash
    p = world.pool
    claims = tuple(tuple(Fraction(world.shares[a]*r, p.supply) for a in members)
                   for r in (p.r0, p.r1))
    return (cash, tuple(c+sum(xs) for c, xs in zip(cash, claims)),
            tuple(c+sum(map(math.floor, xs)) for c, xs in zip(cash, claims)))


def closed_score(before, after, members=(0, 1)):
    """Refuse to score an unclosed account position as a closed strategy."""
    if tuple(before.shares[a] for a in members) != tuple(after.shares[a] for a in members):
        raise AssertionError('positions_not_restored')
    b, a = amounts(before, members), amounts(after, members)
    delta = tuple(tuple(y-x for x, y in zip(xs, ys)) for xs, ys in zip(b, a))
    if min(delta[0]) >= 0 and max(delta[0]) > 0:
        raise AssertionError('closed_spendable_profit')
    return dict(zip(('spendable_delta', 'exact_delta', 'quoted_delta'), delta))


def rational_check(before, cmd, result):
    """Exact rational formulas, independent of the oracle's mf/mc helpers."""
    if result.reason is not None:
        if result.post != before or result.effects or result.outputs:
            raise AssertionError('reject_is_no_op')
        return
    p, post, out = before.pool, result.post.pool, result.outputs
    if cmd.action == 0:
        assert cmd.sqrt**2 <= cmd.a*cmd.b < (cmd.sqrt+1)**2
        assert (post.r0, post.r1, post.supply) == (cmd.a, cmd.b, cmd.sqrt)
        assert out == (cmd.sqrt-K, K, 0, 0, 0, 0, 0, 0)
    elif cmd.action in (1, 2):
        x, y = (p.r0, p.r1) if cmd.direction == 0 else (p.r1, p.r0)
        gross = cmd.a if cmd.action == 1 else math.ceil(Fraction(
            math.ceil(Fraction(x*cmd.a, y-cmd.a))*10000, 9970))
        fee = math.ceil(Fraction(gross*30, 10000))
        protocol = math.floor(Fraction(fee*2000, 10000))
        net = gross-fee
        quote = math.floor(Fraction(y*net, x+net))
        paid = quote if cmd.action == 1 else cmd.a
        gap = 0 if cmd.action == 1 else math.ceil(Fraction((quote-paid)*10000, paid))
        assert out == (gross, paid, fee, protocol, fee-protocol, net,
                       0 if cmd.action == 1 else quote, gap)
        reserves = (x+gross-protocol, y-paid)
        assert (post.r0, post.r1) == (reserves if cmd.direction == 0 else reserves[::-1])
        assert post.supply == p.supply
    elif cmd.action == 3:
        if Fraction(cmd.a, p.r0) <= Fraction(cmd.b, p.r1):
            u, v = cmd.a, math.floor(Fraction(cmd.a*p.r1, p.r0))
        else:
            u, v = math.floor(Fraction(cmd.b*p.r0, p.r1)), cmd.b
        mint = math.floor(min(Fraction(u*p.supply, p.r0), Fraction(v*p.supply, p.r1)))
        assert out == (u, v, mint, cmd.a-u, cmd.b-v, 0, 0, 0)
        assert (post.r0, post.r1, post.supply) == (p.r0+u, p.r1+v, p.supply+mint)
    else:
        u, v = math.floor(Fraction(cmd.a*p.r0, p.supply)), math.floor(Fraction(cmd.a*p.r1, p.supply))
        assert out == (u, v, 0, 0, 0, 0, 0, 0)
        assert (post.r0, post.r1, post.supply) == (p.r0-u, p.r1-v, p.supply-cmd.a)
    check_transition(before, cmd, result, (0, 1))


def funded():
    # Fake-value prestate, not a funding route. Exercise all u128 balance limbs.
    return World(None, (0, 0, 0, 0, U-N, U-N, U-N, U-N))


def generated(world, baseline, rng, index):
    """Stateful two-account proposals; burns never consume baseline shares."""
    p = world.pool
    actor, direction = rng.randrange(2), rng.randrange(2)
    action = (1, 2, 3, 4)[index % 4]
    if action in (1, 2):
        x, y = (p.r0, p.r1) if direction == 0 else (p.r1, p.r0)
        candidates = (1, 2, 3, 333, 334, max(2, x//100), max(2, x//7), N, y-1)
        a = rng.choice(candidates) if action == 1 else rng.choice((1, 2, max(1, y//100), max(1, y//7), y))
        return Command(action, direction, max(1, a), limit0=N if action == 2 else 0,
                       actor=actor, recipient=rng.randrange(2))
    if action == 3:
        # Proportional and deliberately unbalanced offers, with floor neighbors.
        scale = rng.choice((1, 2, 10, 100, 1000))
        a, b = max(1, min(J, p.r0//scale)), max(1, min(J, p.r1//scale))
        if rng.randrange(4) == 0:
            a = 1
        return Command(3, a=a, b=b, actor=actor, recipient=actor)
    excess = world.shares[actor]-baseline.shares[actor]
    return Command(4, a=max(1, excess//rng.choice((1, 2, 3))), actor=actor, recipient=actor)


def encode(value):
    if isinstance(value, Fraction):
        return str(value)
    if isinstance(value, dict):
        return {str(k): encode(v) for k, v in value.items()}
    if isinstance(value, (tuple, list)):
        return [encode(v) for v in value]
    return value


def campaign(cli, out, *, seed=20261017, length=64):
    if not 16 <= length <= 1024:
        raise ValueError('length must be between 16 and 1024')
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    rng, counts, reasons, closures, marks = random.Random(seed), Counter(), Counter(), [], []
    accepted_by_label = Counter()
    identities = [Path(__file__), APP/'tools/spot_reference.py', APP/'tools/native_check.py',
                  APP/'tools/check_exact_value.py', APP/'tools/check_no_free_value.py',
                  APP/'tools/search_compounding.py', APP/'tools/build_contracts.py', APP/'v2/policy.json',
                  APP/'v2/policy.zcve', APP/'v2/schema.zcve', APP/'project.zeno', Path(cli)]
    hashes = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in identities}

    def call(world, cmd, label):
        result = step(world, cmd)
        # Every proposed edge is replayed, including each refusal and retry.
        probe(world, cmd, result, out/'native', f'{counts["calls"]:05d}-{label}', cli=cli)
        counts['calls'] += 1
        rational_check(world, cmd, result)
        counts['rejected' if result.reason else 'accepted'] += 1
        if result.reason:
            reasons[result.reason] += 1
        else:
            counts[f'accepted:{KINDS[cmd.action]}'] += 1
            counts[f'actor:{cmd.actor}'] += 1
            accepted_by_label[f'{label}:{KINDS[cmd.action]}'] += 1
            if cmd.action in (1, 2):
                counts[f'accepted:{KINDS[cmd.action]}:direction:{cmd.direction}'] += 1
        return result

    def create(a, b):
        result = call(funded(), Command(0, a=a, b=b, sqrt=math.isqrt(a*b)), 'create')
        assert result.reason is None
        return result.post

    def close(base, current, label):
        for actor in (0, 1):
            extra = current.shares[actor]-base.shares[actor]
            assert extra >= 0
            if extra:
                result = call(current, Command(4, a=extra, actor=actor, recipient=actor), 'close')
                if result.reason is not None:
                    # Record unclosable dust explicitly; it earns no closure credit.
                    counts['unclosed_traces'] += 1
                    return current
                current = result.post
        score = closed_score(base, current)
        closures.append({'label': label, 'baseline': asdict(base), 'final': asdict(current), **score})
        counts['closed_traces'] += 1
        return current

    # Historical open-position quotation counterexamples remain counterexamples.
    for i, (world, prefix, cmd) in enumerate(seeds()):
        for op in prefix:
            result = call(world, op, f'witness-{i}-prefix')
            assert result.reason is None
            world = result.post
        result = call(world, cmd, f'witness-{i}')
        assert result.reason is None
        mark = endpoint(world, result.post, (0, 1) if i == 1 else (0,))
        if i == 1:
            assert mark['rounding_excess'][1] == Fraction(170371, 154371)
        if i == 2:
            assert mark['floored_delta'] == (1, 0)
            bob_before, bob_after = amounts(world, (1,))[2], amounts(result.post, (1,))[2]
            assert tuple(a-b for a, b in zip(bob_after, bob_before)) == (-1, 0)
        marks.append(mark)

    # Reachable stateful lanes: ordinary, minimum-share, nearly empty, extremes.
    for lane, reserves in enumerate(((100000, 200000), (1001, 1001), (2, 501001),
                                     (1, J), (J, 1))):
        base = current = create(*reserves)
        assert base.shares[1] == 0  # fresh LP account is included from the baseline
        for index in range(length):
            cmd = generated(current, base, rng, index)
            current = call(current, cmd, f'lane-{lane}').post
        close(base, current, f'seeded-lane-{lane}')

    # Single versus split swaps, mints and burns, both directions; independent
    # branches share an identical baseline, and each branch closes its LP delta.
    base = create(100000, 200000)
    for action in (1, 2):
        for direction in (0, 1):
            for pieces in (1, 2, 4):
                current = base
                for _ in range(pieces):
                    result = call(current, Command(action, direction, 400//pieces,
                        limit0=N if action == 2 else 0), 'split-swap')
                    assert result.reason is None
                    current = result.post
                close(base, current, f'swap-{action}-direction-{direction}-pieces-{pieces}')
    for pieces in (1, 2, 4):
        current = base
        for _ in range(pieces):
            result = call(current, Command(3, a=400//pieces, b=800//pieces, actor=1, recipient=1), 'split-mint')
            assert result.reason is None
            current = result.post
        extra = current.shares[1]
        for index in range(pieces):
            burn = extra//pieces if index+1 < pieces else current.shares[1]
            result = call(current, Command(4, a=burn, actor=1, recipient=1), 'split-burn')
            assert result.reason is None
            current = result.post
        close(base, current, f'mint-burn-pieces-{pieces}')

    # A truly empty ordinary slot acquires both assets from in-coalition swap
    # receipts, then mints and burns its own shares. No balance is injected midtrace.
    empty = funded()
    empty = World(None, empty.balances[:6]+(0, 0))
    result = call(empty, Command(0, a=100000, b=200000, sqrt=141421), 'fresh-create')
    assert result.reason is None
    base = current = result.post
    assert current.shares[1] == 0 and current.balances[6:8] == (0, 0)
    for direction in (0, 1):
        result = call(current, Command(1, direction, 1000, recipient=1), 'fresh-receipt')
        assert result.reason is None
        current = result.post
    result = call(current, Command(3, a=current.balances[6], b=current.balances[7],
        actor=1, recipient=1), 'fresh-mint')
    assert result.reason is None
    close(base, result.post, 'fresh-empty-account')

    # Majority LP recovers its share of retained fees on final burn; fee charges
    # are therefore not counted as an extra unrecoverable cost in closed_score.
    base = funded()
    current = call(base, Command(0, a=10**7, b=10**7, sqrt=10**7), 'majority-create').post
    owned = Fraction(current.shares[0], current.pool.supply)
    assert owned > Fraction(999, 1000)
    paid_lp_fees = 0
    for direction in (0, 1, 0, 1):
        result = call(current, Command(1, direction, 500000), 'majority-swap')
        assert result.reason is None
        paid_lp_fees += result.outputs[4]
        current = result.post
    current = close(base, current, 'majority-lp-fee-recovery')
    assert current.shares[:2] == (0, 0) and paid_lp_fees > 0

    # Width and zero-output neighbors. ID 5 is inside the schema domain but is
    # refused by the decision policy; larger IDs are outside admission entirely.
    tiny = create(2, 501001)
    result = call(tiny, Command(1, direction=1, a=600000), 'one-atom-reserve')
    assert result.reason is None and result.post.pool.r0 == 1
    for cmd, reason in ((Command(1, a=1), 'ZERO_NET'),
                        (Command(1, direction=1, a=2), 'ZERO_OUTPUT'),
                        (Command(3, a=1, b=1, actor=1, recipient=1), 'ZERO_USED'),
                        (Command(4, a=1), 'ZERO_PAYOUT')):
        assert call(tiny, cmd, 'boundary').reason == reason
    small = call(funded(), Command(0, a=1000, b=1000, sqrt=1000), 'minimum-refusal')
    assert small.reason == 'INITIAL_TOO_SMALL'
    assert call(base, Command(5, a=1), 'unknown-command').reason == 'DOMAIN'
    for actor in range(2, 6):
        result = call(tiny, Command(1, a=100, actor=actor, recipient=0), 'reserved-actor')
        assert result.reason == 'RESERVED_ROLE'
    for recipient in range(2, 6):
        assert call(tiny, Command(1, a=100, recipient=recipient), 'reserved-recipient').reason == 'RESERVED_ROLE'
    assert call(tiny, Command(4, a=1, actor=3), 'locked-burn').reason == 'RESERVED_LP_LOCK'
    assert call(tiny, Command(1, a=N), 'reserve-cap').reason == 'POST_DOMAIN'

    # Acceptance obligations prevent a campaign of rejections receiving credit.
    required = [f'accepted:{kind}' for kind in KINDS] + ['actor:0', 'actor:1'] + [
        f'accepted:{kind}:direction:{d}' for kind in KINDS[1:3] for d in (0, 1)]
    for name in required:
        assert counts[name] > 0, f'missing accepted-call coverage: {name}'
    for kind in KINDS[1:]:
        assert accepted_by_label[f'lane-0:{kind}'] > 0, f'missing mixed stateful coverage: {kind}'
    assert counts['closed_traces'] >= 10
    assert all(hashlib.sha256(p.read_bytes()).hexdigest() == hashes[str(p)] for p in identities), 'source drift'
    summary = encode({'schema': 'spot-closed-strategy-checks/1', 'seed': seed, 'length': length,
                      'counts': dict(counts), 'accepted_by_label': dict(accepted_by_label),
                      'rejection_reasons': dict(reasons),
                      'closed_scores': closures, 'quotation_regressions': marks,
                      'majority_lp_fraction': owned, 'majority_lp_fees': paid_lp_fees,
                      'source_sha256': hashes,
                      'scope': 'finite native two-account checks; arbitrary-account contract proof separate; no refinement'})
    (out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--seed', type=int, default=20261017)
    parser.add_argument('--length', type=int, default=64)
    args = parser.parse_args()
    summary = campaign(args.cli.resolve(), args.out, seed=args.seed, length=args.length)
    print(json.dumps({key: summary[key] for key in ('seed', 'length', 'counts', 'rejection_reasons')}))


if __name__ == '__main__':
    main()
