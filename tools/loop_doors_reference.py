#!/usr/bin/env python3
"""Independent reference for the bounded loop's door reports (stdlib only).

Written from the public program format (the fixture vocabulary: domains, nodes ``["Op", args...]``,
roots) and the documented finite semantics: every node is evaluated (eagerly, unused nodes and
unselected ``Select`` arms included), ``Add`` and ``Sub`` trap outside signed 64-bit, ``Select`` tests
``== 1``, Booleans are 0 and 1, an output outside its declared domain refuses the tuple, and tuples are
enumerated with the last input fastest. Its own small parser reads the loop expression language.

It checks, without trusting the door:

* ``region``: a reported region formula holds on exactly the tuples where the candidate's observation
  differs from the original's, and the reported count is that many;
* ``unrealizable``: a witness claim (at the witness tuple no completion makes the output right: with one
  hole, over a candidate set this reference proves complete; with several, the output there is decided
  by one hole alone, every other hole left unknown, and is wrong for every value of that hole's complete
  set) or an exhausted-space claim (every hole is enumerated over a complete set this reference derives
  itself: a choice's alternatives, an explicit range, a compared hole-free expression's range widened by
  one, else every integer within a bound; every assignment fails somewhere; the door's stated values lie
  in those sets; up to ``MAX_ASSIGNMENTS`` assignments).

* ``interface`` (the decompose door, stage 2): a reported interface counterexample is two inputs that agree
  on everything the component observes but need different behaviour from it, and its suggested reads tell
  every such pair apart;
* ``teach-back``: a rule's applies, holds-on, exception and missing counts and its verdict, recomputed on the
  contract's decisions; and, when the fixture gives the owner's answers, ``required``: on the inputs where the
  rule holds, how many the answers force it on (every admitted decision is the rule's), leave open (some
  are) and forbid it on (none is, or none is admitted);
* ``rule-meaning``: two teach-back rules have equal meaning keys exactly when they apply on the same inputs,
  claim the same decision and, when they give one, the same successor on every input (teach-back compares
  the successor wherever the core's decision has the rule's label, inside the rule's region and outside it),
  so that equal keys give equal verdicts;
* ``question``: the question door's counts of determined, inconsistent and open inputs and its first region,
  recomputed from what the answers mean (each answer's ``where``, class, reason and successor, read by this
  reference's own evaluator, never from the door's law text); the door's pinned laws must admit exactly
  those decisions on every input; and its region formula must hold on exactly that region.

The contract's decisions and law verdicts for the last three come from ``contract_judges_reference.py`` (the
contract judges' independent reference, imported, not edited); everything else is computed here.

``--self-test`` shows that each check is load-bearing: with its comparison stubbed out it accepts the
planted-wrong fixtures it otherwise rejects.

This tool is LLM-generated.
"""
from __future__ import annotations

import argparse
import itertools
import json
from pathlib import Path
import re
import sys

I64 = (-(1 << 63), (1 << 63) - 1)
UNKNOWN = "unknown"            # the value of an expression that reads an unfilled hole
MAX_ASSIGNMENTS = 2_000_000    # most assignments the reference enumerates for one claim
TRAP = "trap"
OUT_OF_DOMAIN = "output-domain"


class Invalid(ValueError):
    pass


# --- programs ----------------------------------------------------------------------------------------

def domain(d: dict) -> tuple:
    if d["kind"] == "Bool":
        return (0, 1, True)
    if d["kind"] == "Int":
        return (int(d["min"]), int(d["max"]), False)
    raise Invalid(f"unknown domain {d}")


def tuples(program: dict):
    spans = [domain(d) for d in program["inputs"]]
    return itertools.product(*[range(lo, hi + 1) for lo, hi, _ in spans])


def observe(program: dict, point: tuple):
    """The outputs at one tuple, or TRAP, or OUT_OF_DOMAIN."""
    values = []
    trapped = False
    for node in program["nodes"]:
        op, args = node[0], node[1:]
        if op == "Input":
            v = point[args[0]]
        elif op == "Int":
            v = int(args[0])
        elif op == "Bool":
            v = 1 if args[0] is True else 0
        elif op in ("Add", "Sub"):
            a, b = values[args[0]], values[args[1]]
            v = None if a is None or b is None else (a + b if op == "Add" else a - b)
            if v is not None and not I64[0] <= v <= I64[1]:
                v = None
            if v is None:
                trapped = True
        elif op == "Eq":
            a, b = values[args[0]], values[args[1]]
            v = None if a is None or b is None else int(a == b)
        elif op == "Lt":
            a, b = values[args[0]], values[args[1]]
            v = None if a is None or b is None else int(a < b)
        elif op == "And":
            a, b = values[args[0]], values[args[1]]
            v = None if a is None or b is None else int(a == 1 and b == 1)
        elif op == "Not":
            a = values[args[0]]
            v = None if a is None else int(a == 0)
        elif op == "Select":
            c, a, b = (values[i] for i in args)
            v = None if None in (c, a, b) else (a if c == 1 else b)
        else:
            raise Invalid(f"unknown instruction {op}")
        values.append(v)
    if trapped:
        return TRAP
    out = []
    for root, d in zip(program["roots"], program["outputs"]):
        lo, hi, _ = domain(d)
        if not lo <= values[root] <= hi:
            return OUT_OF_DOMAIN
        out.append(values[root])
    return tuple(out)


