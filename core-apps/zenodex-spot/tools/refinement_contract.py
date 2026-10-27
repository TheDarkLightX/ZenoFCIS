"""Executable renderings of the Spot Step contracts, and a finite check.

`int_contract` renders NativeBridge.lean's IntStep/IntCreate for every
coalition that contains the actor, plus the per-account facts the Verus
theorem states (mirrors, conservation, isolation). `rat_step` renders
ExactValue.lean's `Step` with exact fractions. `finite_check` runs the
compiled program (through native_program's evaluator-specification
rendering) and the independent oracle spot_reference.step on every point of
a declared finite domain and compares verdict, first reason and successor.

A finite check is evidence over its declared points only, never a proof.
"""
from __future__ import annotations

import itertools
from fractions import Fraction

import native_program
from build_contracts import REASONS
from spot_reference import K, N, U, Command, Pool, World, limbs, state_values, command_values, step

LIMB = 1 << 32
STATE_FIELDS = 40
NAMES = {number: name for name, number in REASONS.items()}


def value(st, key):
    return sum(st[8 + 4 * key + i] << (32 * i) for i in range(4))


def member(mask, account):
    return mask == 3 or (mask == 1 and account == 0) or (mask == 2 and account == 1)


def aggregate(st, mask):
    """(r0, r1, supply, liquid0, liquid1, outside0, outside1) and outside shares."""
    liquid = [sum(value(st, 4 + 2 * j + asset) for j in (0, 1) if member(mask, j)) for asset in (0, 1)]
    outside = [value(st, 2 + asset) + sum(value(st, 4 + 2 * j + asset) for j in (0, 1) if not member(mask, j))
               for asset in (0, 1)]
    shares = st[7] + sum(st[5 + j] for j in (0, 1) if not member(mask, j))
    return (st[0], st[1], st[2], liquid[0], liquid[1], outside[0], outside[1]), shares


def int_pool(action, a, b):
    if action in (1, 2):
        return b[2] == a[2] and a[0] * a[1] <= b[0] * b[1]
    if action == 3:
        return ((b[2] - a[2]) * a[0] <= (b[0] - a[0]) * a[2]
                and (b[2] - a[2]) * a[1] <= (b[1] - a[1]) * a[2])
    return ((a[0] - b[0]) * a[2] <= (a[2] - b[2]) * a[0]
            and (a[1] - b[1]) * a[2] <= (a[2] - b[2]) * a[1])


def valid(q, s):
    return 0 < s[0] and 0 < s[1] and 0 < s[2] and q <= s[2]


def int_contract(pre, action, actor, post):
    """Every failed clause, by name; empty when the contract holds."""
    failed = []
    if actor not in (0, 1):
        return ["reserved_actor"]
    check = lambda ok, name: ok or failed.append(name)
    for st, side in ((pre, "pre"), (post, "post")):
        check(all(0 <= st[i] < LIMB for i in range(8, STATE_FIELDS)), f"{side}_limbs")
        check(value(st, 0) == st[0] and value(st, 1) == st[1] and st[5] + st[6] + st[7] == st[2], f"{side}_mirror")
    for asset in (0, 1):
        check(sum(value(post, k) for k in range(asset, 8, 2)) == sum(value(pre, k) for k in range(asset, 8, 2)),
              f"conservation{asset}")
    other = 1 - actor
    check(value(post, 2) >= value(pre, 2) and value(post, 3) >= value(pre, 3), "ingress_isolation")
    check(all(value(post, 4 + 2 * other + a) >= value(pre, 4 + 2 * other + a) for a in (0, 1))
          and post[5 + other] == pre[5 + other], "account_isolation")
    for mask in (1, 2, 3):
        if not member(mask, actor):
            continue
        a, q = aggregate(pre, mask)
        b, q_after = aggregate(post, mask)
        if action == 0:
            ok = (valid(q_after, b) and q_after == K
                  and a[3] + a[5] == b[3] + b[0] + b[5] and a[4] + a[6] == b[4] + b[1] + b[6]
                  and a[5] <= b[5] and a[6] <= b[6] and pre[0] == pre[1] == pre[2] == 0)
            check(ok, f"int_create_mask{mask}")
        else:
            ok = (valid(q, a) and valid(q, b) and q_after == q and int_pool(action, a, b)
                  and a[3] + a[0] + a[5] == b[3] + b[0] + b[5] and a[4] + a[1] + a[6] == b[4] + b[1] + b[6]
                  and a[5] <= b[5] and a[6] <= b[6])
            check(ok, f"int_step_mask{mask}")
            check(rat_step(Fraction(q), a, b, action), f"rat_step_mask{mask}")
    return failed


