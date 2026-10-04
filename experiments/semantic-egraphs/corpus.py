#!/usr/bin/env python3
"""Frozen Boolean workload and independent finite-semantics/local-cost baseline."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import random

SEED = 20261003
FAMILIES = {
    "small_gate": 8,
    "distributed_factoring": 18,
    "absorption_complement": 18,
    "mixed_ite_demorgan": 18,
    "shared_guards": 18,
    "balanced_general": 20,
}
MAX_BASELINE_STEPS = 1024
ARITY = {"and": 2, "or": 2, "not": 1, "ite": 3}


def node(op, *children):
    return (op, *children)


def render(expr):
    return expr if isinstance(expr, str) else "(" + " ".join(map(render, expr)) + ")"


def parse(source):
    tokens = source.replace("(", " ( ").replace(")", " ) ").split()
    position = 0

    def term():
        nonlocal position
        if position >= len(tokens):
            raise ValueError("unfinished expression")
        token = tokens[position]
        position += 1
        if token == "(":
            if position >= len(tokens):
                raise ValueError("missing operator")
            op = tokens[position]
            position += 1
            if op not in ARITY:
                raise ValueError(f"unknown operator: {op}")
            children = [term() for _ in range(ARITY[op])]
            if position >= len(tokens) or tokens[position] != ")":
                raise ValueError("wrong arity or missing close")
            position += 1
            return node(op, *children)
        if token in ("true", "false") or (
            token.startswith("v") and token[1:].isdigit() and 0 <= int(token[1:]) <= 5
        ):
            return token
        raise ValueError(f"unknown leaf: {token}")

    expr = term()
    if position != len(tokens):
        raise ValueError("trailing expression")
    return expr


def evaluate(expr, mask, nvars):
    if isinstance(expr, str):
        if expr in ("true", "false"):
            return expr == "true"
        index = int(expr[1:])
        if index >= nvars:
            raise ValueError("variable outside declared input domain")
        return bool(mask & (1 << index))
    # Evaluate every operand: the admitted fragment is total Boolean FCIS.
    values = [evaluate(child, mask, nvars) for child in expr[1:]]
    if expr[0] == "and":
        return values[0] and values[1]
    if expr[0] == "or":
        return values[0] or values[1]
    if expr[0] == "not":
        return not values[0]
    if expr[0] == "ite":
        return values[1] if values[0] else values[2]
    raise ValueError("unknown operation")


def signatures(outputs, nvars):
    if not 0 <= nvars <= 6:
        raise ValueError("input bound")
    return [
        "".join("1" if evaluate(expr, mask, nvars) else "0" for mask in range(1 << nvars))
        for expr in outputs
    ]


def lower(outputs):
    """Exact ordered hash-consing in the FCIS Input/Bool/And/Not/Select basis."""
    instructions = []
    interned = {}

    def intern(key):
        if key not in interned:
            interned[key] = len(instructions)
            instructions.append(key)
        return interned[key]

    def visit(expr):
        if isinstance(expr, str):
            return intern(("bool", expr == "true")) if expr in ("true", "false") else intern(("input", int(expr[1:])))
        children = [visit(child) for child in expr[1:]]
        if expr[0] == "or":
            a = intern(("not", children[0]))
            b = intern(("not", children[1]))
            return intern(("not", intern(("and", a, b))))
        return intern(("select" if expr[0] == "ite" else expr[0], *children))

    roots = [visit(expr) for expr in outputs]
    return instructions, roots


def cost(outputs):
    return len(lower(outputs)[0])


def flatten(expr, op):
    if isinstance(expr, tuple) and expr[0] == op:
        return flatten(expr[1], op) + flatten(expr[2], op)
    return [expr]


def join(op, expressions):
    expressions = list(expressions)
    if not expressions:
        return "true" if op == "and" else "false"
    result = expressions[0]
    for expr in expressions[1:]:
        result = node(op, result, expr)
    return result


def negate(expr):
    return node("not", expr)


def local_candidates(expr):
    """Declared syntactic rules only; this function does not consult truth tables."""
    if isinstance(expr, str):
        return []
    op, *args = expr
    proposals = []
    if op == "not":
        a = args[0]
        if a == "true":
            proposals.append("false")
        if a == "false":
            proposals.append("true")
        if isinstance(a, tuple):
            if a[0] == "not":
                proposals.append(a[1])
            if a[0] in ("and", "or"):
                dual = "or" if a[0] == "and" else "and"
                proposals.append(node(dual, negate(a[1]), negate(a[2])))
            if a[0] == "ite":
                proposals.append(node("ite", a[1], negate(a[2]), negate(a[3])))
    elif op in ("and", "or"):
        a, b = args
        identity, absorbing = ("true", "false") if op == "and" else ("false", "true")
        if a == identity:
            proposals.append(b)
        if b == identity:
            proposals.append(a)
        if a == absorbing or b == absorbing:
            proposals.append(absorbing)
        if a == b:
            proposals.append(a)
        if a == negate(b) or b == negate(a):
            proposals.append(absorbing)
        terms = flatten(expr, op)
        if any(negate(term) in terms for term in terms):
            proposals.append(absorbing)
        # Canonical reassociation and duplicate removal are allowed only by
        # the same aggregate-cost gate as every other proposal.
        proposals.append(join(op, sorted(set(terms), key=render)))
        other = "or" if op == "and" else "and"
        for outer, inner in ((a, b), (b, a)):
            if isinstance(inner, tuple) and inner[0] == other and outer in flatten(inner, other):
                proposals.append(outer)
        if isinstance(a, tuple) and isinstance(b, tuple) and a[0] == b[0] == other:
            left, right = flatten(a, other), flatten(b, other)
            common = sorted(set(left) & set(right), key=render)
            if common:
                remainder_left = join(other, [term for term in left if term not in common])
                remainder_right = join(other, [term for term in right if term not in common])
                proposals.append(node(other, join(other, common), node(op, remainder_left, remainder_right)))
    elif op == "ite":
        c, a, b = args
        if c == "true":
            proposals.append(a)
        if c == "false":
            proposals.append(b)
        if a == b:
            proposals.append(a)
        if a == "true" and b == "false":
            proposals.append(c)
        if a == "false" and b == "true":
            proposals.append(negate(c))
        if isinstance(c, tuple) and c[0] == "not":
            proposals.append(node("ite", c[1], b, a))
        if a == "true":
            proposals.append(node("or", c, b))
        if b == "false":
            proposals.append(node("and", c, a))
        if a == "false":
            proposals.append(node("and", negate(c), b))
        if b == "true":
            proposals.append(node("or", negate(c), a))
        proposals.append(node("or", node("and", c, a), node("and", negate(c), b)))
    return sorted(set(proposals) - {expr}, key=render)


def all_nodes(outputs):
    result = set()

    def visit(expr):
        if expr not in result:
            result.add(expr)
            if isinstance(expr, tuple):
                for child in expr[1:]:
                    visit(child)

    for expr in outputs:
        visit(expr)
    return sorted(result, key=render)


def replace(expr, target, replacement):
    if expr == target:
        return replacement
    if isinstance(expr, str):
        return expr
    return node(expr[0], *(replace(child, target, replacement) for child in expr[1:]))


def baseline(outputs):
    """Best one-rewrite improvement at each step, over the whole shared DAG."""
    outputs = tuple(outputs)

    def key(current):
        spellings = tuple(map(render, current))
        return cost(current), sum(map(len, spellings)), spellings

    for steps in range(MAX_BASELINE_STEPS):
        current_key = key(outputs)
        best, best_key = outputs, current_key
        for target in all_nodes(outputs):
            for replacement in local_candidates(target):
                proposal = tuple(replace(expr, target, replacement) for expr in outputs)
                proposal_key = key(proposal)
                if proposal_key < best_key:
                    best, best_key = proposal, proposal_key
        if best == outputs:
            return list(outputs), steps
        outputs = best
    raise RuntimeError("baseline budget exhausted; no incomplete result is admitted")


def generate():
    rng = random.Random(SEED)
    cases = []

    def add(family, nvars, outputs):
        optimized, steps = baseline(outputs)
        expected = signatures(outputs, nvars)
        if signatures(optimized, nvars) != expected:
            raise AssertionError("local baseline changed truth vector")
        if cost(optimized) > cost(outputs) or cost(outputs) > 256:
            raise AssertionError("cost or FCIS node bound")
        index = sum(case["family"] == family for case in cases)
        cases.append({
            "id": f"{family}-{index:02d}", "nvars": nvars, "family": family,
            "outputs": list(map(render, outputs)),
            "baseline_outputs": list(map(render, optimized)),
            "truth_signatures": expected,
            "raw_fcis_nodes": cost(outputs), "baseline_fcis_nodes": cost(optimized),
            "baseline_steps": steps,
        })

    for outputs in [
        ["v0"], [node("and", "v0", "true")], [node("or", "v0", "false")],
        [negate(negate("v0"))],
        [node("or", node("and", "v0", "v1"), node("and", "v0", "v2"))],
        [node("ite", "v0", "v1", "v1")], [node("or", "v0", negate("v0"))],
        [node("and", "v0", "v1"), node("and", "v0", "v1"), negate("v0")],
    ]:
        add("small_gate", 3, outputs)

    def leaves(nvars):
        values = [f"v{i}" for i in range(nvars)]
        rng.shuffle(values)
        return values

    for i in range(18):
        nvars = 4 + i % 3
        p, q, r, s, *_ = leaves(nvars)
        p = p if i < 6 else negate(p) if i < 12 else node("and", p, s)
        variations = [
            node("or", node("and", p, q), node("and", p, r)),
            node("and", node("or", p, q), node("or", p, r)),
            node("or", node("and", q, p), node("and", r, p)),
            node("and", node("or", q, p), node("or", r, p)),
            node("or", node("and", p, q), node("and", p, node("or", r, s))),
            node("and", node("or", p, q), node("or", p, node("and", r, s))),
        ]
        add("distributed_factoring", nvars, [variations[i % 6]])

    for i in range(18):
        nvars = 3 + i % 4
        p, q, r, *_ = leaves(nvars)
        p = p if i < 6 else negate(p) if i < 12 else node("or", p, r)
        variations = [
            node("or", p, node("and", p, q)), node("and", p, node("or", p, q)),
            node("or", p, negate(p)), node("and", p, negate(p)),
            node("or", node("and", p, q), node("and", p, negate(q))),
            node("and", node("or", p, q), node("or", p, negate(q))),
        ]
        add("absorption_complement", nvars, [variations[i % 6]])

    for i in range(18):
        nvars = 3 + i % 4
        p, q, r, *_ = leaves(nvars)
        if i >= 6:
            q = node("or", q, negate(r))
        if i >= 12:
            p = node("and", p, r)
        variations = [
            node("ite", p, "true", "false"), node("ite", p, q, q),
            node("ite", negate(p), q, r), negate(node("and", p, q)),
            negate(node("or", p, q)), node("ite", p, node("and", q, r), node("or", q, r)),
        ]
        add("mixed_ite_demorgan", nvars, [variations[i % 6]])

    for i in range(18):
        nvars = 4 + i % 3
        p, q, r, s, *_ = leaves(nvars)
        p = node("or" if i % 2 else "and", p, s)
        guard = node("or", node("and", p, q), node("and", p, r))
        outputs = [guard, node("and", p, node("or", q, r)), node("ite", p, q, r)]
        if i % 3 == 0:
            outputs.append(guard)
        if i % 3 == 1:
            outputs.append(negate(node("or", p, q)))
        if i % 3 == 2:
            outputs.extend([node("and", p, q), node("and", p, r)])
        add("shared_guards", nvars, outputs)

    def balanced(nvars, depth):
        if depth == 0:
            return rng.choice([f"v{i}" for i in range(nvars)] + ["true", "false"])
        op = rng.choice(["and", "and", "or", "or", "not", "ite"])
        return node(op, *(balanced(nvars, depth - 1) for _ in range(ARITY[op])))

    for i in range(20):
        nvars = 4 + i % 3
        outputs = [balanced(nvars, 3 + i % 2)]
        if i % 4 == 0:
            outputs.append(balanced(nvars, 3))
        add("balanced_general", nvars, outputs)

    if {family: sum(c["family"] == family for c in cases) for family in FAMILIES} != FAMILIES:
        raise AssertionError("preregistered family count changed")
    return {
        "schema_version": 1, "seed": SEED,
        "semantic_scope": "Total Boolean expressions on declared Bool inputs; eager FCIS lowering, not arithmetic/effects/temporal equivalence.",
        "generation": {"families": FAMILIES, "ordering": "fixed family order, then generator index; no result-dependent filtering", "general_depths": [3, 4]},
        "baseline": {"policy": "global greedy lexicographic descent of (exact shared FCIS node count, spelling length, output spellings)", "max_steps": MAX_BASELINE_STEPS, "oracle_used_for_proposals": False},
        "success_rule": {"all_variants_valid": True, "semantic_aggregate_nodes_strictly_less_than_local": True, "minimum_semantic_strict_case_wins": 10, "minimum_semantic_winning_families": 2, "no_tool_failures": True},
        "cases": cases,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path(__file__).with_name("corpus.json"))
    args = parser.parse_args()
    corpus = generate()
    encoded = (json.dumps(corpus, indent=2, sort_keys=True) + "\n").encode()
    args.out.write_bytes(encoded)
    print(json.dumps({"cases": len(corpus["cases"]), "families": FAMILIES, "seed": SEED,
                      "raw_fcis_nodes": sum(c["raw_fcis_nodes"] for c in corpus["cases"]),
                      "baseline_fcis_nodes": sum(c["baseline_fcis_nodes"] for c in corpus["cases"]),
                      "baseline_max_steps": max(c["baseline_steps"] for c in corpus["cases"]),
                      "sha256": hashlib.sha256(encoded).hexdigest()}))


if __name__ == "__main__":
    main()
