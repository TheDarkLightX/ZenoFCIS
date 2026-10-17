#!/usr/bin/env python3
"""Independent, bounded syntax reference for vocabulary-learning proposals.

This test tool enumerates syntax trees before quotienting; it does not use the
Rust door or its semantic-pruning algorithm. Fixed typed grammar: integer
inputs/constants, +/-, < <= == !=, Boolean inputs/! && ||, eager integer
conditional, and fixed min/max/abs macros of cost one plus their arguments.
Outputs observe values, eager arithmetic failure, or output-domain failure.
Counts are per output; questions rank their product partition by largest bucket,
sum of squares, then unasked input ordinal. All examples are required. No
acceptance, owner, deployment or publication authority is supplied by this tool.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
from pathlib import Path

from program_search_reference import Program, run as run_program, _type_check

I64_MIN, I64_MAX = -(1 << 63), (1 << 63) - 1
SCHEMA = "zeno-fcis/loop-learn/1"
ARITHMETIC, OUTPUT_DOMAIN = (1, 0), (2, 0)
ARGUMENTS = {op: ("Int", "Int") for op in ("+", "-", "<", "<=", "==", "!=", "min", "max")}
ARGUMENTS.update({"!": ("Bool",), "&&": ("Bool", "Bool"), "||": ("Bool", "Bool"),
                  "?": ("Bool", "Int", "Int"), "abs": ("Int",)})
BOOL_OPS = {"<", "<=", "==", "!=", "!", "&&", "||"}


class ReferenceLimit(ValueError):
    """This brute-force test corpus is too large; never a semantic verdict."""


def bounds(domain):
    if domain["kind"] == "Bool":
        return (0, 1)
    return domain["min"], domain["max"]


def value(v):
    return (0, v)


def apply(op, observations):
    # All arguments have already run; a trap in an unselected branch persists.
    if any(o[0] != 0 for o in observations):
        return ARITHMETIC
    xs = [o[1] for o in observations]
    if op == "+":
        v = xs[0] + xs[1]
    elif op == "-":
        v = xs[0] - xs[1]
    elif op == "abs":
        negated = -xs[0]
        if not I64_MIN <= negated <= I64_MAX:
            return ARITHMETIC
        v = negated if xs[0] < 0 else xs[0]
    elif op == "min":
        v = min(xs)
    elif op == "max":
        v = max(xs)
    elif op == "<":
        v = int(xs[0] < xs[1])
    elif op == "<=":
        v = int(xs[0] <= xs[1])
    elif op == "==":
        v = int(xs[0] == xs[1])
    elif op == "!=":
        v = int(xs[0] != xs[1])
    elif op == "!":
        v = int(xs[0] == 0)
    elif op == "&&":
        v = int(xs[0] == 1 and xs[1] == 1)
    elif op == "||":
        v = int(xs[0] == 1 or xs[1] == 1)
    elif op == "?":
        v = xs[1] if xs[0] == 1 else xs[2]
    else:
        raise ValueError("unsupported reference operation")
    return value(v) if I64_MIN <= v <= I64_MAX else ARITHMETIC


def _splits(total, arity):
    if arity == 1:
        if total >= 1:
            yield (total,)
    else:
        for n in range(1, total):
            for rest in _splits(total - n, arity - 1):
                yield (n, *rest)


def enumerate_syntax(spec, syntax_limit=100_000):
    """All typed syntax, with bounded reference-only cost; no semantic pruning."""
    inputs = [i["domain"] for i in spec["inputs"]]
    points = tuple(itertools.product(*(range(lo, hi + 1) for lo, hi in map(bounds, inputs))))
    if len(points) > 4096:
        raise ReferenceLimit("reference-domain")
    vocab = spec.get("vocabulary", {})
    operations = (vocab.get("arithmetic", ["+", "-"]) + vocab.get("macros", [])
                  + vocab.get("compare", []) + vocab.get("boolean", [])
                  + (["?"] if vocab.get("conditional", False) else []))
    if any(op not in ARGUMENTS for op in operations) or spec.get("misses", 0) != 0:
        raise ValueError("fixed checked vocabulary required")
    terms = {(k, 1): [] for k in ("Int", "Bool")}
    for j, inp in enumerate(spec["inputs"]):
        terms[inp["domain"]["kind"], 1].append((tuple(value(p[j]) for p in points), 1))
    for c in vocab.get("constants", [1, 2, 3]):
        terms["Int", 1].append((tuple(value(c) for _ in points), 1))
    syntax = sum(len(t) for t in terms.values())
    for size in range(2, spec.get("max_size", 7) + 1):
        for k in ("Int", "Bool"):
            terms[k, size] = []
        for op in operations:
            kind = "Bool" if op in BOOL_OPS else "Int"
            for sizes in _splits(size - 1, len(ARGUMENTS[op])):
                choices = [terms[k, s] for k, s in zip(ARGUMENTS[op], sizes)]
                for children in itertools.product(*choices):
                    syntax += 1
                    if syntax > syntax_limit:
                        raise ReferenceLimit("reference-syntax")
                    table = tuple(apply(op, [child[0][j] for child in children]) for j in range(len(points)))
                    terms[kind, size].append((table, size))
    smallest = {"Int": {}, "Bool": {}}
    by_size = []
    for size in range(1, spec.get("max_size", 7) + 1):
        old = [len(smallest[k]) for k in ("Int", "Bool")]
        for kind in ("Int", "Bool"):
            for table, cost in terms[kind, size]:
                smallest[kind].setdefault(table, cost)
        by_size.append([len(smallest[k]) - n for k, n in zip(("Int", "Bool"), old)])
    return points, smallest, by_size


def observation(o):
    return {"value": o[1]} if o[0] == 0 else {"failure": "arithmetic" if o[0] == 1 else "output-domain"}


def analyze(spec, syntax_limit=100_000):
    points, smallest, by_size = enumerate_syntax(spec, syntax_limit)
    index = {p: j for j, p in enumerate(points)}
    examples = spec.get("examples", [])
    answered = {index[tuple(e["input"])] for e in examples}
    tables, fits, minimum_sizes = [], [], []
    for output, d in enumerate(spec["outputs"]):
        lo, hi = bounds(d)
        root = {}
        for table, cost in smallest[d["kind"]].items():
            projected = tuple(OUTPUT_DOMAIN if o[0] == 0 and not lo <= o[1] <= hi else o for o in table)
            root[projected] = min(cost, root.get(projected, cost))
        tables.append(root)
        fitting = [table for table in root if all(table[index[tuple(e["input"])]] == value(e["output"][output])
                                                  for e in examples)]
        fits.append(fitting)
        minimum_sizes.append(min((root[t] for t in fitting), default=None))
    counts, before = [len(f) for f in fits], [len(t) for t in tables]
    result = {"counts": counts, "before": before, "by_size": by_size, "question": None,
              "core": None, "minimum_sizes": minimum_sizes}
    if 0 in counts:
        order = sorted(range(len(examples)), key=lambda k: index[tuple(examples[k]["input"])])
        for size in range(len(order) + 1):
            for subset in itertools.combinations(order, size):
                if any(not any(all(t[index[tuple(examples[k]["input"])]] == value(examples[k]["output"][o])
                                       for k in subset) for t in table) for o, table in enumerate(tables)):
                    return {**result, "status": "outside-vocabulary", "core": list(subset)}
        raise AssertionError("missing reference conflict")
    if all(n == 1 for n in counts):
        return {**result, "status": "determined"}
    ranked = []
    for j, point in enumerate(points):
        if j in answered:
            continue
        bucket_sets, worst, squares, split = [], 1, 1, False
        for fit in fits:
            buckets = {}
            for t in fit:
                buckets[t[j]] = buckets.get(t[j], 0) + 1
            worst *= max(buckets.values())
            squares *= sum(n * n for n in buckets.values())
            split |= len(buckets) > 1
            bucket_sets.append([{"observation": observation(o), "remaining": n} for o, n in sorted(buckets.items())])
        if split:
            ranked.append(((worst, squares, j), {"input": list(point), "ordinal": j, "worst": str(worst),
                                                "squares": str(squares), "buckets": bucket_sets}))
    if not ranked:
        return {**result, "status": "ambiguous"}
    return {**result, "status": "question", "question": min(ranked, key=lambda r: r[0])[1]}


def program_fixture(data):
    """Bridge fixture JSON into the already independent canonical-program reference."""
    opcodes = {"Input": 0, "Int": 1, "Bool": 2, "Add": 3, "Sub": 4, "Eq": 5, "Lt": 6,
               "And": 7, "Not": 8, "Select": 9}

    def domain(d):
        if d["kind"] == "Bool":
            return (True, 0, 1)
        return (False, int(d["min"]), int(d["max"]))

    program = Program(tuple(domain(d) for d in data["inputs"]), tuple(domain(d) for d in data["outputs"]),
                      tuple((opcodes[n[0]], *[int(a) for a in n[1:]]) for n in data["nodes"]), tuple(data["roots"]))
    _type_check(program)
    return program


def program_digest(program):
    # ZCVE/1 canonical tuple/profile representation, independent of Rust encoding.
    def encode(v):
        if type(v) is bool:
            return bytes([2 if v else 1])
        if type(v) is bytes:
            return b"\x06" + len(v).to_bytes(4, "big") + v
        if type(v) is tuple:
            return b"\x08" + len(v).to_bytes(4, "big") + b"".join(encode(x) for x in v)
        tag, n = v
        return bytes([tag]) + n.to_bytes(16, "big", signed=tag == 4)
    def integer(n):
        return [4, n]
    def natural(n):
        return [3, n]
    domains = tuple(tuple((boolean, integer(lo), integer(hi))) for boolean, lo, hi in program.inputs), \
              tuple(tuple((boolean, integer(lo), integer(hi))) for boolean, lo, hi in program.outputs)
    nodes = tuple(tuple(integer(a) for a in n) for n in program.nodes)
    body = (b"zeno-fcis/finite-i64/1", domains, nodes, tuple(natural(n) for n in program.roots))
    return hashlib.sha256(encode(body)).hexdigest()


def agree(left, right):
    """Load-bearing comparison; negative tests replace this with always True."""
    if type(left) is not type(right):
        return False
    if type(left) in (tuple, list):
        return len(left) == len(right) and all(agree(a, b) for a, b in zip(left, right))
    if type(left) is dict:
        return left.keys() == right.keys() and all(agree(left[k], right[k]) for k in left)
    return left == right


def output_program(program, output):
    """Observe one root's dependency graph, as this scalar-output grammar defines it."""
    root, reachable = program.roots[output], set()
    stack = [root]
    while stack:
        k = stack.pop()
        if k in reachable:
            continue
        reachable.add(k)
        opcode, *args = program.nodes[k]
        if opcode not in (0, 1, 2):
            stack.extend(args)
    order = sorted(reachable)
    mapping = {k: j for j, k in enumerate(order)}
    nodes = []
    for k in order:
        opcode, *args = program.nodes[k]
        nodes.append((opcode, *args) if opcode in (0, 1, 2) else (opcode, *[mapping[a] for a in args]))
    return Program(program.inputs, (program.outputs[output],), tuple(nodes), (mapping[root],))