def table(program: dict) -> list:
    return [observe(program, p) for p in tuples(program)]


# --- expressions -------------------------------------------------------------------------------------

TOKENS = re.compile(r"\s*(\d+|\$[A-Za-z_]\w*|\?c\[|\?c(?!\w)|\?e(?!\w)|\?\{|[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)?|->|&&|\|\||==|!=|<=|>=|\.\.|[-()!<>+*?:\]|}])")
LEVELS = [("->",), ("||",), ("&&",), ("==", "!="), ("<", "<=", ">", ">="), ("+", "-")]


def parse(text: str):
    """The loop expression language as nested tuples: ("n", v), ("v", name), ("hole", k, kind, extra),
    ("!", a), (op, a, b), ("?", c, a, b)."""
    tokens, at = [], 0
    text = text.strip()
    while at < len(text):
        m = TOKENS.match(text, at)
        if not m:
            raise Invalid(f"cannot read {text[at:at + 10]!r}")
        tokens.append(m.group(1))
        at = m.end()
    pos = [0]
    holes = [0]

    def peek():
        return tokens[pos[0]] if pos[0] < len(tokens) else None

    def take(expected=None):
        tok = peek()
        if expected is not None and tok != expected:
            raise Invalid(f"expected {expected}, found {tok}")
        pos[0] += 1
        return tok

    def atom():
        tok = take()
        if tok is None:
            raise Invalid("expression ends early")
        if tok.isdigit():
            return ("n", int(tok))
        if tok == "-":
            if peek() and peek().isdigit():
                return ("n", -int(take()))
            return ("-", ("n", 0), atom())
        if tok == "!":
            return ("!", atom())
        if tok == "(":
            inner = conditional()
            take(")")
            return inner
        if tok in ("true", "false"):
            return ("n", int(tok == "true"))
        if tok in ("?c", "?e", "?c[", "?{"):
            k = holes[0]
            holes[0] += 1
            if tok == "?c":
                return ("hole", k, "const", None)
            if tok == "?e":
                return ("hole", k, "expr", None)
            if tok == "?c[":
                lo = int(take()) if peek() != "-" else -int((take(), take())[1])
                take("..")
                hi = int(take()) if peek() != "-" else -int((take(), take())[1])
                take("]")
                return ("hole", k, "const", (lo, hi))
            options = [conditional()]
            while peek() == "|":
                take()
                options.append(conditional())
            take("}")
            return ("hole", k, "choice", tuple(options))
        if tok.startswith("$"):
            raise Invalid("parameters must be bound")
        if re.fullmatch(r"[A-Za-z_]\w*", tok):
            return ("v", tok)
        raise Invalid(f"unexpected {tok}")

    def level(i):
        if i == len(LEVELS):
            return atom()
        left = level(i + 1)
        while peek() in LEVELS[i]:
            op = take()
            right = level(i) if op == "->" else level(i + 1)
            left = (op, left, right)
            if op == "->":
                break
        return left

    def conditional():
        test = level(0)
        if peek() == "?":
            take()
            yes = conditional()
            take(":")
            return ("?", test, yes, conditional())
        return test

    node = conditional()
    if pos[0] != len(tokens):
        raise Invalid(f"trailing text after {tokens[pos[0] - 1]}")
    return node


