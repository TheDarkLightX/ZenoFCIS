"""ZAL loop profile ``zal/fcis-loop/1``: the language of the bounded loop's doors.

People and models use it to talk about one functional core inside a loop:
what a proposer offers (programs, sketches, family programs, laws,
invariants, scenarios, probes, questions, rules, architectures), what the
owner answers, and what the judge replies. Like ``zal/finite-fsm/1``:

* two deterministic surfaces, **symbolic** and **controlled English**, parse
  to the same frozen typed acts: ``parse(render(d)) == d`` on either surface;
* every document binds its subject by revision (a core) or by name (a
  family), and names its author: the model, the owner or the judge;
* bounded and fail-closed: ASCII only, one statement per line, at most 256
  statements and 64 KiB; unknown statements, names, reasons and fields,
  ambiguous text and free prose are refused, never guessed at;
* parsing needs no model and executes nothing. Every verdict comes from
  ZenoFCIS's Rust checkers through the ``zeno-fcis loop`` commands, and
  carries ``authority: none``.

This module only parses and renders. The expression grammar is the one the
``zeno-fcis loop`` doors parse (the contract policy-expression grammar with
``c ? a : b``, holes ``?c``, ``?c[lo..hi]``, ``?e``, ``?{a | b}``, family
parameters ``$P`` and ``keep``).

Statement forms (symbolic | controlled English), ``ID`` a lower-case name:

  zal fcis loop 1;                            | ZAL FCIS loop version 1.
  subject TAG at REVISION;                    | The subject is TAG at revision REVISION.
  subject family NAME;                        | The subject is the family NAME.
  author model;                               | The author is the model.   (owner, judge)
  program ID OUT = EXPR;                      | Program ID computes OUT as EXPR.
  family ID OUT = EXPR;                       | Family program ID computes OUT as EXPR.
  law ID on SCOPE: FORMULA;                   | Law ID on SCOPE: FORMULA.
  invariant ID: FORMULA;                      | Invariant ID: FORMULA.
  scenario ID from genesis;                   | Scenario ID starts from genesis.
  scenario ID from F = V, ...;                | Scenario ID starts where F is V and ....
  step ID N: F = V, ... -> OUTCOME;           | Step N of ID: when F is V and ..., OUTCOME.
  probe ID: depends OUT on F [where C];       | Probe ID: does OUT depend on F [where C]?
  probe ID: freedom;                          | Probe ID: where is the decision still open?
  ask ID [where C] at F = V, ...: O | O;      | Question ID: [where C, for example] when ..., should the core O or O?
  answer ID: O;                               | Answer ID: O.
  accepted ID: N tuples, M nodes;             | Accepted ID: equal on all N inputs, with M nodes.
  refuted ID at F = V, ...: OUT expected V got W;  | Refuted ID: when ..., OUT should be V but is W.
  region ID: N of T where C;                  | Region ID: wrong on N of T inputs, exactly where C.
  site ID OUT: E -> E;                        | Site ID: in OUT, replacing E with E repairs it.
  completed ID: ?K = E; ...;                  | Completed ID: hole K is E; ....
  unrealizable ID OUT at F = V, ...;          | Unrealizable ID: no completion of OUT is right when ....
  fact ID: TEXT;                              | Fact ID: TEXT.
  component C of D: owns F, ...; reads F, ...; decides R, ...;
                                              | Component C of D: owns F and ...; reads F and ...; decides R and ....
  case C N: when COND -> O;                   | Case N of C: when COND, O.
  decompose D: C, C, ...;                     | Decomposition D composes C, then C, then ....
  rule ID: where COND -> O;                   | Rule ID: where COND, the core will O.
  decomposed D: N tuples, M nodes, K components;  | Decomposed D: exact on all N inputs, with K components and M nodes.
  blame D C: N of M [where W];                | Blame D: component C is wrong on N of the M inputs it decides[, exactly where W].
  expected D C at F = V, ...: O got O;        | Expected D: when ..., component C should O, but it will O.
  interface D C at F = V, ... and F = V, ...: reads F, ...;
                                              | Interface D: component C cannot tell apart the inputs when ... and when ...; it must also read F and ....
  exact ID: N;                                | Exact ID: the core decides this on exactly the N inputs where the rule applies.
  holds ID: N;                                | Holds ID: the core decides this on all N inputs where the rule applies.
  exception ID: N of M [where W] at F = V, ...: O;  | Exception to ID: on N of its M inputs[, exactly where W], the core will O instead, for example when ....
  missing ID: N [where W] at F = V, ...;      | Missing from ID: the core also decides this on N more inputs[, exactly where W], for example when ....
  same ID as ID: N inputs, event E, VERDICT;  | Same ID: it means exactly what ID means on all N inputs, judged VERDICT at event E.
                                              (``states`` for an invariant)

Composable program-loop forms (same profile, model proposals only):
  pipeline ID OUT = EXPR;                    | Pipeline ID proposes OUT as EXPR.
  stages ID: complete, repair, canonical;    | Stages of ID: complete, then repair, then canonical.
  stages ID: gate-only;                      | Stages of ID: gate only.
  strategy ID OUT = EXPR;                    | Strategy ID proposes OUT as EXPR.
  explore ID: depth N;                       | Explore ID to depth N.
  mine ID OUT = EXPR;                        | Mine ID proposes OUT as EXPR.

A component's clauses are optional but keep this order; its cases are
numbered from 0. An ``accept`` that sets successors gives the whole
successor (fields it does not set keep their value).

This module is LLM-generated.
"""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import re

from behavior import Refusal

# Composable program loops: isolated parse/render additions, no execution.
import loop_composable_language as composable

PROFILE = "zal/fcis-loop/1"
HEADER = {"symbolic": "zal fcis loop 1;", "english": "ZAL FCIS loop version 1."}
MAX_SOURCE = 64 * 1024
MAX_STATEMENTS = 256
MAX_LINE = 8192
VERDICT = r"[a-z][a-z-]{0,23}"
ID = r"[a-z][a-z0-9_]{0,63}"
TAG = r"[A-Za-z0-9][A-Za-z0-9_-]{0,63}"
AUTHORS = {"model": "the model", "owner": "the owner", "judge": "the judge"}
SCOPES_EN = {"accept": "acceptance", "reject": "rejection", "failure": "committed failure", "commit": "commit",
             "any": "every decision"}
TEXT = re.compile(r"[A-Za-z0-9_ ,.;:=<>!?()+\-/'*|%]{1,400}")


