#!/usr/bin/env python3
"""Replay finite-algebra source rules against naive word and Boolean references."""
from __future__ import annotations
import argparse
import itertools
import json
from pathlib import Path
import re
import subprocess
import tempfile
from check_recursive_tables import RuleParser


class AlgebraParser(RuleParser):
    def expression(self, minimum=0):
        op = self.peek()
        if op in ("join", "meet", "complement", "u128"):
            self.take()
            if self.peek() == "(":
                self.take()
                left = self.expression()
                if op in ("join", "meet"):
                    self.take(",")
                    right = self.expression()
                    result = (op, left, right)
                else:
                    result = (op, left)
                self.take(")")
                return result
            var = self.take()
            self.take("in")
            self.take("table")
            table = int(self.take())
            guard = ("literal", True)
            if self.peek() == "where":
                self.take()
                guard = self.expression()
            self.take("{")
            body = self.expression()
            self.take("}")
            return (op + "_fold", var, table, guard, body)
        return super().expression(minimum)


def rule(source):
    declaration = re.search(r"derived table \d+ (\w+)\[(\w+)\] in table \d+ bits (\d+) rounds (\d+) \{(.*?)\n\};", source, re.S)
    assert declaration
    name, index, width, rounds, text = declaration.groups()
    parser = AlgebraParser(text)
    rows = []
    while parser.peek() == "when":
        parser.take()
        guard = parser.expression()
        parser.take("then")
        body = parser.expression()
        parser.take(";")
        rows.append((guard, body))
    parser.take("default")
    default = parser.expression()
    parser.take(";")
    assert parser.peek() is None
    return name, index, int(width), int(rounds), rows, default


def reference(source, parents, grants, boolean=False):
    name, index, width, rounds, rows, default = rule(source)
    sizes = {int(t): int(n) for t, n in re.findall(r"table (\d+) \d+ \w+ \d+ size (\d+);", source)}
    full = (1 << width) - 1

    def evaluate(expr, env, previous, atom=None):
        op, *a = expr
        if op == "literal":
            return a[0]
        if op == "variable":
            return env[a[0]]
        if op == "stored":
            _, _, table, subscript = a
            k = evaluate(subscript[0], env, previous, atom)
            value = (parents if table == 110 else grants)[k]
            return bool(value & (1 << atom)) if atom is not None and table == 210 else value
        if op == "entry":
            assert a[0] == name
            return previous[evaluate(a[1][0], env, previous, atom)]
        if op == "==":
            return evaluate(a[0], env, previous, atom) == evaluate(a[1], env, previous, atom)
        if op == "u128":
            value = evaluate(a[0], env, previous)
            return bool(value & (1 << atom)) if atom is not None else value
        if op == "complement":
            value = evaluate(a[0], env, previous, atom)
            return not value if atom is not None else full ^ value
        if op in ("join", "meet"):
            x, y = (evaluate(t, env, previous, atom) for t in a)
            return x | y if op == "join" else x & y
        if op in ("join_fold", "meet_fold"):
            variable, table, guard, body = a
            value = (False if op == "join_fold" else True) if atom is not None else (0 if op == "join_fold" else full)
            for j in range(sizes[table]):
                inner = {**env, variable: j}
                if evaluate(guard, inner, previous, atom):
                    found = evaluate(body, inner, previous, atom)
                    value = value | found if op == "join_fold" else value & found
            return value
        raise AssertionError(expr)

    def iterate(atom=None):
        current = [False if atom is not None else 0] * len(parents)
        for step in itertools.count(1):
            following = []
            for i in range(len(parents)):
                env = {index: i}
                body = next((body for guard, body in rows if evaluate(guard, env, current, atom)), default)
                following.append(evaluate(body, env, current, atom))
            assert all((a | b) == b for a, b in zip(current, following))
            if following == current:
                assert step <= rounds + 1
                return following
            assert step <= rounds
            current = following
    if boolean:
        return [iterate(atom)[i] for i in range(len(parents)) for atom in range(width)]
    words = iterate()
    return [bool(word & (1 << atom)) for word in words for atom in range(width)]


def fixture(width):
    source = f"""zeno 1;
project 1 permission_words;
type 100 state State; type 101 command Query; type 102 context Context;
type 105 int Index in 0..=2; type 106 bits Permissions width {width}; type 107 int Zero in 0..=0;
table 110 100 parents 105 size 3; table 210 100 grants 106 size 3;
field 120 101 query 107;
reason 200 refused precedence 0;
derived table 700 permissions[i] in table 110 bits {width} rounds {3*width} {{
when true then join(pre.100.210[i], join j in table 110 where pre.100.110[i] == j {{ permissions[j] }});
default u128(0);
}};
law 499 domain on commit, genesis = post.100.110 >= 0;
"""
    source += "".join(f"law {500+i*width+k} entry_{i}_{k} on commit = subset({width}, u128({1<<k}), permissions[{i}]);\n" for i in range(3) for k in range(width))
    fields = list(range(110, 113)) + list(range(210, 213))
    rules = {"schema": "zeno-fcis/template-declarative-policy/4", "template": "permission-words-reference", "roots": {"state": 100, "command": 101, "context": 102}, "leaf_bindings": {"102": ["Bool"]}, "variables": {},
             "cases": [{"when": "false", "class": "Reject", "reason": 200, "post": {}, "outbox": []}, {"when": True, "class": "Accept", "reason": None, "post": {str(f): f"pre.100.{f}" for f in fields}, "outbox": []}],
             "genesis": {str(f): "u128(0)" if f >= 210 else 0 for f in fields}, "law_kinds": {str(500+i): "DebitCreditEffectEquality" for i in range(3*width)}}
    rules["law_kinds"]["499"] = "StateInvariant"
    return source, rules


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    report = []
    with tempfile.TemporaryDirectory(prefix="recursive-bits-") as temp:
        folder = Path(temp)
        for width in (2, 3):
            source, rules = fixture(width)
            project, policy = folder / "project.zeno", folder / "rules.json"
            project.write_text(source)
            policy.write_text(json.dumps(rules))
            states = [(parents, grants) for parents in itertools.product(range(3), repeat=3) for grants in itertools.product(range(1 << width), repeat=3)]
            payload = "".join(json.dumps([*parents, *grants, 0, False]) + "\n" for parents, grants in states)
            run = subprocess.run([str(args.driver), str(project), str(policy)], input=payload, capture_output=True, text=True)
            assert run.returncode == 0, (run.returncode, run.stderr)
            actual = [json.loads(line) for line in run.stdout.splitlines()]
            assert len(actual) == len(states)
            for (parents, grants), found in zip(states, actual):
                word = reference(source, parents, grants)
                boolean = reference(source, parents, grants, True)
                assert found == word == boolean, (width, parents, grants, found, word, boolean)
            report.append({"width": width, "parent_tables": 27, "grant_vectors": (1 << width)**3, "states": len(states), "atom_comparisons": len(states)*3*width, "native_exit": run.returncode})
    args.out.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