def value(node, env: dict, variants: dict, fill: dict):
    tag = node[0]
    if tag == "n":
        return node[1]
    if tag == "v":
        if node[1] in env:
            return env[node[1]]
        if node[1] in variants:
            return variants[node[1]]
        raise Invalid(f"unknown name {node[1]}")
    if tag == "hole":
        if node[1] not in fill:
            return UNKNOWN
        if node[2] == "choice":
            return value(node[3][fill[node[1]]], env, variants, fill)
        return fill[node[1]]
    if tag == "!":
        a = value(node[1], env, variants, fill)
        return UNKNOWN if a == UNKNOWN else int(a == 0)
    if tag == "?":
        test = value(node[1], env, variants, fill)
        if test == UNKNOWN:
            return UNKNOWN
        return value(node[2] if test == 1 else node[3], env, variants, fill)
    a = value(node[1], env, variants, fill)
    b = value(node[2], env, variants, fill)
    if UNKNOWN in (a, b):
        return UNKNOWN
    return {"+": lambda: a + b, "-": lambda: a - b, "==": lambda: int(a == b), "!=": lambda: int(a != b),
            "<": lambda: int(a < b), "<=": lambda: int(a <= b), ">": lambda: int(a > b), ">=": lambda: int(a >= b),
            "&&": lambda: int(a == 1 and b == 1), "||": lambda: a if a == 1 else b,
            "->": lambda: b if a == 1 else 1}[node[0]]()


def holes(node, out=None):
    out = [] if out is None else out
    if node[0] == "hole":
        out.append(node)
    elif node[0] in ("!",):
        holes(node[1], out)
    elif node[0] == "?":
        for child in node[1:]:
            holes(child, out)
    elif node[0] not in ("n", "v"):
        holes(node[1], out)
        holes(node[2], out)
    return out


def literals_and_reads(node) -> tuple[int, int]:
    if node[0] == "n":
        return abs(node[1]), 0
    if node[0] == "v":
        return 0, 1
    if node[0] == "hole":
        if node[2] == "choice":
            parts = [literals_and_reads(a) for a in node[3]]
            return sum(p[0] for p in parts), sum(p[1] for p in parts)
        return 0, 0
    parts = [literals_and_reads(c) for c in node[1:] if isinstance(c, tuple)]
    return sum(p[0] for p in parts), sum(p[1] for p in parts)


# --- checks ------------------------------------------------------------------------------------------

def same(a, b) -> bool:
    """Whether a region formula's truth matches failure at a tuple (stubbed in the self-test)."""
    return a == b


def meets(a, b) -> bool:
    """Whether a completed output's value meets the target at a tuple (stubbed in the self-test)."""
    return a == b


def environment(names: list, point: tuple) -> dict:
    return dict(zip(names, point))


def check_region(original: dict, candidate: dict, vocabulary: dict, region: dict) -> list:
    """Problems with a reported region (an empty list means it holds)."""
    names = vocabulary["inputs"]
    variants = {n: v for table in vocabulary.get("variants", {}).values() for n, v in table.items()}
    failing = [a != b for a, b in zip(table(original), table(candidate))]
    problems = []
    if region["tuples"] != sum(failing):
        problems.append(f"count {region['tuples']} but {sum(failing)} tuples fail")
    if region.get("where"):
        formula = parse(region["where"])
        for point, fails in zip(tuples(original), failing):
            if not same(value(formula, environment(names, point), variants, {}) == 1, fails):
                problems.append(f"the formula is {'false' if fails else 'true'} at {point}")
                break
    return problems


def hole_kinds(node, want: str, input_kinds: dict, out: dict) -> dict:
    """The kind each hole takes from its position: the output's kind at the root, Boolean under ``!``,
    connectives and conditions, Int under arithmetic, and a comparison's other operand's kind."""
    tag = node[0]
    if tag == "hole":
        out[node[1]] = want
    elif tag == "!":
        hole_kinds(node[1], "Bool", input_kinds, out)
    elif tag == "?":
        hole_kinds(node[1], "Bool", input_kinds, out)
        hole_kinds(node[2], want, input_kinds, out)
        hole_kinds(node[3], want, input_kinds, out)
    elif tag in ("&&", "||", "->"):
        hole_kinds(node[1], "Bool", input_kinds, out)
        hole_kinds(node[2], "Bool", input_kinds, out)
    elif tag in ("+", "-"):
        hole_kinds(node[1], "Int", input_kinds, out)
        hole_kinds(node[2], "Int", input_kinds, out)
    elif tag in ("==", "!=", "<", "<=", ">", ">="):
        side = next((k for k in (_kind(node[1], input_kinds), _kind(node[2], input_kinds)) if k), "Int")
        hole_kinds(node[1], side, input_kinds, out)
        hole_kinds(node[2], side, input_kinds, out)
    return out


def _kind(node, input_kinds: dict):
    if node[0] == "v":
        return input_kinds.get(node[1], "Int")
    if node[0] in ("!", "&&", "||", "->", "==", "!=", "<", "<=", ">", ">="):
        return "Bool"
    if node[0] in ("+", "-"):
        return "Int"
    if node[0] == "?":
        return _kind(node[2], input_kinds) or _kind(node[3], input_kinds)
    return None


