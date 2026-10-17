#!/usr/bin/env python3
"""Deterministic guided attack traces, with explicit finite budgets.

Search is proposal evidence only. The independent arithmetic checker judges
every selected edge and every reported endpoint. No native authority is issued.
"""
import argparse
import dataclasses
import json
import math
import random
import time
from collections import Counter
from fractions import Fraction
from pathlib import Path

from check_exact_value import check_transition, endpoint, errors
from check_no_free_value import dominates, valuations
from spot_reference import Command, World, step


def seeds():
    genesis = World(None, (0, 0, 0, 0, 10**9, 10**9, 10**9, 10**9))
    return (
        (genesis, (Command(0, a=100000, b=200000, sqrt=141421),), Command(1, a=51)),
        (genesis, (Command(0, a=156331, b=79390, sqrt=111405),
                   Command(3, a=174046, b=30619, actor=1, recipient=1)),
                   Command(1, direction=1, a=16)),
        (genesis, (Command(0, a=129879, b=61233, sqrt=89178),
                   Command(3, a=191320, b=4395, actor=1, recipient=1)),
                   Command(1, direction=1, a=7)),
    )


def initialized(seed):
    genesis, prefix, _ = seed
    current = genesis
    for command in prefix:
        result = step(current, command)
        if result.reason is not None:
            raise AssertionError("reachable_prefix_refused")
        current = result.post
    return current


def json_value(value):
    if isinstance(value, Fraction):
        return str(value)
    if dataclasses.is_dataclass(value):
        return json_value(dataclasses.asdict(value))
    if isinstance(value, dict):
        return {str(k): json_value(v) for k, v in value.items()}
    if isinstance(value, (tuple, list)):
        return [json_value(v) for v in value]
    return value


def repeated_configuration_gain(before, after, coalition):
    """A growing cycle must restore reserves, supply, status and LP holdings.

    Outside liquid/ingress may differ. A profitable repeat must still pass the
    exact endpoint oracle; a repeated configuration alone proves no gain.
    """
    return (before.pool == after.pool and before.shares == after.shares
            and dominates(valuations(after, coalition)[0], valuations(before, coalition)[0]))


