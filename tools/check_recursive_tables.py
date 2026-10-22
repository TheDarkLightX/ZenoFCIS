#!/usr/bin/env python3
"""Compare a source-rule fixed-point reference with the library law evaluator.

The reference parses the declared rule independently and uses fresh Boolean
arrays for synchronous rounds. It does not inspect the compiled circuit.
Build the recursive_table_vectors example and the CLI first, offline.
"""
from __future__ import annotations

import argparse
import itertools
import json
from pathlib import Path
import re
import subprocess
import tempfile


class RuleParser:
    """Small independent parser for the reference fixtures' declared rules."""

    def __init__(self, text):
        self.tokens = re.findall(r"\d+|[A-Za-z_][A-Za-z_0-9]*|&&|\|\||==|!=|<=|>=|[^\s]", text)
        self.pos = 0

    def take(self, token=None):
        value = self.tokens[self.pos]
        if token is not None:
            assert value == token, (value, token)
        self.pos += 1
        return value

    def peek(self):
        return self.tokens[self.pos] if self.pos < len(self.tokens) else None

    def expression(self, minimum=0):
        op = self.take()
        if op in ("!", "-"):
            left = ("not" if op == "!" else "neg", self.expression(8))
        elif op == "(":
            left = self.expression()
            self.take(")")
        elif op in ("forall", "exists"):
            var = self.take()
            self.take("in")
            self.take("table")
            table = int(self.take())
            condition = ("literal", True)
            if self.peek() == "where":
                self.take()
                condition = self.expression()
            self.take("{")
            body = self.expression()
            self.take("}")
            left = (op, var, table, condition, body)
        elif op in ("true", "false") or op.isdigit():
            left = ("literal", {"true": True, "false": False}.get(op, int(op) if op.isdigit() else None))
        elif op in ("pre", "post", "command", "context"):
            self.take(".")
            owner = int(self.take())
            self.take(".")
            table = int(self.take())
            indices = []
            while self.peek() == "[":
                self.take()
                indices.append(self.expression())
                self.take("]")
            left = ("stored", op, owner, table, indices)
        else:
            indices = []
            while self.peek() == "[":
                self.take()
                indices.append(self.expression())
                self.take("]")
            left = ("entry", op, indices) if indices else ("variable", op)
        precedence = {"||": 1, "&&": 2, "==": 3, "!=": 3, "<": 3, "<=": 3, ">": 3, ">=": 3, "+": 4, "-": 4, "*": 5}
        while self.peek() in precedence and precedence[self.peek()] >= minimum:
            operator = self.take()
            left = (operator, left, self.expression(precedence[operator] + 1))
        return left


def reference(source, fields):
    sizes = {int(t): int(n) for t, n in re.findall(r"(?m)^table (\d+) \d+ \w+ \d+ size (\d+);", source)}
    declaration = re.search(r"derived table \d+ (\w+)((?:\[\w+\])+) in table (\d+) rounds (\d+) \{(.*?)\n\};", source, re.S)
    assert declaration
    name, indices, table, bound, body = declaration.groups()
    variables = re.findall(r"\[(\w+)\]", indices)
    size = sizes[int(table)]
    entries = list(itertools.product(range(size), repeat=len(variables)))
    parser = RuleParser(body)
    rows = []
    while parser.peek() == "when":
        parser.take()
        guard = parser.expression()
        parser.take("then")
        value = parser.expression()
        parser.take(";")
        rows.append((guard, value))
    parser.take("default")
    default = parser.expression()
    parser.take(";")
    assert parser.peek() is None

    def evaluate(expr, env, previous):
        op, *args = expr
        if op == "literal":
            return args[0]
        if op == "variable":
            return env[args[0]]
        if op == "entry":
            assert args[0] == name
            return previous[tuple(evaluate(i, env, previous) for i in args[1])]
        if op == "stored":
            _, _, field, subscript = args
            assert len(subscript) <= 1
            index = evaluate(subscript[0], env, previous) if subscript else 0
            return fields[field + index]
        if op in ("forall", "exists"):
            var, table, condition, value = args
            values = []
            for i in range(sizes[table]):
                local = {**env, var: i}
                guard = evaluate(condition, local, previous)
                holds = evaluate(value, local, previous)
                values.append((not guard or holds) if op == "forall" else (guard and holds))
            return all(values) if op == "forall" else any(values)
        a = evaluate(args[0], env, previous)
        if op == "not":
            return not a
        if op == "neg":
            return -a
        b = evaluate(args[1], env, previous)
        return {"&&": lambda: a and b, "||": lambda: a or b, "==": lambda: a == b, "!=": lambda: a != b,
                "<": lambda: a < b, "<=": lambda: a <= b, ">": lambda: a > b, ">=": lambda: a >= b,
                "+": lambda: a + b, "-": lambda: a - b, "*": lambda: a * b}[op]()

    current = dict.fromkeys(entries, False)
    assert int(bound) == len(entries)
    for _ in entries:
        following = {}
        for entry in entries:
            env = dict(zip(variables, entry))
            value = next((value for guard, value in rows if evaluate(guard, env, current)), default)
            following[entry] = bool(evaluate(value, env, current))
        assert all(not current[e] or following[e] for e in entries)
        if following == current:
            return list(following.values())
        current = following
    return list(current.values())


