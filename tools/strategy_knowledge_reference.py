#!/usr/bin/env python3
"""Independent bounded Datalog reference; test evidence, never product authority."""
from __future__ import annotations

import argparse
from collections import defaultdict, deque
from dataclasses import dataclass
from itertools import product
import json
import hashlib
from pathlib import Path
import re
import loop_strategy_reference as TYPES

EXAMPLE = Path(__file__).resolve().parents[1] / "crates/zeno-fcis-loop-doors/examples/strategy-example.kb"
TOKEN = re.compile(r'\s*(?:([%][^\n]*)|("(?:[^"\\]|\\.)*")|(-?\d+)|([A-Z_][A-Za-z0-9_]*)|([a-z][a-z0-9_-]*)|(:-|!=|<=|>=|[(),.=<>]))', re.ASCII)


@dataclass(frozen=True)
class Var:
    name: str


def key(value):
    return (0, value) if type(value) is str else (1, value)


def compare(actual, expected):
    """Load-bearing comparison seam for reference controls."""
    return actual == expected


def parse(source):
    if len(source.encode()) > 65536:
        raise ValueError("text-budget")
    tokens, pos = [], 0
    while pos < len(source):
        match = TOKEN.match(source, pos)
        if match is None:
            if not source[pos:].strip():
                break
            raise ValueError("syntax")
        pos = match.end()
        comment, string, number, variable, name, punct = match.groups()
        if comment:
            continue
        if string is not None:
            value = json.loads(string)
            try:
                value.encode("utf-8")
            except UnicodeError as error:
                raise ValueError("string") from error
            token = ("term", value)
        elif number is not None:
            value = int(number)
            if not -(1 << 63) <= value < (1 << 63):
                raise ValueError("integer")
            token = ("term", value)
        elif variable is not None:
            token = ("term", Var(variable))
        elif name is not None:
            token = ("name", name)
        else:
            token = ("p", punct)
        term_name = token[1].name if isinstance(token[1], Var) else token[1]
        if token[0] in ("name", "term") and type(term_name) is str and len(term_name.encode()) > 256:
            raise ValueError("term-budget")
        tokens.append(token)
        if len(tokens) > 8192:
            raise ValueError("token-budget")
    return tokens


class Parser:
    def __init__(self, source):
        self.tokens, self.index = parse(source), 0

    def peek(self, ahead=0):
        return self.tokens[self.index + ahead] if self.index + ahead < len(self.tokens) else ("eof", None)

    def take(self, kind=None, value=None):
        token = self.peek()
        if kind is not None and token[0] != kind or value is not None and token[1] != value:
            raise ValueError("syntax")
        self.index += 1
        return token[1]

    def punct(self, value):
        if self.peek() == ("p", value):
            self.index += 1
            return True
        return False

    def term(self):
        if self.peek()[0] not in ("term", "name"):
            raise ValueError("syntax")
        return self.take()

    def atom(self):
        name = self.take("name")
        args = []
        if self.punct("(") and not self.punct(")"):
            args.append(self.term())
            while self.punct(","):
                args.append(self.term())
            self.take("p", ")")
        if len(args) > 8:
            raise ValueError("arity-budget")
        return name, tuple(args)

    def literal(self):
        if self.peek() == ("name", "not"):
            self.take()
            return "not", self.atom()
        if self.peek(1) in tuple(("p",op) for op in ("=","!=","<","<=",">",">=")):
            left, op, right = self.term(), self.take("p"), self.term()
            return op, (left, right)
        return "yes", self.atom()

    def program(self):
        facts, rules = [], []
        while self.peek()[0] != "eof":
            head = self.atom()
            if self.punct(":-"):
                body = [self.literal()]
                while self.punct(","):
                    body.append(self.literal())
                if len(body) > 16 or len(rules) >= 128:
                    raise ValueError("rule-budget")
                rules.append((head, body))
            else:
                if any(isinstance(t, Var) for t in head[1]):
                    raise ValueError("fact-variable")
                if len(facts) >= 4096:
                    raise ValueError("fact-budget")
                facts.append(head)
            self.take("p", ".")
        return facts, rules