def complete_values(node, hole, names, original, variants, kind: str = "Int") -> list:
    """A candidate set for one remaining hole that this reference proves complete: choice alternatives,
    an explicit range, 0/1 for a Boolean hole, else every integer within the bound past which comparisons
    are constant and the output cannot meet any target."""
    if hole[2] == "choice":
        return list(range(len(hole[3])))
    if hole[3] is not None:
        return list(range(hole[3][0], hole[3][1] + 1))
    if kind == "Bool":
        return [0, 1]
    literals, reads = literals_and_reads(node)
    largest_input = max(max(abs(lo), abs(hi)) for lo, hi, _ in map(domain, original["inputs"]))
    largest_output = max((abs(v) for row in table(original) if isinstance(row, tuple) for v in row), default=0)
    bound = literals + reads * largest_input + largest_output + 2
    return list(range(-bound, bound + 1))


def check_unrealizable(original: dict, vocabulary: dict, sketch: list, door: dict) -> list:
    names = vocabulary["inputs"]
    variants = {n: v for t in vocabulary.get("variants", {}).values() for n, v in t.items()}
    nodes, first = [], 0
    for text in sketch:
        node = parse(text)
        nodes.append(_renumber(node, first))
        first += len(holes(node))
    target = table(original)
    evidence = door["evidence"]
    j = int(evidence["output"][3:])
    node = nodes[j]
    input_kinds = {n: ("Bool" if d["kind"] == "Bool" else "Int") for n, d in zip(names, original["inputs"])}
    kinds = hole_kinds(node, original["outputs"][j]["kind"], input_kinds, {})
    if any(isinstance(row, str) for row in target):
        return ["the original fails somewhere; no claim can be checked"]
    if evidence["kind"] == "witness":
        open_holes = holes(node)
        ordinal = evidence["ordinal"]
        point = list(tuples(original))[ordinal]
        env = environment(names, point)
        want = target[ordinal][j]
        if len(open_holes) == 1:
            hole = open_holes[0]
            for fill in complete_values(node, hole, names, original, variants, kinds.get(hole[1], "Int")):
                if meets(value(node, env, variants, {hole[1]: fill}), want):
                    return [f"hole value {fill} makes out{j} right at the witness tuple {ordinal}"]
            return []
        # several holes: some hole must decide the output there by itself (every other hole unknown) and be
        # wrong for each value of its complete set; or the output is known there with every hole unknown
        if value(node, env, variants, {}) not in (UNKNOWN,) and not meets(value(node, env, variants, {}), want):
            return []
        for hole in open_holes:
            decided = True
            for fill in complete_values(node, hole, names, original, variants, kinds.get(hole[1], "Int")):
                got = value(node, env, variants, {hole[1]: fill})
                if got == UNKNOWN or meets(got, want):
                    decided = False
                    break
            if decided:
                return []
        return [f"no hole alone makes out{j} wrong at the witness tuple {ordinal} whatever the others hold"]
    # an exhausted complete space: every assignment of the stated spaces, then every other hole over a
    # complete set
    search = next(o for o in door["searched"] if o["output"] == f"out{j}")
    stated = {s["hole"]: s for s in search["searched"]}
    if not all(s["complete"] for s in stated.values()):
        return ["the stated space is not complete"]
    compared = _compared_ranges(node, names, original, variants)
    order = holes(node)
    spaces = []
    for h in order:
        if h[2] == "choice":
            spaces.append(list(range(len(h[3]))))
        elif h[3] is not None:
            spaces.append(list(range(h[3][0], h[3][1] + 1)))
        elif h[1] in compared:
            spaces.append(list(range(compared[h[1]][0], compared[h[1]][1] + 1)))
        else:
            spaces.append(complete_values(node, h, names, original, variants, kinds.get(h[1], "Int")))
    # the door's stated values must lie in the space this reference derives for that hole
    problems = []
    for h, space in zip(order, spaces):
        s = stated.get(h[1])
        if s and s.get("min") is not None and h[2] != "choice" and \
                (s["min"] < min(space) or s["max"] > max(space)):
            problems.append(f"hole {h[1]}: stated values {s['min']}..{s['max']} leave its complete space")
    count = 1
    for space in spaces:
        count *= max(len(space), 1)
    if count > MAX_ASSIGNMENTS:
        return problems + [f"the complete space has {count} assignments, over the reference's budget"]
    points = list(tuples(original))
    for values in itertools.product(*spaces):
        fill = {h[1]: v for h, v in zip(order, values)}
        if all(meets(value(node, environment(names, pt), variants, fill), row[j]) for pt, row in zip(points, target)):
            return problems + [f"assignment {fill} completes out{j}"]
    return problems


