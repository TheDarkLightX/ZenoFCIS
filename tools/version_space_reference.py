#!/usr/bin/env python3
"""LLM-generated independent scalar reference for finite version-space proposals.

Parsing uses the public ZAL expression grammar; evaluation, raw Cartesian
assignment enumeration, table equality, partitions and minimum conflicts are
implemented independently of Rust's vector search. This is test evidence only.
"""
from __future__ import annotations

import argparse
import itertools
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "integrations" / "zal"))
import loop_language as Z
from behavior import Refusal

I64 = (-(1 << 63), (1 << 63) - 1)  # Declared domains and report coordinates.
I128 = (-(1 << 127), (1 << 127) - 1)  # Eager checked evaluation.


def scalar(node, env, assignment):
    tag = node[0]
    if tag == "int":
        value = node[1]
    elif tag == "var":
        value = env[node[1]]
    elif tag == "hole":
        value = scalar(node[3][assignment[node[1]]], env, assignment) if node[2] == "choice" else assignment[node[1]]
    elif tag == "not":
        value = int(scalar(node[1], env, assignment) == 0)
    elif tag == "if":
        c, a, b = (scalar(n, env, assignment) for n in node[1:])
        value = a if c == 1 else b
    elif tag == "bin":
        a, b = scalar(node[2], env, assignment), scalar(node[3], env, assignment)
        op = node[1]
        if op == "+":
            value = a + b
        elif op == "-":
            value = a - b
        elif op == "==":
            value = int(a == b)
        elif op == "!=":
            value = int(a != b)
        elif op == "<":
            value = int(a < b)
        elif op == "<=":
            value = int(a <= b)
        elif op == ">":
            value = int(a > b)
        elif op == ">=":
            value = int(a >= b)
        elif op == "&&":
            value = int(a == 1 and b == 1)
        elif op == "||":
            value = a if a == 1 else b
        elif op == "->":
            value = b if a == 1 else 1
        else:
            raise Refusal("unsupported reference operator")
    else:
        raise Refusal("unsupported reference expression")
    if not I128[0] <= value <= I128[1]:
        raise OverflowError("eager i128 overflow")
    return value


def bounds(domain):
    return (0, 1) if domain["kind"] == "Bool" else (domain["min"], domain["max"])


def points(spec):
    return tuple(itertools.product(*(range(lo, hi + 1) for lo, hi in (bounds(f["domain"]) for f in spec["inputs"]))))


def _kind(node, want, vocab, kinds):
    if node[0] == "hole":
        kinds[node[1]] = want
    elif node[0] == "if":
        _kind(node[1], "Bool", vocab, kinds)
        for n in node[2:]:
            _kind(n, want, vocab, kinds)
    elif node[0] == "not":
        _kind(node[1], "Bool", vocab, kinds)
    elif node[0] == "bin":
        op = node[1]
        if op in ("+", "-"):
            side = "Int"
        elif op in ("&&", "||", "->"):
            side = "Bool"
        else:
            side = Z.kind_of(node[2], vocab) or Z.kind_of(node[3], vocab) or "Int"
        for n in node[2:]:
            _kind(n, side, vocab, kinds)