def check_report(spec, report):
    """Recompute complete claims, minimum costs, and lowered example reproduction."""
    expected = analyze(spec)
    detail = report.get("detail", {})
    checks = [agree(report.get("authority"), "none"), agree(detail.get("enumeration"), "proposal"),
              agree(detail.get("complete"), True), agree(detail.get("attempts_spent"), 0),
              agree(report.get("status"), expected["status"])]
    for key in ("counts", "before", "by_size", "question", "core"):
        checks.append(agree(detail.get(key), expected[key]))
    candidate = detail.get("candidate")
    if expected["status"] == "outside-vocabulary":
        checks.append(agree(candidate, None))
    else:
        representatives = detail.get("representatives", [])
        checks.append(agree([r.get("size") for r in representatives], expected["minimum_sizes"]))
        if not isinstance(candidate, dict) or not isinstance(candidate.get("program"), dict):
            return False
        program = program_fixture(candidate["program"])
        def declared_domain(d):
            lo, hi = bounds(d)
            return (d["kind"] == "Bool", lo, hi)
        checks.append(agree(program.inputs, tuple(declared_domain(i["domain"]) for i in spec["inputs"])))
        checks.append(agree(program.outputs, tuple(declared_domain(d) for d in spec["outputs"])))
        checks.append(agree(candidate.get("sha256"), program_digest(program)))
        examples = spec.get("examples", [])
        checks.append(agree(candidate.get("examples_check"), {"decided_by": "library-evaluator", "checked": len(examples),
                                                              "vacuous": not examples, "verdict": "reproduced"}))
        for example in examples:
            checks.append(agree(run_program(program, tuple(example["input"])), ("ok", tuple(example["output"]))))
        # Independently check that each final root has a smallest fitting meaning
        # from the declared language, even when no examples were provided.
        points, raw, _ = enumerate_syntax(spec)
        for j, d in enumerate(spec["outputs"]):
            if j >= len(program.roots):
                return False
            lo, hi = bounds(d)
            costs = {}
            for table, cost in raw[d["kind"]].items():
                projected = tuple(OUTPUT_DOMAIN if o[0] == 0 and not lo <= o[1] <= hi else o for o in table)
                costs[projected] = min(cost, costs.get(projected, cost))
            one = output_program(program, j)
            got = []
            for point in points:
                status, outputs = run_program(one, point)
                got.append(value(outputs[0]) if status == "ok" else ARITHMETIC if outputs == "arithmetic" else OUTPUT_DOMAIN)
            checks.append(agree(costs.get(tuple(got)), expected["minimum_sizes"][j]))
    return all(checks)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("spec", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    spec = json.loads(args.spec.read_text())
    if args.report:
        good = check_report(spec, json.loads(args.report.read_text()))
        print(json.dumps({"comparison": "passed" if good else "failed", "authority": "none"}))
        return 0 if good else 1
    print(json.dumps(analyze(spec), indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