def _compared_ranges(node, names, original, variants) -> dict:
    """Constant holes compared with a hole-free expression: that expression's range widened by one, a
    complete candidate set (every other constant compares like one of the ends)."""
    out = {}

    def visit(n):
        if n[0] in ("==", "!=", "<", "<=", ">", ">="):
            for mine, other in ((n[1], n[2]), (n[2], n[1])):
                if mine[0] == "hole" and mine[2] == "const" and mine[3] is None and not holes(other):
                    seen = [value(other, environment(names, p), variants, {}) for p in tuples(original)]
                    out[mine[1]] = (min(seen) - 1, max(seen) + 1)
        for child in n[1:]:
            if isinstance(child, tuple) and child and isinstance(child[0], str):
                visit(child)
    visit(node)
    return out


def _renumber(node, first: int):
    if node[0] == "hole":
        return ("hole", node[1] + first) + node[2:]
    if node[0] in ("n", "v"):
        return node
    return (node[0],) + tuple(_renumber(c, first) if isinstance(c, tuple) else c for c in node[1:])


# --- fixtures and the self-test ------------------------------------------------------------------------

# --- stage 2: contracts ------------------------------------------------------------------------------------

def agree(a, b) -> bool:
    """Whether a door's reported value equals the reference's (stubbed in the self-test)."""
    return a == b


class Table:
    """A contract's names, variants and decisions on every tuple, from the contract judges' reference."""

    def __init__(self, project: str, policy: dict):
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        try:
            import contract_judges_reference as cj
        finally:
            sys.path.pop(0)
        self.cj = cj
        self.contract = cj.Contract(project, policy)
        self.machine = cj.Machine(self.contract)
        self.project, self.policy = project, policy
        field_type, type_variants, self.reason_ids = {}, {}, {}
        path_name = {}
        for raw in project.splitlines():
            words = raw.split("//", 1)[0].strip().rstrip(";").split()
            if words and words[0] == "field":
                fid, root, name, ftype = int(words[1]), int(words[2]), words[3], int(words[4])
                prefix = {100: "pre", 101: "command", 102: "context"}[root]
                path_name[f"{prefix}.{root}.{fid}"] = name
                field_type[name] = ftype
            elif words and words[0] == "variant":
                type_variants.setdefault(int(words[2]), {})[words[3]] = int(words[1])
            elif words and words[0] == "reason":
                self.reason_ids[words[2]] = int(words[1])
        self.path_name = path_name
        self.names = [path_name.get(position[1], position[0]) for position in self.contract.positions]
        self.width = self.contract.width
        self.variants = {}
        for name in self.names:
            self.variants.update(type_variants.get(field_type.get(name), {}))
        self.tuples = [(s, x) for s in self.contract.states for x in self.contract.inputs]

    def env(self, ordinal: int) -> dict:
        state, x = self.tuples[ordinal]
        return dict(zip(self.names, tuple(state) + tuple(x)))

    def value(self, text: str, ordinal: int) -> int:
        return value(parse(text), self.env(ordinal), self.variants, {})

    def case_successor(self, case: int, ordinal: int) -> dict:
        """The policy's case ``case`` successor at ``ordinal``, by state field name (its text read over the
        policy's variables)."""
        env = self.env(ordinal)
        aliases = {alias: env[self.path_name[path]] for alias, path in self.policy.get("variables", {}).items()
                   if path in self.path_name}
        out = {}
        for field, text in self.policy["cases"][case].get("post", {}).items():
            name = next(n for path, n in self.path_name.items() if path.startswith("pre.")
                        and path.rsplit(".", 1)[1] == str(field))
            out[name] = value(parse(text), {**env, **aliases}, self.variants, {})
        return out

    def decision(self, ordinal: int):
        return self.machine.table[self.tuples[ordinal]]

    def label(self, decision):
        cls, reason, _ = decision
        return (cls, None if cls == "Accept" else reason)


def _table(fixture: dict) -> Table:
    return Table(fixture["project"], fixture["policy"])


