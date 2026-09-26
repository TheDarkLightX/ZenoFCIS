"""Build the finite decision problem from the order's reviewed rule table.

Run with ``--check`` to compare the checked-in problem without changing it.
The separate Rust conformance model and tools/check_synthesis.py independently
restate the README rules; this builder is not an oracle for those checks.
"""

import argparse
import json
from pathlib import Path


class Graph:
    def __init__(self, inputs):
        self.nodes = [["input", i] for i in range(inputs)]
        self.index = {json.dumps(node): i for i, node in enumerate(self.nodes)}

    def node(self, *spec):
        spec = list(spec)
        key = json.dumps(spec)
        if key not in self.index:
            self.index[key] = len(self.nodes)
            self.nodes.append(spec)
        return self.index[key]

    def integer(self, value):
        return self.node("int", value)

    def equal(self, left, right):
        return self.node("eq", left, right)

    def neg(self, value):
        return self.node("not", value)

    def both(self, left, right):
        return self.node("and", left, right)

    def either(self, left, right):
        return self.neg(self.both(self.neg(left), self.neg(right)))

    def choose(self, condition, yes, no):
        return self.node("select", condition, yes, no)

    def hole(self, number, choices):
        self.nodes.append({"hole": number, "alternatives": [["int", v] for v in choices]})
        return len(self.nodes) - 1


def decision(g, searching):
    action, status, attempts, callback, caller = range(5)
    is_action = [g.equal(action, g.integer(i)) for i in range(6)]
    is_status = [g.equal(status, g.integer(i)) for i in range(6)]
    customer = g.hole(1, [1, 0]) if searching else g.integer(0)
    provider = g.hole(2, [0, 2, 1]) if searching else g.integer(1)
    carrier = g.hole(3, [1, 2]) if searching else g.integer(2)
    limit = g.hole(4, [2, 3]) if searching else g.integer(3)
    customer_action = g.either(is_action[0], is_action[5])
    provider_action = g.either(is_action[1], is_action[2])
    expected_caller = g.choose(customer_action, customer,
                               g.choose(provider_action, provider, carrier))
    valid = g.either(
        g.both(is_action[0], is_status[0]),
        g.either(
            g.both(provider_action, is_status[1]),
            g.either(
                g.both(is_action[3], is_status[2]),
                g.either(g.both(is_action[4], is_status[3]),
                         g.both(is_action[5], g.either(is_status[0], is_status[1]))))))
    stale = g.neg(g.equal(callback, attempts))
    branch = g.choose(is_status[0], g.integer(9), g.integer(10))
    branch = g.choose(is_action[4], g.integer(8), branch)
    branch = g.choose(is_action[3], g.integer(7), branch)
    branch = g.choose(is_action[2], g.choose(stale, g.integer(2), g.integer(6)), branch)
    branch = g.choose(is_action[1], g.choose(stale, g.integer(2), g.integer(5)), branch)
    branch = g.choose(is_action[0],
                      g.choose(g.equal(attempts, limit), g.integer(3), g.integer(4)), branch)
    return g.choose(g.neg(g.equal(caller, expected_caller)), g.integer(0),
                    g.choose(g.neg(valid), g.integer(1), branch))


def problem():
    fields = [
        ("command.action", 0, 5), ("pre.status", 0, 5),
        ("pre.payment_attempts", 0, 3), ("command.callback_attempt", 0, 3),
        ("context.caller", 0, 2),
    ]
    inputs = [{"name": n, "type": {"kind": "int", "min": a, "max": b}}
              for n, a, b in fields]
    outputs = [{"name": "decision.branch", "type": {"kind": "int", "min": 0, "max": 10}}]
    contract = Graph(6)
    expected = decision(contract, False)
    contract_root = contract.equal(5, expected)
    sketch = Graph(5)
    selected = decision(sketch, True)
    return {"schema": "zeno-fcis/synthesis-problem/1",
            "profile": "zeno-fcis/finite-i64/1", "inputs": inputs, "outputs": outputs,
            "contract": {"nodes": contract.nodes, "roots": [contract_root]},
            "sketch": {"nodes": sketch.nodes, "roots": [selected]}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    path = Path(__file__).with_name("synthesis.json")
    content = json.dumps(problem(), indent=2) + "\n"
    if args.check:
        if path.read_text() != content:
            raise SystemExit("synthesis.json differs from the reviewed decision builder")
    else:
        path.write_text(content)


if __name__ == "__main__":
    main()
