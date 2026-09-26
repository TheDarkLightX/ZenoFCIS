"""Write synthesis.json from rules.txt.

The contract states screening and reinstatement as one relation: for every
input, the outputs are the decision, the rule (a fixed, ignored placeholder
for reinstatement), and the strikes after the decision. The screening sketch is a
priority-ordered program with three holes: the freeze threshold, the boundary
of a large amount, and the verdict code of a hold. Reinstatement has no holes.
`zeno-fcis synth` selects the hole values that satisfy the contract on every
input.

After editing rules.txt:

    python3 rules_to_synthesis.py rules.txt synthesis.json
    rm -rf synthesized
    zeno-fcis synth run synthesis.json --out synthesized

The sketch names the rules of the shipped rule base. A new rule needs a
matching branch in `sketch`, a `RuleId` variant in project.zeno, and, if it
blocks, a committed-failure reason there; `tests/conformance.rs` checks that
the declarations still match the file.

This script is development tooling: it holds no authority, and the checks
that matter run afterwards. `tests/rule_base.rs` compares the selected program
with a separate evaluation of rules.txt on every input, and the library's
`tools/check_synthesis.py` does so again in Python.
"""
import json
import sys

VERDICT = {"allow": 0, "hold": 1, "block": 2}


class Graph:
    """Builds a node list, sharing identical nodes."""

    def __init__(self, inputs):
        self.nodes = [["input", i] for i in range(inputs)]
        self.index = {}

    def node(self, *spec):
        spec = list(spec)
        key = json.dumps(spec)
        if key not in self.index:
            self.nodes.append(spec)
            self.index[key] = len(self.nodes) - 1
        return self.index[key]

    def hole(self, number, alternatives):
        self.nodes.append({"hole": number, "alternatives": alternatives})
        return len(self.nodes) - 1

    def int(self, value):
        return self.node("int", value)

    def all_of(self, *terms):
        result = terms[-1]
        for term in reversed(terms[:-1]):
            result = self.node("and", term, result)
        return result


def parse(text):
    """Reads the features and rules; src/rules.rs is the checked parser."""
    features, rules = [], []
    for line in text.splitlines():
        words = line.split()
        if not words or words[0].startswith("#"):
            continue
        if words[0] == "feature":
            name, spec = words[1], words[2:]
            if len(spec) == 1 and ".." in spec[0]:
                low, high = spec[0].split("..")
                features.append((name, int(low), int(high), None))
            else:
                features.append((name, 0, len(spec) - 1, spec))
            continue
        assert words[0] == "rule" and words[2] == "priority" and words[4] == "when", line
        name, priority = words[1], int(words[3])
        then = words.index("then")
        verdict = words[then + 1]
        conditions = []
        clause = words[5:then]
        if clause != ["always"]:
            while clause:
                feature, op, value = clause[:3]
                clause = clause[4:] if clause[3:4] == ["and"] else clause[3:]
                position = [f[0] for f in features].index(feature)
                names = features[position][3]
                conditions.append((position, op, names.index(value) if names else int(value)))
        rules.append((name, priority, conditions, verdict))
    return features, rules


def condition(g, feature_input, op, value):
    if op == "==":
        return g.node("eq", feature_input, g.int(value))
    if op == "!=":
        return g.node("not", g.node("eq", feature_input, g.int(value)))
    if op == "<":
        return g.node("lt", feature_input, g.int(value))
    if op == "<=":
        return g.node("lt", feature_input, g.int(value + 1))
    if op == ">":
        return g.node("lt", g.int(value), feature_input)
    if op == ">=":
        return g.node("lt", g.int(value - 1), feature_input)
    raise ValueError(op)


def contract(features, rules):
    # Environment: features, action, reviewer, then decision, rule, post strikes.
    n = len(features)
    g = Graph(n + 5)
    strikes = [f[0] for f in features].index("strikes")
    action, reviewer = n, n + 1
    output_verdict, output_rule, output_post = n + 2, n + 3, n + 4
    cap = features[strikes][2]
    struck = g.node("select", g.node("lt", strikes, g.int(cap)),
                    g.node("add", strikes, g.int(1)), g.int(cap))
    ordered = sorted(enumerate(rules), key=lambda item: -item[1][1])
    verdict, rule = g.int(0), g.int(len(rules) - 1)
    for index, (name, priority, conditions, outcome) in reversed(ordered):
        holds = (g.all_of(*[condition(g, *c) for c in conditions]) if conditions
                 else g.node("bool", True))
        verdict = g.node("select", holds, g.int(VERDICT[outcome]), verdict)
        rule = g.node("select", holds, g.int(index), rule)
    screened_post = g.node("select", g.node("eq", verdict, g.int(2)), struck, strikes)
    reinstating = g.node("eq", action, g.int(1))
    reinstate_code = g.node("select", g.node("not", reviewer), g.int(3),
                           g.node("select", g.node("eq", strikes, g.int(0)),
                                  g.int(4), g.int(5)))
    verdict = g.node("select", reinstating, reinstate_code, verdict)
    rule = g.node("select", reinstating, g.int(len(rules) - 1), rule)
    post = g.node("select", reinstating,
                  g.node("select", reviewer, g.int(0), strikes), screened_post)
    root = g.all_of(g.node("eq", output_verdict, verdict),
                    g.node("eq", output_rule, rule),
                    g.node("eq", output_post, post))
    return {"nodes": g.nodes, "roots": [root]}