def check_teach_back(fixture: dict) -> list:
    table = _table(fixture)
    rule, judged = fixture["rule"], fixture["judged"]
    reason = None if rule["class"] == "Accept" else table.reason_ids[rule["reason"]]
    counts = {"applies": 0, "holds_on": 0, "exceptions": 0, "missing": 0}
    for ordinal in range(len(table.tuples)):
        decision = table.decision(ordinal)
        if decision is None:
            return ["the contract refuses somewhere; teach-back is not checked"]
        cond = table.value(rule["where"], ordinal) == 1
        same = table.label(decision) == (rule["class"], reason)
        if same and rule.get("sets") is not None:
            env = table.env(ordinal)
            for slot, name in enumerate(table.names[:table.width]):
                claimed = value(parse(rule["sets"][name]), env, table.variants, {}) if name in rule["sets"] \
                    else env[name]
                same = same and claimed == decision[2][slot]
        counts["applies"] += cond
        counts["holds_on"] += cond and same
        counts["exceptions"] += cond and not same
        counts["missing"] += (not cond) and same
    verdict = ("vacuous" if counts["applies"] == 0 else "wrong" if counts["exceptions"] else
               "holds" if counts["missing"] else "exact")
    problems = [f"{key}: door {judged.get(key)}, reference {mine}" for key, mine in counts.items()
                if not agree(judged.get(key), mine)]
    if not agree(judged.get("verdict"), verdict):
        problems.append(f"verdict: door {judged.get('verdict')}, reference {verdict}")
    if "answers" in fixture and verdict in ("exact", "holds"):
        mine = _rule_required(table, rule, reason, fixture["answers"])
        door = judged.get("required") or {}
        problems += [f"required.{key}: door {door.get(key)}, reference {value}" for key, value in mine.items()
                     if not agree(door.get(key), value)]
    return problems


def _rule_required(table: Table, rule: dict, reason, answers: list) -> dict:
    """Where the rule holds on the core: whether the answers (with the declared laws) force, leave open or
    forbid the rule's decision on each input."""
    out = {"forced": 0, "open": 0, "forbidden": 0}
    for ordinal, (state, x) in enumerate(table.tuples):
        decision = table.decision(ordinal)
        env = table.env(ordinal)
        claimed = None
        if rule.get("sets") is not None:
            claimed = tuple(value(parse(rule["sets"][n]), env, table.variants, {}) if n in rule["sets"] else env[n]
                            for n in table.names[:table.width])

        def claims(d) -> bool:
            return table.label(d) == (rule["class"], reason) and (claimed is None or tuple(d[2]) == claimed)

        if table.value(rule["where"], ordinal) != 1 or not claims(decision):
            continue
        admitted = [d for d in table.cj.candidates(table.contract, state) if table.contract.admits(state, x, d)
                    and all(_answer_admits(table, a, ordinal, d) for a in answers)]
        good = sum(1 for d in admitted if claims(d))
        out["forbidden" if good == 0 else "forced" if good == len(admitted) else "open"] += 1
    return out


def _rule_meaning(table: Table, rule: dict) -> tuple:
    """Everything a teach-back verdict reads: where the rule applies, the decision it claims and, when it gives
    one, its successor on every input (the verdict compares it wherever the core's label is the rule's, so
    also where the rule does not apply)."""
    reason = None if rule["class"] == "Accept" else table.reason_ids[rule["reason"]]
    cells = []
    for ordinal in range(len(table.tuples)):
        env = table.env(ordinal)
        cell = [table.value(rule["where"], ordinal) == 1]
        if rule.get("sets") is not None:
            cell.append(tuple(value(parse(rule["sets"][n]), env, table.variants, {}) if n in rule["sets"] else env[n]
                              for n in table.names[:table.width]))
        cells.append(tuple(cell))
    return (rule["class"], reason, rule.get("sets") is not None, tuple(cells))


def check_rule_meaning(fixture: dict) -> list:
    table = _table(fixture)
    a, b = fixture["rules"]
    same_meaning = _rule_meaning(table, a) == _rule_meaning(table, b)
    same_key = fixture["keys"][0] == fixture["keys"][1]
    if not agree(same_key, same_meaning):
        return [f"keys {'equal' if same_key else 'differ'} but the rules {'mean the same' if same_meaning else 'differ'}"]
    return []


def _needs(table: Table, components: list):
    """Per tuple: the index of the component that owns its decision (len for an accept, None unowned)."""
    owner_of = {}
    for k, component in enumerate(components):
        for name in component.get("decides", []):
            owner_of[table.reason_ids[name]] = k
    owners = []
    for ordinal in range(len(table.tuples)):
        decision = table.decision(ordinal)
        if decision[0] == "Accept":
            owners.append(len(components))
        else:
            owners.append(owner_of.get(decision[1]))
    return owners


def _required(table: Table, components: list, owners: list, k: int, ordinal: int):
    """What component k must do on a tuple: (decision label or None, {owned field: successor} or None)."""
    owner = owners[ordinal]
    if owner is None:
        return None, None
    decision = table.decision(ordinal)
    verdict = None if owner < k else (table.label(decision) if owner == k else ("Accept", None))
    commits = decision[0] == "Accept" or (decision[0] == "CommittedFailure" and owner >= k)
    posts = None
    if commits:
        posts = {name: decision[2][table.names.index(name)] for name in components[k].get("owns", [])}
    return verdict, posts