def variables(terms):
    return {t.name for t in terms if isinstance(t, Var) and t.name != "_"}


class KB:
    """Reference uses relation snapshots and finite assignment enumeration."""
    def __init__(self, source, work=2_000_000, rows=8192):
        self.used, self.limit, self.row_limit = 0, work, rows
        facts, rules = Parser(source).program()
        self.rules=rules
        self.proofs={f:{"fact":f,"rule":None,"premises":(),"absent":(),"comparisons":()} for f in facts}
        self.arities = {}
        self.db = defaultdict(set)
        for name, args in facts:
            self.arity(name, len(args))
            self.db[name].add(args)
        for (name, head), body in rules:
            self.arity(name, len(head))
            if Var("_") in head:
                raise ValueError("head-wildcard")
            supplied = set()
            required = variables(head)
            for kind, args in body:
                if kind in ("yes", "not"):
                    self.arity(args[0], len(args[1]))
                    if kind == "yes":
                        supplied |= variables(args[1])
                    else:
                        required |= variables(args[1])
                else:
                    if Var("_") in args:
                        raise ValueError("comparison-wildcard")
                    required |= variables(args)
            if not required <= supplied:
                raise ValueError("unsafe")
            if len(supplied) > 32:
                raise ValueError("variable-budget")
        levels = {name: 0 for (name, _), _body in rules}
        for _ in range(len(levels) + 1):
            changes = []
            for (head, _), body in rules:
                for kind, atom in body:
                    self.charge()
                    if kind in ("yes", "not") and atom[0] in levels:
                        need = levels[atom[0]] + int(kind == "not")
                        if need > levels[head]:
                            changes.append((head, need))
            if not changes:
                break
            for head, need in changes:
                levels[head] = max(levels[head], need)
        else:
            raise ValueError("negative-cycle")
        self.bound_rows()
        for stratum in sorted(set(levels.values())):
            while True:
                frozen = {k: frozenset(v) for k, v in self.db.items()}
                additions = defaultdict(set)
                new_proofs={}
                for index,((pred, head), body) in enumerate(rules):
                    if levels[pred] != stratum:
                        continue
                    positive = [a for kind, a in body if kind == "yes"]
                    domains = [sorted(frozen.get(a[0], ()),key=lambda r:tuple(key(v) for v in r)) for a in positive]
                    for tuple_rows in product(*domains):
                        self.charge()
                        binding = {}
                        if not all(self.match(a[1], row, binding) for a, row in zip(positive, tuple_rows)):
                            continue
                        valid = True
                        for kind, args in body:
                            if kind == "not":
                                valid = not any(self.match(args[1], row, dict(binding)) for row in frozen.get(args[0], ()))
                            elif kind in ("=", "!=", "<", "<=", ">", ">="):
                                x,y=self.value(args[0],binding),self.value(args[1],binding)
                                if kind in ("<","<=",">",">=") and (type(x) is not int or type(y) is not int):raise ValueError("comparison-type")
                                valid={"=":lambda:x==y,"!=":lambda:x!=y,"<":lambda:x<y,"<=":lambda:x<=y,">":lambda:x>y,">=":lambda:x>=y}[kind]()
                            if not valid:
                                break
                        if valid:
                            result=tuple(self.value(t,binding) for t in head)
                            additions[pred].add(result)
                            conclusion=(pred,result)
                            if result not in self.db[pred] and conclusion not in new_proofs:
                                new_proofs[conclusion]={"fact":conclusion,"rule":index,"premises":tuple((a[0],row) for a,row in zip(positive,tuple_rows)),"absent":tuple((a[0],tuple(Var("_") if t==Var("_") else self.value(t,binding) for t in a[1])) for kind,a in body if kind=="not"),"comparisons":tuple((kind,tuple(self.value(t,binding) for t in terms)) for kind,terms in body if kind in ("=","!=","<","<=",">",">="))}
                changed = any(rows - self.db[pred] for pred, rows in additions.items())
                for pred, new in additions.items():
                    self.db[pred] |= new
                self.proofs.update(new_proofs)
                self.bound_rows()
                if not changed:
                    break

    def charge(self):
        self.used += 1
        if self.used > self.limit:
            raise ValueError("work-budget")

    def bound_rows(self):
        if sum(map(len, self.db.values())) > self.row_limit:
            raise ValueError("row-budget")

    def arity(self, name, size):
        if name in self.arities and self.arities[name] != size:
            raise ValueError("arity")
        self.arities[name] = size
        if len(self.arities) > 128:
            raise ValueError("predicate-budget")

    @staticmethod
    def value(term, binding):
        return binding[term.name] if isinstance(term, Var) else term

    def match(self, terms, row, binding):
        self.charge()
        if len(terms) != len(row):
            return False
        for term, val in zip(terms, row):
            if isinstance(term, Var):
                if term.name == "_":
                    continue
                if term.name in binding and binding[term.name] != val:
                    return False
                binding[term.name] = val
            elif term != val:
                return False
        return True

    def query(self, query):
        p = Parser(query)
        pred, args = p.atom()
        p.punct(".")
        if p.peek()[0] != "eof":
            raise ValueError("query-trailing")
        self.arity(pred, len(args))
        answers = set()
        for row in self.db.get(pred, ()):
            binding = {}
            if self.match(args, row, binding):
                answers.add(tuple(sorted(binding.items())))
        return [dict(b) for b in sorted(answers, key=lambda b: tuple((k, key(v)) for k, v in b))]


