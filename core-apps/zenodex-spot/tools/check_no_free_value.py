#!/usr/bin/env python3
"""Exhaustive, finite sequence checks of the independent Spot oracle.

This is evidence about the declared finite initial domain and command alphabet,
not an all-size proof, a native replay, or an extension of Spot's command set.
Counts include rejected commands and repeated paths. Equal complete immutable
Worlds are merged only at equal depth for the same initial state and actor.
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


def holdings(world, actor, *, floor=False):
    values = list(world.balances[4 + 2 * actor:6 + 2 * actor])
    if world.pool is not None:
        for asset, reserve in enumerate((world.pool.r0, world.pool.r1)):
            claim = Fraction(world.shares[actor] * reserve, world.pool.supply)
            values[asset] += claim.numerator // claim.denominator if floor else claim
    return tuple(values)


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


def exhaustive(bound):
    started = time.monotonic()
    states = tuple(initial_domain())
    counts = Counter()
    # This cache is checker scratch, never state or decision authority.
    transitions = {}
    floor_witness = None
    for initial in states:
        for actor in (0, 1):
            commands = command_domain(actor)
            baseline = holdings(initial, actor)
            baseline_floor = holdings(initial, actor, floor=True)
            frontier = {initial: (1, ())}
            for depth in range(1, bound + 1):
                following = {}
                for before, (multiplicity, path) in frontier.items():
                    for command in commands:
                        key = before, command
                        if key not in transitions:
                            result = step(before, command)
                            check_step(before, command, result)
                            transitions[key] = result
                        result = transitions[key]
                        post = result.post
                        counts[f"sequences_length_{depth}"] += multiplicity
                        counts["accepted_steps" if result.reason is None else "rejected_steps"] += multiplicity
                        if dominates(holdings(post, actor), baseline):
                            raise AssertionError(f"no_free_value: {initial!r}, {path + (command,)!r}")
                        if floor_witness is None and dominates(holdings(post, actor, floor=True), baseline_floor):
                            floor_witness = {"initial": dataclasses.asdict(initial), "actor": actor,
                                             "commands": [dataclasses.asdict(c) for c in path + (command,)]}
                        old_count, old_path = following.get(post, (0, path + (command,)))
                        following[post] = old_count + multiplicity, old_path
                counts["distinct_endpoints_checked"] += len(following)
                frontier = following
    return {"schema": "spot-no-free-value/1", "bound": bound, "initial_states": len(states),
            "actors_per_state": 2, "commands_per_actor": len(command_domain(0)), "counts": dict(counts),
            "distinct_transition_checks": len(transitions), "exact_claim_dominance_violations": 0,
            "floored_claim_first_witness": floor_witness, "seconds": round(time.monotonic() - started, 3),
            "domain": "initial_domain() and command_domain(); every Cartesian value; all statuses; real K=1000; successors unrestricted by initial domain",
            "claim_definition": "balance + exact rational shares*reserve/supply; floored claims checked separately",
            "nonclaims": "finite oracle domain/alphabet only; no arbitrary amounts, native equivalence or donation command"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bound", type=int, default=4)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.bound < 3:
        parser.error("the brief requires a bound of at least three")
    result = exhaustive(args.bound)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result), flush=True)


if __name__ == "__main__":
    main()