def project(kind, n, optional=False):
    relation = kind in ("relation", "delegation")
    header = f"""zeno 1;
project 1 recursive_reference;
type 100 state State;
type 101 command Query;
type 102 context Context;
type 105 int Parent in 0..={0 if kind == 'relation' else n if optional else n-1};
type 106 int Flag in 0..=1;
table 110 100 parent 105 size {n};
variant 150 101 Query none;
variant 130 102 Context none;
reason 200 refused precedence 0;
"""
    if kind in ("relation", "roles"):
        header += f"table 210 100 lower 106 size {n*n if kind == 'relation' else n};\n"
    if relation:
        base = f"exists e in table 210 where e == i * {n} + j {{ pre.100.210[e] == 1 }}" if kind == "relation" else f"pre.100.110[i] == j{' + 1' if optional else ''}"
        rule = f"""derived table 700 reach[i][j] in table 110 rounds {n*n} {{
when true then ({base}) || exists k in table 110 {{ reach[i][k] && reach[k][j] }};
default false;
}};
"""
        queries = [f"reach[{i}][{j}]" for i in range(n) for j in range(n)]
    else:
        rule = f"""derived table 700 permission[i] in table 110 rounds {n} {{
when true then pre.100.210[i] == 1 || exists j in table 110 {{ pre.100.110[i] == j{' + 1' if optional else ''} && permission[j] }};
default false;
}};
"""
        queries = [f"permission[{i}]" for i in range(n)]
    laws = "law 499 domain on commit, genesis = post.100.110 >= 0;\n" + "".join(f"law {500+i} entry_{i} on commit = {q};\n" for i, q in enumerate(queries))
    fields = list(range(110, 110+n))
    if kind in ("relation", "roles"):
        fields += list(range(210, 210 + (n*n if kind == "relation" else n)))
    rules = {"schema": "zeno-fcis/template-declarative-policy/2", "template": "recursive-reference",
             "roots": {"state": 100, "command": 101, "context": 102}, "leaf_bindings": {}, "variables": {},
             "cases": [{"rule": "unselected rejection", "when": "false", "class": "Reject", "reason": 200, "post": {}, "outbox": []},
                       {"rule": "preserve state", "when": "true", "class": "Accept", "reason": None,
                        "post": {str(f): f"pre.100.{f}" for f in fields}, "outbox": []}],
             "genesis": {str(f): 0 for f in fields}, "law_kinds": {str(500+i): "DebitCreditEffectEquality" for i in range(len(queries))}}
    rules["law_kinds"]["499"] = "StateInvariant"
    return header + rule + laws, rules, fields


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    results = {"comparisons": [], "sizes": []}
    with tempfile.TemporaryDirectory(prefix="recursive-reference-") as directory:
        folder = Path(directory)
        for kind, n, optional in [("relation", 3, False), ("delegation", 4, False), ("delegation", 4, True), ("roles", 4, False), ("roles", 4, True)]:
            source, rules, ids = project(kind, n, optional)
            (folder / "project.zeno").write_text(source)
            (folder / "rules.json").write_text(json.dumps(rules))
            states = []
            if kind == "relation":
                states = [(0,) * n + edges for edges in itertools.product(range(2), repeat=n*n)]
            elif kind == "delegation":
                states = list(itertools.product(range(n + int(optional)), repeat=n))
            else:
                states = [parents + grants for parents in itertools.product(range(n + int(optional)), repeat=n)
                          for grants in itertools.product(range(2), repeat=n)]
            tuples = "".join(json.dumps([*state, 150, 130]) + "\n" for state in states)
            run = subprocess.run([str(args.driver), str(folder / "project.zeno"), str(folder / "rules.json")],
                                 input=tuples, text=True, capture_output=True)
            assert run.returncode == 0, (kind, run.returncode, run.stderr)
            actual = [json.loads(line) for line in run.stdout.splitlines()]
            assert len(actual) == len(states)
            for state, values in zip(states, actual):
                expected = reference(source, dict(zip(ids, state)))
                assert values == expected, (kind, state, expected, values)
            results["comparisons"].append({"kind": kind, "optional_none": optional, "states": len(states), "entries": len(actual[0]),
                                           "entry_checks": len(states) * len(actual[0]), "exit": run.returncode})
        for n in range(2, 7):
            source, _, _ = project("relation", n)
            # One law reads every result so it must share the rounds across queries too.
            source = source[:source.index("law 500")] + "law 500 closure = forall i in table 110 { forall j in table 110 { reach[i][j] || !reach[i][j] } };\n"
            (folder / "project.zeno").write_text(source)
            run = subprocess.run([str(args.cli), "check", str(folder / "project.zeno"), "--expand", "--format", "json"],
                                 capture_output=True, text=True, check=True)
            expanded = json.loads(run.stdout)
            text = expanded["expansion"]
            results["sizes"].append({"n": n, "bytes": len(text.encode()), "nodes": text.count("; "),
                                      "rounds": n*n, "canonical_match": expanded["canonical_match"], "exit": run.returncode})
    args.out.write_text(json.dumps(results, indent=2) + "\n")
    print(json.dumps(results, sort_keys=True))


if __name__ == "__main__":
    main()