def sketch(features, rules):
    """A priority-ordered decision program for exactly this rule base."""
    g = Graph(len(features) + 2)
    strikes, tier, region, band, risk = range(5)
    action, reviewer = 5, 6
    freeze = g.hole(1, [["int", 2], ["int", 3]])
    large_boundary = g.hole(2, [["int", 1], ["int", 2], ["int", 3]])
    hold = g.hole(3, [["int", 0], ["int", 1], ["int", 2]])
    frozen = g.node("eq", strikes, freeze)
    large = g.node("lt", large_boundary, band)
    sanctioned = g.node("eq", region, g.int(2))
    restricted = g.node("eq", region, g.int(1))
    allowed = g.node("eq", region, g.int(0))
    tier0 = g.node("eq", tier, g.int(0))
    tier1 = g.node("eq", tier, g.int(1))
    high = g.node("eq", risk, g.int(2))
    medium = g.node("eq", risk, g.int(1))
    fired = [
        sanctioned,
        frozen,
        g.all_of(tier0, high),
        g.all_of(restricted, large),
        g.all_of(allowed, tier0, large),
        high,
        restricted,
        g.all_of(tier0, g.node("lt", g.int(0), band)),
        g.all_of(tier1, large),
        g.all_of(g.node("lt", g.int(1), strikes), g.node("lt", g.int(1), band)),
        g.all_of(medium, g.node("eq", band, g.int(4))),
    ]
    codes = [g.int(2)] * 5 + [hold] * 6
    rule = g.int(len(fired))
    verdict = g.int(0)
    for index in reversed(range(len(fired))):
        rule = g.node("select", fired[index], g.int(index), rule)
        verdict = g.node("select", fired[index], codes[index], verdict)
    blocked = g.node("eq", verdict, g.int(2))
    struck = g.node("select", g.node("lt", strikes, freeze),
                    g.node("add", strikes, g.int(1)), freeze)
    post = g.node("select", blocked, struck, strikes)
    reinstating = g.node("eq", action, g.int(1))
    missing_strikes = g.node("eq", strikes, g.int(0))
    reinstate_code = g.node("select", g.node("not", reviewer), g.int(3),
                           g.node("select", missing_strikes, g.int(4), g.int(5)))
    return {"nodes": g.nodes, "roots": [
        g.node("select", reinstating, reinstate_code, verdict),
        g.node("select", reinstating, g.int(len(rules) - 1), rule),
        g.node("select", reinstating,
               g.node("select", reviewer, g.int(0), strikes), post),
    ]}


def integer(low, high):
    return {"kind": "int", "min": low, "max": high}


features, rules = parse(open(sys.argv[1]).read())
assert [f[0] for f in features] == ["strikes", "identity_tier", "region", "amount_band", "counterparty_risk"]
problem = {
    "schema": "zeno-fcis/synthesis-problem/1",
    "profile": "zeno-fcis/finite-i64/1",
    "inputs": [{"name": f"pre.{name}" if name == "strikes" else f"feature.{name}",
                "type": integer(low, high)} for name, low, high, _ in features] + [
                    {"name": "command.action.screen_reinstate", "type": integer(0, 1)},
                    {"name": "context.reviewer", "type": {"kind": "bool"}},
                ],
    "outputs": [
        {"name": "decision.allow_hold_block_not_reviewer_no_strikes_reinstated",
         "type": integer(0, 5)},
        {"name": "decision.rule", "type": integer(0, len(rules) - 1)},
        {"name": "post.strikes", "type": integer(features[0][1], features[0][2])},
    ],
    "contract": contract(features, rules),
    "sketch": sketch(features, rules),
}
with open(sys.argv[2], "w") as output:
    json.dump(problem, output, indent=2)
    output.write("\n")
print(f"{len(rules)} rules; contract {len(problem['contract']['nodes'])} nodes, "
      f"sketch {len(problem['sketch']['nodes'])} nodes")