def universe(spec):
    nodes = Z.renumber([Z.parse_expression(t, allow_params=False) for t in spec["sketch"]])
    names = [f["name"] for f in spec["inputs"]]
    vocab = Z.Vocabulary(names, [f["domain"]["kind"] for f in spec["inputs"]], [d["kind"] for d in spec["outputs"]])
    kinds = {}
    for node, d in zip(nodes, spec["outputs"]):
        _kind(node, d["kind"], vocab, kinds)
    holes = [h for node in nodes for h in Z.holes_of(node)]
    spaces = []
    for h in holes:
        if h[2] == "choice":
            spaces.append(range(len(h[3])))
        elif h[2] == "const" and h[3] is not None:
            spaces.append(range(h[3], h[4] + 1))
        elif h[2] == "const" and kinds[h[1]] == "Bool":
            spaces.append(range(2))
        else:
            raise Refusal("reference requires finite hole spaces")
    size = 1
    for s in spaces:
        size *= len(s)
    if size > 200000 or len(points(spec)) > 4096:
        raise Refusal("reference bounded enumeration exceeded")
    tables = {}
    for values in itertools.product(*spaces):
        assignment = dict(zip((h[1] for h in holes), values))
        try:
            table = tuple(tuple(scalar(n, dict(zip(names, p)), assignment) for n in nodes) for p in points(spec))
        except OverflowError:
            continue
        if any(not lo <= v <= hi for row in table for v, (lo, hi) in zip(row, map(bounds, spec["outputs"]))):
            continue
        tables.setdefault(table, assignment)
    return tables


def admitted(spec, table, requirements=None):
    names = [f["name"] for f in spec["inputs"]]
    requirements = spec["requirements"] if requirements is None else requirements
    for req in requirements:
        if "law" in req:
            law = Z.parse_expression(req["law"], allow_holes=False, allow_params=False)
            try:
                if not all(scalar(law, {**dict(zip(names, p)), **{f"out{k}": v for k, v in enumerate(row)}}, {}) == 1
                           for p, row in zip(points(spec), table)):
                    return False
            except OverflowError:
                return False
        else:
            if table[points(spec).index(tuple(req["input"]))] != tuple(req["output"]):
                return False
    return True


def expected(spec):
    all_ = universe(spec)
    survivors = [t for t in all_ if admitted(spec, t)]
    if not survivors:
        ordered = sorted(spec["requirements"], key=lambda r: r["id"])
        for size in range(len(ordered) + 1):
            for subset in itertools.combinations(ordered, size):
                if not any(admitted(spec, t, subset) for t in all_):
                    return {"count": 0, "conflict": [r["id"] for r in subset]}
        raise AssertionError("empty space must have a relative conflict")
    if len(survivors) == 1:
        return {"count": 1, "table": survivors[0]}
    options = []
    for k, p in enumerate(points(spec)):
        buckets = {}
        for table in survivors:
            buckets[table[k]] = buckets.get(table[k], 0) + 1
        if len(buckets) > 1:
            key = max(buckets.values()), sum(n*n for n in buckets.values()), k
            options.append((key, p, buckets))
    key, point, buckets = min(options, key=lambda x: x[0])
    return {"count": len(survivors), "question": {"input": list(point), "ordinal": key[2],
            "answers": [{"output": list(v), "remaining": n} for v, n in sorted(buckets.items())],
            "worst_remaining": key[0], "sum_squared_buckets": key[1]}}


def program_table(program):
    domains = program["inputs"]
    result = []
    for p in itertools.product(*(range(int(lo), int(hi) + 1) for lo, hi in map(bounds, domains))):
        values = []
        for op in program["nodes"]:
            code, *args = op
            if code == "Input":
                value = p[args[0]]
            elif code in ("Int", "Bool"):
                value = int(args[0])
            elif code == "Select":
                c, a, b = (values[k] for k in args)
                value = a if c == 1 else b
            elif code == "Not":
                value = int(values[args[0]] == 0)
            else:
                a, b = (values[k] for k in args)
                value = {"Add": lambda: a+b, "Sub": lambda: a-b, "Eq": lambda: int(a == b),
                         "Lt": lambda: int(a < b), "And": lambda: int(a == 1 and b == 1)}[code]()
            if not I128[0] <= value <= I128[1]:
                raise OverflowError("program i128 overflow")
            values.append(value)
        row = tuple(values[k] for k in program["roots"])
        if any(not int(lo) <= v <= int(hi) for v, (lo, hi) in zip(row, map(bounds, program["outputs"]))):
            raise ValueError("program output-domain failure")
        result.append(row)
    return tuple(result)