def candidates(world, coalition, rng, count):
    choices = []
    for _ in range(count):
        actor = rng.choice(coalition)
        recipient = rng.choice(coalition)
        action = rng.choices((1, 2, 3, 4), (5, 2, 2, 2))[0]
        amount = rng.choice((1, 2, 3, 7, 16, 31, 51, 99, 100, 333, rng.randrange(2, 2000)))
        if action == 3:
            # Both proportional deposits and deliberately mismatched offers.
            b = max(1, amount*world.pool.r1//world.pool.r0)
            if rng.randrange(3) == 0:
                b = rng.randrange(1, 2000)
            command = Command(3, a=amount, b=b, actor=actor, recipient=actor)
        elif action == 4:
            owned = world.shares[actor]
            amount = rng.choice((1, amount, owned, max(1, owned//2)))
            command = Command(4, a=max(1, amount), actor=actor, recipient=recipient)
        else:
            command = Command(action, a=amount, direction=rng.randrange(2),
                              limit0=100000 if action == 2 else 0, actor=actor, recipient=recipient)
        choices.append(command)
    return choices


def search(length, trials=12, width=12, seed=20261017):
    rng = random.Random(seed)
    started = time.monotonic()
    counts = Counter()
    maximum = [0, 0]
    best_sum = 0
    best = None
    other_lp = None
    cycle = None
    max_positive_marks = [0, 0]
    maximum_excess = [Fraction(0), Fraction(0)]
    maximum_step_excess = [Fraction(0), Fraction(0)]
    maximum_positive_excess = [Fraction(0), Fraction(0)]
    excess_witnesses = [None, None]
    step_excess_witnesses = [None, None]
    accepted_kinds = Counter()
    for trial in range(trials):
        chosen_seed = seeds()[trial % 3]
        initial = initialized(chosen_seed)
        coalition = (0, 1) if trial % 4 == 3 else (0,)
        baseline = valuations(initial, coalition)[0]
        initial_errors = errors(initial, coalition)
        current, path = initial, []
        visited = {(initial.pool, initial.shares): (initial, 0)}
        positive_marks = [0, 0]
        positive_excess = [Fraction(0), Fraction(0)]
        for index in range(length):
            # Dedicated motifs plus guided mutation. Keep rejected proposals in
            # the count; they never become a credited completed search depth.
            mode = trial % 6
            if mode == 0:
                proposals = [chosen_seed[2]]
            elif mode == 1:
                if index % 2 == 0:
                    proposals = [Command(3, a=51, b=max(1, 51*current.pool.r1//current.pool.r0))]
                else:
                    proposals = [Command(4, a=max(1, current.shares[0]-initial.shares[0]))]
            elif mode == 2:
                proposals = [Command(1, a=51, direction=index % 2)]
            else:
                proposals = candidates(current, coalition, rng, width)
                if index == 0:
                    proposals.append(chosen_seed[2])
            evaluated = []
            current_errors = errors(current, coalition)
            for command in proposals:
                result = step(current, command)
                counts['proposed_edges'] += 1
                counts['accepted_proposals' if result.reason is None else 'rejected_proposals'] += 1
                # Signed per-step discrepancies telescope; never sum only
                # favorable marks and label that sum extracted value.
                post_errors = errors(result.post, coalition)
                excess = tuple(a-b for a, b in zip(initial_errors, post_errors))
                step_excess = tuple(a-b for a, b in zip(current_errors, post_errors))
                if (any(e > m for e, m in zip(excess, maximum_excess))
                        or any(e > m for e, m in zip(step_excess, maximum_step_excess))):
                    check_transition(current, command, result, coalition)
                    endpoint(initial, result.post, coalition)
                    witness = {'trial': trial, 'coalition': coalition, 'genesis': chosen_seed[0],
                               'prefix': chosen_seed[1], 'initial': initial,
                               'commands': path+[command], 'before': current, 'after': result.post,
                               'cumulative_excess': excess, 'step_excess': step_excess}
                    for asset in (0, 1):
                        if excess[asset] > maximum_excess[asset]:
                            excess_witnesses[asset] = witness
                        if step_excess[asset] > maximum_step_excess[asset]:
                            step_excess_witnesses[asset] = witness
                maximum_excess = [max(m, e) for m, e in zip(maximum_excess, excess)]
                maximum_step_excess = [max(m, e) for m, e in zip(maximum_step_excess, step_excess)]
                floored = valuations(result.post, coalition)[0]
                delta = tuple(a-b for a, b in zip(floored, baseline))
                # Integer weights avoid a floating-point search objective.
                score = (min(delta), initial.pool.r1*delta[0]+initial.pool.r0*delta[1])
                evaluated.append((score, command, result))
                if all(d >= 0 for d in delta) and any(delta):
                    maximum = [max(m, d) for m, d in zip(maximum, delta)]
                    if sum(delta) > best_sum:
                        endpoint(initial, result.post, coalition)
                        best_sum = sum(delta)
                        best = {'trial': trial, 'coalition': coalition, 'initial': initial,
                                'prefix': chosen_seed[1], 'commands': path+[command],
                                'after': result.post, 'accounting': endpoint(initial, result.post, coalition)}
                    if coalition == (0,) and initial.shares[1] and other_lp is None:
                        bob0, bob1 = valuations(initial, (1,))[0], valuations(result.post, (1,))[0]
                        if dominates(bob0, bob1):
                            other_lp = {'trial': trial, 'commands': path+[command],
                                        'initial': initial, 'after': result.post, 'bob_before': bob0, 'bob_after': bob1}
            _, command, result = (rng.choice(evaluated) if mode >= 3 and rng.randrange(5) == 0
                                   else max(evaluated, key=lambda row: row[0]))
            check_transition(current, command, result, coalition)
            endpoint(initial, result.post, coalition)
            if result.reason is None:
                accepted_kinds[str(command.action)] += 1
            selected_errors = errors(result.post, coalition)
            positive_excess = [p+max(Fraction(0), a-b)
                               for p, a, b in zip(positive_excess, current_errors, selected_errors)]
            previous_floor = valuations(current, coalition)[0]
            next_floor = valuations(result.post, coalition)[0]
            positive_marks = [p+max(0, a-b) for p, a, b in zip(positive_marks, next_floor, previous_floor)]
            key = result.post.pool, result.post.shares
            if key in visited:
                prior, start = visited[key]
                counts['repeated_configurations'] += 1
                if repeated_configuration_gain(prior, result.post, coalition):
                    cycle = {'initial': prior, 'after': result.post, 'commands': path[start:]+[command]}
            else:
                visited[key] = result.post, len(path)+1
            current = result.post
            path.append(command)
            counts['selected_steps'] += 1
        max_positive_marks = [max(a, b) for a, b in zip(max_positive_marks, positive_marks)]
        maximum_positive_excess = [max(a, b) for a, b in zip(maximum_positive_excess, positive_excess)]
    return json_value({'schema': 'spot-compounding-search/1', 'length_budget': length,
        'completed_trace_length': length, 'trials': trials, 'candidate_width': width, 'seed': seed,
        'counts': dict(counts), 'largest_weakly_dominating_net_gain_by_asset': maximum,
        'selected_accepted_action_counts': dict(accepted_kinds),
        'largest_cumulative_signed_rounding_excess_by_asset': maximum_excess,
        'largest_per_step_rounding_excess_by_asset': maximum_step_excess,
        'cumulative_excess_witnesses': excess_witnesses,
        'per_step_excess_witnesses': step_excess_witnesses,
        'largest_sum_positive_rounding_excess_by_asset': maximum_positive_excess,
        'largest_total_atoms_in_one_dominating_endpoint': best_sum, 'best_endpoint': best,
        'other_lp_floor_loss': other_lp, 'growing_cycle': cycle,
        'largest_sum_positive_intermediate_marks': max_positive_marks,
        'seconds': round(time.monotonic()-started, 3),
        'coverage': 'three reachable dominant-LP prefixes; singletons and pair; same-direction tiny swaps, alternating swaps, add/burn, guided command mutations; all five commands including prefixes',
        'capabilities': 'two users, one pool, isolated coalition, either ordinary recipient; no flash lender, outside income, admin, oracle, external market or donation command',
        'nonclaims': 'sampled finite search; positive intermediate marks are not net gain; no native refinement or all-trace proof from this search'})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--length', type=int, required=True)
    parser.add_argument('--trials', type=int, default=12)
    parser.add_argument('--width', type=int, default=12)
    parser.add_argument('--seed', type=int, default=20261017)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if min(args.length, args.trials, args.width) <= 0:
        parser.error('positive budgets required')
    result = search(args.length, args.trials, args.width, args.seed)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({k: v for k, v in result.items() if k not in (
        'best_endpoint', 'other_lp_floor_loss', 'cumulative_excess_witnesses', 'per_step_excess_witnesses')}))
    return 1 if result['growing_cycle'] else 0


if __name__ == '__main__':
    raise SystemExit(main())