def rat_step(q, a, b, action):
    """ExactValue.lean's Step q a b for the abstraction x = r0/supply, y = r1/supply."""
    def state(s):
        supply = Fraction(s[2])
        return {"x": s[0] / supply, "y": s[1] / supply, "supply": supply, "liquid0": Fraction(s[3]),
                "liquid1": Fraction(s[4]), "outside0": Fraction(s[5]), "outside1": Fraction(s[6])}
    sa, sb = state(a), state(b)
    good = lambda s: 0 < s["x"] and 0 < s["y"] and 0 < s["supply"] and q <= s["supply"]
    total = lambda s, i: s[f"liquid{i}"] + s["supply"] * s["xy"[i]] + s[f"outside{i}"]
    if action in (1, 2):
        pool = (sb["supply"] == sa["supply"] and (sa["supply"] * sa["x"]) * (sa["supply"] * sa["y"])
                <= (sb["supply"] * sb["x"]) * (sb["supply"] * sb["y"]))
    elif action == 3:
        m = sb["supply"] - sa["supply"]
        u = sb["supply"] * sb["x"] - sa["supply"] * sa["x"]
        v = sb["supply"] * sb["y"] - sa["supply"] * sa["y"]
        pool = m * sa["x"] <= u and m * sa["y"] <= v
    else:
        z = sa["supply"] - sb["supply"]
        u = sa["supply"] * sa["x"] - sb["supply"] * sb["x"]
        v = sa["supply"] * sa["y"] - sb["supply"] * sb["y"]
        pool = u <= z * sa["x"] and v <= z * sa["y"]
    return (good(sa) and good(sb) and pool and total(sa, 0) == total(sb, 0) and total(sa, 1) == total(sb, 1)
            and sa["outside0"] <= sb["outside0"] and sa["outside1"] <= sb["outside1"])


def world_inputs(world, command):
    return list(state_values(world) + command_values(command) + (1,))


def oracle_state(decision):
    return list(state_values(decision.post))


# ---------------------------------------------------------------------------
# The declared finite domain: boundary-structured, real K = 1000 lock.

POOLS = ((1000, 1000, 1000), (1001, 1000, 1000), (2000, 3000, 2449), (3000, 2000, 2600),
         (1000000, 1000000, 1000000), (1, 1000000000, 31622), (999999, 1, 1000),
         (N - 1, N, N - 1000))
# (alice, bob) spendable pairs, each (asset0, asset1): empty, small, large and
# one atom below or at the u128 ceiling, so credits can overflow.
HOLDINGS = (((10 ** 9, 10 ** 9), (0, 0)), ((U - 7, U - 7), (1001, 1001)),
            ((0, 0), (U, U)), ((1001, 1001), (10 ** 9, 10 ** 9)))
INGRESS = ((0, 0), (U - 5, U))
AMOUNTS = (1, 2, 3, 333, 334, 999, 1000, 1001, 1333, 1334, 10000, 999999, 1000000000)
LIMITS = ((0, 0), (10 ** 9, 10 ** 9))
ROLES = ((0, 0), (0, 1), (1, 0), (1, 1), (2, 0))
RESERVED = ((3, 3), (0, 4), (5, 0))