def check_interface(fixture: dict) -> list:
    table = _table(fixture)
    components, clash = fixture["architecture"]["components"], fixture["clash"]
    k = next(i for i, c in enumerate(components) if c["name"] == clash["component"])
    observed = set(components[k].get("owns", [])) | set(components[k].get("reads", []))
    owners = _needs(table, components)
    a, b = clash["pair"]
    ea, eb = table.env(a), table.env(b)
    problems = []
    if not agree(all(ea[n] == eb[n] for n in observed), True):
        problems.append("the pair differs on a field the component observes")
    ra, rb = _required(table, components, owners, k, a), _required(table, components, owners, k, b)
    if clash["what"] == "decision":
        differs = ra[0] is not None and rb[0] is not None and ra[0] != rb[0]
    else:
        field = clash["what"].removeprefix("successor of ")
        differs = ra[1] is not None and rb[1] is not None and ra[1].get(field) != rb[1].get(field)
    if not agree(differs, True):
        problems.append(f"the pair does not need different {clash['what']}")
    if clash.get("suggested_reads") is not None:
        fields = observed | set(clash["suggested_reads"])
        seen = {}
        for ordinal in range(len(table.tuples)):
            key = tuple(table.env(ordinal)[n] for n in sorted(fields))
            required = _required(table, components, owners, k, ordinal)
            for part, wanted in (("decision", required[0]), ("posts", required[1])):
                if wanted is None:
                    continue
                earlier = seen.setdefault((part, key), wanted)
                if part == "posts":
                    common = sorted(set(earlier) & set(wanted))
                    same_posts = agree([earlier[f] for f in common], [wanted[f] for f in common])
                else:
                    same_posts = agree(earlier, wanted)
                if not same_posts:
                    problems.append(f"the suggested reads still leave two inputs apart: {key}")
            if problems:
                break
    return problems


def _answer_admits(table: Table, answer: dict, ordinal: int, decision) -> bool:
    """Whether an answer admits ``decision`` at ``ordinal``: anything outside its ``where``; inside, a reject
    with its reason, or an accept with its successor (its ``sets``, the core case's when it names one, else
    any)."""
    if table.value(answer["where"], ordinal) != 1:
        return True
    cls, reason, successor = decision
    if answer["class"] == "Reject":
        return cls == "Reject" and reason == table.reason_ids[answer["reason"]]
    if answer["class"] != "Accept" or cls != "Accept":
        return False
    if answer.get("sets") is not None:
        env = table.env(ordinal)
        wanted = {n: value(parse(answer["sets"][n]), env, table.variants, {}) for n in answer["sets"]}
    elif answer.get("case") is not None:
        wanted = table.case_successor(answer["case"], ordinal)
    else:
        return True
    env = table.env(ordinal)
    return all(successor[slot] == wanted.get(name, env[name]) for slot, name in enumerate(table.names[:table.width]))