def digest(data) -> str:
    """SHA-256 of canonical JSON (sorted keys, no spaces, ASCII)."""
    text = json.dumps(data, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(text.encode("ascii")).hexdigest()


# --- the expression grammar ------------------------------------------------------------------------

MAX_TEXT = 8192
MAX_NODES = 512
MAX_HOLES = 16
# Parser descents (parentheses, `!`, unary minus, conditionals, right-associative chains), counted before
# descending, and the depth of the tree read: deep input is refused, never left to exhaust the stack.
MAX_DESCENT = 192
MAX_DEPTH = 128


class _Descent:
    """Counts nested parser calls; past ``MAX_DESCENT`` the parse is refused before it descends further."""

    def __init__(self):
        self.level = 0

    def __enter__(self):
        self.level += 1
        if self.level > MAX_DESCENT:
            raise Refusal("expression-too-deeply-nested")

    def __exit__(self, *exc):
        self.level -= 1
        return False


def _children(node) -> list:
    tag = node[0]
    if tag == "not":
        return [node[1]]
    if tag == "bin":
        return [node[2], node[3]]
    if tag == "if":
        return list(node[1:])
    if tag == "call":
        return list(node[2])
    if tag == "hole" and node[2] == "choice":
        return list(node[3])
    return []


def depth(node) -> int:
    """The tree's depth, without recursion (stops counting past ``MAX_DEPTH``)."""
    best, stack = 0, [(node, 1)]
    while stack:
        n, d = stack.pop()
        best = max(best, d)
        if d > MAX_DEPTH:
            return d
        stack.extend((c, d + 1) for c in _children(n))
    return best


def _bounded(node):
    """Refuses a tree too deep or too large for the recursive passes that follow."""
    if depth(node) > MAX_DEPTH:
        raise Refusal("expression-too-deep")
    if size(node) > MAX_NODES:
        raise Refusal("expression-too-large")
    return node

TOKEN = re.compile(r"\s*(?:(\d+)|(\$[A-Za-z_][A-Za-z_0-9]*)|(\?c\[|\?c(?![A-Za-z0-9_])|\?e(?![A-Za-z0-9_])|\?\{)|"
                   r"([A-Za-z_][A-Za-z_0-9]*(?:\.[A-Za-z_][A-Za-z_0-9]*)?)|"
                   r"(->|&&|\|\||==|!=|<=|>=|\.\.|[(),!<>+\-*?:\]|}]))")
BINARY = {"->": (1, "right"), "||": (2, "left"), "&&": (3, "left"), "==": (4, "left"), "!=": (4, "left"),
          "<": (5, "left"), "<=": (5, "left"), ">": (5, "left"), ">=": (5, "left"),
          "+": (6, "left"), "-": (6, "left"), "*": (7, "left")}
COMPARE = ("==", "!=", "<", "<=", ">", ">=")
BOOLEAN = ("&&", "||", "->")
ARITH = ("+", "-", "*")
# Closed function vocabulary, with the same arities as main-route rules.
FUNCTIONS = {"choose": (3,), "div_floor": (2,), "div_ceil": (2,), "mul_div_floor": (3,), "mul_div_ceil": (3,),
             "products_ge": (4, 6)}
RESERVED = frozenset({"true", "false", "keep", "not", "and", "or", "is", "if", "then", "otherwise", "implies",
                      "plus", "minus", "times", "less", "greater", "than", "at", "most", "least", "some",
                      "constant", "expression", "one", "of", "from", "to", "parameter", "before", "after",
                      "the", "original", "output", "where"})
I64 = (-(1 << 63), (1 << 63) - 1)


def _integer(text: str, negative: bool = False) -> int:
    value = -int(text) if negative else int(text)
    if not I64[0] <= value <= I64[1]:
        raise Refusal("expression-integer-range")
    return value


def tokenize(text: str) -> list[tuple[str, str]]:
    if type(text) is not str or len(text) > MAX_TEXT or not text.isascii():
        raise Refusal("expression-text")
    out, at = [], 0
    text = text.strip()
    while at < len(text):
        m = TOKEN.match(text, at)
        if not m or m.end() == at:
            raise Refusal(f"expression-token:{text[at:at + 12]}")
        num, param, hole, name, op = m.groups()
        if num:
            out.append(("num", num))
        elif param:
            out.append(("param", param[1:]))
        elif hole:
            out.append(("hole", hole))
        elif name:
            out.append(("name", name))
        else:
            out.append(("op", op))
        at = m.end()
    return out


def parse_expression(text: str, first_hole: int = 0, *, allow_holes: bool = True, allow_params: bool = True,
                     allow_keep: bool = False):
    """One symbolic expression as a nested tuple; holes are numbered in reading order."""
    tokens = tokenize(text)
    pos = [0]
    holes = [first_hole]
    descent = _Descent()

    def peek():
        return tokens[pos[0]] if pos[0] < len(tokens) else ("end", "")

    def take():
        token = peek()
        pos[0] += 1
        return token

    def accept(value: str) -> bool:
        if peek()[1] == value and peek()[0] in ("op", "name"):
            pos[0] += 1
            return True
        return False

    def expect(value: str) -> None:
        if not accept(value):
            raise Refusal(f"expression-expected:{value}")

    def primary():
        with descent:
            return primary_body()

    def primary_body():
        kind, value = take()
        if kind == "num":
            return ("int", _integer(value))
        if kind == "param":
            if not allow_params:
                raise Refusal("expression-parameter-not-allowed")
            return ("param", value)
        if kind == "hole":
            if not allow_holes:
                raise Refusal("expression-hole-not-allowed")
            return hole(value)
        if kind == "name":
            if value == "true":
                return ("int", 1)
            if value == "false":
                return ("int", 0)
            if value == "keep":
                if not allow_keep:
                    raise Refusal("expression-keep-only-as-a-whole-output")
                return ("keep",)
            if value in RESERVED:
                raise Refusal(f"expression-reserved-word:{value}")
            if peek() == ("op", "("):
                if value not in FUNCTIONS:
                    raise Refusal(f"expression-call-unsupported:{value}")
                take()
                arguments = [conditional()]
                while accept(","):
                    if len(arguments) >= 6:
                        raise Refusal("expression-call-arity")
                    arguments.append(conditional())
                expect(")")
                if len(arguments) not in FUNCTIONS[value]:
                    raise Refusal("expression-call-arity")
                if value == "choose":
                    return ("if", *arguments)
                return ("call", value, tuple(arguments))
            return ("var", value)
        if value == "(":
            node = conditional()
            expect(")")
            return node
        if value == "!":
            return ("not", primary())
        if value == "-":
            if peek()[0] == "num":
                return ("int", _integer(take()[1], negative=True))
            return ("bin", "-", ("int", 0), primary())
        raise Refusal(f"expression-primary:{value or 'end'}")

    def integer() -> int:
        negative = accept("-")
        kind, value = take()
        if kind != "num":
            raise Refusal("hole-range-integer")
        return _integer(value, negative)

    def hole(token: str):
        k = holes[0]
        holes[0] += 1
        if k >= MAX_HOLES:
            raise Refusal("too-many-holes")
        if token == "?c":
            return ("hole", k, "const", None, None)
        if token == "?c[":
            lo = integer()
            expect("..")
            hi = integer()
            expect("]")
            if hi < lo or hi - lo > 1 << 16:
                raise Refusal("hole-range")
            return ("hole", k, "const", lo, hi)
        if token == "?e":
            return ("hole", k, "expr")
        alternatives = [conditional()]
        while accept("|"):
            alternatives.append(conditional())
        expect("}")
        if len(alternatives) < 2 or len(alternatives) > 16:
            raise Refusal("choice-hole-arity")
        if any(holes_of(a) for a in alternatives):
            raise Refusal("choice-alternatives-must-be-hole-free")
        return ("hole", k, "choice", tuple(alternatives))

    def expression(min_prec):
        with descent:
            return expression_body(min_prec)

    def expression_body(min_prec):
        left = primary()
        while True:
            kind, value = peek()
            if kind != "op" or value not in BINARY or BINARY[value][0] < min_prec:
                return left
            prec, assoc = BINARY[value]
            take()
            right = expression(prec + 1 if assoc == "left" else prec)
            left = ("bin", value, left, right)

    def conditional():
        with descent:
            return conditional_body()

    def conditional_body():
        test = expression(0)
        if accept("?"):
            yes = conditional()
            expect(":")
            return ("if", test, yes, conditional())
        return test

    node = conditional()
    if pos[0] != len(tokens):
        raise Refusal("expression-trailing")
    _bounded(node)
    if node != ("keep",) and _contains_keep(node):
        raise Refusal("expression-keep-only-as-a-whole-output")
    return node


def size(node) -> int:
    tag = node[0]
    if tag == "not":
        return 1 + size(node[1])
    if tag == "bin":
        return 1 + size(node[2]) + size(node[3])
    if tag == "if":
        return 1 + size(node[1]) + size(node[2]) + size(node[3])
    if tag == "call":
        return 1 + sum(size(a) for a in node[2])
    if tag == "hole" and node[2] == "choice":
        return 1 + sum(size(a) for a in node[3])
    return 1


def substitute(node, fn):
    """Bottom-up rewrite: ``fn(node)`` returns a replacement or None to keep the node."""
    tag = node[0]
    if tag == "not":
        node = ("not", substitute(node[1], fn))
    elif tag == "bin":
        node = ("bin", node[1], substitute(node[2], fn), substitute(node[3], fn))
    elif tag == "if":
        node = ("if", substitute(node[1], fn), substitute(node[2], fn), substitute(node[3], fn))
    elif tag == "call":
        node = ("call", node[1], tuple(substitute(a, fn) for a in node[2]))
    elif tag == "hole" and node[2] == "choice":
        node = ("hole", node[1], "choice", tuple(substitute(a, fn) for a in node[3]))
    out = fn(node)
    return node if out is None else out


def _contains_keep(node) -> bool:
    found = []
    substitute(node, lambda n: found.append(1) if n == ("keep",) else None)
    return bool(found)


def holes_of(node) -> list:
    out = []

    def visit(n):
        tag = n[0]
        if tag == "hole":
            out.append(n)
        elif tag == "not":
            visit(n[1])
        elif tag == "bin":
            visit(n[2])
            visit(n[3])
        elif tag == "if":
            for child in n[1:]:
                visit(child)
        elif tag == "call":
            for child in n[2]:
                visit(child)
    visit(node)
    return out


def names_of(node) -> set:
    found = set()
    substitute(node, lambda n: found.add(n[1]) if n[0] == "var" else None)
    return found


def params_of(node) -> set:
    found = set()
    substitute(node, lambda n: found.add(n[1]) if n[0] == "param" else None)
    return found


def renumber(nodes: list) -> list:
    """Number the holes of several expressions consecutively (one sketch, several outputs)."""
    counter = [0]

    def walk(node):
        tag = node[0]
        if tag == "hole":
            k = counter[0]
            counter[0] += 1
            if k >= MAX_HOLES:
                raise Refusal("too-many-holes")
            return ("hole", k) + node[2:]
        if tag == "not":
            return ("not", walk(node[1]))
        if tag == "bin":
            return ("bin", node[1], walk(node[2]), walk(node[3]))
        if tag == "if":
            return ("if",) + tuple(walk(c) for c in node[1:])
        if tag == "call":
            return ("call", node[1], tuple(walk(c) for c in node[2]))
        return node
    return [walk(n) for n in nodes]


class Vocabulary:
    """What names mean: fields with kinds, outputs, variants, and (for laws) after-state paths."""

    def __init__(self, names: list, kinds: list, outputs: list = (), variants: dict | None = None,
                 paths: dict | None = None):
        self.names = list(names)
        self.kinds = dict(zip(names, kinds))
        self.outputs = list(outputs)
        self.output_index = {f"out{j}": j for j in range(len(self.outputs))}
        self.variants = dict(variants or {})
        self.paths = dict(paths or {})

    def kind(self, name: str) -> str:
        if name in self.kinds:
            return self.kinds[name]
        if name in self.output_index:
            return self.outputs[self.output_index[name]]
        if name in self.paths:
            return self.paths[name]
        if name in self.variants:
            return "Int"
        raise Refusal(f"unknown-name:{name}")

    def resolve_variants(self, node):
        def fn(n):
            if n[0] == "var" and n[1] in self.variants and n[1] not in self.kinds:
                return ("int", self.variants[n[1]])
            return None
        return substitute(node, fn)


def kind_of(node, vocab: Vocabulary):
    tag = node[0]
    if tag == "var":
        return vocab.kind(node[1])
    if tag in ("int", "hole", "keep"):
        if tag == "hole" and node[2] == "choice":
            for a in node[3]:
                k = kind_of(a, vocab)
                if k is not None:
                    return k
        return None
    if tag == "param":
        return "Int"
    if tag == "not":
        return "Bool"
    if tag == "if":
        return kind_of(node[2], vocab) or kind_of(node[3], vocab)
    if tag == "call":
        return "Bool" if node[1] == "products_ge" else "Int"
    return "Int" if node[1] in ARITH else "Bool"


ENGLISH_OPS = {"->": "implies", "||": "or", "&&": "and", "==": "is", "!=": "is not", "<": "is less than",
               "<=": "is at most", ">": "is greater than", ">=": "is at least", "+": "plus", "-": "minus",
               "*": "times"}


def _prec(node) -> int:
    if node[0] == "if":
        return 0
    if node[0] == "bin":
        return BINARY[node[1]][0]
    return 9


class _Printer:
    """Deterministic printer for both surfaces."""

    def __init__(self, symbolic: bool, variant_of=None, english_name=None, vocab=None):
        self.symbolic = symbolic
        self.variant_of = variant_of or (lambda var, value: None)
        self.english_name = english_name or (lambda name: name)
        self.vocab = vocab

    def kind(self, node):
        if self.vocab is None:
            return None
        try:
            return kind_of(node, self.vocab)
        except Refusal:
            return None

    def show(self, node, parent_prec: int = 0, want: str | None = None) -> str:
        tag = node[0]
        if tag == "int":
            if want == "Bool" and node[1] in (0, 1):
                return "true" if node[1] == 1 else "false"
            return str(node[1])
        if tag == "var":
            return node[1] if self.symbolic else self.english_name(node[1])
        if tag == "param":
            return f"${node[1]}" if self.symbolic else f"parameter {node[1]}"
        if tag == "keep":
            return "keep" if self.symbolic else "the original output"
        if tag == "hole":
            return self.hole(node, want)
        if tag == "call":
            # Function names and comma-delimited arguments are exact on both
            # surfaces; arguments retain their surface's operators and names.
            return node[1] + "(" + ", ".join(self.show(a, 0, "Int") for a in node[2]) + ")"
        if tag == "not":
            inner = node[1]
            text = self.show(inner, 9, "Bool")
            if inner[0] in ("bin", "if") or (not self.symbolic and inner[0] == "not"):
                text = f"({self.show(inner, 0, 'Bool')})"
            return f"!{text}" if self.symbolic else f"not {text}"
        if tag == "if":
            branch = want or self.kind(node[2]) or self.kind(node[3])
            c = self.show(node[1], 0, "Bool")
            c = f"({c})" if node[1][0] == "if" else c
            a = self.show(node[2], 0, branch)
            a = f"({a})" if node[2][0] == "if" else a
            b = self.show(node[3], 0, branch)
            text = f"{c} ? {a} : {b}" if self.symbolic else f"if {c} then {a} otherwise {b}"
            return f"({text})" if parent_prec > 0 else text
        op, a, b = node[1], node[2], node[3]
        prec, assoc = BINARY[op]
        if op in BOOLEAN:
            side = "Bool"
        elif op in ARITH:
            side = "Int"
        else:
            side = self.kind(a) or self.kind(b)
        left = self.operand(a, prec, assoc == "right", side)
        right = self.operand(b, prec, assoc == "left", side)
        if op in ("==", "!=") and a[0] == "var" and b[0] == "int":
            name = self.variant_of(a[1], b[1])
            if name is not None:
                right = name
        text = f"{left} {op} {right}" if self.symbolic else f"{left} {ENGLISH_OPS[op]} {right}"
        return f"({text})" if prec < parent_prec else text

    def operand(self, node, prec: int, strict: bool, want: str | None) -> str:
        if node[0] == "if":
            return f"({self.show(node, 0, want)})"
        child = _prec(node)
        text = self.show(node, 0, want)
        if child < prec or (strict and child == prec and node[0] == "bin"):
            return f"({text})"
        if node[0] == "bin" and node[1] in COMPARE and prec == BINARY["=="][0]:
            return f"({text})"
        if node[0] == "bin" and node[1] == "&&" and prec == BINARY["||"][0]:
            return f"({text})"
        if not self.symbolic and node[0] == "not":
            return f"({text})"
        return text

    def hole(self, node, want) -> str:
        kind = node[2]
        if kind == "const":
            lo, hi = node[3], node[4]
            if lo is None:
                return "?c" if self.symbolic else "some constant"
            return f"?c[{lo}..{hi}]" if self.symbolic else f"some constant from {lo} to {hi}"
        if kind == "expr":
            return "?e" if self.symbolic else "some expression"
        alts = " | ".join(self.show(a, 0, want) for a in node[3])
        return f"?{{{alts}}}" if self.symbolic else f"one of [{alts}]"


def print_symbolic(node) -> str:
    """Canonical symbolic text, independent of any vocabulary: the ``zeno-fcis loop`` doors parse it."""
    return _Printer(symbolic=True).show(node)


# --- the subject's vocabulary ------------------------------------------------------------------------

class Context:
    """Names a document may use, from one subject description (see ``loop_workspace.describe``)."""

    def __init__(self, subject: dict):
        self.subject = subject
        self.family = subject.get("family_subject")
        self.tag = subject["tag"]
        self.revision = subject["revision"]
        self.names = list(subject["names"])
        self.state = list(subject["state"])
        self.inputs = [n for n in self.names if n not in self.state]
        self.kinds = dict(zip(self.names, subject["kinds"]))
        self.output_kinds = list(subject["outputs"])
        self.outputs = [f"out{j}" for j in range(len(self.output_kinds))]
        self.field_variants = {f: {int(v): n for n, v in table.items()} for f, table in subject["variants"].items()}
        self.variants = {}
        for table in subject["variants"].values():
            for name, value in table.items():
                if self.variants.get(name, value) != value:
                    raise Refusal(f"ambiguous-variant-name:{name}")
                self.variants[name] = value
        clash = set(self.variants) & (set(self.names) | set(self.outputs) | RESERVED)
        if clash:
            raise Refusal(f"variant-name-clash:{sorted(clash)[0]}")
        self.reasons = set(subject["reasons"])
        self.roots = dict(subject["roots"])
        self.vocab = Vocabulary(self.names, [self.kinds[n] for n in self.names], self.output_kinds, self.variants)
        law_names, law_kinds, paths = [], [], {}
        for name in self.names:
            law_names.append(f"pre.{name}" if name in self.state else f"{self.roots[name]}.{name}")
            law_kinds.append(self.kinds[name])
        for name in self.state:
            paths[f"post.{name}"] = self.kinds[name]
        self.law_vocab = Vocabulary(law_names, law_kinds, (), self.variants, paths)

    @property
    def subject_ref(self) -> tuple:
        if self.family is not None:
            return ("family", self.family)
        return ("core", self.tag, self.revision)

    def output_kind(self, out: str) -> str:
        return self.output_kinds[int(out[3:])]

    def variant_name(self, var: str, value: int):
        return self.field_variants.get(var.split(".")[-1], {}).get(value)

    def variant_of(self, var: str, value: int):
        base = var.split(".")[-1]
        if self.kinds.get(base) == "Bool" and value in (0, 1):
            return "true" if value == 1 else "false"
        return self.variant_name(var, value)

    def law_name(self, name: str) -> str:
        root, _, rest = name.partition(".")
        if root == "pre":
            return f"{rest} before"
        if root == "post":
            return f"{rest} after"
        return rest if rest else name

    def value_text(self, name: str, value: int) -> str:
        if self.kinds.get(name) == "Bool":
            return "true" if value else "false"
        return self.field_variants.get(name, {}).get(value, str(value))

    def parse_value(self, name: str, text: str) -> int:
        text = text.strip()
        if name not in self.kinds:
            raise Refusal(f"zal-unknown-field:{name}")
        if text in ("true", "false"):
            if self.kinds[name] != "Bool":
                raise Refusal(f"zal-boolean-for-integer-field:{name}")
            return int(text == "true")
        if text in self.variants:
            return self.variants[text]
        if re.fullmatch(r"-?\d{1,19}", text):
            return _integer(text.lstrip("-"), text.startswith("-"))
        raise Refusal(f"zal-value:{name}={text[:20]}")


def show(node, ctx: Context, english: bool, law: bool = False, want: str | None = None) -> str:
    vocab = ctx.law_vocab if law else ctx.vocab
    if english:
        return _Printer(False, ctx.variant_of, ctx.law_name if law else None, vocab).show(node, 0, want)
    return _Printer(True, ctx.variant_of, None, vocab).show(node, 0, want)


def read(text: str, ctx: Context, english: bool, law: bool = False, allow_keep: bool = False):
    node = parse_english(text, ctx, law) if english else parse_expression(text, allow_keep=allow_keep)
    if node == ("keep",) and not allow_keep:
        raise Refusal("zal-keep-only-for-program-outputs")
    vocab = ctx.law_vocab if law else ctx.vocab
    node = vocab.resolve_variants(node)
    known = set(vocab.names) | set(vocab.paths) | set(ctx.outputs if not law else ())
    unknown = names_of(node) - known
    if unknown:
        raise Refusal(f"zal-unknown-name:{sorted(unknown)[0]}")
    return node


_WORD = re.compile(r"\s*(?:(-?\d+)|([A-Za-z_][A-Za-z_0-9]*)|(\?c\[|\?c|\?e|\?\{|\$[A-Za-z_][A-Za-z_0-9]*)|"
                   r"(->|&&|\|\||==|!=|<=|>=|\.\.|[(),\[\]|!<>+\-*?:}]))")
_PHRASES = [("is", "less", "than", "<"), ("is", "at", "most", "<="), ("is", "greater", "than", ">"),
            ("is", "at", "least", ">="), ("is", "not", "!="), ("is", "=="), ("plus", "+"), ("minus", "-"),
            ("times", "*"), ("and", "&&"), ("or", "||"), ("implies", "->")]


def parse_english(text: str, ctx: Context, law: bool = False):
    """Controlled-English expressions (the symbolic operators are accepted too)."""
    if not text.isascii() or len(text) > MAX_LINE:
        raise Refusal("zal-expression-text")
    tokens, at = [], 0
    text = text.strip()
    while at < len(text):
        m = _WORD.match(text, at)
        if not m or m.end() == at:
            raise Refusal(f"zal-expression-token:{text[at:at + 12]}")
        num, word, special, op = m.groups()
        tokens.append(("num", num) if num else ("word", word) if word else ("sym", special or op))
        at = m.end()
    pos = [0]

    def peek(k: int = 0):
        i = pos[0] + k
        return tokens[i] if i < len(tokens) else ("end", "")

    def take():
        token = peek()
        pos[0] += 1
        return token

    def accept_word(w: str) -> bool:
        if peek() == ("word", w):
            pos[0] += 1
            return True
        return False

    def expect_word(w: str) -> None:
        if not accept_word(w):
            raise Refusal(f"zal-expected:{w}")

    def accept_sym(s: str) -> bool:
        if peek() == ("sym", s):
            pos[0] += 1
            return True
        return False

    def operator():
        kind, value = peek()
        if kind == "sym" and value in BINARY:
            return value, 1
        if kind != "word":
            return None, 0
        for phrase in _PHRASES:
            words, op = phrase[:-1], phrase[-1]
            if all(peek(i) == ("word", w) for i, w in enumerate(words)):
                if op == "!=" and peek(2) == ("sym", "("):
                    return "==", 1
                return op, len(words)
        return None, 0

    def name(word: str):
        if word in ("true", "false"):
            return ("int", 1 if word == "true" else 0)
        if word in RESERVED:
            raise Refusal(f"zal-reserved-word:{word}")
        if law:
            if accept_word("before"):
                return ("var", f"pre.{word}")
            if accept_word("after"):
                return ("var", f"post.{word}")
            if word in ctx.state:
                raise Refusal(f"zal-state-field-needs-before-or-after:{word}")
            for full in ctx.law_vocab.names:
                if full.endswith("." + word) and not full.startswith("pre."):
                    return ("var", full)
        return ("var", word)

    holes = [0]
    descent = _Descent()

    def primary():
        with descent:
            return primary_body()

    def primary_body():
        kind, value = take()
        if kind == "num":
            return ("int", _integer(value.lstrip("-"), value.startswith("-")))
        if kind == "sym":
            if value == "(":
                node = conditional()
                if not accept_sym(")"):
                    raise Refusal("zal-expected:)")
                return node
            if value == "!":
                return ("not", primary())
            if value == "-":
                return ("bin", "-", ("int", 0), primary())
            if value.startswith("$"):
                return ("param", value[1:])
            raise Refusal(f"zal-expression-primary:{value}")
        if kind != "word":
            raise Refusal("zal-expression-ended")
        if value in FUNCTIONS and accept_sym("("):
            arguments = [conditional()]
            while accept_sym(","):
                if len(arguments) >= 6:
                    raise Refusal("expression-call-arity")
                arguments.append(conditional())
            if not accept_sym(")") or len(arguments) not in FUNCTIONS[value]:
                raise Refusal("expression-call-arity")
            if value == "choose":
                return ("if", *arguments)
            return ("call", value, tuple(arguments))
        if value == "not":
            return ("not", primary())
        if value == "parameter":
            k, p = take()
            if k != "word":
                raise Refusal("zal-parameter-name")
            return ("param", p)
        if value == "the" and peek() == ("word", "original") and peek(1) == ("word", "output"):
            pos[0] += 2
            return ("keep",)
        if value == "some":
            k = holes[0]
            holes[0] += 1
            if k >= MAX_HOLES:
                raise Refusal("too-many-holes")
            if accept_word("expression"):
                return ("hole", k, "expr")
            expect_word("constant")
            if accept_word("from"):
                lo = take()
                expect_word("to")
                hi = take()
                if lo[0] != "num" or hi[0] != "num":
                    raise Refusal("zal-hole-range")
                low, high = int(lo[1]), int(hi[1])
                if high < low or high - low > 1 << 16:
                    raise Refusal("hole-range")
                return ("hole", k, "const", low, high)
            return ("hole", k, "const", None, None)
        if value == "one" and peek() == ("word", "of"):
            pos[0] += 1
            k = holes[0]
            holes[0] += 1
            if k >= MAX_HOLES:
                raise Refusal("too-many-holes")
            if not accept_sym("["):
                raise Refusal("zal-expected:[")
            alternatives = [conditional()]
            while accept_sym("|"):
                alternatives.append(conditional())
            if not accept_sym("]"):
                raise Refusal("zal-expected:]")
            if len(alternatives) < 2 or len(alternatives) > 16 or any(holes_of(a) for a in alternatives):
                raise Refusal("choice-hole-arity")
            return ("hole", k, "choice", tuple(alternatives))
        if value == "if":
            raise Refusal("zal-conditional-needs-parentheses-here")
        return name(value)

    def binary(min_prec: int):
        with descent:
            return binary_body(min_prec)

    def binary_body(min_prec: int):
        left = primary()
        while True:
            op, width = operator()
            if op is None or BINARY[op][0] < min_prec:
                return left
            prec, assoc = BINARY[op]
            pos[0] += width
            right = binary(prec + 1 if assoc == "left" else prec)
            left = ("bin", op, left, right)

    def conditional():
        with descent:
            return conditional_body()

    def conditional_body():
        if accept_word("if"):
            test = conditional()
            expect_word("then")
            yes = conditional()
            expect_word("otherwise")
            return ("if", test, yes, conditional())
        node = binary(0)
        if accept_sym("?"):
            yes = conditional()
            if not accept_sym(":"):
                raise Refusal("zal-expected::")
            return ("if", node, yes, conditional())
        return node

    node = conditional()
    if pos[0] != len(tokens):
        raise Refusal(f"zal-expression-trailing:{peek()[1]}")
    return _bounded(node)


# --- typed acts ----------------------------------------------------------------------------------------

@dataclass(frozen=True)
class Option:
    """A decision: a class, a reason (Reject / CommittedFailure) and successor fields (Accept / failure)."""
    cls: str
    reason: str | None = None
    sets: tuple = ()

    def data(self) -> dict:
        return {"class": self.cls, "reason": self.reason, "sets": [[f, print_symbolic(v)] for f, v in self.sets]}


@dataclass(frozen=True)
class Act:
    kind: str
    id: str
    fields: tuple = ()

    def get(self, name, default=None):
        for k, v in self.fields:
            if k == name:
                return v
        return default

    def data(self) -> dict:
        return {"kind": self.kind, "id": self.id, **{k: _json(v) for k, v in self.fields}}


@dataclass(frozen=True)
class Document:
    subject: tuple
    author: str
    acts: tuple

    def data(self) -> dict:
        return {"profile": PROFILE, "subject": list(self.subject), "author": self.author,
                "acts": [a.data() for a in self.acts]}

    @property
    def revision(self) -> str:
        return digest(self.data())


AST_TAGS = ("int", "var", "param", "hole", "keep", "not", "bin", "if")


def _json(value):
    if isinstance(value, Option):
        return value.data()
    if isinstance(value, tuple) and value and isinstance(value[0], str) and value[0] in AST_TAGS:
        return print_symbolic(value)
    if isinstance(value, tuple):
        return [_json(v) for v in value]
    return value


def act(kind: str, id_: str, **fields) -> Act:
    return Act(kind, id_, tuple(sorted(fields.items())))


# --- shared slot syntax -------------------------------------------------------------------------------

def _value_shown(f: str, v, ctx: Context, english: bool) -> str:
    if isinstance(v, tuple):
        return show(v, ctx, english)
    return ctx.value_text(f, v)


def _inputs_symbolic(values: tuple, ctx: Context) -> str:
    return ", ".join(f"{f} = {_value_shown(f, v, ctx, False)}" for f, v in values)


def _inputs_english(values: tuple, ctx: Context) -> str:
    return " and ".join(f"{f} is {_value_shown(f, v, ctx, True)}" for f, v in values)


def _parse_inputs(text: str, ctx: Context, english: bool, allowed: list) -> tuple:
    parts = [p.strip() for p in (re.split(r"\s+and\s+", text.strip()) if english else text.split(","))]
    out = []
    for part in parts:
        m = re.fullmatch(r"([a-z][a-z0-9_]*)\s+is\s+(.+)" if english else r"([a-z][a-z0-9_]*)\s*=\s*(.+)", part)
        if not m:
            raise Refusal(f"zal-input-assignment:{part[:40]}")
        if m.group(1) not in allowed:
            raise Refusal(f"zal-input-field:{m.group(1)}")
        text = m.group(2).strip()
        if "$" in text or text.startswith("parameter "):
            if ctx.family is None:
                raise Refusal("zal-parameters-need-a-family-subject")
            value = read(text, ctx, english)
            if names_of(value):
                raise Refusal("zal-input-value-reads-fields")
            out.append((m.group(1), value))
        else:
            out.append((m.group(1), ctx.parse_value(m.group(1), text)))
    names = [f for f, _ in out]
    if len(set(names)) != len(names):
        raise Refusal("zal-duplicate-field")
    if set(names) != set(allowed):
        raise Refusal(f"zal-inputs-must-name-every-field:{sorted(set(allowed) - set(names))}")
    order = {n: k for k, n in enumerate(ctx.names)}
    return tuple(sorted(out, key=lambda fv: order[fv[0]]))


def _set_value(f: str, v, ctx: Context, english: bool) -> str:
    if v[0] == "int" and ctx.kinds.get(f) != "Bool":
        named = ctx.field_variants.get(f, {}).get(v[1])
        if named is not None:
            return named
    return show(v, ctx, english, want=ctx.kinds.get(f))


def option_symbolic(o: Option, ctx: Context) -> str:
    if o.cls == "NoCase":
        return "nocase"
    if o.cls == "Refused":
        return "refused"
    head = {"Accept": "accept", "Reject": f"reject {o.reason}", "CommittedFailure": f"failure {o.reason}"}[o.cls]
    if o.sets:
        head += " " + ", ".join(f"{f} = {_set_value(f, v, ctx, False)}" for f, v in o.sets)
    return head


def option_english(o: Option, ctx: Context) -> str:
    if o.cls == "NoCase":
        return "match no case"
    if o.cls == "Refused":
        # a decomposition's step 3 marks a refusal by the door's own evaluation, so no Authority is named
        return "be refused"
    head = {"Accept": "accept", "Reject": f"reject because {o.reason}",
            "CommittedFailure": f"fail and commit because {o.reason}"}[o.cls]
    if o.sets:
        head += " with " + "; ".join(f"{f} set to {_set_value(f, v, ctx, True)}" for f, v in o.sets)
    return head


def _parse_option(text: str, ctx: Context, english: bool) -> Option:
    text = text.strip()
    if english:
        if text in ("be refused", "be refused by the Authority"):
            return Option("Refused")
        if text == "match no case":
            return Option("NoCase")
        m = re.fullmatch(r"(accept|reject because ([a-z][a-z0-9_]*)|fail and commit because ([a-z][a-z0-9_]*))"
                         r"(?: with (.+))?", text)
        if not m:
            raise Refusal(f"zal-option:{text[:40]}")
        cls = "Accept" if m.group(1) == "accept" else ("Reject" if m.group(2) else "CommittedFailure")
        reason = m.group(2) or m.group(3)
        sets = []
        if m.group(4):
            for part in m.group(4).split(";"):
                mm = re.fullmatch(r"\s*([a-z][a-z0-9_]*) set to (.+?)\s*", part)
                if not mm:
                    raise Refusal(f"zal-option-assignment:{part[:40]}")
                sets.append((mm.group(1), read(mm.group(2), ctx, True)))
    else:
        if text == "refused":
            return Option("Refused")
        if text == "nocase":
            return Option("NoCase")
        m = re.fullmatch(r"(accept|reject ([a-z][a-z0-9_]*)|failure ([a-z][a-z0-9_]*))(?: (.+))?", text)
        if not m:
            raise Refusal(f"zal-option:{text[:40]}")
        cls = "Accept" if m.group(1) == "accept" else ("Reject" if m.group(2) else "CommittedFailure")
        reason = m.group(2) or m.group(3)
        sets = []
        if m.group(4):
            # Function arguments also use commas; only field assignments
            # separate successors. An equality inside a call is not one.
            for part in re.split(r",(?=\s*[a-z][a-z0-9_]*\s*=(?!=))", m.group(4)):
                mm = re.fullmatch(r"\s*([a-z][a-z0-9_]*)\s*=\s*(.+?)\s*", part)
                if not mm:
                    raise Refusal(f"zal-option-assignment:{part[:40]}")
                sets.append((mm.group(1), read(mm.group(2), ctx, False)))
    if reason is not None and reason not in ctx.reasons:
        raise Refusal(f"zal-unknown-reason:{reason}")
    if cls == "Reject" and sets:
        raise Refusal("zal-reject-keeps-the-state")
    for f, _ in sets:
        if f not in ctx.state:
            raise Refusal(f"zal-successor-field:{f}")
    if len({f for f, _ in sets}) != len(sets):
        raise Refusal("zal-successor-field-twice")
    order = {n: k for k, n in enumerate(ctx.state)}
    return Option(cls, reason, tuple(sorted(sets, key=lambda fv: order[fv[0]])))


def _scope_symbolic(scope: str) -> str:
    return scope.replace(":", " ")


def _scope_english(scope: str) -> str:
    if scope.startswith("reject:"):
        return f"rejection because {scope.split(':', 1)[1]}"
    return SCOPES_EN[scope]


def _parse_scope(text: str, ctx: Context, english: bool) -> str:
    text = text.strip()
    if english:
        m = re.fullmatch(r"rejection because ([a-z][a-z0-9_]*)", text)
        if m:
            scope = f"reject:{m.group(1)}"
        else:
            inverse = {v: k for k, v in SCOPES_EN.items()}
            if text not in inverse:
                raise Refusal(f"zal-scope:{text[:30]}")
            scope = inverse[text]
    else:
        m = re.fullmatch(r"reject ([a-z][a-z0-9_]*)", text)
        scope = f"reject:{m.group(1)}" if m else text
        if not m and scope not in SCOPES_EN:
            raise Refusal(f"zal-scope:{text[:30]}")
    if scope.startswith("reject:") and scope.split(":", 1)[1] not in ctx.reasons:
        raise Refusal(f"zal-unknown-reason:{scope.split(':', 1)[1]}")
    return scope


# --- statements ---------------------------------------------------------------------------------------

PROBE_KINDS = ("depends", "values", "distinguish", "implies", "count", "reachable", "explain", "freedom")


def render_act(a: Act, ctx: Context, english: bool) -> str:
    # BEGIN version-space act rendering
    import loop_versions
    if a.kind in loop_versions.ACT_KINDS:
        rendered = loop_versions.render_act(a, ctx, english)
        if rendered is not None:
            return rendered
    # END version-space act rendering
    # Capture acts use the same bounded document/profile and separate vocabulary.
    import capture_language
    if ctx.tag == "capture" and a.kind in capture_language.ACT_KINDS:
        from capture_language import render as render_capture
        capture = render_capture(a, ctx, english)
        if capture is not None:
            return capture
    # BEGIN composable program loop render hook (own act kinds only).
    if a.kind in composable.ACT_KINDS:
        rendered = composable.render(a, ctx, english)
        if rendered is None:
            raise Refusal("zal-composable-act-shape")
        return rendered
    # END composable program loop render hook.
    k, i, g = a.kind, a.id, a.get
    # Fixed-vocabulary learning, additive to the existing acts.
    if k == "learn" or k.startswith("learn-"):
        import loop_learn
        return loop_learn.render_act(a, ctx, english)
    if k in ("program", "family"):
        out = g("output")
        expr = show(g("expr"), ctx, english, want=ctx.output_kind(out))
        if english:
            return f"{'Family program' if k == 'family' else 'Program'} {i} computes {out} as {expr}."
        return f"{k} {i} {out} = {expr};"
    if k == "law":
        f = show(g("formula"), ctx, english, law=True, want="Bool")
        return f"Law {i} on {_scope_english(g('scope'))}: {f}." if english else \
            f"law {i} on {_scope_symbolic(g('scope'))}: {f};"
    if k == "invariant":
        f = show(g("formula"), ctx, english, want="Bool")
        return f"Invariant {i}: {f}." if english else f"invariant {i}: {f};"
    if k == "scenario":
        start = g("start")
        if not start:
            return f"Scenario {i} starts from genesis." if english else f"scenario {i} from genesis;"
        return f"Scenario {i} starts where {_inputs_english(start, ctx)}." if english else \
            f"scenario {i} from {_inputs_symbolic(start, ctx)};"
    if k == "step":
        n, inputs, outcome = g("n"), g("inputs"), g("outcome")
        if english:
            return f"Step {n} of {i}: when {_inputs_english(inputs, ctx)}, {option_english(outcome, ctx)}."
        return f"step {i} {n}: {_inputs_symbolic(inputs, ctx)} -> {option_symbolic(outcome, ctx)};"
    if k == "probe":
        return _render_probe(a, ctx, english)
    if k == "ask":
        where, example, options = g("where"), g("example"), g("options")
        if english:
            lead = f"where {show(where, ctx, True, law=True, want='Bool')}, for example " if where is not None else ""
            opts = " or ".join(option_english(o, ctx) for o in options)
            return f"Question {i}: {lead}when {_inputs_english(example, ctx)}, should the core {opts}?"
        lead = f" where {show(where, ctx, False, law=True, want='Bool')}" if where is not None else ""
        return f"ask {i}{lead} at {_inputs_symbolic(example, ctx)}: " + \
            " | ".join(option_symbolic(o, ctx) for o in options) + ";"
    if k == "answer":
        choice = g("choice")
        if choice == "any":
            return f"Answer {i}: any offered option is acceptable." if english else f"answer {i}: any;"
        if choice == "none":
            return f"Answer {i}: none of the offered options." if english else f"answer {i}: none;"
        return f"Answer {i}: {option_english(choice, ctx)}." if english else \
            f"answer {i}: {option_symbolic(choice, ctx)};"
    if k == "accepted":
        return f"Accepted {i}: equal on all {g('tuples')} inputs, with {g('nodes')} nodes." if english else \
            f"accepted {i}: {g('tuples')} tuples, {g('nodes')} nodes;"
    if k == "refuted":
        if english:
            return (f"Refuted {i}: when {_inputs_english(g('example'), ctx)}, {g('output')} should be "
                    f"{g('expected')} but is {g('got')}.")
        return f"refuted {i} at {_inputs_symbolic(g('example'), ctx)}: {g('output')} expected {g('expected')} got {g('got')};"
    if k == "region":
        where = g("where")
        if english:
            tail = f", exactly where {show(where, ctx, True, want='Bool')}" if where is not None else ""
            return f"Region {i}: wrong on {g('count')} of {g('of')} inputs{tail}."
        tail = f" where {show(where, ctx, False, want='Bool')}" if where is not None else ""
        return f"region {i}: {g('count')} of {g('of')}{tail};"
    if k == "site":
        before, after = show(g("before"), ctx, english), show(g("after"), ctx, english)
        return f"Site {i}: in {g('output')}, replacing {before} with {after} repairs it." if english else \
            f"site {i} {g('output')}: {before} -> {after};"
    if k == "completed":
        holes = g("holes")
        if english:
            return f"Completed {i}: " + "; ".join(f"hole {h} is {show(v, ctx, True)}" for h, v in holes) + "."
        return f"completed {i}: " + "; ".join(f"?{h} = {show(v, ctx, False)}" for h, v in holes) + ";"
    if k == "unrealizable":
        if english:
            return f"Unrealizable {i}: no completion of {g('output')} is right when {_inputs_english(g('example'), ctx)}."
        return f"unrealizable {i} {g('output')} at {_inputs_symbolic(g('example'), ctx)};"
    if k == "fact":
        return f"Fact {i}: {g('text')}." if english else f"fact {i}: {g('text')};"
    if k == "same":
        if english:
            return (f"Same {i}: it means exactly what {g('as')} means on all {g('count')} {g('over')}, judged "
                    f"{g('verdict')} at event {g('event')}.")
        return f"same {i} as {g('as')}: {g('count')} {g('over')}, event {g('event')}, {g('verdict')};"
    rendered = _render_structure(a, ctx, english)
    if rendered is not None:
        return rendered
    raise Refusal(f"zal-unknown-act:{k}")


def _names_en(names: tuple) -> str:
    return " and ".join(names)


def _render_structure(a: Act, ctx: Context, english: bool):
    """The decompose and teach-back forms."""
    k, i, g = a.kind, a.id, a.get
    if k == "component":
        parts = []
        for clause in ("owns", "reads", "decides"):
            if g(clause):
                parts.append(f"{clause} " + (_names_en(g(clause)) if english else ", ".join(g(clause))))
        if english:
            return f"Component {i} of {g('of')}: " + "; ".join(parts) + "."
        return f"component {i} of {g('of')}: " + "; ".join(parts) + ";"
    if k == "case":
        when = show(g("when"), ctx, english, want="Bool")
        if english:
            return f"Case {g('n')} of {i}: when {when}, {option_english(g('option'), ctx)}."
        return f"case {i} {g('n')}: when {when} -> {option_symbolic(g('option'), ctx)};"
    if k == "decompose":
        if english:
            return f"Decomposition {i} composes " + ", then ".join(g("order")) + "."
        return f"decompose {i}: " + ", ".join(g("order")) + ";"
    if k == "rule":
        where = show(g("where"), ctx, english, want="Bool")
        if english:
            return f"Rule {i}: where {where}, the core will {option_english(g('option'), ctx)}."
        return f"rule {i}: where {where} -> {option_symbolic(g('option'), ctx)};"
    if k == "decomposed":
        if english:
            return (f"Decomposed {i}: exact on all {g('tuples')} inputs, with {g('components')} components and "
                    f"{g('nodes')} nodes.")
        return f"decomposed {i}: {g('tuples')} tuples, {g('nodes')} nodes, {g('components')} components;"
    if k == "blame":
        where = g("where")
        if english:
            tail = f", exactly where {show(where, ctx, True, want='Bool')}" if where is not None else ""
            return f"Blame {i}: component {g('component')} is wrong on {g('count')} of the {g('of')} inputs it decides{tail}."
        tail = f" where {show(where, ctx, False, want='Bool')}" if where is not None else ""
        return f"blame {i} {g('component')}: {g('count')} of {g('of')}{tail};"
    if k == "expected":
        if english:
            return (f"Expected {i}: when {_inputs_english(g('example'), ctx)}, component {g('component')} should "
                    f"{option_english(g('required'), ctx)}, but it will {option_english(g('actual'), ctx)}.")
        return (f"expected {i} {g('component')} at {_inputs_symbolic(g('example'), ctx)}: "
                f"{option_symbolic(g('required'), ctx)} got {option_symbolic(g('actual'), ctx)};")
    if k == "interface":
        a_, b_ = g("pair")
        if english:
            return (f"Interface {i}: component {g('component')} cannot tell apart the inputs when "
                    f"{_inputs_english(a_, ctx)} and when {_inputs_english(b_, ctx)}; it must also read "
                    f"{_names_en(g('reads'))}.")
        return (f"interface {i} {g('component')} at {_inputs_symbolic(a_, ctx)} and {_inputs_symbolic(b_, ctx)}: "
                f"reads {', '.join(g('reads'))};")
    if k == "exact":
        return (f"Exact {i}: the core decides this on exactly the {g('count')} inputs where the rule applies."
                if english else f"exact {i}: {g('count')};")
    if k == "holds":
        return (f"Holds {i}: the core decides this on all {g('count')} inputs where the rule applies."
                if english else f"holds {i}: {g('count')};")
    if k == "exception":
        where = g("where")
        if english:
            tail = f", exactly where {show(where, ctx, True, want='Bool')}" if where is not None else ""
            return (f"Exception to {i}: on {g('count')} of its {g('of')} inputs{tail}, the core will "
                    f"{option_english(g('actual'), ctx)} instead, for example when {_inputs_english(g('example'), ctx)}.")
        tail = f" where {show(where, ctx, False, want='Bool')}" if where is not None else ""
        return (f"exception {i}: {g('count')} of {g('of')}{tail} at {_inputs_symbolic(g('example'), ctx)}: "
                f"{option_symbolic(g('actual'), ctx)};")
    if k == "missing":
        where = g("where")
        if english:
            tail = f", exactly where {show(where, ctx, True, want='Bool')}" if where is not None else ""
            return (f"Missing from {i}: the core also decides this on {g('count')} more inputs{tail}, for example when "
                    f"{_inputs_english(g('example'), ctx)}.")
        tail = f" where {show(where, ctx, False, want='Bool')}" if where is not None else ""
        return f"missing {i}: {g('count')}{tail} at {_inputs_symbolic(g('example'), ctx)};"
    return None


def _render_probe(a: Act, ctx: Context, english: bool) -> str:
    i, q = a.id, a.get("query")
    args = a.get("args")
    where = a.get("where")
    w = (f" where {show(where, ctx, english, want='Bool')}" if where is not None else "")
    if q == "explain":
        return f"Probe {i}: explain the decision when {_inputs_english(args, ctx)}." if english else \
            f"probe {i}: explain {_inputs_symbolic(args, ctx)};"
    if q == "freedom":
        return f"Probe {i}: where is the decision still open?" if english else f"probe {i}: freedom;"
    s = [show(x, ctx, english, want="Bool" if q in ("implies", "count", "reachable") else None)
         if isinstance(x, tuple) else x for x in args]
    if english and q in ("implies", "distinguish", "depends"):
        s = [f"({t})" if isinstance(x, tuple) and x[0] in ("bin", "if", "not") else t for t, x in zip(s, args)]
    if english:
        text = {"depends": lambda: f"does {s[0]} depend on {s[1]}{w}?",
                "values": lambda: f"which values does {s[0]} take{w}?",
                "distinguish": lambda: f"where does {s[0]} differ from {s[1]}{w}?",
                "implies": lambda: f"does {s[0]} imply {s[1]}?",
                "count": lambda: f"how many inputs satisfy {s[0]}?",
                "reachable": lambda: f"is a state where {s[0]} reachable?"}[q]()
        return f"Probe {i}: {text}"
    text = {"depends": lambda: f"depends {s[0]} on {s[1]}{w}", "values": lambda: f"values {s[0]}{w}",
            "distinguish": lambda: f"distinguish {s[0]}, {s[1]}{w}", "implies": lambda: f"implies {s[0]}, {s[1]}",
            "count": lambda: f"count {s[0]}", "reachable": lambda: f"reachable {s[0]}"}[q]()
    return f"probe {i}: {text};"


def _split_where(text: str):
    head, sep, tail = text.partition(" where ")
    return (head, tail) if sep else (text, None)


def _parse_probe(i: str, body: str, ctx: Context, english: bool) -> Act:
    body = body.strip()
    if english:
        m = re.fullmatch(r"explain the decision when (.+)", body)
        if m:
            return act("probe", i, query="explain", args=_parse_inputs(m.group(1), ctx, True, ctx.names))
        if not body.endswith("?"):
            raise Refusal("zal-probe-question-mark")
        if body == "where is the decision still open?":
            return act("probe", i, query="freedom", args=())
        body = body[:-1]
        for q, pattern in (("depends", r"does (.+) depend on ([a-z][a-z0-9_]*)(?: where (.+))?"),
                           ("values", r"which values does (.+) take(?: where (.+))?"),
                           ("distinguish", r"where does (.+) differ from (.+?)(?: where (.+))?"),
                           ("implies", r"does (.+) imply (.+)"), ("count", r"how many inputs satisfy (.+)"),
                           ("reachable", r"is a state where (.+) reachable")):
            m = re.fullmatch(pattern, body)
            if m:
                return _probe_act(i, q, m.groups(), ctx, True)
        raise Refusal("zal-probe-form")
    m = re.fullmatch(r"explain (.+)", body)
    if m:
        return act("probe", i, query="explain", args=_parse_inputs(m.group(1), ctx, False, ctx.names))
    if body == "freedom":
        return act("probe", i, query="freedom", args=())
    m = re.fullmatch(r"(depends|values|distinguish|implies|count|reachable) (.+)", body)
    if not m:
        raise Refusal("zal-probe-form")
    q, rest = m.groups()
    if q == "depends":
        mm = re.fullmatch(r"(.+?) on ([a-z][a-z0-9_]*)(?: where (.+))?", rest)
        groups = mm.groups() if mm else None
    elif q == "values":
        groups = _split_where(rest)
    elif q == "distinguish":
        head, where = _split_where(rest)
        a, sep, b = head.partition(",")
        groups = (a, b, where) if sep else None
    elif q == "implies":
        a, sep, b = rest.partition(",")
        groups = (a, b) if sep else None
    else:
        groups = (rest,)
    if groups is None:
        raise Refusal("zal-probe-form")
    return _probe_act(i, q, groups, ctx, False)


def _probe_act(i: str, q: str, groups: tuple, ctx: Context, english: bool) -> Act:
    groups = tuple(g.strip() if isinstance(g, str) else g for g in groups)
    if q == "depends":
        target, fld, where = groups
        if fld not in ctx.names:
            raise Refusal(f"zal-unknown-field:{fld}")
        args = (read(target, ctx, english), fld)
    elif q == "values":
        target, where = groups
        args = (read(target, ctx, english),)
    elif q == "distinguish":
        a, b, where = groups
        args = (read(a, ctx, english), read(b, ctx, english))
    elif q == "implies":
        args, where = (read(groups[0], ctx, english), read(groups[1], ctx, english)), None
    else:
        args, where = (read(groups[0], ctx, english),), None
    fields = {"query": q, "args": args}
    if where is not None:
        fields["where"] = read(where, ctx, english)
    return act("probe", i, **fields)


def parse_statement(line: str, ctx: Context, english: bool) -> Act:
    line = line.strip()
    if len(line) > MAX_LINE:
        raise Refusal("zal-statement-too-long")
    # BEGIN version-space act parsing
    import loop_versions
    if loop_versions.is_statement(line, english):
        version_act = loop_versions.parse_act(line, ctx, english)
        if version_act is not None:
            return version_act
    # END version-space act parsing
    # Additive capture syntax; existing acts retain their parsing order.
    import capture_language
    if ctx.tag == "capture" and capture_language.is_statement(line, english):
        from capture_language import parse as parse_capture
        capture = parse_capture(line, ctx, english)
        if capture is not None:
            return capture
    # BEGIN composable program loop parse hook (own prefixes only).
    if composable.claims(line, english):
        parsed = composable.parse(line, ctx, english)
        if parsed is None:
            raise Refusal("zal-composable-act-shape")
        return parsed
    # END composable program loop parse hook.
    # The extension parses both surfaces; unknown statements still fail below.
    if line.startswith(("learn ", "learn-", "Learn ")):
        import loop_learn
        learned = loop_learn.parse_statement(line, ctx, english)
        if learned is not None:
            return learned
    if english:
        return _parse_english_statement(line, ctx)
    if not line.endswith(";"):
        raise Refusal("zal-statement-terminator")
    body = line[:-1].strip()
    m = re.fullmatch(rf"(program|family) ({ID}) (out\d+) = (.+)", body)
    if m:
        return _propose(m.group(1), m.group(2), m.group(3), m.group(4), ctx, False)
    m = re.fullmatch(rf"law ({ID}) on ([a-z_ ]+?): (.+)", body)
    if m:
        return act("law", m.group(1), scope=_parse_scope(m.group(2), ctx, False),
                   formula=read(m.group(3), ctx, False, law=True))
    m = re.fullmatch(rf"invariant ({ID}): (.+)", body)
    if m:
        return act("invariant", m.group(1), formula=_state_formula(m.group(2), ctx, False))
    m = re.fullmatch(rf"scenario ({ID}) from (.+)", body)
    if m:
        start = () if m.group(2) == "genesis" else _parse_inputs(m.group(2), ctx, False, ctx.state)
        return act("scenario", m.group(1), start=start)
    m = re.fullmatch(rf"step ({ID}) (\d{{1,3}}): (.+?) -> (.+)", body)
    if m:
        return act("step", m.group(1), n=int(m.group(2)), inputs=_parse_inputs(m.group(3), ctx, False, ctx.inputs),
                   outcome=_parse_option(m.group(4), ctx, False))
    m = re.fullmatch(rf"probe ({ID}): (.+)", body)
    if m:
        return _parse_probe(m.group(1), m.group(2), ctx, False)
    m = re.fullmatch(rf"ask ({ID})(?: where (.+?))? at (.+?): (.+)", body)
    if m:
        where = read(m.group(2), ctx, False, law=True) if m.group(2) else None
        options = tuple(_parse_option(o, ctx, False) for o in m.group(4).split("|"))
        return _ask(m.group(1), where, _parse_inputs(m.group(3), ctx, False, ctx.names), options)
    m = re.fullmatch(rf"answer ({ID}): (.+)", body)
    if m:
        text = m.group(2).strip()
        choice = text if text in ("any", "none") else _parse_option(text, ctx, False)
        return act("answer", m.group(1), choice=choice)
    m = re.fullmatch(rf"component ({ID}) of ({ID}): (.+)", body)
    if m:
        return _component(m.group(1), m.group(2), [c.strip() for c in m.group(3).split(";")], ctx, False)
    m = re.fullmatch(rf"case ({ID}) (\d{{1,3}}): when (.+?) -> (accept\b.*|reject [a-z].*|failure [a-z].*)", body)
    if m:
        return _case(m.group(1), int(m.group(2)), m.group(3), m.group(4), ctx, False)
    m = re.fullmatch(rf"decompose ({ID}): ({ID}(?:, {ID})*)", body)
    if m:
        return _decompose(m.group(1), tuple(x.strip() for x in m.group(2).split(",")))
    m = re.fullmatch(rf"rule ({ID}): where (.+?) -> (accept\b.*|reject [a-z].*|failure [a-z].*)", body)
    if m:
        return _rule(m.group(1), m.group(2), m.group(3), ctx, False)
    return _parse_judge(body, ctx, False)


def _parse_english_statement(line: str, ctx: Context) -> Act:
    if line.endswith("?"):
        m = re.fullmatch(rf"Probe ({ID}): (.+\?)", line)
        if m:
            return _parse_probe(m.group(1), m.group(2), ctx, True)
        m = re.fullmatch(rf"Question ({ID}): (?:where (.+?), for example )?when (.+?), should the core (.+)\?", line)
        if m:
            where = read(m.group(2), ctx, True, law=True) if m.group(2) else None
            options = tuple(_parse_option(o, ctx, True) for o in _split_options(m.group(4)))
            return _ask(m.group(1), where, _parse_inputs(m.group(3), ctx, True, ctx.names), options)
        raise Refusal("zal-unknown-statement")
    if not line.endswith("."):
        raise Refusal("zal-statement-terminator")
    body = line[:-1].strip()
    m = re.fullmatch(rf"(Program|Family program) ({ID}) computes (out\d+) as (.+)", body)
    if m:
        return _propose("program" if m.group(1) == "Program" else "family", m.group(2), m.group(3), m.group(4),
                        ctx, True)
    m = re.fullmatch(rf"Law ({ID}) on ([a-z_ ]+?): (.+)", body)
    if m:
        return act("law", m.group(1), scope=_parse_scope(m.group(2), ctx, True),
                   formula=read(m.group(3), ctx, True, law=True))
    m = re.fullmatch(rf"Invariant ({ID}): (.+)", body)
    if m:
        return act("invariant", m.group(1), formula=_state_formula(m.group(2), ctx, True))
    m = re.fullmatch(rf"Scenario ({ID}) starts from genesis", body)
    if m:
        return act("scenario", m.group(1), start=())
    m = re.fullmatch(rf"Scenario ({ID}) starts where (.+)", body)
    if m:
        return act("scenario", m.group(1), start=_parse_inputs(m.group(2), ctx, True, ctx.state))
    m = re.fullmatch(rf"Step (\d{{1,3}}) of ({ID}): when (.+?), (accept.*|reject because .+|fail and commit because .+|"
                     r"be refused(?: by the Authority)?)", body)
    if m:
        return act("step", m.group(2), n=int(m.group(1)), inputs=_parse_inputs(m.group(3), ctx, True, ctx.inputs),
                   outcome=_parse_option(m.group(4), ctx, True))
    m = re.fullmatch(rf"Probe ({ID}): (explain the decision when .+)", body)
    if m:
        return _parse_probe(m.group(1), m.group(2), ctx, True)
    m = re.fullmatch(rf"Answer ({ID}): (.+)", body)
    if m:
        text = m.group(2).strip()
        if text == "any offered option is acceptable":
            choice = "any"
        elif text == "none of the offered options":
            choice = "none"
        else:
            choice = _parse_option(text, ctx, True)
        return act("answer", m.group(1), choice=choice)
    m = re.fullmatch(rf"Component ({ID}) of ({ID}): (.+)", body)
    if m:
        return _component(m.group(1), m.group(2), [c.strip() for c in m.group(3).split(";")], ctx, True)
    m = re.fullmatch(rf"Case (\d{{1,3}}) of ({ID}): when (.+?), (accept\b.*|reject because .+|fail and commit because .+)",
                     body)
    if m:
        return _case(m.group(2), int(m.group(1)), m.group(3), m.group(4), ctx, True)
    m = re.fullmatch(rf"Decomposition ({ID}) composes ({ID}(?:, then {ID})*)", body)
    if m:
        return _decompose(m.group(1), tuple(x.strip() for x in m.group(2).split(", then ")))
    m = re.fullmatch(rf"Rule ({ID}): where (.+?), the core will (accept\b.*|reject because .+|fail and commit because .+)",
                     body)
    if m:
        return _rule(m.group(1), m.group(2), m.group(3), ctx, True)
    return _parse_judge(body, ctx, True)


def _component(i: str, of: str, clauses: list, ctx: Context, english: bool) -> Act:
    fields: dict = {}
    order = ["owns", "reads", "decides"]
    last = -1
    for clause in clauses:
        m = re.fullmatch(r"(owns|reads|decides) (.+)", clause)
        if not m or m.group(1) in fields or order.index(m.group(1)) < last:
            raise Refusal("zal-component-clauses")
        last = order.index(m.group(1))
        names = tuple(x.strip() for x in (m.group(2).split(" and ") if english else m.group(2).split(",")))
        if len(set(names)) != len(names) or not all(re.fullmatch(ID, x) for x in names):
            raise Refusal("zal-component-names")
        if m.group(1) == "decides":
            unknown = [x for x in names if x not in ctx.reasons]
        else:
            unknown = [x for x in names if x not in ctx.names]
        if unknown:
            raise Refusal(f"zal-component-unknown:{unknown[0]}")
        if m.group(1) == "owns" and any(x not in ctx.state for x in names):
            raise Refusal("zal-component-owns-state-fields-only")
        fields[m.group(1)] = names
    if not fields.get("owns") and not fields.get("reads"):
        raise Refusal("zal-component-needs-owns-or-reads")
    return act("component", i, of=of, owns=fields.get("owns", ()), reads=fields.get("reads", ()),
               decides=fields.get("decides", ()))


def _case(i: str, n: int, when: str, option: str, ctx: Context, english: bool) -> Act:
    node = read(when, ctx, english)
    if params_of(node) and ctx.family is None:
        raise Refusal("zal-parameters-need-a-family-subject")
    o = _parse_option(option, ctx, english)
    if o.cls in ("Refused", "NoCase"):
        raise Refusal("zal-case-decides-accept-reject-or-failure")
    return act("case", i, n=n, when=node, option=o)


def _decompose(i: str, order: tuple) -> Act:
    if len(set(order)) != len(order):
        raise Refusal("zal-decompose-repeats-a-component")
    return act("decompose", i, order=order)


def _rule(i: str, where: str, option: str, ctx: Context, english: bool) -> Act:
    node = read(where, ctx, english)
    o = _parse_option(option, ctx, english)
    if (params_of(node) or any(params_of(v) for _, v in o.sets)) and ctx.family is None:
        raise Refusal("zal-parameters-need-a-family-subject")
    if o.cls in ("Refused", "NoCase"):
        raise Refusal("zal-rule-decides-accept-reject-or-failure")
    return act("rule", i, where=node, option=o)


def _split_options(text: str) -> list:
    parts = re.split(r" or (?=accept\b|reject because\b|fail and commit because\b|be refused\b)", text)
    return [p.strip() for p in parts]


def _parse_judge(body: str, ctx: Context, english: bool) -> Act:
    if english:
        forms = [
            (rf"Accepted ({ID}): equal on all (\d+) inputs, with (\d+) nodes",
             lambda m: act("accepted", m.group(1), tuples=int(m.group(2)), nodes=int(m.group(3)))),
            (rf"Refuted ({ID}): when (.+?), (out\d+) should be (-?\d+) but is (-?\d+|failure)",
             lambda m: act("refuted", m.group(1), example=_parse_inputs(m.group(2), ctx, True, ctx.names),
                           output=m.group(3), expected=int(m.group(4)), got=_int_or(m.group(5)))),
            (rf"Region ({ID}): wrong on (\d+) of (\d+) inputs(?:, exactly where (.+))?",
             lambda m: act("region", m.group(1), count=int(m.group(2)), of=int(m.group(3)),
                           where=read(m.group(4), ctx, True) if m.group(4) else None)),
            (rf"Site ({ID}): in (out\d+), replacing (.+) with (.+?) repairs it",
             lambda m: act("site", m.group(1), output=m.group(2), before=read(m.group(3), ctx, True),
                           after=read(m.group(4), ctx, True))),
            (rf"Completed ({ID}): (.+)",
             lambda m: act("completed", m.group(1), holes=_parse_holes(m.group(2), ctx, True))),
            (rf"Unrealizable ({ID}): no completion of (out\d+) is right when (.+)",
             lambda m: act("unrealizable", m.group(1), output=m.group(2),
                           example=_parse_inputs(m.group(3), ctx, True, ctx.names))),
            (rf"Same ({ID}): it means exactly what ({ID}) means on all (\d+) (inputs|states), judged ({VERDICT}) "
             rf"at event (\d+)",
             lambda m: act("same", m.group(1), **{"as": m.group(2)}, count=int(m.group(3)), over=m.group(4),
                           verdict=m.group(5), event=int(m.group(6)))),
            (rf"Fact ({ID}): (.+)", lambda m: act("fact", m.group(1), text=_text(m.group(2)))),
            (rf"Decomposed ({ID}): exact on all (\d+) inputs, with (\d+) components and (\d+) nodes",
             lambda m: act("decomposed", m.group(1), tuples=int(m.group(2)), components=int(m.group(3)),
                           nodes=int(m.group(4)))),
            (rf"Blame ({ID}): component ({ID}) is wrong on (\d+) of the (\d+) inputs it decides(?:, exactly where (.+))?",
             lambda m: act("blame", m.group(1), component=m.group(2), count=int(m.group(3)), of=int(m.group(4)),
                           where=read(m.group(5), ctx, True) if m.group(5) else None)),
            (rf"Expected ({ID}): when (.+?), component ({ID}) should (.+), but it will (.+)",
             lambda m: act("expected", m.group(1), component=m.group(3),
                           example=_parse_inputs(m.group(2), ctx, True, ctx.names),
                           required=_parse_option(m.group(4), ctx, True), actual=_parse_option(m.group(5), ctx, True))),
            (rf"Interface ({ID}): component ({ID}) cannot tell apart the inputs when (.+?) and when (.+?); "
             rf"it must also read ({ID}(?: and {ID})*)",
             lambda m: act("interface", m.group(1), component=m.group(2),
                           pair=(_parse_inputs(m.group(3), ctx, True, ctx.names),
                                 _parse_inputs(m.group(4), ctx, True, ctx.names)),
                           reads=_field_list(m.group(5).split(" and "), ctx))),
            (rf"Exact ({ID}): the core decides this on exactly the (\d+) inputs where the rule applies",
             lambda m: act("exact", m.group(1), count=int(m.group(2)))),
            (rf"Holds ({ID}): the core decides this on all (\d+) inputs where the rule applies",
             lambda m: act("holds", m.group(1), count=int(m.group(2)))),
            (rf"Exception to ({ID}): on (\d+) of its (\d+) inputs(?:, exactly where (.+?))?, the core will (.+) instead, "
             rf"for example when (.+)",
             lambda m: act("exception", m.group(1), count=int(m.group(2)), of=int(m.group(3)),
                           where=read(m.group(4), ctx, True) if m.group(4) else None,
                           actual=_parse_option(m.group(5), ctx, True),
                           example=_parse_inputs(m.group(6), ctx, True, ctx.names))),
            (rf"Missing from ({ID}): the core also decides this on (\d+) more inputs(?:, exactly where (.+?))?, "
             rf"for example when (.+)",
             lambda m: act("missing", m.group(1), count=int(m.group(2)),
                           where=read(m.group(3), ctx, True) if m.group(3) else None,
                           example=_parse_inputs(m.group(4), ctx, True, ctx.names))),
        ]
    else:
        forms = [
            (rf"accepted ({ID}): (\d+) tuples, (\d+) nodes",
             lambda m: act("accepted", m.group(1), tuples=int(m.group(2)), nodes=int(m.group(3)))),
            (rf"refuted ({ID}) at (.+?): (out\d+) expected (-?\d+) got (-?\d+|failure)",
             lambda m: act("refuted", m.group(1), example=_parse_inputs(m.group(2), ctx, False, ctx.names),
                           output=m.group(3), expected=int(m.group(4)), got=_int_or(m.group(5)))),
            (rf"region ({ID}): (\d+) of (\d+)(?: where (.+))?",
             lambda m: act("region", m.group(1), count=int(m.group(2)), of=int(m.group(3)),
                           where=read(m.group(4), ctx, False) if m.group(4) else None)),
            (rf"site ({ID}) (out\d+): (.+) -> (.+)",
             lambda m: act("site", m.group(1), output=m.group(2), before=read(m.group(3), ctx, False),
                           after=read(m.group(4), ctx, False))),
            (rf"completed ({ID}): (.+)",
             lambda m: act("completed", m.group(1), holes=_parse_holes(m.group(2), ctx, False))),
            (rf"unrealizable ({ID}) (out\d+) at (.+)",
             lambda m: act("unrealizable", m.group(1), output=m.group(2),
                           example=_parse_inputs(m.group(3), ctx, False, ctx.names))),
            (rf"same ({ID}) as ({ID}): (\d+) (inputs|states), event (\d+), ({VERDICT})",
             lambda m: act("same", m.group(1), **{"as": m.group(2)}, count=int(m.group(3)), over=m.group(4),
                           event=int(m.group(5)), verdict=m.group(6))),
            (rf"fact ({ID}): (.+)", lambda m: act("fact", m.group(1), text=_text(m.group(2)))),
            (rf"decomposed ({ID}): (\d+) tuples, (\d+) nodes, (\d+) components",
             lambda m: act("decomposed", m.group(1), tuples=int(m.group(2)), nodes=int(m.group(3)),
                           components=int(m.group(4)))),
            (rf"blame ({ID}) ({ID}): (\d+) of (\d+)(?: where (.+))?",
             lambda m: act("blame", m.group(1), component=m.group(2), count=int(m.group(3)), of=int(m.group(4)),
                           where=read(m.group(5), ctx, False) if m.group(5) else None)),
            (rf"expected ({ID}) ({ID}) at (.+?): (.+) got (.+)",
             lambda m: act("expected", m.group(1), component=m.group(2),
                           example=_parse_inputs(m.group(3), ctx, False, ctx.names),
                           required=_parse_option(m.group(4), ctx, False), actual=_parse_option(m.group(5), ctx, False))),
            (rf"interface ({ID}) ({ID}) at (.+?) and (.+?): reads ({ID}(?:, {ID})*)",
             lambda m: act("interface", m.group(1), component=m.group(2),
                           pair=(_parse_inputs(m.group(3), ctx, False, ctx.names),
                                 _parse_inputs(m.group(4), ctx, False, ctx.names)),
                           reads=_field_list(m.group(5).split(","), ctx))),
            (rf"exact ({ID}): (\d+)", lambda m: act("exact", m.group(1), count=int(m.group(2)))),
            (rf"holds ({ID}): (\d+)", lambda m: act("holds", m.group(1), count=int(m.group(2)))),
            (rf"exception ({ID}): (\d+) of (\d+)(?: where (.+?))? at (.+?): (.+)",
             lambda m: act("exception", m.group(1), count=int(m.group(2)), of=int(m.group(3)),
                           where=read(m.group(4), ctx, False) if m.group(4) else None,
                           example=_parse_inputs(m.group(5), ctx, False, ctx.names),
                           actual=_parse_option(m.group(6), ctx, False))),
            (rf"missing ({ID}): (\d+)(?: where (.+?))? at (.+)",
             lambda m: act("missing", m.group(1), count=int(m.group(2)),
                           where=read(m.group(3), ctx, False) if m.group(3) else None,
                           example=_parse_inputs(m.group(4), ctx, False, ctx.names))),
        ]
    for pattern, build in forms:
        m = re.fullmatch(pattern, body)
        if m:
            return build(m)
    raise Refusal("zal-unknown-statement")


def _field_list(names: list, ctx: Context) -> tuple:
    out = tuple(x.strip() for x in names)
    unknown = [x for x in out if x not in ctx.names]
    if unknown or not out:
        raise Refusal(f"zal-unknown-field:{unknown[0] if unknown else ''}")
    return out


def _int_or(text: str):
    return text if text == "failure" else int(text)


def _text(text: str) -> str:
    if not TEXT.fullmatch(text):
        raise Refusal("zal-fact-text")
    return text


def _parse_holes(text: str, ctx: Context, english: bool) -> tuple:
    out = []
    for part in text.split(";"):
        m = re.fullmatch(r"\s*hole (\d+) is (.+?)\s*" if english else r"\s*\?(\d+) = (.+?)\s*", part)
        if not m:
            raise Refusal("zal-completed-holes")
        out.append((int(m.group(1)), read(m.group(2), ctx, english)))
    return tuple(out)


def _propose(kind: str, i: str, out: str, text: str, ctx: Context, english: bool) -> Act:
    if out not in ctx.outputs:
        raise Refusal(f"zal-unknown-output:{out}")
    node = read(text, ctx, english, allow_keep=True)
    if kind == "program" and params_of(node):
        raise Refusal("zal-parameters-need-a-family-program")
    if kind == "family" and ctx.family is None:
        raise Refusal("zal-family-program-needs-a-family-subject")
    return act(kind, i, output=out, expr=node)


def _state_formula(text: str, ctx: Context, english: bool):
    node = read(text, ctx, english)
    extra = names_of(node) - set(ctx.state)
    if extra:
        raise Refusal(f"zal-invariant-reads-non-state-names:{sorted(extra)[0]}")
    return node


def _ask(i: str, where, example: tuple, options: tuple) -> Act:
    if len(options) < 2 or len(set(options)) != len(options):
        raise Refusal("zal-question-needs-two-distinct-options")
    return act("ask", i, where=where, example=example, options=options)


# --- documents ------------------------------------------------------------------------------------------

def subject_line(ref: tuple, english: bool) -> str:
    if ref[0] == "family":
        return f"The subject is the family {ref[1]}." if english else f"subject family {ref[1]};"
    return f"The subject is {ref[1]} at revision {ref[2]}." if english else f"subject {ref[1]} at {ref[2]};"


def render(doc: Document, ctx: Context, english: bool = False) -> str:
    lines = [HEADER["english" if english else "symbolic"], subject_line(doc.subject, english),
             f"The author is {AUTHORS[doc.author]}." if english else f"author {doc.author};"]
    lines += [render_act(a, ctx, english) for a in doc.acts]
    return "\n".join(lines) + "\n"


def parse(source: str, ctx: Context) -> Document:
    """Parse a document in either surface against the workspace's context."""
    if type(source) is not str or len(source.encode("utf-8")) > MAX_SOURCE or not source.isascii():
        raise Refusal("zal-source-must-be-ascii-and-bounded")
    lines = [ln.strip() for ln in source.splitlines() if ln.strip() and not ln.lstrip().startswith("#")]
    if len(lines) < 3:
        raise Refusal("zal-document-needs-header-subject-and-author")
    if lines[0] == HEADER["symbolic"]:
        english = False
    elif lines[0] == HEADER["english"]:
        english = True
    else:
        raise Refusal("zal-header")
    if len(lines) - 3 > MAX_STATEMENTS:
        raise Refusal("zal-too-many-statements")
    subject = _parse_subject(lines[1], english)
    if subject != ctx.subject_ref:
        if subject[0] == "core" and ctx.subject_ref[0] == "core" and subject[1] == ctx.tag:
            raise Refusal("zal-stale-subject-revision")
        raise Refusal("zal-document-subject-differs-from-the-workspace")
    author = _parse_author(lines[2], english)
    acts = tuple(parse_statement(line, ctx, english) for line in lines[3:])
    return Document(subject, author, acts)


def _parse_subject(line: str, english: bool) -> tuple:
    if english:
        m = re.fullmatch(rf"The subject is the family ({TAG})\.", line)
        if m:
            return ("family", m.group(1))
        m = re.fullmatch(rf"The subject is ({TAG}) at revision ([0-9a-f]{{64}})\.", line)
    else:
        m = re.fullmatch(rf"subject family ({TAG});", line)
        if m:
            return ("family", m.group(1))
        m = re.fullmatch(rf"subject ({TAG}) at ([0-9a-f]{{64}});", line)
    if not m:
        raise Refusal("zal-subject")
    return ("core", m.group(1), m.group(2))


def _parse_author(line: str, english: bool) -> str:
    for name, text in AUTHORS.items():
        if line == (f"The author is {text}." if english else f"author {name};"):
            return name
    raise Refusal("zal-author")


def legend() -> str:
    """The paired statement forms, for people and models (no model is consulted)."""
    from capture_language import LEGEND
    base = __doc__.split("Statement forms", 1)[1].split("A component's clauses", 1)[0].strip() + "\n\nRequirements-only acts: versions ID; version-count ID: N; " \
        "version-question ID at [inputs]: [outputs] -> N | [outputs] -> N; " \
        "version-suggest ID: [outputs]; version-answer ID: [outputs]; " \
        "version-determined ID: SHA256 on N inputs; version-conflict ID: [requirements]; " \
        "version-stop ID; version-stopped ID;. Each also has a Version ... controlled-English form." + "\n" + LEGEND
    # Fixed-vocabulary learn acts; these do not add an owner tool to MCP.
    return base + "\n\n" + (
        "learn ID; | Learn ID.\n"
        "learn-suggest ID: [OUTPUT]; | Learn suggestion ID: [OUTPUT] (advisory).\n"
        "learn-answer ID: [OUTPUT]; | Learn answer ID: [OUTPUT]. (owner only)\n"
        "learn-question ID: [INPUT] remaining [COUNTS]; | Learn question ID: when [INPUT], [COUNTS] meanings remain (proposal).\n"
        "learn-determined ID: SHA on N examples; | Learn determined ID: program SHA reproduces N examples; uniqueness is a vocabulary proposal.\n"
        "learn-outside ID: [EXAMPLES]; | Learn outside ID: examples [EXAMPLES] exclude this bounded vocabulary (proposal).\n"
        "learn-ambiguous ID: [COUNTS]; | Learn ambiguous ID: [COUNTS] meanings remain (proposal).\n"
        "learn-inconclusive ID; | Learn inconclusive ID: no completed enumeration."
    )
