#!/usr/bin/env python3
"""Small, closed declared domains for the pinned 2.4 contract judges.

The reserve/supply caps of the swap/LP projects are explicit specializations.
The real fee, protocol share, lock and quote-gap cap are never changed. The
create and LP encodings retain correlated physical fields in one scalar.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

from build_contracts import BASE, balance_contract, pool_contract, source_hash


def fold(c, bindings):
    """Substitute a declared exact correlated encoding, never sample a domain."""
    removed = set(bindings)
    c.fields = [f for f in c.fields if f"{f[0]}.{100 if f[0]=='pre' else 101 if f[0]=='command' else 102}.{f[1]}" not in removed]
    removed_state = {key.rsplit(".", 1)[1] for key in removed if key.startswith("pre.")}
    c.genesis = {k: v for k, v in c.genesis.items() if k not in removed_state}
    pattern = re.compile("|".join(re.escape(k) + r"\b" for k in sorted(bindings, key=len, reverse=True)))
    def replace(value, law=False):
        if type(value) is not str: return value
        return pattern.sub(lambda m: f"({bindings[m.group()][1 if law else 0]})", value)
    c.variables = {k: replace(v) for k, v in c.variables.items()}
    for case in c.cases:
        case["when"] = replace(case["when"])
        case["post"] = {k: replace(v) for k, v in case["post"].items() if k not in removed_state}
    c.laws = [(n, name, scope, replace(formula, True), kind) for n, name, scope, formula, kind in c.laws]


def frozen_ranges(c):
    return {f"{root}.{number}": (0, 0) for root, number, _, _, _ in c.fields}


def projects(out):
    result = []
    def save(name, c, ranges, genesis, scenarios, explanation):
        target = out / name
        c.name = f"spot-{name}"
        c.first_reason_law(ranges)
        c.write(target, ranges, genesis)
        if name == "ingress-edge":
            # 2.4 lemma allocates a law ID above every u32 numeral in .zeno.
            # A range endpoint UINT32_MAX exhausts that allocator. The supported
            # I128 leaf binding moves the identical bounds into policy.json.
            project = target / "project.zeno"
            policy_path = target / "v2/policy.json"
            policy = json.loads(policy_path.read_text())
            def leaf(match):
                policy["leaf_bindings"][match[1]] = ["I128", int(match[3]), int(match[4])]
                return f"type {match[1]} int {match[2]};"
            project.write_text(re.sub(r"type (\d+) int (\w+) in (\d+)\.\.=(\d+);", leaf, project.read_text()))
            policy_path.write_text(json.dumps(policy, indent=2) + "\n")
        (target / "tests").mkdir(exist_ok=True)
        (target / "tests/decision-scenarios.txt").write_text(scenarios)
        tuples = 1
        for low, high in ranges.values(): tuples *= high - low + 1
        result.append({"name": name, "tuples": tuples, "encoding": explanation,
                       "sources": source_hash(target), "ranges": ranges,
                       "constants": {"fee_bps": 30, "protocol_bps": 2000, "locked_shares": 1000, "quote_gap_bps": 200}})
    # Exact-in/out: genuine 1000-share lock with deliberately tiny seeded
    # reserves. These seeded states are not asserted reachable in the full app.
    for out_mode in (False, True):
        c = pool_contract(reserve_cap=4, supply_cap=1000)
        ranges = frozen_ranges(c)
        ranges.update({"pre.110": (1, 4), "pre.111": (1, 4), "pre.112": (1000, 1000), "pre.113": (1, 1),
                       "command.120": (2 if out_mode else 1,) * 2, "command.121": (0, 1),
                       "command.122": (1, 1) if out_mode else (2, 2),
                       "command.124": (1, 3) if out_mode else (0, 0),
                       "context.130": (1, 1), "context.134": (1, 1)})
        genesis = {110: 1, 111: 1, 112: 1000, 113: 1, 114: 0}
        if out_mode:
            scenarios = ("# scenario exact-out-and-drain\n"
                         "1 2 1000 1 0 | 2 0 1 0 2 0 0 | 1 0 0 0 1 | accept - 3 1 1000 1 0 | -\n"
                         "3 1 1000 1 0 | 2 0 1 0 2 0 0 | 1 0 0 0 1 | reject 213 3 1 1000 1 0 | -\n"
                         "# scenario slippage-before-gap\n"
                         "1 4 1000 1 0 | 2 0 1 0 1 0 0 | 1 0 0 0 1 | reject 212 1 4 1000 1 0 | -\n"
                         "1 4 1000 1 0 | 2 0 1 0 2 0 0 | 1 0 0 0 1 | reject 215 1 4 1000 1 0 | -\n")
        else:
            scenarios = ("# scenario reverse-swap\n"
                         "1 4 1000 1 0 | 1 0 2 0 0 0 0 | 1 0 0 0 1 | accept - 3 2 1000 1 0 | -\n"
                         "3 2 1000 1 0 | 1 1 2 0 0 0 0 | 1 0 0 0 1 | accept - 2 4 1000 1 0 | -\n"
                         "# scenario gross-cap-before-output\n"
                         "3 1 1000 1 0 | 1 0 2 0 0 0 0 | 1 0 0 0 1 | reject 209 3 1 1000 1 0 | -\n")
        save("swap-out" if out_mode else "swap-in", c, ranges, genesis, scenarios,
             "reserve cap 4, supply fixed at real K=1000; seeded reserves 1..4; both directions; fixed owner constants")
    c = pool_contract(reserve_cap=1002, supply_cap=1002)
    fold(c, {"pre.100.111": ("pre.100.110",) * 2, "pre.100.112": ("pre.100.110",) * 2,
             "post.100.111": ("post.100.110",) * 2, "post.100.112": ("post.100.110",) * 2,
             "command.101.123": ("choose(command.101.120 == 3, command.101.122, 0)", "command.101.122 * (4 - command.101.120)"),
             "context.102.133": ("pre.100.110 - 1000",) * 2})
    ranges = frozen_ranges(c)
    ranges.update({"pre.110": (1000, 1002), "pre.113": (1, 1), "command.120": (3, 4), "command.122": (1, 2),
                   "command.124": (0, 1), "command.125": (0, 1), "context.130": (1, 1), "context.134": (1, 1)})
    save("lp", c, ranges, {110: 1000, 113: 1, 114: 0},
         "# scenario add-and-immediate-burn\n"
         "1000 1 0 | 3 0 1 0 0 0 | 1 0 0 1 | accept - 1001 1 0 | -\n"
         "1001 1 0 | 4 0 1 0 0 0 | 1 0 0 1 | accept - 1000 1 0 | -\n"
         "1000 1 0 | 4 0 1 0 0 0 | 1 0 0 1 | reject 218 1000 1 0 | -\n",
         "exact correlated r0=r1=S in 1000..1002; cap 1002; actor owns S-K; balanced desires; create not included")
    c = pool_contract(reserve_cap=1001, supply_cap=1001)
    bindings = {}
    for root in ("pre", "post"):
        for number in (110, 111, 112): bindings[f"{root}.100.{number}"] = (f"{root}.100.113 * 1001",) * 2
    fold(c, bindings)
    ranges = frozen_ranges(c)
    ranges.update({"pre.113": (0, 1), "command.122": (1000, 1001), "command.123": (1000, 1001), "command.126": (1000, 1001),
                   "context.130": (1, 1), "context.134": (1, 1)})
    save("create", c, ranges, {113: 0, 114: 0},
         "# scenario initialize-once\n"
         "0 0 | 0 0 1001 1001 0 0 1001 | 1 0 0 0 1 | accept - 1 0 | -\n"
         "1 0 | 0 0 1001 1001 0 0 1001 | 1 0 0 0 1 | reject 201 1 0 | -\n"
         "# scenario exact-sqrt-before-size\n"
         "0 0 | 0 0 1000 1000 0 0 1001 | 1 0 0 0 1 | reject 207 0 0 | -\n"
         "0 0 | 0 0 1000 1000 0 0 1000 | 1 0 0 0 1 | reject 208 0 0 | -\n",
         "absent or balanced initialized r0=r1=S=1001; state scalar exists; real K=1000; sqrt 1000/1001")
    c = balance_contract()
    ranges = frozen_ranges(c)
    ranges.update({"pre.110": (BASE - 2, BASE - 1), **{f"pre.{111+i}": (BASE - 1, BASE - 1) for i in range(3)},
                   "command.120": (0, 1), "command.124": (0, 2), "context.130": (1, 1), "context.131": (0, 2)})
    state0, state1 = f"{BASE-2} {BASE-1} {BASE-1} {BASE-1}", f"{BASE-1} {BASE-1} {BASE-1} {BASE-1}"
    save("ingress-edge", c, ranges, {110: BASE-2, 111: BASE-1, 112: BASE-1, 113: BASE-1},
         f"# scenario reach-u128-max\n{state0} | 0 0 0 0 1 0 0 0 | 1 1 | accept - {state1} | -\n"
         f"{state1} | 0 0 0 0 1 0 0 0 | 1 1 | reject 223 {state1} | -\n"
         f"# scenario role-before-overflow\n{state1} | 1 0 0 0 2 0 0 0 | 1 1 | reject 222 {state1} | -\n",
         "actual base 2^32, full u128 b=U-1/U; debit 0/1, credit and expected P 0..2; ingress role")
    (out / "domains.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    print(json.dumps(projects(args.out), indent=2))


if __name__ == "__main__": main()