def report_question(question):
    """Decode report coordinates within this reference's original i64 scope."""
    def scalar(value):
        if type(value) is int:
            number = value
        elif type(value) is str and len(value) <= 40:
            number = int(value)
            if str(number) != value:
                raise ValueError("noncanonical report integer")
        else:
            raise ValueError("report integer")
        if not I64[0] <= number <= I64[1]:
            raise ValueError("report outside reference i64 scope")
        return number
    return {**question, "input": [scalar(v) for v in question["input"]],
            "answers": [{**a, "output": [scalar(v) for v in a["output"]]} for a in question["answers"]]}


def compare(spec, report, stub=False):
    """Exact comparisons, separate from the oracle to support mutation controls."""
    wanted = expected(spec)
    detail = report.get("detail", report)
    comparisons = [detail.get("count") == wanted["count"]]
    if "question" in wanted:
        try:
            q = report_question(detail.get("question") or {})
        except (ValueError, KeyError, TypeError):
            return False
        comparisons.extend(q.get(k) == v for k, v in wanted["question"].items())
    elif "conflict" in wanted:
        comparisons.append((detail.get("conflict") or {}).get("requirements") == wanted["conflict"])
    else:
        candidate = detail.get("candidate") or {}
        try:
            comparisons.append(program_table(candidate["program"]) == wanted["table"])
        except (KeyError, ValueError, OverflowError):
            comparisons.append(False)
    return all(True if stub else v for v in comparisons)


def self_test(stub=False):
    spec = {"schema": "zeno-fcis/loop-version-space/1", "inputs": [{"name": "x", "domain": {"kind": "Bool"}}],
            "outputs": [{"kind": "Bool"}], "sketch": ["?c"], "requirements": []}
    correct = expected(spec)
    assert compare(spec, correct, stub), "reference must accept the positive control"
    wrong_count = {**correct, "count": 99}
    assert not compare(spec, wrong_count, stub), "stubbed counting must fail this planted count control"
    wrong_partition = json.loads(json.dumps(correct))
    wrong_partition["question"]["answers"][0]["remaining"] = 2
    assert not compare(spec, wrong_partition, stub), "stubbed partition must fail this planted bucket control"
    conflict = {**spec, "requirements": [{"id": "zero", "input": [0], "output": [0]}, {"id": "one", "input": [0], "output": [1]}]}
    assert compare(conflict, {"count": 0, "conflict": {"requirements": ["one", "zero"]}}, stub), "positive minimum conflict must pass"
    assert not compare(conflict, {"count": 0, "conflict": {"requirements": ["zero"]}}, stub), "conflict must be minimum and actually empty"
    single = {**spec, "sketch": ["false"], "requirements": [{"id": "zero", "law": "out0 == false"}]}
    wrong = {"count": 1, "candidate": {"program": {"inputs": [{"kind": "Bool"}], "outputs": [{"kind": "Bool"}], "nodes": [["Bool", True]], "roots": [0]}}}
    correct_single = json.loads(json.dumps(wrong))
    correct_single["candidate"]["program"]["nodes"] = [["Bool", False]]
    assert compare(single, correct_single, stub), "positive final table must pass"
    assert not compare(single, wrong, stub), "stubbed final comparison must fail this planted candidate control"


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--self-test", action="store_true")
    p.add_argument("--stub-comparison", action="store_true")
    p.add_argument("--specification", type=Path)
    p.add_argument("--report", type=Path)
    a = p.parse_args()
    if a.self_test:
        self_test(a.stub_comparison)
        print(json.dumps({"self_test": "passed", "authority": "none"}))
        return 0
    if not a.specification or not a.report:
        p.error("supply --self-test or --specification and --report")
    ok = compare(json.loads(a.specification.read_text()), json.loads(a.report.read_text()), a.stub_comparison)
    print(json.dumps({"reference_matches": ok, "authority": "none"}))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
