#!/usr/bin/env python3
"""Exhaustive, finite sequence checks of the independent Spot oracle.

This is evidence about the declared finite initial domain and command alphabet,
not an all-size proof, a native replay, or an extension of Spot's command set.
Counts include rejected commands and repeated paths. Equal complete immutable
Worlds are merged only at equal depth for the same initial state and coalition.
"""
from __future__ import annotations

import argparse
import dataclasses
import itertools
import json
import time
from collections import Counter
from fractions import Fraction
from pathlib import Path

from spot_reference import Command, K, Pool, World, step


def coalition_members(actors):
    members = (actors,) if type(actors) is int else tuple(actors)
    if not members or len(set(members)) != len(members) or any(type(a) is not int or a not in (0, 1) for a in members):
        raise ValueError("a coalition is a nonempty subset of the two ordinary accounts")
    return members


def valuations(world, actors):
    """One beneficiary: sum account balances and each account's burn payout.

    Also retain a hypothetical pooled-share burn upper bound. Spot has no share
    transfer command; pooling is an extra guard, not an added capability.
    Integer numerators preserve the exact guard without Fraction allocation in
    the search loop. All returned collections are owned immutable tuples.
    """
    members = coalition_members(actors)
    liquid = tuple(sum(world.balances[4 + 2 * a + i] for a in members) for i in (0, 1))
    if world.pool is None:
        return liquid, liquid, 1, liquid
    p = world.pool
    shares = sum(world.shares[a] for a in members)
    reserves = (p.r0, p.r1)
    floored = tuple(liquid[i] + sum(world.shares[a] * r // p.supply for a in members)
                    for i, r in enumerate(reserves))
    exact = tuple(liquid[i] * p.supply + shares * r for i, r in enumerate(reserves))
    pooled = tuple(liquid[i] + shares * r // p.supply for i, r in enumerate(reserves))
    return floored, exact, p.supply, pooled


def holdings(world, actor, *, floor=True):
    floored, numerators, denominator, _ = valuations(world, actor)
    return floored if floor else tuple(Fraction(n, denominator) for n in numerators)


def valuation_gains(before, after, actors):
    b, bn, bd, bp = valuations(before, actors)
    a, an, ad, ap = valuations(after, actors)
    return {"floored_burn_payout": dominates(a, b),
            "exact_pro_rata": dominates(tuple(n * bd for n in an), tuple(n * ad for n in bn)),
            "pooled_burn_upper_bound": dominates(ap, bp)}


def dominates(after, before):
    return all(a >= b for a, b in zip(after, before)) and any(a > b for a, b in zip(after, before))


def command_domain(actor):
    commands = [Command(0, a=1001, b=1001, sqrt=1001, actor=actor, recipient=actor)]
    commands += [Command(action, direction=direction, a=amount,
                         limit0=4 if action == 2 else 0, actor=actor, recipient=recipient)
                 for action, direction, amount, recipient in itertools.product((1, 2), (0, 1), (1, 2), (0, 1))]
    commands += [Command(3, a=a, b=b, actor=actor, recipient=actor)
                 for a, b in itertools.product((1, 2), repeat=2)]
    commands += [Command(4, a=a, actor=actor, recipient=recipient)
                 for a, recipient in itertools.product((1, 2), (0, 1))]
    return tuple(commands)


def initial_domain():
    # Complete Cartesian product, with the actual permanent K=1000.
    for x, y, alice, bob, bx, by, status in itertools.product(
            range(1, 4), range(1, 4), (0, 2), (0, 1), (0, 4), (0, 4), (0, 1, 2)):
        yield World(Pool(x, y, K + alice + bob, status),
                    (x, y, 0, 0, bx, by, 4, 4), (alice, bob, K))
    # Correlated actual-lock LP states exercise positive burns, adds, creation.
    for supply, bx, by in itertools.product((1000, 1001, 1002), (0, 1004), (0, 1004)):
        yield World(Pool(supply, supply, supply),
                    (supply, supply, 0, 0, bx, by, 1004, 1004), (supply - K, 0, K))
    for bx, by in itertools.product((0, 1004), repeat=2):
        yield World(None, (0, 0, 0, 0, bx, by, 1004, 1004))


def check_step(before, command, result):
    """Independent policy checks used by the three planted-defect controls."""
    if result.reason is not None:
        if result.post != before or result.effects or result.outputs:
            raise AssertionError("reject_is_no_op")
        return
    if command.action in (1, 2):
        gross, _, fee, protocol, retained, net, _, _ = result.outputs
        if not (0 <= 10000 * fee - 30 * gross < 10000
                and 10000 * protocol <= 2000 * fee < 10000 * (protocol + 1)
                and gross == net + retained + protocol):
            raise AssertionError("fixed_fee_law")
    if command.action == 4:
        for paid, reserve in zip(result.outputs[:2], (before.pool.r0, before.pool.r1)):
            if not (paid > 0 and paid * before.pool.supply <= command.a * reserve
                    < (paid + 1) * before.pool.supply):
                raise AssertionError("burn_floor_law")
    if command.action == 3:
        u0, u1, mint = result.outputs[:3]
        if not (mint * before.pool.r0 <= u0 * before.pool.supply
                and mint * before.pool.r1 <= u1 * before.pool.supply
                and ((mint + 1) * before.pool.r0 > u0 * before.pool.supply
                     or (mint + 1) * before.pool.r1 > u1 * before.pool.supply)):
            raise AssertionError("mint_uses_current_reserves_law")


def exhaustive(bound, *, coalitions=((0,), (1,)), max_seconds=120):
    started = time.monotonic()
    states = tuple(initial_domain())
    coalitions = tuple(coalition_members(c) for c in coalitions)
    commands = {c: tuple(cmd for actor in c for cmd in command_domain(actor)) for c in coalitions}
    # These caches/frontiers are exclusively owned checker scratch. They never
    # supply native decision, metering, receipt or publication authority.
    transitions, values = {}, {}
    def value(world, coalition):
        key = world, coalition
        if key not in values:
            values[key] = valuations(world, coalition)
        return values[key]
    groups = [(initial, c, {initial: (1, ())}) for initial in states for c in coalitions]
    counts, by_coalition = Counter(), {str(c): Counter() for c in coalitions}
    witnesses = {name: None for name in ("floored_burn_payout", "exact_pro_rata", "pooled_burn_upper_bound")}
    completed = 0
    timed_out = False
    for depth in range(1, bound + 1):
        next_groups, stage, stage_by = [], Counter(), {str(c): Counter() for c in coalitions}
        for initial, coalition, frontier in groups:
            if time.monotonic() - started >= max_seconds:
                timed_out = True
                break
            baseline, bn, bd, bp = value(initial, coalition)
            following = {}
            for before, (multiplicity, path) in frontier.items():
                for command in commands[coalition]:
                    key = before, command
                    if key not in transitions:
                        result = step(before, command)
                        check_step(before, command, result)
                        transitions[key] = result
                    result = transitions[key]
                    post = result.post
                    stage[f"sequences_length_{depth}"] += multiplicity
                    stage_by[str(coalition)][f"sequences_length_{depth}"] += multiplicity
                    stage["accepted_steps" if result.reason is None else "rejected_steps"] += multiplicity
                    floored, an, ad, pooled = value(post, coalition)
                    gains = (dominates(floored, baseline),
                             dominates(tuple(n * bd for n in an), tuple(n * ad for n in bn)),
                             dominates(pooled, bp))
                    for name, gain in zip(witnesses, gains):
                        if gain and witnesses[name] is None:
                            witnesses[name] = {"initial": dataclasses.asdict(initial), "coalition": coalition,
                                               "commands": [dataclasses.asdict(c) for c in path + (command,)]}
                    old_count, old_path = following.get(post, (0, path + (command,)))
                    following[post] = old_count + multiplicity, old_path
            stage["distinct_endpoints_checked"] += len(following)
            next_groups.append((initial, coalition, following))
        if timed_out:
            break
        counts.update(stage)
        for c in coalitions:
            by_coalition[str(c)].update(stage_by[str(c)])
        groups = next_groups
        completed = depth
    return {"schema": "spot-no-free-value/2", "requested_bound": bound, "bound": completed,
            "budget_seconds": max_seconds, "budget_exhausted": timed_out, "initial_states": len(states),
            "coalitions": coalitions, "commands_per_coalition": {str(c): len(commands[c]) for c in coalitions},
            "counts": dict(counts), "counts_per_coalition": {c: dict(v) for c, v in by_coalition.items()},
            "distinct_transition_checks_including_partial_depth": len(transitions),
            "primary_measure": "exact_pro_rata",
            "measures": {name: {"first_witness": w, "status": "counterexample" if w else "no violation through completed bound"}
                         for name, w in witnesses.items()},
            "seconds": round(time.monotonic() - started, 3),
            "domain": "initial_domain() and command_domain(); every Cartesian value; all statuses; real K=1000; successors unrestricted by initial domain",
            "claim_definition": "combined liquid balances + exact combined pro-rata claim",
            "extra_guards": "sum of each member's floored burn payouts; hypothetical floor of pooled shares (no share transfer command)",
            "capabilities": "only coalition members submit commands; either ordinary recipient; interleaved atomic sequential steps; no borrowing, outside income, admin, external markets or oracle",
            "nonclaims": "finite oracle domain/alphabet only; no arbitrary amounts, native equivalence, flash lender or donation command; partial depth is not exhaustive"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bound", type=int, default=5)
    parser.add_argument("--max-seconds", type=float, default=120)
    parser.add_argument("--coalitions", choices=("singletons", "pair", "all"), default="singletons")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.bound < 1 or not 0 < args.max_seconds <= 120:
        parser.error("positive bound and a budget in (0,120] are required")
    coalitions = {"singletons": ((0,), (1,)), "pair": ((0, 1),), "all": ((0,), (1,), (0, 1))}[args.coalitions]
    result = exhaustive(args.bound, coalitions=coalitions, max_seconds=args.max_seconds)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result), flush=True)
    return 1 if result["bound"] == 0 or any(m["first_witness"] for m in result["measures"].values()) else 0


if __name__ == "__main__":
    raise SystemExit(main())
