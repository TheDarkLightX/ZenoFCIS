#!/usr/bin/env python3
"""Emit the closed two-user, one-pair Spot application on the scalar route.

No callback decides economics. The emitted complete contract stages Pool, eight
u128 Balance keys and LP ownership in one Authority invocation. ZenoLedger
publication, authentication, durable retry and recovery are separate work.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from build_contracts import (APP, BASE, K, N, Contract, Expr, REASONS, all_of,
                             any_of, balance_contract, call, expr, field,
                             pool_contract, select, source_hash, step)

# Canonical full-key order for the fixed ledger/assets/users in this profile:
# pool(X), pool(Y), protocol:fee-ingress(X), protocol:fee-ingress(Y),
# user:alice(X), user:alice(Y), user:bob(X), user:bob(Y).
KEYS = ("pool-x", "pool-y", "ingress-x", "ingress-y", "alice-x", "alice-y", "bob-x", "bob-y")


def numeric_if(condition, yes, no, indicator):
    return select(condition, yes, no, indicator)


def equals_indicator(value, constant):
    return expr(1) - step((value - constant) * (value - constant))


def compose():
    c = pool_contract()
    c.name = "zenodex-spot"
    c.cases.pop()  # Pool's Accept is staged; Balance must still accept every key.
    c.fields = [f for f in c.fields if f[0] != "context" or f[1] == 134]
    actor = c.add_field("command", 127, "actor", 0, 5)
    recipient = c.add_field("command", 128, "recipient", 0, 5)
    shares = [c.add_field("pre", 150 + i, f"lp_{name}", 0, N) for i, name in enumerate(("alice", "bob", "lock"))]
    balances = [[c.add_field("pre", 2000 + 4 * k + i, f"{name.replace('-', '_')}_limb{i}", 0, BASE - 1)
                 for i in range(4)] for k, name in enumerate(KEYS)]
    def actor_role(value):
        reserved = numeric_if(value.eq(3), 1, 2, equals_indicator(value, 3))
        reserved = numeric_if(value.ge(4), 3, reserved, step(value - 3))
        return numeric_if(value.lt(2), 0, reserved, expr(1) - step(value - 1))
    actor_role_value, recipient_role_value = actor_role(actor), actor_role(recipient)
    actor_lp = numeric_if(actor.eq(0), shares[0], numeric_if(actor.eq(1), shares[1], 0, equals_indicator(actor, 1)),
                          equals_indicator(actor, 0))
    def mirror(root):
        r0, r1, supply, exists = [field(root, n) for n in range(110, 114)]
        lp = [field(root, 150 + i) for i in range(3)]
        pool_b = [[field(root, 2000 + 4 * k + i) for i in range(4)] for k in range(2)]
        holdings = all_of([pool_b[0][0].eq(r0), pool_b[1][0].eq(r1)]
                          + [v.eq(0) for row in pool_b for v in row[1:]])
        lp_ok = (lp[0] + lp[1] + lp[2]).eq(supply)
        return holdings & lp_ok & ((exists.eq(0) & supply.eq(0)) | (exists.eq(1) & lp[2].ge(K)))
    mirror_before = mirror("pre")
    replacements = {
        "context.102.130": c.let("snapshot_mirrors", mirror_before),
        "context.102.131": c.let("snapshot_actor_role", actor_role_value),
        "context.102.132": c.let("snapshot_recipient_role", recipient_role_value),
        "context.102.133": c.let("snapshot_actor_lp", actor_lp),
    }
    # Replace only complete projection tokens; field IDs are stable interface IDs.
    import re
    def substitute(text, law=False):
        def replacement(match):
            value = replacements[match.group()]
            return "1" if law and match.group() == "context.102.130" else f"({value.law if law else value.rule})"
        return re.sub(r"context\.102\.(?:130|131|132|133)\b", replacement, text)
    c.variables = {k: substitute(v) for k, v in c.variables.items()}
    for case in c.cases:
        if type(case["when"]) is str: case["when"] = substitute(case["when"])
    rewritten = []
    for number, name, scope, formula, kind in c.laws:
        formula = substitute(formula, law=True)
        if name == "refuses_only_ineligible":
            # Global Balance failures can refuse an otherwise eligible Pool.
            # The exact first reason and frame remain framework-conformance
            # obligations; the component gap judges inspect the local law.
            continue
        rewritten.append((number, name, scope, formula, kind))
    # Keep law IDs dense after removing the local-only progress law.
    c.laws = [(500 + i, name, scope, formula, kind) for i, (_, name, scope, formula, kind) in enumerate(rewritten)]
    c.law("complete_snapshot_mirrors", "accept", mirror_before, "AuthoritySubjectRecipient")
    c.law("complete_successor_mirrors", "commit, genesis", mirror("post"), "StateInvariant")

    action, direction, a, b = [field("command", n) for n in range(120, 124)]
    create, swap_in, swap_out, add, remove = [action.eq(i) for i in range(5)]
    swap = swap_in | swap_out
    c.cases.insert(1, {"rule": "normalized create/add recipient is actor", "when": ((create | add) & actor.ne(recipient)).rule,
                       "class": "Reject", "reason": REASONS["DOMAIN"], "post": {}, "outbox": []})
    c.law("normalized_recipient", "accept", (~(create | add)) | actor.eq(recipient), "AuthoritySubjectRecipient")
    def choose_action(values):
        result = expr(values[-1])
        for i in reversed(range(4)):
            result = numeric_if(action.eq(i), values[i], result, equals_indicator(action, i))
        return result
    # These variables are exact outputs of the Pool calculation, not caller proposals.
    v = {name: Expr(name, name) for name in c.variables}
    debits, credits = [], []
    zero = expr(0)
    for key in range(8):
        asset = key % 2
        in_asset = direction.eq(asset)
        pool_in = numeric_if(in_asset, v["gross"] - v["protocol"], 0, equals_indicator(direction, asset))
        pool_out = numeric_if(in_asset, 0, v["paid"], equals_indicator(direction, asset))
        ordinary_in = numeric_if(in_asset, v["gross"], 0, equals_indicator(direction, asset))
        used = v[f"used{asset}"]
        payout = v[f"payout{asset}"]
        create_amount = a if asset == 0 else b
        if key < 2:
            debit = choose_action([0, pool_out, pool_out, 0, payout])
            credit = choose_action([create_amount, pool_in, pool_in, used, 0])
        elif key < 4:
            debit = zero
            credit = numeric_if(swap & in_asset, v["protocol"], 0,
                                (equals_indicator(action, 1) + equals_indicator(action, 2)) * equals_indicator(direction, asset))
        else:
            who = 0 if key < 6 else 1
            debit = numeric_if(actor.eq(who), choose_action([create_amount, ordinary_in, ordinary_in, used, 0]), 0,
                               equals_indicator(actor, who))
            credit = numeric_if(recipient.eq(who), choose_action([0, pool_out, pool_out, 0, payout]), 0,
                                equals_indicator(recipient, who))
        debits.append(c.let(f"debit_key{key}", debit))
        credits.append(c.let(f"credit_key{key}", credit))
    # Reuse the entire Balance rule graph, with command-derived debit/credit and
    # one immutable pre-state. Process complete keys in the order above.
    balance_source = balance_contract()
    posts = {str(n): Expr(name, name).rule for n, name in ((110, "next0"), (111, "next1"), (112, "next_supply"))}
    posts.update({"113": "1", "114": "0"})
    for key, limbs in enumerate(balances):
        mapping = {f"pre.100.{110+i}": limbs[i].rule for i in range(4)}
        mapping.update({f"command.101.{120+i}": debits[key].rule if i == 0 else "0" for i in range(4)})
        mapping.update({f"command.101.{124+i}": credits[key].rule if i == 0 else "0" for i in range(4)})
        mapping["context.102.130"] = "1" if key in (2, 3) else "0"
        mapping["context.102.131"] = credits[key].rule if key in (2, 3) else "0"
        def map_balance(text):
            text = re.sub(r"(?:pre\.100\.(?:110|111|112|113)|command\.101\.(?:120|121|122|123|124|125|126|127)|context\.102\.(?:130|131))\b",
                          lambda m: f"({mapping[m.group()]})", text)
            names = "|".join(map(re.escape, balance_source.variables))
            return re.sub(rf"\b({names})\b", lambda m: f"key{key}_{m.group()}", text)
        for name, value in balance_source.variables.items(): c.variables[f"key{key}_{name}"] = map_balance(value)
        for case in balance_source.cases[:-1]:
            c.cases.append({**case, "rule": f"Balance {KEYS[key]}: {case['rule']}", "when": map_balance(case["when"])})
        for i in range(4): posts[str(2000 + 4 * key + i)] = f"key{key}_next{i}"
    mint = choose_action([field("command", 126) - K, 0, 0, v["mint"], expr(0) - a])
    for who in range(2):
        posts[str(150 + who)] = (shares[who] + numeric_if(actor.eq(who), mint, 0, equals_indicator(actor, who))).rule
    posts["152"] = (shares[2] + numeric_if(create, K, 0, equals_indicator(action, 0))).rule
    # Complete conservation is independent of the rule graph: compare normalized
    # column sums of all four holders of each asset, including ingress, without
    # ever reconstructing a >i128 total.
    complete_conservation = []
    for asset in range(2):
        carry_pre, carry_post = expr(0), expr(0)
        equal = []
        for limb in range(4):
            before, after = carry_pre, carry_post
            for key in range(asset, 8, 2):
                before = before + field("pre", 2000 + 4 * key + limb)
                after = after + field("post", 2000 + 4 * key + limb)
            carry_pre, carry_post = call("div_floor", before, BASE), call("div_floor", after, BASE)
            equal.append((before - carry_pre * BASE).eq(after - carry_post * BASE))
        equal.append(carry_pre.eq(carry_post))
        c.law(f"asset_{asset}_conservation_including_ingress", "accept", all_of(equal), "AssetConservation")
        # Independently compare the computed candidate's columns before Accept,
        # so a failed complete candidate has the spec's semantic reject reason.
        pre_carry, next_carry, candidate_equal = expr(0), expr(0), []
        for limb in range(4):
            before, after = pre_carry, next_carry
            for key in range(asset,8,2):
                number = 2000 + 4*key + limb
                before = before + field("pre",number)
                after = after + expr(posts[str(number)])
            pre_carry, next_carry = call("div_floor",before,BASE), call("div_floor",after,BASE)
            candidate_equal.append((before-pre_carry*BASE).eq(after-next_carry*BASE))
        candidate_equal.append(pre_carry.eq(next_carry))
        complete_conservation.extend(candidate_equal)
        ingress_key = 2 + asset
        equal_prefix, nondecreasing = expr(True), expr(False)
        for limb in reversed(range(4)):
            before = field("pre", 2000 + 4 * ingress_key + limb)
            after = field("post", 2000 + 4 * ingress_key + limb)
            nondecreasing = nondecreasing | (equal_prefix & after.gt(before))
            equal_prefix = equal_prefix & after.eq(before)
        c.law(f"ingress_{asset}_no_debit", "accept", nondecreasing | equal_prefix, "AssetConservation")
    lp_total = expr(posts["150"]) + expr(posts["151"]) + expr(posts["152"])
    complete_conservation.append((lp_total - shares[0] - shares[1] - shares[2]).eq(
        expr("next_supply") - field("pre",112)))
    c.reject("UNBALANCED_EFFECTS", ~all_of(complete_conservation), "complete per-asset and LP conservation")
    candidate_mirrors = [expr(posts["2000"]).eq(expr("next0")),expr(posts["2004"]).eq(expr("next1")),
                         lp_total.eq(expr("next_supply")),expr(posts["152"]).ge(K)]
    candidate_mirrors += [expr(posts[str(2000+4*key+limb)]).eq(0) for key in range(2) for limb in (1,2,3)]
    c.reject("POOL_BALANCE_MISMATCH", ~all_of(candidate_mirrors), "complete pool/Balance and LP/lock equations")
    # Deliveries are effect plans for ZenoLedger, not direct token settlement.
    c.declarations = ["type 103 destination ZenoLedger;", "type 900 int AccountSlot in 0..=5;",
                      f"type 901 int Delta in -{N}..={N};"]
    for asset in range(3):
        payload, channel = 104 + asset, 300 + asset
        c.declarations += [f"type {payload} payload Plan{asset};",
                           f"field {3000+3*asset} {payload} source 900;",
                           f"field {3001+3*asset} {payload} destination 900;",
                           f"field {3002+3*asset} {payload} quantity 901;",
                           f"channel {channel} plan{asset} destination 103 payload {payload};"]
        c.channels.append(channel)
    # The generic writer uses this supported destination leaf binding.
    c.destination_binding = {"103": ["Text", 1, 32]}
    c.declarations += ["type 107 payload DecisionOutput;"]
    c.declarations += [f"field {3010+i} 107 value{i} 901;" for i in range(8)]
    c.declarations += ["channel 303 decision_output destination 103 payload 107;"]
    c.channels.append(303)
    def delivery(asset, source, target, quantity, ordinal):
        return {"ordinal": ordinal, "channel": 300 + asset, "destination": "zenoledger",
                "payload": {str(3000+3*asset): expr(source).rule, str(3001+3*asset): expr(target).rule,
                            str(3002+3*asset): expr(quantity).rule}, "idempotency_ordinal": ordinal}
    def accepting(guard, outbox):
        # Observation payload, distinct from the economic transfer/LP channels.
        # Generic positions are documented by operation in composition.json.
        if guard is create:
            values = [field("command", 126) - K, K, 0, 0, 0, 0, 0, 0]
        elif guard is add:
            values = [v["used0"], v["used1"], v["mint"], a - v["used0"], b - v["used1"], 0, 0, 0]
        elif guard is True:
            values = [v["payout0"], v["payout1"], 0, 0, 0, 0, 0, 0]
        else:
            # q/h are output-only for exact-out; exact-in positions are zero.
            is_out = equals_indicator(action, 2)
            values = [v["gross"], v["paid"], v["fee"], v["protocol"], v["fee"] - v["protocol"], v["net"],
                      numeric_if(swap_out, v["quote"], 0, is_out), numeric_if(swap_out, v["gap_bps"], 0, is_out)]
        outbox = outbox + [{"ordinal": len(outbox), "channel": 303, "destination": "decision",
                            "payload": {str(3010+i): expr(value).rule for i, value in enumerate(values)},
                            "idempotency_ordinal": len(outbox)}]
        c.cases.append({"rule": "complete atomic Spot plan", "when": expr(guard).rule, "class": "Accept",
                        "reason": None, "post": posts, "outbox": outbox})
    accepting(create, [delivery(0, actor, 2, a, 0), delivery(1, actor, 2, b, 1),
                       delivery(2, actor, actor, field("command", 126) - K, 2), delivery(2, 3, 3, K, 3)])
    for direction_value in range(2):
        for has_protocol in (True, False):
            guard = swap & direction.eq(direction_value) & (v["protocol"].gt(0) if has_protocol else v["protocol"].eq(0))
            outbox = [delivery(direction_value, actor, 2, v["gross"] - v["protocol"], 0)]
            if has_protocol: outbox.append(delivery(direction_value, actor, 4 + direction_value, v["protocol"], 1))
            outbox.append(delivery(1 - direction_value, 2, recipient, v["paid"], len(outbox)))
            accepting(guard, outbox)
    accepting(add, [delivery(0, actor, 2, v["used0"], 0), delivery(1, actor, 2, v["used1"], 1),
                    delivery(2, actor, actor, v["mint"], 2)])
    accepting(True, [delivery(2, actor, actor, expr(0) - a, 0), delivery(0, 2, recipient, v["payout0"], 1),
                     delivery(1, 2, recipient, v["payout1"], 2)])
    return c


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=APP)
    args = parser.parse_args()
    c = compose()
    c.write(args.out)
    policy_path = args.out / "v2/policy.json"
    policy = json.loads(policy_path.read_text())
    policy["leaf_bindings"].update(c.destination_binding)
    policy_path.write_text(json.dumps(policy, indent=2) + "\n")
    print(json.dumps(source_hash(args.out)))


if __name__ == "__main__": main()