def check_question(fixture: dict) -> list:
    """Counts and the first region of the question door, recomputed from what the answers mean; the door's
    pinned laws must admit exactly that."""
    project = fixture["project"]
    next_id = 9000
    for record in fixture["records"]:
        for law in record.get("laws", []):
            next_id += 1
            project += f"\nlaw {next_id} reference_requirement_{next_id} on {law['scope']} = {law['formula']};"
    policy = dict(fixture["policy"])
    policy["law_kinds"] = {**policy["law_kinds"], **{str(i): "AuthoritySubjectRecipient"
                                                      for i in range(9001, next_id + 1)}}
    base = _table(fixture)
    table = Table(project + "\n", policy)
    reasons = [(record["where"], table.reason_ids[record["reason_requirement"]])
               for record in fixture["records"] if record.get("reason_requirement")]
    answers = fixture["answers"]
    determined = inconsistent = refused = 0
    translation = []
    groups = []
    for ordinal, (state, x) in enumerate(table.tuples):
        if base.decision(ordinal) is None:
            refused += 1
            continue
        candidates = base.cj.candidates(base.contract, state)
        # what the answers mean, with the declared laws only
        admitted = [d for d in candidates if base.contract.admits(state, x, d)
                    and all(_answer_admits(base, a, ordinal, d) for a in answers)]
        # what the door's pinned laws and recorded reasons admit
        pinned = [d for d in table.cj.candidates(table.contract, state) if table.contract.admits(state, x, d)]
        pinned = [d for d in pinned if d[0] != "Reject" or
                  all(base.value(where, ordinal) != 1 or d[1] == reason for where, reason in reasons)]
        if sorted(admitted, key=str) != sorted(pinned, key=str):
            translation.append(ordinal)
        if not admitted:
            inconsistent += 1
        elif len(admitted) == 1:
            determined += 1
        else:
            labels = sorted({table.label(d) for d in admitted}, key=str)
            case = base.contract.case_of(state, x)
            group = next((g for g in groups if g["case"] == case and g["labels"] == labels), None)
            if group is None:
                groups.append({"case": case, "labels": labels, "ordinals": [ordinal]})
            else:
                group["ordinals"].append(ordinal)
    regions = []
    for case in dict.fromkeys(g["case"] for g in groups):
        members = [g for g in groups if g["case"] == case]
        common = set(members[0]["labels"])
        for m in members[1:]:
            common &= set(m["labels"])
        if common:
            regions.append({"case": case, "ordinals": sorted(o for m in members for o in m["ordinals"])})
        else:
            regions.extend(members)
    regions.sort(key=lambda r: (-len(r["ordinals"]), r["case"]))
    stats = fixture["stats"]
    problems = []
    if not agree(translation, []):
        problems.append(f"the door's pinned laws admit other decisions than the answers mean on "
                        f"{len(translation)} inputs (first at ordinal {translation[0]})")
    problems += [f"{key}: door {stats.get(key)}, reference {mine}" for key, mine in
                (("determined", determined), ("inconsistent", inconsistent), ("reference_refused", refused),
                 ("regions", len(regions))) if not agree(stats.get(key), mine)]
    question = fixture.get("question")
    if question and regions:
        region = set(regions[0]["ordinals"])
        if not agree(question["tuples"], len(region)):
            problems.append(f"question tuples: door {question['tuples']}, reference {len(region)}")
        wrong = [o for o in range(len(base.tuples)) if (base.value(question["where"], o) == 1) != (o in region)]
        if not agree(wrong, []):
            problems.append(f"the question's formula is not exact on its region (first at ordinal {wrong[0]})")
    return problems


def run_fixture(fixture: dict) -> list:
    if fixture["kind"] == "teach-back":
        return check_teach_back(fixture)
    if fixture["kind"] == "interface":
        return check_interface(fixture)
    if fixture["kind"] == "question":
        return check_question(fixture)
    if fixture["kind"] == "rule-meaning":
        return check_rule_meaning(fixture)
    if fixture["kind"] == "region":
        return check_region(fixture["original"], fixture["candidate"], fixture["vocabulary"], fixture["region"])
    if fixture["kind"] == "unrealizable":
        return check_unrealizable(fixture["original"], fixture["vocabulary"], fixture["sketch"], fixture["door"])
    raise Invalid(f"unknown fixture kind {fixture['kind']}")


def self_test(fixtures: list) -> list:
    """Planted-wrong fixtures must be rejected, and accepted once the comparison is stubbed."""
    global same, meets, agree
    failures = [] if fixtures else ["no fixtures: the self-test needs planted cases"]
    real = (same, meets, agree)
    for fixture in fixtures:
        problems = run_fixture(fixture)
        if fixture.get("expect") == "rejected" and not problems:
            failures.append(f"{fixture['name']}: a planted error was accepted")
        if fixture.get("expect") == "accepted" and problems:
            failures.append(f"{fixture['name']}: {problems}")
    try:
        same = lambda a, b: True  # noqa: E731 - every formula looks exact
        meets = lambda a, b: False  # noqa: E731 - no completion ever looks right
        agree = lambda a, b: True  # noqa: E731 - every door value looks right
        for fixture in fixtures:
            if fixture.get("expect") == "rejected" and fixture.get("stub_sensitive", True):
                if run_fixture(fixture):
                    failures.append(f"{fixture['name']}: still rejected with the comparison stubbed; "
                                    "the comparison is not what decides")
    finally:
        same, meets, agree = real
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("fixtures", type=Path, nargs="*")
    parser.add_argument("--self-test", action="store_true")
    options = parser.parse_args()
    fixtures = []
    for path in options.fixtures:
        loaded = json.loads(path.read_text(encoding="ascii"))
        fixtures.extend(loaded if isinstance(loaded, list) else [loaded])
    if options.self_test:
        failures = self_test(fixtures)
        print(json.dumps({"self_test": "passed" if not failures else "failed", "fixtures": len(fixtures),
                          "failures": failures}, indent=1))
        return 1 if failures else 0
    results = [{"name": f.get("name"), "problems": run_fixture(f)} for f in fixtures]
    print(json.dumps(results, indent=1))
    return 1 if any(r["problems"] for r in results) else 0


if __name__ == "__main__":
    sys.exit(main())
