#!/usr/bin/env python3
"""Write authored Spot contracts, without invoking a compiler or build system.

Rules and laws share named arithmetic expressions. Independent laws below use
product inequalities rather than simply restating the generated case table.
The emitted .zeno and policy.json files are the reviewable contract sources.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from dataclasses import dataclass
from pathlib import Path

APP = Path(__file__).resolve().parents[1]
N, J, K, BASE = 3_000_000_000, 1_000_000_000, 1000, 1 << 32
REASONS = {
    "DOMAIN": 200, "POOL_EXISTS": 201, "NO_POOL": 202,
    "POOL_INACTIVE": 203, "EMPTY_POOL": 204, "RESERVED_LP_LOCK": 205,
    "RESERVED_ROLE": 206, "BAD_SQRT": 207, "INITIAL_TOO_SMALL": 208,
    "POST_DOMAIN": 209, "ZERO_NET": 210, "ZERO_OUTPUT": 211,
    "SLIPPAGE": 212, "DRAIN": 213, "INSUFFICIENT_QUOTE": 214,
    "QUOTE_GAP": 215, "ZERO_USED": 216, "ZERO_MINT": 217,
    "INSUFFICIENT_LP": 218, "ZERO_PAYOUT": 219, "INVARIANT": 220,
    "ARITHMETIC": 221, "INGRESS_ROLE": 222, "INGRESS_OVERFLOW": 223,
    "INSUFFICIENT_BALANCE": 224, "BALANCE_OVERFLOW": 225,
    "UNBALANCED_EFFECTS": 226, "POOL_BALANCE_MISMATCH": 227,
}


@dataclass(frozen=True)
class Expr:
    rule: str
    law: str

    def binary(self, op, other):
        other = expr(other)
        # At the start of a relation, .zeno parses '(' as a formula group.
        # A literal prefix makes the left side unambiguously a scalar value.
        law = (f"(0 + ({self.law}) {op} ({other.law}))"
               if op in ("==", "!=", "<", "<=", ">", ">=")
               else f"({self.law} {op} {other.law})")
        return Expr(f"({self.rule} {op} {other.rule})",
                    law)

    def __add__(self, other): return self.binary("+", other)
    def __sub__(self, other): return self.binary("-", other)
    def __mul__(self, other): return self.binary("*", other)
    def __and__(self, other): return self.binary("&&", other)
    def __or__(self, other): return self.binary("||", other)
    def eq(self, other): return self.binary("==", other)
    def ne(self, other): return self.binary("!=", other)
    def lt(self, other): return self.binary("<", other)
    def le(self, other): return self.binary("<=", other)
    def gt(self, other): return self.binary(">", other)
    def ge(self, other): return self.binary(">=", other)
    def __invert__(self): return Expr(f"!({self.rule})", f"!({self.law})")


def expr(value):
    if isinstance(value, Expr): return value
    text = str(value).lower() if type(value) is bool else str(value)
    return Expr(text, text)


def field(root, number): return expr(f"{root}.100.{number}" if root in ("pre", "post")
                                     else f"{root}.{101 if root == 'command' else 102}.{number}")


def call(name, *values):
    values = tuple(map(expr, values))
    return Expr(f"{name}({', '.join(v.rule for v in values)})",
                f"{name}({', '.join(v.law for v in values)})")


def mf(a, b, d):
    a, b, d = map(expr, (a, b, d))
    return Expr(f"mul_div_floor({a.rule}, {b.rule}, {d.rule})",
                f"div_floor(({a.law}) * ({b.law}), {d.law})")


def mc(a, b, d):
    a, b, d = map(expr, (a, b, d))
    return Expr(f"mul_div_ceil({a.rule}, {b.rule}, {d.rule})",
                f"div_ceil(({a.law}) * ({b.law}), {d.law})")


def pg(a, b, c, d): return call("products_ge", a, b, c, d)


def select(guard, left, right, indicator):
    """The law language has no choose; an exact 0/1 arithmetic indicator does.

    Each caller supplies an indicator with a small, explicitly bounded input.
    There is no division by zero and no multiplication beyond signed i128.
    """
    guard, left, right, indicator = map(expr, (guard, left, right, indicator))
    return Expr(f"choose({guard.rule}, {left.rule}, {right.rule})",
                f"(({right.law}) + ({indicator.law}) * (({left.law}) - ({right.law})))")


def step(value):
    # ceil(v/(v*v+1)) is exactly 1 for positive integer v, otherwise 0.
    value = expr(value)
    return call("div_ceil", value, value * value + 1)


def positive(value):
    value = expr(value)
    return select(value.gt(0), value, 0, step(value))


def denominator(value):
    value = expr(value)
    return select(value.gt(1), value, 1, step(value - 1))


def all_of(values):
    result = expr(True)
    for value in values: result = result & value
    return result


def any_of(values):
    result = expr(False)
    for value in values: result = result | value
    return result


class Contract:
    def __init__(self, name):
        self.name, self.fields, self.variables, self.cases, self.laws = name, [], {}, [], []
        self.genesis, self.expressions = {}, {}
        self.declarations, self.channels, self.delivery_laws = [], [], {}

    def add_field(self, root, number, name, low, high, initial=None):
        self.fields.append((root, number, name, low, high))
        if root == "pre": self.genesis[str(number)] = low if initial is None else initial
        return field(root, number)

    def let(self, name, value):
        value = expr(value)
        self.variables[name] = value.rule
        result = Expr(name, value.law)
        self.expressions[name] = result
        return result

    def reject(self, name, guard, rule):
        self.cases.append({"rule": rule, "when": expr(guard).rule, "class": "Reject",
                           "reason": REASONS[name], "post": {}, "outbox": []})

    def accept(self, post, outbox=None):
        self.cases.append({"rule": "accept exact candidate", "when": True, "class": "Accept",
                           "reason": None, "post": {str(k): expr(v).rule for k, v in post.items()},
                           "outbox": [] if outbox is None else outbox})

    def law(self, name, scope, formula, kind):
        number = 500 + len(self.laws)
        self.laws.append((number, name, scope, expr(formula).law, kind))

    def first_reason_law(self, ranges):
        """Freeze the exact finite table as an independently evaluated law.

        Full guard inlining exceeded the unchanged 1 MiB source limit. This
        finite relation preserves that limit and all existing arithmetic laws.
        It is deliberately only for the declared reduced judge domains.
        """
        import itertools
        import re
        from math import prod
        fields = [(f"{root}.{100 if root == 'pre' else 101 if root == 'command' else 102}.{number}",
                   ranges[f"{root}.{number}"]) for root, number, *_ in self.fields]
        def evaluate(text, values):
            if type(text) is bool: return text
            # Only locally authored expressions enter this authoring utility.
            text = re.sub(r"(?:pre|post|command|context)\.\d+\.\d+", lambda m: str(values[m[0]]), text)
            text = re.sub(r"!(?!=)", " not ", text.replace("&&", " and ").replace("||", " or "))
            return eval(text.strip(), {"__builtins__": {}}, {**functions, **values})
        functions = {"true": True, "false": False, "choose": lambda c, a, b: a if c else b,
                     "div_floor": lambda a, b: a // b, "div_ceil": lambda a, b: (a + b - 1) // b,
                     "mul_div_floor": lambda a, b, d: a * b // d,
                     "mul_div_ceil": lambda a, b, d: (a * b + d - 1) // d,
                     "products_ge": lambda *a: prod(a[:len(a)//2]) >= prod(a[len(a)//2:])}
        alternatives = []
        for row in itertools.product(*(range(low, high + 1) for _, (low, high) in fields)):
            values = {name: value for (name, _), value in zip(fields, row)}
            for name, expression in self.variables.items(): values[name] = evaluate(expression, values)
            selected = next(case for case in self.cases if evaluate(case["when"], values))
            if selected["class"] == "Reject":
                conditions = [f"{name} == {value}" for (name, (low, high)), value in zip(fields, row) if low != high]
                conditions.append(f"decision_reason == {selected['reason']}")
                alternatives.append("(" + " && ".join(conditions) + ")")
        def disjoin(parts):
            if not parts: return "false"
            if len(parts) == 1: return parts[0]
            middle = len(parts) // 2
            return f"({disjoin(parts[:middle])} || {disjoin(parts[middle:])})"
        self.law("ordered_first_reason", "reject", disjoin(alternatives), "RejectNoAuthority")

    def write(self, directory, ranges=None, genesis=None, laws=True):
        directory = Path(directory)
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "v2").mkdir(exist_ok=True)
        ranges = {} if ranges is None else ranges
        lines = ["zeno 1;", f"project 1 {self.name.replace('-', '_')};", "namespace 2 spot;",
                 "// Fixed owner policy: f=30, p=2000, K=1000, gap cap=200.",
                 "// Regenerate: python3 core-apps/zenodex-spot/tools/build_contracts.py",
                 "type 100 state State;", "type 101 command Command;", "type 102 context Context;"]
        for index, (root, number, name, low, high) in enumerate(self.fields):
            low, high = ranges.get(f"{root}.{number}", (low, high))
            type_id = 1000 + index
            lines.append(f"type {type_id} int {name.title().replace('_', '')} in {low}..={high};")
            lines.append(f"field {number} {100 if root == 'pre' else 101 if root == 'command' else 102} {name} {type_id};")
        used = sorted({case["reason"] for case in self.cases if case["reason"] is not None})
        for precedence, number in enumerate(used):
            name = next(name for name, value in REASONS.items() if value == number)
            lines.append(f"reason {number} {name} precedence {precedence};")
        lines.extend(self.declarations)
        lines += [f"component 400 {self.name.replace('-', '_')} {{", "  owns 100;",
                  "  reads pre.100;", "  writes post.100;", "  contexts context.102;"]
        lines.extend(f"  outbox outbox.{n};" for n in self.channels)
        lines += ["  budget steps 4096;", "}", "merge [400];"]
        if laws:
            for number, name, scope, formula, kind in self.laws:
                lines.append(f"law {number} {name} on {scope} = {formula};")
        policy = {"schema": "zeno-fcis/template-declarative-policy/2", "template": self.name,
                  "roots": {"state": 100, "command": 101, "context": 102}, "leaf_bindings": {},
                  "variables": self.variables, "cases": self.cases,
                  "genesis": self.genesis if genesis is None else {str(k): v for k, v in genesis.items()},
                  "law_kinds": {str(n): kind for n, _, _, _, kind in self.laws} if laws else {}}
        if self.delivery_laws: policy["delivery_laws"] = self.delivery_laws
        (directory / "project.zeno").write_text("\n".join(lines) + "\n")
        (directory / "v2/policy.json").write_text(json.dumps(policy, indent=2) + "\n")


def pool_contract(reserve_cap=N, supply_cap=N):
    c = Contract("spot-pool")
    r0 = c.add_field("pre", 110, "reserve0", 0, reserve_cap)
    r1 = c.add_field("pre", 111, "reserve1", 0, reserve_cap)
    supply = c.add_field("pre", 112, "supply", 0, supply_cap)
    exists = c.add_field("pre", 113, "exists", 0, 1)
    status = c.add_field("pre", 114, "status", 0, 2)
    action = c.add_field("command", 120, "action", 0, 5)
    direction = c.add_field("command", 121, "direction", 0, 2)
    a = c.add_field("command", 122, "amount0", 0, N)
    b = c.add_field("command", 123, "amount1", 0, N)
    lo0 = c.add_field("command", 124, "limit0", 0, N)
    lo1 = c.add_field("command", 125, "limit1", 0, N)
    sqrt = c.add_field("command", 126, "sqrt", 0, J)
    mirror = c.add_field("context", 130, "mirror_valid", 0, 1, 1)
    actor = c.add_field("context", 131, "actor_role", 0, 3)
    recipient = c.add_field("context", 132, "recipient_role", 0, 3)
    lp = c.add_field("context", 133, "actor_lp", 0, N)
    shape = c.add_field("context", 134, "shape_valid", 0, 1, 1)
    create, swap_in, swap_out, add, remove = [action.eq(i) for i in range(5)]
    swap = swap_in | swap_out
    x = c.let("x", select(direction.eq(0), r0, r1, expr(1) - step(direction)))
    y = c.let("y", select(direction.eq(0), r1, r0, expr(1) - step(direction)))
    t = c.let("inverse_net", mc(x, a, denominator(y - a)))
    raw = c.let("inverse_gross", mc(t, 10000, 9970))
    gross = c.let("gross", select(swap_out, raw, a,
                                  expr(1) - step((action - 2) * (action - 2))))
    fee = c.let("fee", mc(gross, 30, 10000))
    protocol = c.let("protocol", mf(fee, 2000, 10000))
    net = c.let("net", gross - fee)
    quote = c.let("quote", mf(y, net, denominator(x + net)))
    gap = c.let("gap_bps", mc(positive(quote - a), 10000, denominator(a)))
    branch = pg(b, r0, a, r1)
    branch_indicator = expr(1) - step(a * r1 - b * r0)
    u0 = c.let("used0", select(branch, a, mf(b, r0, denominator(r1)), branch_indicator))
    u1 = c.let("used1", select(branch, mf(a, r1, denominator(r0)), b, branch_indicator))
    m0, m1 = mf(u0, supply, denominator(r0)), mf(u1, supply, denominator(r1))
    mint = c.let("mint", select(m0.le(m1), m0, m1, expr(1) - step(m0 - m1)))
    o0 = c.let("payout0", mf(a, r0, denominator(supply)))
    o1 = c.let("payout1", mf(a, r1, denominator(supply)))
    paid = c.let("paid", select(swap_out, a, quote,
                                expr(1) - step((action - 2) * (action - 2))))
    sx, sy = x + gross - protocol, y - paid
    swap0 = select(direction.eq(0), sx, sy, expr(1) - step(direction))
    swap1 = select(direction.eq(0), sy, sx, expr(1) - step(direction))
    def by_action(values):
        result = values[-1]
        for i in reversed(range(4)):
            result = select(action.eq(i), values[i], result,
                            expr(1) - step((action - i) * (action - i)))
        return result
    nr0 = c.let("next0", by_action([a, swap0, swap0, r0 + u0, r0 - o0]))
    nr1 = c.let("next1", by_action([b, swap1, swap1, r1 + u1, r1 - o1]))
    ns = c.let("next_supply", by_action([sqrt, supply, supply, supply + mint, supply - a]))
    guards = []
    def reject(name, guard, description):
        guards.append(expr(guard))
        c.reject(name, guard, description)
    normalized = all_of([
        (create | add) | b.eq(0), create | sqrt.eq(0), (swap_in | swap_out) | direction.eq(0),
        (~create) | (lo0.eq(0) & lo1.eq(0)), (~(swap_in | swap_out)) | lo1.eq(0)])
    reject("DOMAIN", shape.eq(0) | action.gt(4) | direction.gt(1) | a.eq(0) | ~normalized
           | ((create | add) & (a.gt(J) | b.eq(0) | b.gt(J)))
           | (add & (lo0.gt(J) | lo1.gt(J))), "1 closed shape and input ranges")
    reject("POOL_EXISTS", create & exists.eq(1), "2 pair already exists")
    reject("NO_POOL", ~create & exists.eq(0), "2 pair absent")
    reject("POOL_INACTIVE", exists.eq(1) & status.ne(0), "3 ACTIVE only")
    reject("EMPTY_POOL", exists.eq(1) & (r0.eq(0) | r1.eq(0) | supply.lt(K)),
           "4 existing empty or below permanent lock")
    reject("DOMAIN", mirror.eq(0) | (exists.eq(0) & (r0.ne(0) | r1.ne(0) | supply.ne(0))),
           "5 pre-state pool/balance and LP equations")
    reject("RESERVED_LP_LOCK", remove & actor.eq(1), "6 burn lock actor before other roles")
    reject("RESERVED_ROLE", actor.ne(0) | ((swap | remove) & recipient.ne(0)),
           "6 no reserved actor or recipient")
    reject("BAD_SQRT", create & (~pg(a, b, sqrt, sqrt) | pg(a, b, sqrt + 1, sqrt + 1)),
           "7 floor square witness")
    reject("INITIAL_TOO_SMALL", create & sqrt.le(K), "7 positive creator mint after K")
    reject("POST_DOMAIN", swap_in & (x + a).gt(reserve_cap), "7 exact-in conservative gross reserve cap")
    reject("ZERO_NET", swap_in & net.le(0), "7 exact-in positive net")
    reject("ZERO_OUTPUT", swap_in & quote.le(0), "7 exact-in positive output")
    reject("SLIPPAGE", swap_in & quote.lt(lo0), "7 exact-in minimum output")
    reject("DRAIN", swap_out & a.ge(y), "7 exact-out no drain")
    reject("POST_DOMAIN", swap_out & (raw.gt(N) | (x + raw).gt(reserve_cap)),
           "7 exact-out raw gross bounds before narrowing")
    reject("INSUFFICIENT_QUOTE", swap_out & (net.le(0) | quote.lt(a)), "7 exact-out sufficient quote")
    reject("SLIPPAGE", swap_out & gross.gt(lo0), "7 exact-out maximum input")
    reject("QUOTE_GAP", swap_out & gap.gt(200), "7 exact-out ceil quote-gap cap")
    reject("ZERO_USED", add & (u0.le(0) | u1.le(0)), "7 ratio-selected positive deposits")
    reject("SLIPPAGE", add & (u0.lt(lo0) | u1.lt(lo1)), "7 add minima before mint")
    reject("ZERO_MINT", add & mint.le(0), "7 positive ratio mint")
    reject("INSUFFICIENT_LP", remove & (lp.lt(a) | a.gt(supply - K)), "7 burn balance and lock")
    reject("ZERO_PAYOUT", remove & (o0.le(0) | o1.le(0)), "7 both payouts before minima")
    reject("SLIPPAGE", remove & (o0.lt(lo0) | o1.lt(lo1)), "7 remove minima")
    reject("POST_DOMAIN", nr0.lt(0) | nr1.lt(0) | ns.lt(0)
           | nr0.gt(reserve_cap) | nr1.gt(reserve_cap) | ns.gt(supply_cap), "8 remaining successor bounds")
    # All factors are nonnegative even on rejected inputs: rules are eager.
    k_bad = ~pg(positive(sx), positive(sy), x, y)
    reject("INVARIANT", swap & (paid.le(0) | paid.ge(y) | k_bad), "9 swap output and k")
    reject("ARITHMETIC", False, "unreachable on admitted bounds; evaluator refusal is not a semantic reject")
    c.accept({110: nr0, 111: nr1, 112: ns, 113: 1, 114: 0})
    pool_laws(c, reserve_cap, supply_cap)
    return c


def pool_laws(c, reserve_cap, supply_cap):
    """Compact law surface, without expanding the rule program's selects.

    Split orientation and the ratio branch as logical alternatives. Safe
    denominators are total also on earlier-rejected tuples. Arithmetic factors
    inferred from a proposed successor are clamped before unsigned products;
    the associated signed inequalities still reject a wrong successor.
    """
    r0, r1, supply, exists, status = [field("pre", n) for n in range(110, 115)]
    action, direction, a, b, lo0, lo1, sqrt = [field("command", n) for n in range(120, 127)]
    mirror, actor, recipient, lp, shape = [field("context", n) for n in range(130, 135)]
    p0, p1, ps, pe, pt = [field("post", n) for n in range(110, 115)]
    create, swap_in, swap_out, add, remove = [action.eq(i) for i in range(5)]
    swap = swap_in | swap_out
    c.law("pool_shape", "commit, genesis",
          (pe.eq(0) & p0.eq(0) & p1.eq(0) & ps.eq(0))
          | (pe.eq(1) & p0.gt(0) & p1.gt(0) & ps.ge(K)), "StateInvariant")
    base = all_of([shape.eq(1), action.le(4), direction.le(1), a.gt(0), mirror.eq(1), actor.eq(0),
                   (create | add) | b.eq(0), create | sqrt.eq(0), swap | direction.eq(0),
                   (~create) | (lo0.eq(0) & lo1.eq(0)), (~swap) | lo1.eq(0),
                   (~(swap | remove)) | recipient.eq(0),
                   ((~create) | (exists.eq(0) & r0.eq(0) & r1.eq(0) & supply.eq(0))),
                   create | (exists.eq(1) & status.eq(0) & r0.gt(0) & r1.gt(0) & supply.ge(K))])
    create_ok = all_of([a.le(J), b.gt(0), b.le(J), sqrt.gt(K), a.le(reserve_cap), b.le(reserve_cap), sqrt.le(supply_cap),
                       pg(a, b, sqrt, sqrt), ~pg(a, b, sqrt + 1, sqrt + 1)])
    eligible_parts = [create & create_ok]
    c.law("create_exact", "accept", (~create) | all_of([
        p0.eq(a), p1.eq(b), ps.eq(sqrt), create_ok]), "MintBurnAuthorization")
    # On nonnegative state fields, v + floor(1/(v+1)) is max(1,v).
    safe0, safe1, safe_s = [v + call("div_floor", 1, v + 1) for v in (r0, r1, supply)]
    for d, (x, y, px, py) in enumerate(((r0, r1, p0, p1), (r1, r0, p1, p0))):
        for out in (False, True):
            mode = swap_out if out else swap_in
            t = mc(x, a, denominator(y - a))
            gross = mc(t, 10000, 9970) if out else a
            fee, protocol = mc(gross, 30, 10000), mf(mc(gross, 30, 10000), 2000, 10000)
            net = gross - fee
            q = mf(y, net, denominator(x + net))
            paid = a if out else q
            guard = all_of([gross.le(N), (x + gross).le(reserve_cap), net.gt(0), paid.gt(0), paid.lt(y)])
            if out:
                guard = guard & all_of([a.lt(y), q.ge(a), gross.le(lo0),
                    mc(positive(q - a), 10000, a + call("div_floor", 1, a + 1)).le(200)])
            else:
                guard = guard & q.ge(lo0)
            eligible_parts.append(mode & direction.eq(d) & guard)
            when = mode & direction.eq(d)
            c.law(f"{'out' if out else 'in'}_exact_{d}", "accept", (~when) | all_of([
                guard, px.eq(x + gross - protocol), py.eq(y - paid), ps.eq(supply)]), "FeeAndRounding")
            c.law(f"{'out' if out else 'in'}_fee_split_{d}", "accept", (~when) | all_of([
                pg(fee, 10000, gross, 30),
                (fee * 10000 - gross * 30).lt(10000),
                pg(fee, 2000, protocol, 10000), ~pg(fee, 2000, protocol + 1, 10000),
                (px - x + protocol).eq(gross)]), "AssetConservation")
            c.law(f"{'out' if out else 'in'}_round_trip_{d}", "accept", (~when)
                  | pg(gross, y, px, positive(y - py)), "FeeAndRounding")
            c.law(f"{'out' if out else 'in'}_quote_floor_{d}", "accept", (~when) | all_of([
                pg(y, net, q, x + net), ~pg(y, net, q + 1, x + net), paid.le(q)]), "FeeAndRounding")
            if out:
                # n=t and the two ceiling inequalities establish the fixed-fee
                # inverse's minimality; no brute-force search of g<A is needed.
                c.law(f"out_inverse_minimal_{d}", "accept", (~when) | all_of([
                    net.eq(t), pg(t, positive(y - a), x, a),
                    ~pg(positive(t - 1), positive(y - a), x, a)]), "FeeAndRounding")
    ratio_branch = pg(b, r0, a, r1)
    for branch, u0, u1 in ((ratio_branch, a, mf(a, r1, safe0)),
                            (~ratio_branch, mf(b, r0, safe1), b)):
        m0, m1 = mf(u0, supply, safe0), mf(u1, supply, safe1)
        add_ok = all_of([a.le(J), b.gt(0), b.le(J), lo0.le(J), lo1.le(J),
                         u0.gt(0), u1.gt(0), u0.ge(lo0), u1.ge(lo1), m0.gt(0), m1.gt(0),
                         (r0 + u0).le(reserve_cap), (r1 + u1).le(reserve_cap),
                         (supply + m0).le(supply_cap) | (supply + m1).le(supply_cap)])
        eligible_parts.append(add & branch & add_ok)
        c.law(f"add_ratio_{len(eligible_parts)}", "accept", (~(add & branch)) | all_of([
            p0.eq(r0 + u0), p1.eq(r1 + u1), add_ok]), "FeeAndRounding")
    out0, out1 = mf(a, r0, safe_s), mf(a, r1, safe_s)
    burn_ok = all_of([lp.ge(a), a.le(supply - K), out0.gt(0), out1.gt(0),
                      out0.ge(lo0), out1.ge(lo1)])
    eligible_parts.append(remove & burn_ok)
    c.law("remove_exact", "accept", (~remove) | all_of([
        burn_ok, p0.eq(r0 - out0), p1.eq(r1 - out1), ps.eq(supply - a)]), "MintBurnAuthorization")
    eligible = base & any_of(eligible_parts)
    c.law("accepts_only_eligible", "accept", eligible, "AuthoritySubjectRecipient")
    c.law("refuses_only_ineligible", "reject", ~eligible, "RejectNoAuthority")
    c.law("present_active_after_accept", "accept", pe.eq(1) & pt.eq(0),
          "AuthoritySubjectRecipient")
    c.law("swap_k_nondecrease", "accept", (~swap) | pg(p0, p1, r0, r1), "FeeAndRounding")
    used0, used1, minted = positive(p0 - r0), positive(p1 - r1), positive(ps - supply)
    c.law("add_mint_floor", "accept", (~add) | all_of([
        pg(used0, supply, minted, r0), pg(used1, supply, minted, r1),
        (~pg(used0, supply, minted + 1, r0)) | (~pg(used1, supply, minted + 1, r1))]), "FeeAndRounding")
    c.law("add_burn_no_profit", "accept", (~add) | all_of([
        pg(used0, ps, minted, p0), pg(used1, ps, minted, p1)]), "FeeAndRounding")
    for i, (r, post) in enumerate(((r0, p0), (r1, p1))):
        out = positive(r - post)
        c.law(f"burn_floor_{i}", "accept", (~remove) | all_of([
            out.gt(0), pg(a, r, out, supply), ~pg(a, r, out + 1, supply)]), "FeeAndRounding")


def balance_contract():
    c = Contract("spot-balance")
    balance = [c.add_field("pre", 110 + i, f"balance{i}", 0, BASE - 1) for i in range(4)]
    debit = [c.add_field("command", 120 + i, f"debit{i}", 0, BASE - 1) for i in range(4)]
    credit = [c.add_field("command", 124 + i, f"credit{i}", 0, BASE - 1) for i in range(4)]
    ingress = c.add_field("context", 130, "ingress", 0, 1)
    expected = c.add_field("context", 131, "expected_protocol_credit", 0, 1_800_000)
    borrow, carry, successors = expr(0), expr(0), []
    for i, (b, d, cr) in enumerate(zip(balance, debit, credit)):
        diff = b - d - borrow
        borrow = c.let(f"borrow{i}", select(diff.lt(0), 1, 0, step(expr(0) - diff)))
        column = c.let(f"column{i}", b - d + cr + carry)
        carry = c.let(f"carry{i}", call("div_floor", column, BASE))
        successors.append(c.let(f"next{i}", column - carry * BASE))
    invalid_ingress = any_of([v.ne(0) for v in debit] + [credit[0].ne(expected)]
                             + [v.ne(0) for v in credit[1:]])
    c.reject("INGRESS_ROLE", ingress.eq(1) & invalid_ingress, "1 ingress debit zero and exact protocol credit")
    c.reject("INGRESS_OVERFLOW", ingress.eq(1) & carry.gt(0), "2 ingress overflow before ordinary keys")
    c.reject("INSUFFICIENT_BALANCE", borrow.ne(0), "3 debit must be funded before credit")
    c.reject("BALANCE_OVERFLOW", carry.ne(0), "4 complete u128 result must fit")
    c.accept({110 + i: v for i, v in enumerate(successors)})
    c.law("limb_domain", "commit, genesis", all_of([field("post", 110 + i).ge(0) for i in range(4)]),
          "StateInvariant")
    eligible = (~(ingress.eq(1) & invalid_ingress)) & borrow.eq(0) & carry.eq(0)
    c.law("accepts_only_funded_exact_role", "accept", eligible, "AuthoritySubjectRecipient")
    c.law("refuses_only_invalid_role_or_balance", "reject", ~eligible, "RejectNoAuthority")
    c.law("exact_u128_columns", "accept", all_of([
        field("post", 110 + i).eq(value) for i, value in enumerate(successors)]), "DebitCreditEffectEquality")
    c.law("ingress_monotone", "accept", ingress.eq(0) | all_of([v.eq(0) for v in debit]),
          "AssetConservation")
    return c


def source_hash(directory):
    return {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in (directory / "project.zeno", directory / "v2/policy.json")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=APP)
    args = parser.parse_args()
    for name, contract in (("pool", pool_contract()), ("balance", balance_contract())):
        target = args.out / "components" / name
        contract.write(target)
        print(json.dumps({"component": name, "files": source_hash(target)}))


if __name__ == "__main__": main()