def artifact(name):
    return "owner-gate" if name == "OwnerGate" else name.lower()


SIGNATURES = {name: (set(map(artifact, needs)), set(map(artifact, makes)))
              for name, (needs, makes) in TYPES.SIGNATURES.items()}
STRATEGIES = {
    "ask-when-it-matters": ({"proposal", "model", "owner-gate"}, {"answer"},
                           {"formalize-alternatives", "distinct-readings", "solve-every-reading", "confirm-reading", "decide"}),
    "diagnose": ({"model"}, {"answer"}, {"observe", "explain", "choose-probe", "measure", "isolate"}),
    "split-by-cases": ({"original", "vocabulary"}, {"qualified", "agreement"},
                       {"sample", "learn-terms", "cover", "learn-split", "assemble", "committee-question", "spot-check", "capture", "check-program"}),
}
ARTIFACTS = set().union(*(needs | makes for needs, makes in SIGNATURES.values()))


def expected():
    db = defaultdict(set)
    db["artifact"] = {(a,) for a in ARTIFACTS}
    for name, (needs, makes) in SIGNATURES.items():
        db["loop"].add((name,))
        db["consumes"] |= {(name, a) for a in needs}
        db["produces"] |= {(name, a) for a in makes}
    for name, (needs, goals, used) in STRATEGIES.items():
        db["strategy"].add((name,))
        db["needs"] |= {(name, a) for a in needs}
        db["goal"] |= {(name, a) for a in goals}
        db["uses"] |= {(name, s) for s in used}
    return db


def check_code(kb):
    problems = []
    for name, target in expected().items():
        actual = kb.db.get(name, set())
        if name == "artifact":
            if any(len(row) != 2 or any(type(v) is not str for v in row) for row in actual):
                problems.append("kb-artifact-schema")
                continue
            actual = {(row[0],) for row in actual}
        if not compare(actual, target):
            problems.append("kb-" + name + "-code-mismatch")
    return sorted(problems)


