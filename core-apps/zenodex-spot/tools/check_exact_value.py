"""Exact-accounting contracts and endpoint dust bounds for the Spot oracle.

This is an independent arithmetic checker, not native decision authority or a
proof of source refinement. Snapshots and numeric observations are immutable;
report mappings are fresh caller-owned data, with no decision authority.
"""
from fractions import Fraction

from check_no_free_value import coalition_members, dominates, valuations
from spot_reference import K


def exact(world, coalition):
    _, numerator, denominator, _ = valuations(world, coalition)
    return tuple(Fraction(n, denominator) for n in numerator)


def errors(world, coalition):
    floored, _, _, _ = valuations(world, coalition)
    return tuple(e-f for e, f in zip(exact(world, coalition), floored))


def outside_liquid(world, coalition):
    members = coalition_members(coalition)
    return tuple(world.balances[2+i] + sum(world.balances[4+2*a+i]
                 for a in (0, 1) if a not in members) for i in (0, 1))


def check_transition(before, command, result, coalition):
    """Check local induction premises, including refused work and isolation."""
    members = coalition_members(coalition)
    if command.actor not in members:
        raise AssertionError("isolated_actor")
    after = result.post
    if result.reason is not None:
        if after != before or result.effects or result.outputs:
            raise AssertionError("reject_is_no_op")
        return
    if any(sum(before.balances[i::2]) != sum(after.balances[i::2]) for i in (0, 1)):
        raise AssertionError("asset_conservation")
    if any(b > a for b, a in zip(outside_liquid(before, members), outside_liquid(after, members))):
        raise AssertionError("outside_liquid_non_decreasing")
    a, b = before.pool, after.pool
    if b is None or min(b.r0, b.r1, b.supply) <= 0 or after.shares[2] < K:
        raise AssertionError("positive_locked_pool")
    if a is None:
        if command.action != 0 or any(x > y for x, y in zip(exact(after, members), exact(before, members))):
            raise AssertionError("creation_cost")
    else:
        q = a.supply-sum(before.shares[i] for i in members)
        if q <= 0 or q != b.supply-sum(after.shares[i] for i in members):
            raise AssertionError("fixed_positive_outside_shares")
        if a.r0*a.r1*b.supply**2 > b.r0*b.r1*a.supply**2:
            raise AssertionError("reserve_per_share_product")
        if command.action in (3, 4) and any(x*b.supply > y*a.supply
                for x, y in zip((a.r0, a.r1), (b.r0, b.r1))):
            raise AssertionError("liquidity_reserves_per_share")
    eb, ea = errors(before, members), errors(after, members)
    if any(not (0 <= e < len(members)) for e in eb+ea):
        raise AssertionError("sum_of_member_floor_errors")
    if any(not (-len(members) < x-y < len(members)) for x, y in zip(eb, ea)):
        raise AssertionError("endpoint_dust")


def endpoint(before, after, coalition):
    """Length-independent bound, at the INITIAL pool's positive price weights.

    Returns exact rational pairs. If F=W-E, weighted Delta F <= weighted E0.
    The bound concerns NET endpoint change; summing positive intermediate marks
    discards their later losses and is not cumulative extracted value.
    """
    members = coalition_members(coalition)
    wb, wa = exact(before, members), exact(after, members)
    fb, _, _, _ = valuations(before, members)
    fa, _, _, _ = valuations(after, members)
    eb, ea = errors(before, members), errors(after, members)
    floor_delta = tuple(a-b for a, b in zip(fa, fb))
    exact_delta = tuple(a-b for a, b in zip(wa, wb))
    excess = tuple(Fraction(f)-e for f, e in zip(floor_delta, exact_delta))
    if excess != tuple(x-y for x, y in zip(eb, ea)):
        raise AssertionError("dust_telescope_identity")
    if any(not (-len(members) < e < len(members)) for e in excess):
        raise AssertionError("endpoint_dust")
    if dominates(wa, wb):
        raise AssertionError("exact_no_free_value")
    weights = (1, 1) if before.pool is None else (Fraction(before.pool.r1, before.pool.supply),
                                                Fraction(before.pool.r0, before.pool.supply))
    weighted_exact = sum(w*d for w, d in zip(weights, exact_delta))
    weighted_floor = sum(w*d for w, d in zip(weights, floor_delta))
    bound = sum(w*e for w, e in zip(weights, eb))
    if weighted_exact > 0 or weighted_floor > bound:
        raise AssertionError("cumulative_weighted_dust_bound")
    return {"floored_delta": floor_delta, "exact_delta": exact_delta,
            "rounding_excess": excess, "initial_errors": eb, "final_errors": ea,
            "initial_price_weights": weights, "weighted_exact_change": weighted_exact,
            "weighted_floor_change": weighted_floor, "weighted_dust_bound": bound}