def isqrt(n):
    x = int(n ** 0.5)
    while x * x > n:
        x -= 1
    while (x + 1) * (x + 1) <= n:
        x += 1
    return x


def worlds():
    for (alice, bob), ingress in itertools.product(HOLDINGS, INGRESS):
        yield World(None, (0, 0) + ingress + alice + bob, (0, 0, 0))
    for (r0, r1, supply), (alice, bob), ingress in itertools.product(POOLS, HOLDINGS, INGRESS):
        extra = supply - K
        for split in sorted({extra, extra // 2, 0}):
            yield World(Pool(r0, r1, supply), (r0, r1) + ingress + alice + bob, (split, extra - split, K))


def commands(world, roles):
    p = world.pool
    for actor, recipient in roles:
        if p is None:
            for a, b in itertools.product((1000, 1001, 4000, 1000000), (1001, 4000, 1000000)):
                s = isqrt(a * b)
                for sqrt in (s - 1, s, s + 1):
                    yield Command(0, a=a, b=b, sqrt=sqrt, actor=actor, recipient=recipient)
            continue
        for action, direction, a in itertools.product((1, 2), (0, 1), AMOUNTS):
            for limit0 in (0, 10 ** 9):
                yield Command(action, direction=direction, a=a, limit0=limit0, actor=actor, recipient=recipient)
        for a, b in itertools.product(AMOUNTS[::2], AMOUNTS[::3]):
            for limit0, limit1 in LIMITS:
                yield Command(3, a=a, b=b, limit0=limit0, limit1=limit1, actor=actor, recipient=actor)
        for a in AMOUNTS + (world.shares[0], p.supply - K):
            yield Command(4, a=max(a, 1), actor=actor, recipient=recipient)


def points():
    """Every declared (world, command); reserved roles on the first world of each kind."""
    seen = set()
    for world in worlds():
        kind = world.pool is None
        roles = ROLES + (RESERVED if kind not in seen else ())
        seen.add(kind)
        for command in commands(world, roles):
            yield world, command


def finite_check(native, limit=None, examples=3):
    """Compare every declared point; return counts and the first failures by kind."""
    counts = {"points": 0, "accepts": 0, "rejects": 0, "accepts_by_action": [0] * 5, "reasons": {}}
    failures: dict[str, list] = {}

    def fail(kind, world, command, detail):
        found = failures.setdefault(kind, [])
        if len(found) < examples:
            found.append({"inputs": world_inputs(world, command), "detail": detail})

    for world, command in points():
        if limit is not None and counts["points"] >= limit:
            break
        counts["points"] += 1
        inputs = world_inputs(world, command)
        expected = step(world, command)
        try:
            accept, reason, post = native_program.decide(native, inputs)
        except native_program.Failure as error:
            fail("native_refusal", world, command, str(error))
            continue
        got = None if reason is None else NAMES[reason]
        pre = inputs[:STATE_FIELDS]
        state = [post[field] for _, field in native.bindings[:STATE_FIELDS]]
        if got != expected.reason:
            fail("verdict_or_reason", world, command, {"native": got, "reference": expected.reason})
        elif accept and state != oracle_state(expected):
            fail("successor", world, command, {"native": state, "reference": oracle_state(expected)})
        if not accept:
            counts["rejects"] += 1
            counts["reasons"][got] = counts["reasons"].get(got, 0) + 1
            if state != pre:
                fail("reject_changed_state", world, command, {})
            continue
        counts["accepts"] += 1
        counts["accepts_by_action"][command.action] += 1
        for clause in int_contract(pre, command.action, command.actor, state):
            fail(f"contract:{clause}", world, command, {})
    return {"counts": counts, "failures": failures,
            "domain": {"pools": POOLS, "holdings": HOLDINGS, "ingress": INGRESS, "amounts": AMOUNTS,
                       "limits": LIMITS, "roles": ROLES, "reserved_roles": RESERVED}}