def plan(kb, have, want, max_steps=8, max_states=4096):
    if check_code(kb):
        raise ValueError("code-inconsistent")
    have, want = frozenset(have), set(want)
    if not want or not set(have) | want <= ARTIFACTS:
        raise ValueError("types")
    eligible = sorted(name for name, (needs, goals, _uses) in STRATEGIES.items()
                      if needs <= have and want <= goals)
    queue, seen = deque([(have, [])]), {have}
    if max_states < 1:
        raise ValueError("state-budget")
    while queue:
        state, path = queue.popleft()
        if want <= state:
            return {"steps": path, "eligible_strategies": eligible,
                    "selected_strategy": eligible[0] if eligible else None, "states": len(seen)}
        if len(path) == max_steps:
            continue
        for name, (needs, makes) in sorted(SIGNATURES.items()):
            if needs <= state and not makes <= state:
                after = state | makes
                if after not in seen:
                    if len(seen) >= max_states:
                        raise ValueError("state-budget")
                    seen.add(after)
                    queue.append((after, path + [name]))
    raise ValueError("bounded-no-plan")


def verify(source, operation, report, query=None, have=(), want=()):
    from usage_knowledge_reference import Usage,validation_inputs
    public=EXAMPLE.read_text()
    usage=Usage([public] if source==public else [public,source],validations=validation_inputs(public))
    kb = usage.kb
    detail = report["detail"]
    assert compare(detail["derived_rows"], sum(map(len, kb.db.values()))), "derived-row-comparison"
    assert compare(report["authority"], "none"), "authority-comparison"
    assert compare(detail["attempts_spent"], 0), "attempt-comparison"
    assert compare(detail["binding"]["knowledge_sha256"], hashlib.sha256(source.encode()).hexdigest()), "source-comparison"
    if operation == "query":
        assert compare(report["status"], "proposed-query"), "query-status"
        assert compare(detail["rows"], kb.query(query)), "query-comparison"
    elif operation == "check":
        expected_problems = sorted(check_code(kb)+usage.check())
        assert compare(sorted(detail["problems"]), expected_problems), "code-comparison"
        assert compare(report["status"], "inconsistent" if expected_problems else "consistent"), "code-status"
    else:
        assert compare(report["status"], "proposed-plan"), "plan-status"
        p = plan(kb, have, want)
        for key_ in p:
            assert compare(detail[key_], p[key_]), "plan-comparison"
        assert compare(detail["type_check"], "passed")
        assert compare(TYPES.check_plan(detail["composition"]), [])
        if detail["strategy_plan"] is not None:
            assert compare(TYPES.check_plan(detail["strategy_plan"]), [])


def self_test():
    kb = KB("edge(a,b). edge(b,c). path(X,Y):-edge(X,Y). path(X,Z):-edge(X,Y),path(Y,Z).")
    assert compare(kb.query("path(a,Y)"), [{"Y": "b"}, {"Y": "c"}])
    assert not compare(kb.query("path(a,Y)"), [{"Y": "b"}]), "query-stub-not-detected"
    public = EXAMPLE.read_text()
    assert not check_code(KB(public))
    bad = public.replace("needs(diagnose,model).", "needs(diagnose,proposal).")
    assert "kb-needs-code-mismatch" in check_code(KB(bad)), "code-stub-not-detected"
    p = plan(KB(public), {"model"}, {"answer"})
    assert compare(p["steps"], ["observe", "explain", "isolate"])
    assert not compare(p["selected_strategy"], "split-by-cases"), "plan-stub-not-detected"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, default=EXAMPLE)
    parser.add_argument("--operation", choices=("query", "check", "plan"))
    parser.add_argument("--query")
    parser.add_argument("--have", default="")
    parser.add_argument("--want", default="")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--stub", action="store_true")
    args = parser.parse_args()
    global compare
    if args.stub:
        compare = lambda *_: True
    if args.self_test:
        self_test()
        return 0
    source = args.source.read_bytes().decode("utf-8")
    have, want = args.have.split(",") if args.have else [], args.want.split(",") if args.want else []
    if args.report:
        verify(source, args.operation, json.loads(args.report.read_text()), args.query, have, want)
        return 0
    kb = KB(source)
    result = (kb.query(args.query) if args.operation == "query"
              else check_code(kb) if args.operation == "check" else plan(kb, have, want))
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
