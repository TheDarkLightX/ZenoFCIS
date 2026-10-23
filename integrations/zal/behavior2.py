"""Experimental ZAL finite-FSM profile 2: bounded integers, requirements and temporal properties.

No eval, model calls, I/O, authority capabilities or production effect execution.
The symbolic and controlled-English parsers build exactly the same value.
Profile 1 keeps its canonical model; parser source-position helpers are shared.
"""

from __future__ import annotations

from collections import deque
from dataclasses import dataclass
import hashlib
import itertools
import re
from types import MappingProxyType

from behavior import FUTURE, NAME, Refusal, at_source, canonical, grammar_refusal, source_lines


PROFILE = "zal/finite-fsm/2"
MAX_SOURCE = 64 * 1024
MAX_STATEMENTS = 96
MAX_LITERAL = 32767
MAX_RANGE = 64
MAX_CONFIGURATIONS = 4096
MAX_TUPLES = 16384
MAX_WITHIN = 64
MAX_TRACE = 64
INTEGER = r"-?[0-9]{1,5}"
COMPARISONS = ("==", "!=", "<", "<=", ">", ">=")
# Words of the English formula phrases are reserved so that a declared name can
# never be read as part of an operator phrase.
RESERVED = frozenset({
    "none", "true", "false", "state", "not", "and", "or", "is", "default", "event", "accepted",
    "always", "never", "eventually", "next", "within", "until", "if", "then", "only", "equals",
    "does", "equal", "less", "than", "greater", "at", "most", "least", "plus", "minus", "after",
    "this", "now", "the", "events", "which", "must", "exist", "holds", "happens", "that", "happen",
    "infinitely", "often", "assume", "with", "set", "to", "by", "increased", "decreased"})
TEMPORAL_SYMBOLIC = {"always": "always({a})", "never": "never({a})", "eventually": "eventually({a})",
                     "next": "next({a})", "within": "within({k}, {a})", "until": "until({a}, {b})"}
# The intuitive-rung templates of zal/ladder-temporal/1, so one statement reads alike in both.
TEMPORAL_ENGLISH = {"always": "always ({a})", "never": "never ({a})", "eventually": "eventually ({a})",
                    "next": "at the next event, which must exist, ({a})",
                    "within": "now or within the next {k} events, ({a})",
                    "until": "({a}) holds until ({b}) happens, and that must happen"}
CONNECTIVES = {"and": (" & ", " and "), "or": (" | ", " or ")}
BINARY_TEMPLATES = {"implies": ("({a} => {b})", "(if {a}, then {b})"),
                    "iff": ("({a} <=> {b})", "({a} if and only if {b})")}
COMPARISON_ENGLISH = {"==": "equals", "!=": "does not equal", "<": "is less than",
                      "<=": "is at most", ">": "is greater than", ">=": "is at least"}
UPDATE_ENGLISH = {"=": "set to", "+=": "increased by", "-=": "decreased by"}
BOOLEAN_OPS = {"true", "false", "accepted", "fact", "state", "state_next", "event",
               "not", "and", "or", "implies", "iff", "cmp"}
TERM_OPS = {"int", "var", "var_next", "input", "add", "sub"}
TEMPORAL_OPS = set(TEMPORAL_SYMBOLIC)
ATOMIC_TERMS = {"int", "var", "var_next", "input"}
GAME_OBJECTIVES = ("eventually", "always", "infinitely often")
SCOPES = {
    "guard": ({"true", "false", "fact", "state", "not", "and", "or", "implies", "iff", "cmp"},
              {"int", "var", "input", "add", "sub"}),
    "invariant": ({"true", "false", "state", "not", "and", "or", "implies", "iff", "cmp"},
                  {"int", "var", "add", "sub"}),
    "step": (BOOLEAN_OPS, TERM_OPS),
}


@dataclass(frozen=True)
class Expr:
    """One owned typed node: a Boolean formula, an integer term or a temporal operator."""
    op: str
    args: tuple = ()

    def __post_init__(self):
        if not isinstance(self.args, tuple) or self.op not in BOOLEAN_OPS | TERM_OPS | TEMPORAL_OPS:
            raise Refusal("Unknown or mutable formula node")
        args, op = self.args, self.op
        if op in {"true", "false", "accepted"}:
            ok = args == ()
        elif op in {"fact", "state", "state_next", "event", "var", "var_next", "input"}:
            ok = len(args) == 1 and isinstance(args[0], str) and re.fullmatch(NAME, args[0]) is not None
        elif op == "int":
            ok = len(args) == 1 and type(args[0]) is int and abs(args[0]) <= MAX_LITERAL
        elif op in {"not", "always", "never", "eventually", "next"}:
            ok = len(args) == 1 and _boolean(args[0])
        elif op in {"and", "or"}:
            ok = len(args) >= 2 and all(_boolean(a) for a in args)
        elif op in {"implies", "iff", "until"}:
            ok = len(args) == 2 and all(_boolean(a) for a in args)
        elif op == "within":
            ok = (len(args) == 2 and type(args[0]) is int and 0 <= args[0] <= MAX_WITHIN
                  and _boolean(args[1]))
        elif op == "cmp":
            ok = len(args) == 3 and args[0] in COMPARISONS and all(_term(a) for a in args[1:])
        else:
            # Sums are left-nested: the right operand of + and - is one atomic term.
            ok = len(args) == 2 and _term(args[0]) and _term(args[1]) and args[1].op in ATOMIC_TERMS
        if not ok:
            raise Refusal("Wrong formula arity or operand type")

    def data(self):
        return [self.op, *[a.data() if isinstance(a, Expr) else a for a in self.args]]

    def render(self, english=False) -> str:
        op, args = self.op, self.args
        if op in {"true", "false", "accepted"}:
            return op
        if op in {"fact", "var", "input"}:
            return args[0]
        if op == "int":
            return str(args[0])
        if op == "var_next":
            return args[0] + (" after this event" if english else "'")
        if op in {"state", "state_next", "event"}:
            subject = {"state": "state", "state_next": "state after this event" if english else "state'",
                       "event": "event"}[op]
            return f"{subject} {'is' if english else '=='} {args[0]}"
        if op == "not":
            return f"({'not ' if english else '!'}{args[0].render(english)})"
        if op in CONNECTIVES:
            return "(" + CONNECTIVES[op][english].join(a.render(english) for a in args) + ")"
        if op in BINARY_TEMPLATES:
            return BINARY_TEMPLATES[op][english].format(a=args[0].render(english), b=args[1].render(english))
        if op == "cmp":
            symbol = COMPARISON_ENGLISH[args[0]] if english else args[0]
            return f"{args[1].render(english)} {symbol} {args[2].render(english)}"
        if op in {"add", "sub"}:
            symbol = ("plus" if op == "add" else "minus") if english else ("+" if op == "add" else "-")
            return f"{args[0].render(english)} {symbol} {args[1].render(english)}"
        template = (TEMPORAL_ENGLISH if english else TEMPORAL_SYMBOLIC)[op]
        if op == "within":
            return template.format(k=args[0], a=_bare(args[1].render(english)))
        return template.format(a=_bare(args[0].render(english)),
                               b=_bare(args[1].render(english)) if op == "until" else "")


def _boolean(node):
    return isinstance(node, Expr) and node.op in BOOLEAN_OPS | TEMPORAL_OPS


def _term(node):
    return isinstance(node, Expr) and node.op in TERM_OPS


def _bare(text):
    """Drop one pair of parentheses that encloses the whole text; templates supply their own."""
    if text.startswith("("):
        depth = 0
        for index, char in enumerate(text):
            depth += (char == "(") - (char == ")")
            if depth == 0:
                return text[1:-1] if index == len(text) - 1 else text
    return text


def _temporal(node):
    return isinstance(node, Expr) and (node.op in TEMPORAL_OPS or any(_temporal(a) for a in node.args))


def combine(op: str, args: list[Expr]) -> Expr:
    flat = []
    for arg in args:
        flat.extend(arg.args if arg.op == op else [arg])
    # Boolean conjunction/disjunction are total; arithmetic keeps its written order.
    unique = {canonical(arg.data()): arg for arg in flat}
    values = tuple(unique[key] for key in sorted(unique))
    return values[0] if len(values) == 1 else Expr(op, values)


TRUE = Expr("true")


class _Parser:
    """Recursive descent over one formula; English words are matched only in their phrases."""

    def __init__(self, text, declarations, english):
        if not isinstance(text, str) or len(text) > 4096:
            raise Refusal("Formula exceeds 4096 characters")
        self.tokens = re.findall(r"<=>|=>|==|!=|<=|>=|[!&|()<>,+'-]|[0-9]+|[a-z][a-z0-9_]*|\S", text)
        if len(self.tokens) > 1024:
            raise Refusal("Formula exceeds 1024 lexical tokens")
        self.index, self.english, self.decl = 0, english, declarations

    def peek(self, offset=0):
        position = self.index + offset
        return self.tokens[position] if position < len(self.tokens) else None

    def at(self, *words):
        return tuple(self.tokens[self.index:self.index + len(words)]) == words

    def take(self, *words):
        if words and not self.at(*words):
            raise Refusal(f"Expected {' '.join(words)} near {self.peek() or 'the end of the formula'}")
        if self.index >= len(self.tokens):
            raise Refusal("Unexpected end of formula")
        count = max(len(words), 1)
        self.index += count
        return self.tokens[self.index - 1]

    def complete(self):
        result = self.formula(0)
        if self.index != len(self.tokens):
            raise Refusal(f"Unexpected formula token {self.tokens[self.index]}")
        return result

    def formula(self, depth):
        if depth > 64:
            raise Refusal("Formula nesting exceeds 64 levels")
        left = self.implication(depth)
        marker = ("if", "and", "only", "if") if self.english else ("<=>",)
        if self.at(*marker):
            self.take(*marker)
            right = self.implication(depth)
            if self.at(*marker):
                raise Refusal("Group chained equivalences with parentheses")
            return Expr("iff", (left, right))
        return left

    def implication(self, depth):
        if self.english and self.at("if"):
            self.take("if")
            premise = self.disjunction(depth)
            self.take(",", "then")
            return Expr("implies", (premise, self.disjunction(depth)))
        left = self.disjunction(depth)
        if not self.english and self.at("=>"):
            self.take("=>")
            right = self.disjunction(depth)
            if self.at("=>"):
                raise Refusal("Group chained implications with parentheses")
            return Expr("implies", (left, right))
        return left

    def disjunction(self, depth):
        terms = [self.conjunction(depth)]
        while self.at("or" if self.english else "|"):
            self.take()
            terms.append(self.conjunction(depth))
        return combine("or", terms)

    def conjunction(self, depth):
        terms = [self.unary(depth)]
        while self.at("and" if self.english else "&"):
            self.take()
            terms.append(self.unary(depth))
        return combine("and", terms)

    def unary(self, depth):
        if self.at("not" if self.english else "!"):
            self.take()
            return Expr("not", (self.unary(depth + 1),))
        return self.primary(depth + 1)

    def group(self, depth):
        self.take("(")
        inner = self.formula(depth + 1)
        self.take(")")
        return inner

    def primary(self, depth):
        token, english = self.peek(), self.english
        if token == "(":
            inner = self.group(depth)
            if english and self.at("holds", "until"):
                self.take("holds", "until")
                goal = self.group(depth)
                self.take("happens", ",", "and", "that", "must", "happen")
                return Expr("until", (inner, goal))
            return inner
        if token in {"true", "false", "accepted"}:
            self.take()
            return Expr(token)
        if token in {"state", "event"}:
            self.take()
            after = token == "state" and (self.at("after", "this", "event") if english else self.at("'"))
            if after:
                self.take(*(("after", "this", "event") if english else ("'",)))
            self.take("is" if english else "==")
            name = self.take()
            pool = self.decl["states"] if token == "state" else self.decl["events"]
            if name not in pool:
                raise Refusal(f"Unknown {token} {name}")
            return Expr("state_next" if after else token if token == "event" else "state", (name,))
        if token in {"always", "never", "eventually"} or token in {"next", "within", "until"} and not english:
            self.take()
            if token == "within":
                self.take("(")
                bound = self.bound()
                self.take(",")
                operand = self.formula(depth + 1)
                self.take(")")
                return Expr("within", (bound, operand))
            if token == "until":
                self.take("(")
                hold = self.formula(depth + 1)
                self.take(",")
                goal = self.formula(depth + 1)
                self.take(")")
                return Expr("until", (hold, goal))
            return Expr(token, (self.group(depth),))
        if english and self.at("at", "the", "next", "event", ",", "which", "must", "exist", ","):
            self.take("at", "the", "next", "event", ",", "which", "must", "exist", ",")
            return Expr("next", (self.group(depth),))
        if english and self.at("now", "or", "within", "the", "next"):
            self.take("now", "or", "within", "the", "next")
            bound = self.bound()
            self.take("events", ",")
            return Expr("within", (bound, self.group(depth)))
        if token in self.decl["facts"]:
            following = self.peek(1)
            if following in {"+", "-", *COMPARISONS} or english and following in {"plus", "minus", "equals", "does"}:
                raise Refusal(f"{token} is a Boolean observation, not an integer")
            self.take()
            return Expr("fact", (token,))
        left = self.term()
        for symbol, words in COMPARISON_ENGLISH.items():
            phrase = tuple(words.split()) if english else (symbol,)
            if self.at(*phrase):
                self.take(*phrase)
                return Expr("cmp", (symbol, left, self.term()))
        raise Refusal(f"Expected a comparison after {left.render(english)}")

    def bound(self):
        token = self.take()
        if not token.isdigit() or len(token) > 3 or int(token) > MAX_WITHIN:
            raise Refusal(f"within needs a literal bound from 0 to {MAX_WITHIN}")
        return int(token)

    def term(self):
        result = self.atom_term()
        while self.at("plus" if self.english else "+") or self.at("minus" if self.english else "-"):
            op = "add" if self.take() in {"plus", "+"} else "sub"
            result = Expr(op, (result, self.atom_term()))
        return result

    def atom_term(self):
        token = self.peek()
        if token is None:
            raise Refusal("Expected an integer term")
        negative = token == "-" and (self.peek(1) or "").isdigit()
        if negative:
            self.take()
            token = self.peek()
        if token.isdigit():
            self.take()
            if len(token) > 5 or int(token) > MAX_LITERAL:
                raise Refusal(f"Integer literals lie within -{MAX_LITERAL}..={MAX_LITERAL}")
            return Expr("int", (-int(token) if negative else int(token),))
        if token in self.decl["variables"]:
            self.take()
            if self.at(*(("after", "this", "event") if self.english else ("'",))):
                self.take(*(("after", "this", "event") if self.english else ("'",)))
                return Expr("var_next", (token,))
            return Expr("var", (token,))
        if token in self.decl["inputs"]:
            self.take()
            return Expr("input", (token,))
        if token in self.decl["facts"]:
            raise Refusal(f"{token} is a Boolean observation, not an integer")
        raise Refusal(f"Unknown observation, variable or input {token}")


def _measure(node):
    children = [a for a in node.args if isinstance(a, Expr)]
    sizes = [_measure(child) for child in children]
    return 1 + sum(n for n, _ in sizes), 1 + max([d for _, d in sizes], default=0)


def admit_formula(node, declarations, scope, depth=1):
    """Check scope, references and canonical form of one typed formula; return its node count."""
    if not isinstance(node, Expr) or depth > 24:
        raise Refusal("IR formula has an invalid type or depth")
    if scope == "property":
        shape = property_shape(node)
        operands = [shape["trigger"], shape["goal"], shape.get("hold")]
        return 1 + sum(admit_formula(x, declarations, "step") for x in operands if x is not None)
    boolean_ops, term_ops = SCOPES[scope]
    if node.op not in boolean_ops | term_ops:
        allowed = "guards" if scope == "guard" else "invariants" if scope == "invariant" else "step formulas"
        raise Refusal(f"{node.op} is not allowed in {allowed}")
    if node.op in {"and", "or"}:
        keys = [canonical(arg.data()) for arg in node.args]
        if any(arg.op == node.op for arg in node.args) or keys != sorted(set(keys)):
            raise Refusal("IR Boolean children must be flattened, distinct and canonically ordered")
    pools = {"fact": "facts", "state": "states", "state_next": "states", "event": "events",
             "var": "variables", "var_next": "variables", "input": "inputs"}
    if node.op in pools and node.args[0] not in declarations[pools[node.op]]:
        raise Refusal(f"IR formula has an unbound {pools[node.op][:-1]} reference {node.args[0]}")
    return 1 + sum(admit_formula(a, declarations, scope, depth + 1) for a in node.args if isinstance(a, Expr))


def parse_formula(text, declarations, scope, english=False) -> Expr:
    result = _Parser(text, declarations, english).complete()
    nodes, depth = _measure(result)
    if nodes > 128 or depth > 16 or any(len(result.render(e)) > 4096 for e in [False, True]):
        raise Refusal("Canonical formula exceeds 128 nodes, depth 16 or 4096 characters")
    admit_formula(result, declarations, scope)
    return result


def property_shape(node):
    """Name the supported temporal shape of a property, or refuse it by name."""
    def response(expr):
        return (isinstance(expr, Expr) and expr.op in {"next", "within", "eventually", "until"}
                and not any(_temporal(a) for a in expr.args if isinstance(a, Expr)))
    def shape(trigger, expr, initial):
        kind = {"next": "next", "within": "bounded", "eventually": "liveness", "until": "liveness"}[expr.op]
        hold = expr.args[0] if expr.op == "until" else TRUE if expr.op == "eventually" else None
        return {"kind": kind, "operator": expr.op, "trigger": trigger, "initial": initial,
                "bound": expr.args[0] if expr.op == "within" else None, "hold": hold, "goal": expr.args[-1]}
    if not isinstance(node, Expr):
        raise Refusal("A property needs a typed temporal formula")
    if node.op in {"always", "never"} and not _temporal(node.args[0]):
        goal = node.args[0] if node.op == "always" else Expr("not", (node.args[0],))
        return {"kind": "safety", "operator": node.op, "trigger": None, "initial": False,
                "bound": None, "hold": None, "goal": goal}
    if node.op == "always":
        body = node.args[0]
        if body.op == "implies" and not _temporal(body.args[0]) and response(body.args[1]):
            return shape(body.args[0], body.args[1], False)
        if response(body):
            return shape(TRUE, body, False)
    elif response(node):
        return shape(None, node, True)
    raise Refusal("Unsupported temporal shape; use always(P), never(P), always(P => T), always(T) or T, "
                  "where T is next, within, eventually or until over step formulas")


@dataclass(frozen=True)
class Variable:
    name: str
    low: int
    high: int
    initial: int

    def data(self):
        return [self.name, self.low, self.high, self.initial]


@dataclass(frozen=True)
class Input:
    name: str
    low: int
    high: int

    def data(self):
        return [self.name, self.low, self.high]


@dataclass(frozen=True)
class Update:
    variable: str
    op: str
    term: Expr

    def data(self):
        return [self.variable, self.op, self.term.data()]

    def render(self, english=False):
        if english:
            return f"{self.variable} {UPDATE_ENGLISH[self.op]} {self.term.render(True)}"
        return f"{self.variable} {self.op} {self.term.render()}"


@dataclass(frozen=True)
class Rule:
    id: str
    source: str
    event: str
    guard: Expr
    target: str | None
    reason: str | None
    updates: tuple[Update, ...] = ()

    def data(self):
        return {"id": self.id, "source": self.source, "event": self.event, "guard": self.guard.data(),
                "target": self.target, "reason": self.reason, "updates": [u.data() for u in self.updates]}


class _Step:
    """One step of the machine: pre-state, event, observations, outcome and post-state."""
    __slots__ = ("state", "values", "event", "context", "accepted", "post_state", "post_values",
                 "rule", "reason", "pre", "post")

    def __init__(self, state, values, event, context):
        self.state, self.values, self.event, self.context = state, values, event, context
        self.accepted = self.post_state = self.post_values = self.rule = self.reason = None
        self.pre = self.post = None


def value_of(term, step):
    op = term.op
    if op == "int":
        return term.args[0]
    if op == "var":
        return step.values[term.args[0]]
    if op == "var_next":
        return step.post_values[term.args[0]]
    if op == "input":
        return step.context[term.args[0]]
    left, right = value_of(term.args[0], step), value_of(term.args[1], step)
    return left + right if op == "add" else left - right


def holds(node, step) -> bool:
    """Evaluate one non-temporal formula on one step; every operand is total."""
    op = node.op
    if op in {"true", "false"}:
        return op == "true"
    if op == "fact":
        return step.context[node.args[0]]
    if op == "state":
        return step.state == node.args[0]
    if op == "state_next":
        return step.post_state == node.args[0]
    if op == "event":
        return step.event == node.args[0]
    if op == "accepted":
        return step.accepted
    if op == "not":
        return not holds(node.args[0], step)
    if op == "and":
        return all([holds(a, step) for a in node.args])
    if op == "or":
        return any([holds(a, step) for a in node.args])
    if op == "implies":
        premise, conclusion = holds(node.args[0], step), holds(node.args[1], step)
        return not premise or conclusion
    if op == "iff":
        return holds(node.args[0], step) == holds(node.args[1], step)
    if op == "cmp":
        left, right = value_of(node.args[1], step), value_of(node.args[2], step)
        return {"==": left == right, "!=": left != right, "<": left < right,
                "<=": left <= right, ">": left > right, ">=": left >= right}[node.args[0]]
    raise Refusal("Temporal operators are judged over executions, not one step")


def truth_tree(node, step):
    """The deterministic truth tree of one formula or term on one step."""
    if node.op in TERM_OPS:
        result = {"operator": node.op, "value": value_of(node, step)}
        if node.op in {"var", "var_next", "input"}:
            result["name"] = node.args[0]
        elif node.op in {"add", "sub"}:
            result["operands"] = [truth_tree(a, step) for a in node.args]
        return result
    result = {"operator": node.op, "value": holds(node, step)}
    if node.op == "fact":
        result.update(name=node.args[0], observed=step.context[node.args[0]])
    elif node.op in {"state", "state_next"}:
        result.update(expected=node.args[0], observed=step.state if node.op == "state" else step.post_state)
    elif node.op == "event":
        result.update(expected=node.args[0], observed=step.event)
    elif node.op == "accepted":
        result["observed"] = "Accept" if step.accepted else "Reject"
    elif node.op == "cmp":
        result.update(comparison=node.args[0], left=truth_tree(node.args[1], step), right=truth_tree(node.args[2], step))
    elif node.op not in {"true", "false"}:
        result["operands"] = [truth_tree(a, step) for a in node.args]
    return result


@dataclass(frozen=True)
class Machine:
    name: str
    states: tuple[str, ...]
    events: tuple[str, ...]
    facts: tuple[str, ...]
    inputs: tuple[Input, ...]
    variables: tuple[Variable, ...]
    initial: str
    default: str
    rules: tuple[Rule, ...]
    invariants: tuple[tuple[str, Expr], ...] = ()
    requirements: tuple[tuple[str, Expr], ...] = ()
    fairness: tuple[tuple[str, Expr], ...] = ()
    properties: tuple[tuple[str, Expr], ...] = ()
    obligations: tuple[tuple[str, str], ...] = ()
    players: tuple[tuple[str, tuple[str, ...]], ...] = ()
    games: tuple[tuple[str, str, str, Expr], ...] = ()

    def __post_init__(self):
        validate_machine(self)
        index = {}
        for rule in self.rules:
            index.setdefault((rule.source, rule.event), []).append(rule)
        # These derived indexes bind decision behavior just as the declared rules do.
        # Own their backing dictionaries; neither rule lists nor ranges may change.
        object.__setattr__(self, "_index", MappingProxyType({key: tuple(rules) for key, rules in index.items()}))
        object.__setattr__(self, "_ranges", MappingProxyType({v.name: (v.low, v.high) for v in self.variables}))

    @property
    def declarations(self):
        return {"facts": set(self.facts), "inputs": {i.name for i in self.inputs},
                "variables": {v.name for v in self.variables}, "states": set(self.states),
                "events": set(self.events)}

    def data(self):
        result = {"profile": PROFILE, "name": self.name, "states": self.states, "events": self.events,
                  "context": self.facts, "inputs": [i.data() for i in self.inputs],
                  "variables": [v.data() for v in self.variables], "initial": self.initial,
                  "default": self.default, "rules": [r.data() for r in self.rules],
                  "invariants": [[n, f.data()] for n, f in self.invariants],
                  "requirements": [[n, f.data()] for n, f in self.requirements],
                  "fairness": [[n, f.data()] for n, f in self.fairness],
                  "properties": [[n, f.data()] for n, f in self.properties],
                  "obligations": self.obligations, "atomic": True, "frame": "control-and-variables"}
        if self.players:
            result["players"] = [[name, states] for name, states in self.players]
        if self.games:
            result["games"] = [[name, player, objective, formula.data()]
                               for name, player, objective, formula in self.games]
        return result

    @property
    def revision(self):
        return hashlib.sha256(canonical(self.data()).encode()).hexdigest()

    def render(self, english=False):
        if english:
            lines = ["ZAL version 2.", f"The machine is {self.name}.",
                     f"The states are {', '.join(self.states)}.", f"The events are {', '.join(self.events)}.",
                     f"The context observations are {', '.join(self.facts) or 'none'}."]
            lines += [f"The input {i.name} is an integer from {i.low} to {i.high} inclusive." for i in self.inputs]
            lines += [f"The variable {v.name} is an integer from {v.low} to {v.high} inclusive, initially {v.initial}."
                      for v in self.variables]
            lines += [f"Initially the state is {self.initial}.",
                      "Each step is atomic. Only the control state and the declared variables may change.",
                      f"Otherwise reject because {self.default} and keep the state unchanged."]
            for rule in self.rules:
                if rule.target:
                    changes = " and ".join(u.render(True) for u in rule.updates)
                    outcome = f"move to {rule.target}" + (f" with {changes}" if changes else "")
                else:
                    outcome = f"reject because {rule.reason} and keep the state unchanged"
                lines.append(f"Rule {rule.id}: when the state is {rule.source} and the event is {rule.event}, "
                             f"if {rule.guard.render(True)}, {outcome}.")
            lines += [f"Invariant {n}: {f.render(True)}." for n, f in self.invariants]
            lines += [f"Requirement {n}: {f.render(True)}." for n, f in self.requirements]
            lines += [f"Fairness {n}: assume ({_bare(f.render(True))}) happens infinitely often." for n, f in self.fairness]
            lines += [f"Property {n}: {f.render(True)}." for n, f in self.properties]
            lines += [f"Player {name} moves in {', '.join(states)}." for name, states in self.players]
            phrases = {"eventually": "to happen eventually", "always": "to hold always",
                       "infinitely often": "to happen infinitely often"}
            lines += [f"Property {name}: {player} can force ({_bare(formula.render(True))}) {phrases[objective]}."
                      for name, player, objective, formula in self.games]
            lines += [f"Unresolved {kind}: {text}." for kind, text in self.obligations]
        else:
            lines = ["zal 2;", f"machine {self.name};", f"states {', '.join(self.states)};",
                     f"events {', '.join(self.events)};", f"context {', '.join(self.facts) or 'none'};"]
            lines += [f"input {i.name} in {i.low}..={i.high};" for i in self.inputs]
            lines += [f"var {v.name} in {v.low}..={v.high} initially {v.initial};" for v in self.variables]
            lines += [f"initial {self.initial};", "step atomic; frame control_and_variables;",
                      f"default reject {self.default} unchanged;"]
            for rule in self.rules:
                if rule.target:
                    outcome = ", ".join([f"accept {rule.target}", *[u.render() for u in rule.updates]])
                else:
                    outcome = f"reject {rule.reason} unchanged"
                lines.append(f"rule {rule.id}: {rule.source} + {rule.event} [{rule.guard.render()}] -> {outcome};")
            lines += [f"invariant {n}: {f.render()};" for n, f in self.invariants]
            lines += [f"require {n}: {f.render()};" for n, f in self.requirements]
            lines += [f"fairness {n}: infinitely often ({_bare(f.render())});" for n, f in self.fairness]
            lines += [f"property {n}: {f.render()};" for n, f in self.properties]
            lines += [f"player {name} moves in {', '.join(states)};" for name, states in self.players]
            lines += [f"property {name}: {player} can force {objective} ({_bare(formula.render())});"
                      for name, player, objective, formula in self.games]
            lines += [f"unresolved {kind}: {text};" for kind, text in self.obligations]
        return "\n".join(lines) + "\n"

    # Finite domains, in canonical order.
    def value_tuples(self):
        return itertools.product(*[range(v.low, v.high + 1) for v in self.variables])

    def configurations(self):
        names = [v.name for v in self.variables]
        for state, values in itertools.product(self.states, list(self.value_tuples())):
            yield state, dict(zip(names, values))

    def contexts(self):
        names = [*self.facts, *[i.name for i in self.inputs]]
        domains = [(False, True)] * len(self.facts) + [range(i.low, i.high + 1) for i in self.inputs]
        for values in itertools.product(*domains):
            yield dict(zip(names, values))

    def inputs_domain(self):
        contexts = list(self.contexts())
        for (state, values), event in itertools.product(list(self.configurations()), self.events):
            for context in contexts:
                yield state, dict(values), event, dict(context)

    def initial_values(self):
        return {v.name: v.initial for v in self.variables}

    def _validate_input(self, state, values, event, context):
        expected_context = set(self.facts) | {i.name for i in self.inputs}
        if (not isinstance(state, str) or not isinstance(event, str) or not isinstance(values, dict)
                or not isinstance(context, dict) or state not in self.states or event not in self.events
                or set(values) != set(self._ranges) or set(context) != expected_context):
            raise Refusal("Input outside the declared state/variable/event/context domain")
        for name, (low, high) in self._ranges.items():
            if type(values[name]) is not int or not low <= values[name] <= high:
                raise Refusal("Input outside the declared state/variable/event/context domain")
        for name in self.facts:
            if type(context[name]) is not bool:
                raise Refusal("Input outside the declared state/variable/event/context domain")
        for item in self.inputs:
            if type(context[item.name]) is not int or not item.low <= context[item.name] <= item.high:
                raise Refusal("Input outside the declared state/variable/event/context domain")

    def outcome(self, state, values, event, context):
        """('overlap', rules) | ('trap', rule, variable, value) | ('step', step) for a valid input."""
        step = _Step(state, values, event, context)
        enabled = [r for r in self._index.get((state, event), ()) if holds(r.guard, step)]
        if len(enabled) > 1:
            return ("overlap", enabled)
        if not enabled or enabled[0].target is None:
            step.accepted, step.post_state, step.post_values = False, state, values
            step.rule, step.reason = (enabled[0].id, enabled[0].reason) if enabled else ("default", self.default)
            return ("step", step)
        rule, post = enabled[0], dict(values)
        for update in rule.updates:
            operand = value_of(update.term, step)
            before = values[update.variable]
            after = operand if update.op == "=" else before + operand if update.op == "+=" else before - operand
            low, high = self._ranges[update.variable]
            if not low <= after <= high:
                return ("trap", rule, update.variable, after)
            post[update.variable] = after
        step.accepted, step.post_state, step.post_values = True, rule.target, post
        step.rule, step.reason = rule.id, None
        return ("step", step)

    def decide(self, state, variables, event, context):
        self._validate_input(state, variables, event, context)
        result = self.outcome(state, dict(variables), event, dict(context))
        if result[0] == "overlap":
            raise Refusal("Overlapping transitions; no precedence is implied")
        if result[0] == "trap":
            _, rule, name, value = result
            low, high = self._ranges[name]
            raise Refusal(f"Rule {rule.id} would set {name} to {value}, outside {low}..={high}; "
                          "a step never wraps around")
        return _outcome_data(result[1])


def _outcome_data(step):
    return {"class": "Accept" if step.accepted else "Reject", "state": step.post_state,
            "variables": dict(step.post_values), "reason": step.reason, "rule": step.rule}


def step_data(step):
    return {"state": step.state, "variables": dict(step.values), "event": step.event,
            "context": dict(step.context), "outcome": _outcome_data(step)}


def _post_state_only(formula):
    allowed = {"true", "false", "state_next", "var_next", "int", "not", "and", "or", "implies", "iff",
               "cmp", "add", "sub"}
    return (isinstance(formula, Expr) and formula.op in allowed
            and all(_post_state_only(arg) for arg in formula.args if isinstance(arg, Expr)))


def validate_machine(model: Machine):
    """Validate direct IR construction as strictly as surface elaboration."""
    def valid_name(value):
        return isinstance(value, str) and re.fullmatch(NAME, value) and value not in RESERVED
    if not valid_name(model.name) or not valid_name(model.default):
        raise Refusal("Invalid machine or default reason name")
    for label, values, maximum, minimum in [("states", model.states, 8, 1), ("events", model.events, 8, 1),
                                            ("facts", model.facts, 4, 0)]:
        if (not isinstance(values, tuple) or not minimum <= len(values) <= maximum
                or any(not valid_name(x) for x in values) or tuple(sorted(set(values))) != values):
            raise Refusal(f"IR {label} must be bounded, distinct, canonically sorted owned names")
    for label, values, kind, maximum in [("inputs", model.inputs, Input, 2), ("variables", model.variables, Variable, 4)]:
        if (not isinstance(values, tuple) or len(values) > maximum or any(not isinstance(x, kind) for x in values)
                or [x.name for x in values] != sorted({x.name for x in values})):
            raise Refusal(f"IR {label} must be at most {maximum} distinct values sorted by name")
        for item in values:
            bounds = [item.low, item.high] + ([item.initial] if kind is Variable else [])
            if (not valid_name(item.name) or any(type(b) is not int or abs(b) > MAX_LITERAL for b in bounds)
                    or not item.low <= item.high or item.high - item.low + 1 > MAX_RANGE):
                raise Refusal(f"Integer {item.name} needs an inclusive range of 1..{MAX_RANGE} values "
                              f"within -{MAX_LITERAL}..={MAX_LITERAL}")
            if kind is Variable and not item.low <= item.initial <= item.high:
                raise Refusal(f"Variable {item.name} starts outside its declared range")
    value_names = [*model.facts, *[i.name for i in model.inputs], *[v.name for v in model.variables]]
    if len(set(value_names)) != len(value_names):
        raise Refusal("Context facts, inputs and variables share one namespace and must be distinct")
    player_error = "Players need distinct names and disjoint declared states; at most two players"
    if not isinstance(model.players, tuple) or len(model.players) > 2:
        raise Refusal(player_error)
    player_names, owned_states = [], set()
    for player in model.players:
        if (not isinstance(player, tuple) or len(player) != 2 or not valid_name(player[0])
                or player[0] in player_names or not isinstance(player[1], tuple) or not player[1]
                or any(not isinstance(state, str) for state in player[1])
                or tuple(sorted(set(player[1]))) != player[1]
                or any(state not in model.states or state in owned_states for state in player[1])):
            raise Refusal(player_error)
        player_names.append(player[0])
        owned_states.update(player[1])
    if player_names != sorted(player_names):
        raise Refusal(player_error)
    occupied_names = {model.name, model.default, *model.states, *model.events, *model.facts,
                       *[item.name for item in model.inputs], *[item.name for item in model.variables]}
    if set(player_names) & occupied_names:
        raise Refusal(player_error)
    if model.initial not in model.states:
        raise Refusal("Initial set is empty or outside the declared states")
    configurations = len(model.states)
    for item in model.variables:
        configurations *= item.high - item.low + 1
    tuples = configurations * len(model.events) * 2 ** len(model.facts)
    for item in model.inputs:
        tuples *= item.high - item.low + 1
    if configurations > MAX_CONFIGURATIONS or tuples > MAX_TUPLES:
        raise Refusal(f"Declared finite domain has {configurations} configurations and {tuples} input tuples; "
                      f"this profile refuses more than {MAX_CONFIGURATIONS} and {MAX_TUPLES}")
    for values, maximum in [(model.rules, 32), (model.invariants, 8), (model.requirements, 8),
                            (model.fairness, 4), (model.properties, 8), (model.obligations, 8),
                            (model.games, 4)]:
        if not isinstance(values, tuple) or len(values) > maximum:
            raise Refusal("IR collection is mutable or exceeds the profile bound")
    if len(model.properties) + len(model.games) > 8:
        raise Refusal("Bounds: at most 8 temporal and game properties")
    declarations = model.declarations
    identifiers = []
    for rule in model.rules:
        if (not isinstance(rule, Rule) or not valid_name(rule.id) or rule.source not in model.states
                or rule.event not in model.events):
            raise Refusal("Invalid IR transition")
        if ((rule.target is None) == (rule.reason is None) or rule.target is not None and rule.target not in model.states
                or rule.reason is not None and not valid_name(rule.reason)):
            raise Refusal("IR transition needs exactly one valid target or rejection reason")
        if admit_formula(rule.guard, declarations, "guard") > 128 or any(len(rule.guard.render(e)) > 4096 for e in [False, True]):
            raise Refusal("IR guard exceeds canonical bounds")
        updates = rule.updates
        if (not isinstance(updates, tuple) or rule.target is None and updates
                or any(not isinstance(u, Update) for u in updates)
                or [u.variable for u in updates] != sorted({u.variable for u in updates})):
            raise Refusal(f"Rule {rule.id}: updates need an Accept and at most one update per variable, sorted")
        for update in updates:
            if (update.variable not in declarations["variables"] or update.op not in UPDATE_ENGLISH
                    or not isinstance(update.term, Expr) or update.term.op not in {"int", "var", "input"}):
                raise Refusal(f"Rule {rule.id}: an update assigns, increases or decreases a declared variable by "
                              "a literal, variable or input")
            admit_formula(update.term, declarations, "guard")
        identifiers.append(rule.id)
    if [r.id for r in model.rules] != sorted(r.id for r in model.rules):
        raise Refusal("IR transition IDs must be canonically sorted")
    for collection, scope in [(model.invariants, "invariant"), (model.requirements, "step"),
                              (model.fairness, "step"), (model.properties, "property")]:
        for item in collection:
            if (not isinstance(item, tuple) or len(item) != 2 or not valid_name(item[0])
                    or admit_formula(item[1], declarations, scope) > 128
                    or any(len(item[1].render(e)) > 4096 for e in [False, True])):
                raise Refusal("Invalid IR invariant, requirement, fairness or property")
            identifiers.append(item[0])
        if [n for n, _ in collection] != sorted(n for n, _ in collection):
            raise Refusal("IR object IDs must be canonically sorted")
    for item in model.games:
        if (not isinstance(item, tuple) or len(item) != 4 or not valid_name(item[0])
                or item[2] not in GAME_OBJECTIVES or not isinstance(item[3], Expr)):
            raise Refusal("Invalid IR game property")
        name, player, objective, formula = item
        if player not in player_names:
            raise Refusal(f"Game property {name} names an undeclared player")
        try:
            if admit_formula(formula, declarations, "invariant") > 128:
                raise Refusal("Invalid IR game property")
        except Refusal as error:
            if str(error) == "Invalid IR game property":
                raise
            raise Refusal("Invalid IR game property") from error
        if any(len(formula.render(english)) > 4096 for english in [False, True]):
            raise Refusal("Invalid IR game property")
        identifiers.append(name)
    if [name for name, _, _, _ in model.games] != sorted(name for name, _, _, _ in model.games):
        raise Refusal("IR game property IDs must be canonically sorted")
    if model.games and (len(model.players) != 2 or owned_states != set(model.states)):
        raise Refusal("Game properties need two players whose states cover every state exactly once")
    all_declared_names = occupied_names | set(identifiers)
    if set(player_names) & all_declared_names:
        raise Refusal(player_error)
    if len(set(identifiers)) != len(identifiers) or "default" in identifiers:
        raise Refusal("Rule, invariant, requirement, fairness and property IDs must be distinct and not default")
    if any(objective in {"eventually", "infinitely often"} for _, _, objective, _ in model.games):
        if any(not _post_state_only(formula) for _, formula in model.fairness):
            raise Refusal("Game fairness reads only the state after a step (state' and primed variables)")
    for name, formula in model.properties:
        if property_shape(formula)["kind"] == "liveness" and not model.fairness:
            raise Refusal(f"Property {name}: liveness needs a fairness declaration")
    for item in model.obligations:
        if (not isinstance(item, tuple) or len(item) != 2 or item[0] not in FUTURE or not isinstance(item[1], str)
                or not re.fullmatch(r"[a-zA-Z0-9_ ,!?()/-]{1,200}", item[1])):
            raise Refusal("Invalid unresolved obligation")
    if tuple(sorted(model.obligations)) != model.obligations:
        raise Refusal("Unresolved obligations must be canonically sorted")
    if any(len(model.render(english).encode("ascii")) > MAX_SOURCE for english in [False, True]):
        raise Refusal("Canonical symbolic or English model exceeds 64 KiB")


def grammar_patterns(english=False):
    """The parser's singleton statement registry, also used by the live legend."""
    return {
        "header": r"ZAL version 2\." if english else r"zal 2;",
        "name": rf"The machine is ({NAME})\." if english else rf"machine ({NAME});",
        "states": r"The states are (.+)\." if english else r"states (.+);",
        "events": r"The events are (.+)\." if english else r"events (.+);",
        "facts": r"The context observations are (.+)\." if english else r"context (.+);",
        "initial": rf"Initially the state is ({NAME})\." if english else rf"initial ({NAME});",
        "frame": (r"Each step is atomic\. Only the control state and the declared variables may change\." if english
                  else r"step atomic; frame control_and_variables;"),
        "default": (rf"Otherwise reject because ({NAME}) and keep the state unchanged\." if english
                    else rf"default reject ({NAME}) unchanged;"),
    }


def statement_patterns(english=False):
    """The repeatable statement registry, keyed by concept."""
    if english:
        return {
            "input": rf"The input ({NAME}) is an integer from ({INTEGER}) to ({INTEGER}) inclusive\.",
            "var": rf"The variable ({NAME}) is an integer from ({INTEGER}) to ({INTEGER}) inclusive, initially ({INTEGER})\.",
            "rule": rf"Rule ({NAME}): when the state is ({NAME}) and the event is ({NAME}), if (.+), (move to .+|reject because .+)\.",
            "invariant": rf"Invariant ({NAME}): (.+)\.",
            "require": rf"Requirement ({NAME}): (.+)\.",
            "fairness": rf"Fairness ({NAME}): assume (.+) happens infinitely often\.",
            "player": rf"Player ({NAME}) moves in (.+)\.",
            "game": rf"Property ({NAME}): ({NAME}) can force \((.+)\) to (happen infinitely often|happen eventually|hold always)\.",
            "property": rf"Property ({NAME}): (.+)\.",
            "unresolved": rf"Unresolved ({NAME}): ([a-zA-Z0-9_ ,!?()/-]{{1,200}})\.",
        }
    return {
        "input": rf"input ({NAME}) in ({INTEGER})\.\.=({INTEGER});",
        "var": rf"var ({NAME}) in ({INTEGER})\.\.=({INTEGER}) initially ({INTEGER});",
        "rule": rf"rule ({NAME}): ({NAME}) \+ ({NAME}) \[(.+)\] -> (.+);",
        "invariant": rf"invariant ({NAME}): (.+);",
        "require": rf"require ({NAME}): (.+);",
        "fairness": rf"fairness ({NAME}): infinitely often (.+);",
        "player": rf"player ({NAME}) moves in (.+);",
        "game": rf"property ({NAME}): ({NAME}) can force (eventually|always|infinitely often) \((.+)\);",
        "property": rf"property ({NAME}): (.+);",
        "unresolved": rf"unresolved ({NAME}): ([a-zA-Z0-9_ ,!?()/-]{{1,200}});",
    }


def _outcome_parts(rid, text, english, declarations, states):
    rejected = re.fullmatch(rf"reject {'because ' if english else ''}({NAME}) "
                            f"{'and keep the state unchanged' if english else 'unchanged'}", text)
    if rejected:
        return None, rejected[1], ()
    accepted = re.fullmatch(rf"move to ({NAME})(?: with (.+))?" if english else rf"accept ({NAME})((?:\s*,\s*[^,]+)*)", text)
    if not accepted:
        raise Refusal(f"Rule {rid} needs an explicit outcome and frame")
    if accepted[1] not in states:
        raise Refusal(f"Rule {rid} targets an undeclared state")
    raw = accepted[2] or ""
    pieces = [p.strip() for p in (raw.split(" and ") if english else raw.split(",")[1:])] if raw else []
    updates = []
    for piece in pieces:
        if english:
            match = re.fullmatch(rf"({NAME}) (set to|increased by|decreased by) ({INTEGER}|{NAME})", piece)
            op = match and {v: k for k, v in UPDATE_ENGLISH.items()}[match[2]]
        else:
            match = re.fullmatch(rf"({NAME})\s*(=|\+=|-=)\s*({INTEGER}|{NAME})", piece)
            op = match and match[2]
        if not match:
            raise Refusal(f"Rule {rid}: an update reads VARIABLE = TERM, VARIABLE += TERM or VARIABLE -= TERM")
        if match[1] not in declarations["variables"]:
            raise Refusal(f"Rule {rid} updates an undeclared variable {match[1]}")
        term = _Parser(match[3], declarations, english).atom_term() if not re.fullmatch(INTEGER, match[3]) else Expr("int", (int(match[3]),))
        updates.append(Update(match[1], op, term))
    if len({u.variable for u in updates}) != len(updates):
        raise Refusal(f"Rule {rid} updates one variable more than once")
    return accepted[1], None, tuple(sorted(updates, key=lambda u: u.variable))


def parse(text: str, syntax="symbolic") -> Machine:
    if not isinstance(text, str) or len(text.encode()) > MAX_SOURCE or not text.isascii():
        raise Refusal("Source must be ASCII text at most 64 KiB")
    if syntax not in {"symbolic", "english"}:
        raise Refusal("Choose symbolic or english syntax")
    english = syntax == "english"
    lines = source_lines(text)
    if len(lines) > MAX_STATEMENTS:
        raise Refusal(f"Source exceeds {MAX_STATEMENTS} statements")
    singletons, repeated = grammar_patterns(english), statement_patterns(english)
    fields, found, locations = {}, {key: [] for key in repeated}, {}
    for number, (physical, column, line) in enumerate(lines, 1):
        for key, pattern in singletons.items():
            match = re.fullmatch(pattern, line)
            if match:
                if key in fields:
                    raise Refusal(f"Statement {number}: duplicate {key}", location=(physical, column))
                fields[key] = match.group(1) if match.groups() else True
                locations[key] = (physical, column)
                break
        else:
            for key, pattern in repeated.items():
                match = re.fullmatch(pattern, line)
                if match and (key != "unresolved" or match[1] in FUTURE):
                    found[key].append(match.groups())
                    locations[(key, match[1])] = (physical, column)
                    break
            else:
                if line.startswith(("player ", "Player ")):
                    raise Refusal("Players need distinct names and disjoint declared states; at most two players",
                                  location=(physical, column))
                raise grammar_refusal(number, line, english, (physical, column))
    missing = set(singletons) - set(fields)
    if missing:
        raise Refusal("Missing explicit declarations: " + ", ".join(sorted(missing)),
                      location=(len(text.splitlines()) + 1, 1))

    def names(key, maximum, empty=False):
        raw = [] if empty and fields[key] == "none" else [x.strip() for x in fields[key].split(",")]
        if any(len(name) > 32 for name in raw):
            raise Refusal(f"{key}: identifier exceeds 32 characters", location=locations[key])
        if (not (0 if empty else 1) <= len(raw) <= maximum or len(set(raw)) != len(raw)
                or any(not re.fullmatch(NAME, x) for x in raw)):
            raise Refusal(f"{key} needs {'0' if empty else '1'}..{maximum} distinct names", location=locations[key])
        return tuple(sorted(raw))

    states, events, facts = names("states", 8), names("events", 8), names("facts", 4, True)
    inputs = tuple(sorted((Input(n, int(lo), int(hi)) for n, lo, hi in found["input"]), key=lambda i: i.name))
    variables = tuple(sorted((Variable(n, int(lo), int(hi), int(v)) for n, lo, hi, v in found["var"]),
                             key=lambda v: v.name))
    declared = [*states, *events, *facts, *[i.name for i in inputs], *[v.name for v in variables]]
    if RESERVED & set(declared):
        groups = [(locations[key], values) for key, values in [("states", states), ("events", events), ("facts", facts)]]
        groups.extend((locations[(key, item.name)], (item.name,)) for key, values in [("input", inputs), ("var", variables)] for item in values)
        location = next(location for location, names in groups if RESERVED & set(names))
        raise Refusal("A declaration uses a reserved word: " + ", ".join(sorted(RESERVED & set(declared))), location=location)
    if fields["initial"] not in states:
        raise Refusal("Initial state is undeclared or empty", location=locations["initial"])
    if (len(found["rule"]) > 32 or len(found["invariant"]) > 8 or len(found["require"]) > 8
            or len(found["fairness"]) > 4 or len(found["property"]) + len(found["game"]) > 8
            or len(found["game"]) > 4 or len(found["unresolved"]) > 8
            or len(inputs) > 2 or len(variables) > 4):
        raise Refusal("Bounds: 2 inputs, 4 variables, 32 rules, 8 invariants, 8 requirements, "
                      "4 fairness assumptions, 8 total properties, 4 game properties, 8 unresolved obligations")
    declarations = {"facts": set(facts), "inputs": {i.name for i in inputs},
                    "variables": {v.name for v in variables}, "states": set(states), "events": set(events)}
    players = []
    player_error = "Players need distinct names and disjoint declared states; at most two players"
    for player, raw_states in found["player"]:
        owned = tuple(sorted(item.strip() for item in raw_states.split(",")))
        if (len(players) >= 2 or not owned or len(set(owned)) != len(owned)
                or player in RESERVED
                or any(not re.fullmatch(NAME, state) or state not in states for state in owned)
                or any(player == prior or set(owned) & set(prior_states) for prior, prior_states in players)):
            raise Refusal(player_error)
        players.append((player, owned))
    players = tuple(sorted(players))
    rules = []
    for rid, source, event, guard, outcome in found["rule"]:
        with at_source(locations[("rule", rid)]):
            if source not in states or event not in events:
                raise Refusal(f"Rule {rid} refers to an undeclared state or event")
            target, reason, updates = _outcome_parts(rid, outcome, english, declarations, states)
            rules.append(Rule(rid, source, event, parse_formula(guard, declarations, "guard", english),
                              target, reason, updates))
    collections = {}
    for key, scope in [("invariant", "invariant"), ("require", "step"), ("fairness", "step"), ("property", "property")]:
        formulas = []
        for name, body in found[key]:
            with at_source(locations[(key, name)]):
                formulas.append((name, parse_formula(body, declarations, scope, english)))
        collections[key] = tuple(sorted(formulas))
    games = []
    for item in found["game"]:
        if english:
            name, player, body, phrase = item
            objective = {"happen infinitely often": "infinitely often", "happen eventually": "eventually",
                         "hold always": "always"}[phrase]
        else:
            name, player, objective, body = item
        with at_source(locations[("game", name)]):
            games.append((name, player, objective, parse_formula(body, declarations, "invariant", english)))
    try:
        return Machine(fields["name"], states, events, facts, inputs, variables, fields["initial"], fields["default"],
                       tuple(sorted(rules, key=lambda r: r.id)), collections["invariant"], collections["require"],
                       collections["fairness"], collections["property"], tuple(sorted(found["unresolved"])),
                       players, tuple(sorted(games)))
    except Refusal as error:
        # Admission is still the single validator of ranges and the owned IR.
        # Add the declaration's physical position without changing its verdict.
        match = re.match(r"(Integer|Variable|Property) ([a-z][a-z0-9_]*)", str(error))
        if match:
            keys = ["property"] if match[1] == "Property" else ["var", "input"]
            location = next((locations[(key, match[2])] for key in keys if (key, match[2]) in locations), None)
            raise Refusal(str(error), location=location) from error
        raise


def parse_any(text: str, syntax="symbolic"):
    """Select the finite-FSM profile by its exact header; profile 1 admission is unchanged."""
    import behavior
    if not isinstance(text, str):
        raise Refusal("Source must be ASCII text at most 64 KiB")
    first = next((line.strip() for line in text.splitlines()
                  if line.strip() and not line.lstrip().startswith("#")), "")
    if first in {"zal 2;", "ZAL version 2."}:
        return parse(text, syntax)
    return behavior.parse(text, syntax)


def counterexample(model, checker_identity, obligation, checks, **details):
    return {"status": "counterexample", "revision": model.revision, "profile": PROFILE,
            "checker": checker_identity, "obligation": obligation, **details,
            "checks": checks, "authority": "none"}


class _Graph:
    """The complete raw step table and the reachable configuration graph."""

    def __init__(self, model):
        self.model = model
        names = [v.name for v in model.variables]
        self.configs = [(state, values) for state, values in
                        itertools.product(model.states, list(model.value_tuples()))]
        self.index = {config: i for i, config in enumerate(self.configs)}
        self.value_dicts = [dict(zip(names, values)) for _, values in self.configs]
        self.contexts = list(model.contexts())
        self.names = names
        self.tuples = len(self.configs) * len(model.events) * len(self.contexts)

    def raw(self):
        """Yield (config index, outcome) over every declared input tuple, in canonical order."""
        for ci, (state, _) in enumerate(self.configs):
            for event in self.model.events:
                for context in self.contexts:
                    result = self.model.outcome(state, self.value_dicts[ci], event, context)
                    if result[0] == "step":
                        step = result[1]
                        step.pre = ci
                        step.post = self.index[(step.post_state, tuple(step.post_values[n] for n in self.names))]
                    yield ci, event, context, result

    def explore(self, steps):
        """Breadth-first reachability from the initial configuration over complete raw steps."""
        start = self.index[(self.model.initial, tuple(v.initial for v in self.model.variables))]
        self.start, self.parent, self.order = start, {start: None}, [start]
        self.position = {start: 0}
        queue = deque([start])
        while queue:
            ci = queue.popleft()
            for step in steps[ci]:
                if step.post not in self.parent:
                    self.parent[step.post] = step
                    self.position[step.post] = len(self.order)
                    self.order.append(step.post)
                    queue.append(step.post)
        self.steps = steps

    def trace_to(self, ci):
        path = []
        while self.parent[ci] is not None:
            path.append(self.parent[ci])
            ci = self.parent[ci].pre
        return path[::-1]

    def reachable_steps(self):
        for ci in self.order:
            yield from self.steps[ci]

    def config_data(self, ci):
        return {"state": self.configs[ci][0], "variables": dict(self.value_dicts[ci])}


@dataclass(frozen=True)
class _GameGraph:
    """Owned finite game graph; both solvers receive only this immutable value."""
    successors: tuple[tuple[int, ...], ...]
    owners: tuple[int, ...]
    goal: tuple[bool, ...]
    fairness: tuple[tuple[bool, ...], ...]
    configurations: tuple[tuple, ...]
    configuration_nodes: tuple[int, ...]
    initial: int
    numbered_names: bool


def _game_graph(graph, model, player):
    state_owner = {state: owner for owner, states in model.players for state in states}
    config_index = {ci: index for index, ci in enumerate(graph.order)}
    descriptors = [
        ("configuration", graph.configs[ci][0], tuple(zip(graph.names, graph.configs[ci][1])))
        for ci in graph.order
    ]
    owners = [0 if state_owner[graph.configs[ci][0]] == player else 1 for ci in graph.order]
    successors = [[] for _ in graph.order]
    has_context = bool(model.facts or model.inputs)
    for ci in graph.order:
        source = config_index[ci]
        state = graph.configs[ci][0]
        rows = graph.steps[ci]
        if state_owner[state] == player and has_context:
            for event in model.events:
                choices = [step for step in rows if step.event == event]
                intermediate = len(owners)
                descriptors.append(("context", state, tuple(zip(graph.names, graph.configs[ci][1])), event))
                owners.append(1)
                successors.append([config_index[step.post] for step in choices])
                successors[source].append(intermediate)
        else:
            successors[source].extend(config_index[step.post] for step in rows)
    false_labels = tuple(False for _ in descriptors)
    return _GameGraph(tuple(tuple(row) for row in successors), tuple(owners), false_labels, (),
                      tuple(descriptors), tuple(range(len(graph.order))), config_index[graph.start],
                      bool(model.variables or model.facts or model.inputs))


def _configuration_holds(formula, descriptor):
    if descriptor[0] != "configuration":
        return False
    _, state, pairs = descriptor
    values = dict(pairs)
    step = _Step(state, values, None, {})
    step.post_state, step.post_values = state, values
    return holds(formula, step)


def _reduce_game(graph, model, objective, formula):
    goal = [_configuration_holds(formula, node) for node in graph.configurations]
    successors = [tuple(row) for row in graph.successors]
    if objective == "always":
        fairness = (tuple(True for _ in graph.successors),)
        goal = [True for _ in graph.successors]
        for node in graph.configuration_nodes:
            if not _configuration_holds(formula, graph.configurations[node]):
                goal[node] = False
                successors[node] = (node,)
        return _GameGraph(tuple(successors), graph.owners, tuple(goal), fairness,
                          graph.configurations, graph.configuration_nodes, graph.initial, graph.numbered_names)

    if model.fairness:
        fairness = [tuple(_configuration_holds(item, node) for node in graph.configurations)
                    for _, item in model.fairness]
    else:
        fairness = [tuple(True for _ in graph.successors)]
    if objective == "eventually":
        for node in graph.configuration_nodes:
            if goal[node]:
                successors[node] = (node,)
                for index in range(len(fairness)):
                    row = list(fairness[index])
                    row[node] = True
                    fairness[index] = tuple(row)
    return _GameGraph(tuple(successors), graph.owners, tuple(goal), tuple(fairness),
                      graph.configurations, graph.configuration_nodes, graph.initial, graph.numbered_names)


def _fixed_point_strata(graph):
    """Solver A returns its winning set and the first stable outer/middle strata."""
    universe = set(range(len(graph.successors)))

    def cpre(target):
        return {node for node, choices in enumerate(graph.successors)
                if ((graph.owners[node] == 0 and any(child in target for child in choices))
                    or (graph.owners[node] == 1 and all(child in target for child in choices)))}

    z = set(universe)
    z_chain = [frozenset(z)]
    levels, middle_chains = {}, {}
    outer = 0
    while True:
        outer += 1
        cpre_z = cpre(z)
        y = set()
        y_chain = [frozenset(y)]
        level = 0
        while True:
            level += 1
            cpre_y = cpre(y)
            union = set()
            for assumption in graph.fairness:
                x = set(universe)
                while True:
                    cpre_x = cpre(x)
                    updated_x = {
                        node for node in universe
                        if ((graph.goal[node] and node in cpre_z)
                            or node in cpre_y
                            or (not assumption[node] and node in cpre_x))
                    }
                    if updated_x == x:
                        break
                    x = updated_x
                union.update(x)
            y_chain.append(frozenset(union))
            if union == y:
                break
            y = union
        levels[outer] = level
        middle_chains[outer] = tuple(y_chain)
        z_chain.append(frozenset(y))
        if y == z:
            return outer, levels, tuple(z_chain), middle_chains
        z = y


def _fixed_point_winners(graph):
    """Solver A: iterate the stated nested nu/mu/nu fixed points to stability."""
    _, _, z_chain, _ = _fixed_point_strata(graph)
    return z_chain[-1]


def _parity_winners(graph):
    """Solver B: round-robin parity product, solved by Zielonka recursion."""
    count = len(graph.successors)
    assumptions = len(graph.fairness)
    product_size = count * assumptions
    owners, priorities, successors = [], [], []
    for node in range(count):
        for counter in range(assumptions):
            owners.append(graph.owners[node])
            priorities.append(2 if graph.goal[node] else
                              1 if graph.fairness[counter][node] and counter == assumptions - 1 else 0)
            next_counter = ((counter + 1) % assumptions
                            if graph.fairness[counter][node] else counter)
            successors.append(tuple(child * assumptions + next_counter for child in graph.successors[node]))

    def attractor(player, target, subgame):
        reached = set(target)
        while True:
            added = set()
            for node in subgame - reached:
                choices = [child for child in successors[node] if child in subgame]
                if ((owners[node] == player and any(child in reached for child in choices))
                        or (owners[node] != player and all(child in reached for child in choices))):
                    added.add(node)
            if not added:
                return reached
            reached.update(added)

    def zielonka(subgame):
        if not subgame:
            return set(), set()
        highest = max(priorities[node] for node in subgame)
        player = highest % 2
        top = {node for node in subgame if priorities[node] == highest}
        first_attractor = attractor(player, top, subgame)
        first_even, first_odd = zielonka(subgame - first_attractor)
        first_winners = [first_even, first_odd]
        opponent = 1 - player
        if not first_winners[opponent]:
            first_winners[player].update(first_attractor)
            return first_winners[0], first_winners[1]
        second_attractor = attractor(opponent, first_winners[opponent], subgame)
        second_even, second_odd = zielonka(subgame - second_attractor)
        second_winners = [second_even, second_odd]
        second_winners[opponent].update(second_attractor)
        return second_winners[0], second_winners[1]

    even_wins, _ = zielonka(set(range(product_size)))
    return frozenset(node for node in range(count) if node * assumptions in even_wins)


def _game_table_project(graph, proposed_strata=None):
    """Render a reduced game using solver-proposed strata and a checked stability table."""
    count, assumptions = len(graph.successors), graph.fairness
    if proposed_strata is None:
        outer, levels, _, _ = _fixed_point_strata(graph)
    else:
        outer, levels = proposed_strata
    if outer < 1 or set(levels) != set(range(1, outer + 1)) or any(levels[k] < 1 for k in levels):
        raise Refusal("Invalid fixed-point stratum proposal")
    slots = tuple(tuple(dict.fromkeys(row)) for row in graph.successors)
    width = max(len(row) for row in slots)
    goal_table = max(840, 800 + width)
    fairness_table = max(860, goal_table + 1)
    listing = lambda values: "[" + ",".join(map(str, values)) + "]"
    source = [
        "zeno 1;",
        "project 1 constant_role_hierarchy;",
        "namespace 2 permissions;",
        "type 100 state Roles;",
        "type 101 command Request;",
        "type 102 context Session;",
        f"type 105 int Parent in 0..={count};",
        "type 106 index Role of 110;",
        "type 107 int Grant in 0..=1;",
        "// Who moves: 0 = the property's player, 1 = the other player.",
        f"const table 110 mover 107 size {count} {listing(graph.owners)};",
    ]
    for slot in range(width):
        values = [row[slot] + 1 if slot < len(row) else 0 for row in slots]
        source.append(f"const table {800 + slot} next{slot} 105 size {count} {listing(values)};")
    source.append(f"const table {goal_table} goal 107 size {count} {listing(int(value) for value in graph.goal)};")
    for index, row in enumerate(assumptions):
        name = "fair" if len(assumptions) == 1 else f"fair{index}"
        source.append(f"const table {fairness_table + index} {name} 107 size {count} {listing(int(value) for value in row)};")
    source += [
        f"table 210 100 grants 107 size {count};",
        "field 120 101 role 106;",
        "variant 130 102 Session none;",
        "reason 200 permission_required precedence 0;",
        "component 400 inheritance {",
        "  owns 100;",
        "  reads pre.100;",
        "  writes post.100;",
        "  contexts context.102;",
        "  budget steps 4096;",
        "}",
        "merge [400];",
    ]
    edge = "(" + " || ".join(f"next{slot}[s] == t + 1" for slot in range(width)) + ")"
    has_successor = "(" + " || ".join(f"next{slot}[s] != 0" for slot in range(width)) + ")"

    def other_predecessor(target):
        return (f"((mover[s] == 0 && forall t in table 110 {{ !{edge} || {target} }})"
                f" || (mover[s] == 1 && (!{has_successor} || exists t in table 110 {{ {edge} && {target} }})))")

    def escape(k, level, fair, at):
        return f"w{k}_{level}_{fair}[{at}]"

    def not_y(k, level, at):
        if level == 0:
            return "true"
        return "(" + " && ".join(escape(k, level, i, at) for i in range(len(assumptions))) + ")"

    def not_z(k, at):
        if k == 0:
            return "false"
        return "(" + " && ".join(escape(k, levels[k], i, at) for i in range(len(assumptions))) + ")"

    next_table = max(1000, fairness_table + len(assumptions))
    source.append("// w(k,l,i)[s]: the other player can escape middle stratum (k,l) through assumption i.")
    for outer_index in range(1, outer + 1):
        for level in range(1, levels[outer_index] + 1):
            for fair_index in range(len(assumptions)):
                fair_name = "fair" if len(assumptions) == 1 else f"fair{fair_index}"
                not_a = (f"((goal[s] == 0 || {other_predecessor(not_z(outer_index - 1, 't'))}) && "
                         f"{other_predecessor(not_y(outer_index, level - 1, 't'))})")
                body = (f"{not_a} && ({fair_name}[s] == 1 || "
                        f"{other_predecessor(escape(outer_index, level, fair_index, 't'))})")
                source += [f"derived table {next_table} w{outer_index}_{level}_{fair_index}[s] "
                           f"in table 110 rounds {count} {{",
                           f"  when true then {body};",
                           "  default false;",
                           "};"]
                next_table += 1
    source += ["// win[s]: the property player wins from s under the fairness assumptions.",
               f"derived table {next_table} win[s] in table 110 rounds {count} {{",
               f"  when true then !{not_z(outer, 's')};",
               "  default false;",
               "};"]
    next_table += 1

    def same(left, right):
        return f"(({left} && {right}) || (!{left} && !{right}))"

    stable_conditions = [same(not_z(outer, "t"), not_z(outer - 1, "t"))]
    stable_conditions.extend(same(not_y(index, levels[index], "t"),
                                  not_y(index, levels[index] - 1, "t"))
                             for index in range(1, outer + 1))
    stable_body = "forall t in table 110 { " + " && ".join(stable_conditions) + " }"
    source += [f"derived table {next_table} stable[s] in table 110 rounds {count} {{",
               f"  when true then {stable_body};",
               "  default false;",
               "};",
               "law 500 domains on commit, genesis = forall s in table 110 { post.100.210[s] >= 0 && post.100.210[s] <= 1 };",
               "law 510 stable on commit = stable[0];"]
    for node, descriptor in enumerate(graph.configurations):
        if graph.numbered_names:
            law_name = f"from_n{node}"
            if descriptor[0] == "configuration":
                _, state, pairs = descriptor
                detail = f"configuration state={state} variables={','.join(f'{name}={value}' for name, value in pairs)}"
            else:
                _, state, pairs, event = descriptor
                detail = f"context choice state={state} variables={','.join(f'{name}={value}' for name, value in pairs)} event={event}"
            source.append(f"// n{node}: {detail}")
        else:
            law_name = f"from_{descriptor[1]}"
        source.append(f"law {600 + node} {law_name} on commit = win[{node}];")
    return "\n".join(source) + "\n"


def export_game_project(model, property_id):
    """Lower one game's reduced reachable graph; table export never changes check authority."""
    selected = next((item for item in model.games if item[0] == property_id), None)
    if selected is None:
        raise Refusal(f"Game property {property_id} is not declared")
    graph = _Graph(model)
    steps = [[] for _ in graph.configs]
    for ci, _, _, result in graph.raw():
        if result[0] == "overlap":
            raise Refusal("Game export needs a deterministic transition graph")
        if result[0] == "trap":
            raise Refusal("Game export needs every accepted transition to stay within variable ranges")
        steps[ci].append(result[1])
    graph.explore(steps)
    _, player, objective, formula = selected
    reduced = _reduce_game(_game_graph(graph, model, player), model, objective, formula)
    count = len(reduced.successors)
    if count > 64:
        raise Refusal(f"The table form of this game has {count} nodes; a table holds at most 64")
    return _game_table_project(reduced), count


def _shortest_path(graph, sources, targets, allowed):
    """Shortest path of allowed steps from any source to a target configuration; None if none."""
    parent = {s: None for s in sources}
    queue = deque(sources)
    while queue:
        ci = queue.popleft()
        if ci in targets:
            path = []
            while parent[ci] is not None:
                path.append(parent[ci])
                ci = parent[ci].pre
            return path[::-1]
        for step in graph.steps[ci]:
            if step.post not in parent and allowed(step):
                parent[step.post] = step
                queue.append(step.post)
    return None


def _strongly_connected(nodes, successors):
    """Tarjan's algorithm without recursion; components in reverse topological order."""
    index, low, on_stack, stack, components, counter = {}, {}, set(), [], [], 0
    for root in nodes:
        if root in index:
            continue
        work = [(root, iter(successors(root)))]
        index[root] = low[root] = counter
        counter += 1
        stack.append(root)
        on_stack.add(root)
        while work:
            node, children = work[-1]
            advanced = False
            for child in children:
                if child not in index:
                    index[child] = low[child] = counter
                    counter += 1
                    stack.append(child)
                    on_stack.add(child)
                    work.append((child, iter(successors(child))))
                    advanced = True
                    break
                if child in on_stack:
                    low[node] = min(low[node], index[child])
            if advanced:
                continue
            work.pop()
            if work:
                low[work[-1][0]] = min(low[work[-1][0]], low[node])
            if low[node] == index[node]:
                component = set()
                while True:
                    member = stack.pop()
                    on_stack.discard(member)
                    component.add(member)
                    if member == node:
                        break
                components.append(component)
    return components


def _fair_cycles(graph, allowed, fairness):
    """Configurations of nontrivial allowed-step components that contain a step of every fairness formula."""
    nodes = list(graph.order)
    def successors(ci):
        return [s.post for s in graph.steps[ci] if allowed(s)]
    fair = {}
    for component in _strongly_connected(nodes, successors):
        inner = [s for ci in sorted(component) for s in graph.steps[ci] if s.post in component and allowed(s)]
        if inner and all(any(holds(formula, s) for s in inner) for _, formula in fairness):
            for ci in component:
                fair[ci] = component
    return fair


def _lasso_cycle(graph, entry, component, allowed, fairness):
    """A nonempty cycle from entry inside one component that takes a step of every fairness formula."""
    inside = lambda s: s.post in component and allowed(s)
    cycle, current, positions = [], entry, {}
    for name, formula in fairness:
        goals = {ci for ci in component if any(inside(s) and holds(formula, s) for s in graph.steps[ci])}
        path = _shortest_path(graph, [current], goals, inside)
        cycle += path
        current = path[-1].post if path else current
        step = next(s for s in graph.steps[current] if inside(s) and holds(formula, s))
        positions[name] = len(cycle)
        cycle.append(step)
        current = step.post
    cycle += _shortest_path(graph, [current], {entry}, inside)
    return cycle, positions


def _check_property(graph, model, name, formula):
    """None when the property holds on every (fair, for liveness) execution; else a witness."""
    shape = property_shape(formula)
    goal = shape["goal"]
    if shape["kind"] == "safety":
        for step in graph.reachable_steps():
            if not holds(goal, step):
                return {"kind": "safety", "trace": [*graph.trace_to(step.pre), step], "position": None}
        return None
    if shape["initial"]:
        triggers = list(graph.steps[graph.start])
    else:
        triggers = [s for s in graph.reachable_steps() if holds(shape["trigger"], s)]
    if shape["kind"] == "next":
        for step in triggers:
            failed = next((t for t in graph.steps[step.post] if not holds(goal, t)), None)
            if failed is not None:
                return {"kind": "next", "trace": [*graph.trace_to(step.pre), step, failed]}
        return None
    if shape["kind"] == "bounded":
        # avoid[j]: configurations with a path of j consecutive steps that all miss the goal.
        avoid = [set(graph.order)]
        for _ in range(shape["bound"]):
            avoid.append({ci for ci in graph.order
                          if any(not holds(goal, s) and s.post in avoid[-1] for s in graph.steps[ci])})
        for step in triggers:
            if not holds(goal, step) and step.post in avoid[shape["bound"]]:
                trace, current = [*graph.trace_to(step.pre), step], step.post
                for j in range(shape["bound"], 0, -1):
                    nxt = next(s for s in graph.steps[current] if not holds(goal, s) and s.post in avoid[j - 1])
                    trace.append(nxt)
                    current = nxt.post
                return {"kind": "bounded", "trace": trace}
        return None
    hold = shape["hold"]
    waiting = lambda s: holds(hold, s) and not holds(goal, s)
    anything = lambda s: True
    # Liveness ranges over fair executions only, so every counterexample is a fair lasso.
    fair_all = _fair_cycles(graph, anything, model.fairness)
    fair_reach, queue = set(fair_all), deque(fair_all)
    predecessors = {}
    for ci in graph.order:
        for s in graph.steps[ci]:
            predecessors.setdefault(s.post, set()).add(ci)
    while queue:
        for ci in predecessors.get(queue.popleft(), ()):
            if ci not in fair_reach:
                fair_reach.add(ci)
                queue.append(ci)

    def continuation(ci):
        path = _shortest_path(graph, [ci], set(fair_all), anything)
        entry = path[-1].post if path else ci
        cycle, positions = _lasso_cycle(graph, entry, fair_all[entry], anything, model.fairness)
        return path, cycle, positions

    def lasso(kind, bad):
        path, cycle, positions = continuation(bad[-1].post)
        return {"kind": kind, "prefix": [*bad, *path], "cycle": cycle, "fairness_positions": positions,
                "violation_length": len(bad)}

    exit_step = lambda s: not holds(goal, s) and not holds(hold, s) and s.post in fair_reach
    exits = {ci for ci in graph.order if any(exit_step(s) for s in graph.steps[ci])}
    fair_waiting = _fair_cycles(graph, waiting, model.fairness)
    for step in triggers:
        if holds(goal, step):
            continue
        prefix = [*graph.trace_to(step.pre), step]
        if not holds(hold, step):
            if step.post in fair_reach:
                return lasso("liveness-finite", prefix)
            continue
        path = _shortest_path(graph, [step.post], exits, waiting)
        if path is not None:
            current = path[-1].post if path else step.post
            failed = next(s for s in graph.steps[current] if exit_step(s))
            return lasso("liveness-finite", [*prefix, *path, failed])
        path = _shortest_path(graph, [step.post], set(fair_waiting), waiting)
        if path is not None:
            entry = path[-1].post if path else step.post
            cycle, positions = _lasso_cycle(graph, entry, fair_waiting[entry], waiting, model.fairness)
            return {"kind": "liveness", "prefix": [*prefix, *path], "cycle": cycle, "fairness_positions": positions,
                    "violation_length": None}
    return None


def check(model: Machine, checker_identity="unbound") -> dict:
    checks = {"initial_nonempty": "pass-with-scope", "typing": "pass-with-scope", "determinism": "not-run",
              "update_ranges": "not-run", "coverage_with_explicit_default": "not-run",
              "reachable_invariants": "not-run", "invariant_preservation": "not-run",
              "transition_requirements": "not-run", "temporal_properties": "not-run"}
    if model.games:
        checks["game_properties"] = "not-run"
    graph = _Graph(model)
    steps, traps, defaults = [[] for _ in graph.configs], {}, []
    for ci, event, context, result in graph.raw():
        if result[0] == "overlap":
            state, values = graph.configs[ci][0], graph.value_dicts[ci]
            return counterexample(model, checker_identity, "determinism", {**checks, "determinism": "counterexample"},
                                  witness={"state": state, "variables": dict(values), "event": event, "context": context},
                                  rules=[r.id for r in result[1]])
        if result[0] == "trap":
            traps.setdefault(ci, (event, context, result))
            continue
        steps[ci].append(result[1])
        if result[1].rule == "default":
            defaults.append(result[1])
    checks["determinism"] = "pass-with-scope"
    graph.explore(steps)
    # Admissible configurations satisfy every invariant: the states a lowered contract can hold.
    admissible = [ci for ci in range(len(graph.configs))
                  if all(holds(f, _Step(graph.configs[ci][0], graph.value_dicts[ci], None, {})) for _, f in model.invariants)]
    def range_counterexample(ci):
        event, context, (_, rule, variable, value) = traps[ci]
        low, high = model._ranges[variable]
        witness = {**graph.config_data(ci), "event": event, "context": context, "rule": rule.id,
                   "variable": variable, "value": value, "range": [low, high], "reachable": ci in graph.parent,
                   "trace": [step_data(s) for s in graph.trace_to(ci)] if ci in graph.parent else None}
        return counterexample(model, checker_identity, "range", {**checks, "update_ranges": "counterexample"}, witness=witness)
    reachable_traps = sorted((ci for ci in traps if ci in graph.parent), key=graph.position.get)
    if reachable_traps:
        return range_counterexample(reachable_traps[0])
    for ci in graph.order:
        probe = _Step(graph.configs[ci][0], graph.value_dicts[ci], None, {})
        for name, formula in model.invariants:
            if not holds(formula, probe):
                return counterexample(model, checker_identity, "invariant", {**checks, "reachable_invariants": "counterexample"},
                                      invariant=name, witness={**graph.config_data(ci),
                                                               "trace": [step_data(s) for s in graph.trace_to(ci)]})
    checks["reachable_invariants"] = "pass-with-scope"
    admitted_traps = [ci for ci in admissible if ci in traps]
    if admitted_traps:
        return range_counterexample(admitted_traps[0])
    checks.update(update_ranges="pass-with-scope", coverage_with_explicit_default="pass-with-scope")
    admitted_steps = [s for ci in admissible for s in steps[ci]]
    for step in admitted_steps:
        if step.accepted:
            probe = _Step(step.post_state, step.post_values, None, {})
            for name, formula in model.invariants:
                if not holds(formula, probe):
                    return counterexample(model, checker_identity, "invariant-preservation",
                                          {**checks, "invariant_preservation": "counterexample"},
                                          invariant=name, witness=step_data(step))
    checks["invariant_preservation"] = "pass-with-scope"
    for name, formula in model.requirements:
        failed = next((s for s in graph.reachable_steps() if not holds(formula, s)), None)
        reachable = failed is not None
        failed = failed or next((s for s in admitted_steps if not holds(formula, s)), None)
        if failed is not None:
            witness = {"step": step_data(failed), "reachable": reachable,
                       "trace": [step_data(s) for s in graph.trace_to(failed.pre)] if reachable else None,
                       "tree": truth_tree(formula, failed), "english": formula.render(True)}
            return counterexample(model, checker_identity, "requirement",
                                  {**checks, "transition_requirements": "counterexample"},
                                  requirement=name, witness=witness)
    checks["transition_requirements"] = "pass-with-scope"
    results = {}
    for name, formula in model.properties:
        shape = property_shape(formula)
        mode = ("every infinite execution that satisfies every fairness declaration" if shape["kind"] == "liveness"
                else "every infinite execution")
        found = _check_property(graph, model, name, formula)
        if found is not None:
            witness = {"property": name, "english": formula.render(True), "kind": found["kind"], "mode": mode}
            if "trace" in found:
                witness["trace"] = [step_data(s) for s in found["trace"]]
            else:
                witness.update(prefix=[step_data(s) for s in found["prefix"]],
                               cycle=[step_data(s) for s in found["cycle"]],
                               fairness_positions=found["fairness_positions"],
                               violation_length=found["violation_length"])
            return counterexample(model, checker_identity, "property", {**checks, "temporal_properties": "counterexample"},
                                  property=name, witness=witness)
        results[name] = {"status": "pass-with-scope", "kind": shape["kind"], "mode": mode}
    checks["temporal_properties"] = "pass-with-scope"
    for name, player, objective, formula in model.games:
        base = _game_graph(graph, model, player)
        game = _reduce_game(base, model, objective, formula)
        fixed = _fixed_point_winners(game)
        parity = _parity_winners(game)
        english_objective = {"eventually": "to happen eventually", "always": "to hold always",
                             "infinitely often": "to happen infinitely often"}[objective]
        english = f"Property {name}: {player} can force ({_bare(formula.render(True))}) {english_objective}."
        config_data = lambda node: {"state": game.configurations[node][1],
                                    "variables": dict(game.configurations[node][2])}
        winning_configs = [node for node in game.configuration_nodes if node in fixed]
        if fixed != parity:
            result = counterexample(model, checker_identity, "game", {**checks, "game_properties": "unknown"},
                                    property=name, english=english, player=player, objective=objective,
                                    initial=config_data(game.initial),
                                    winning_configurations=[config_data(node) for node in winning_configs[:16]],
                                    winning_configuration_count=len(winning_configs),
                                    diagnostic="solvers disagree")
            result["status"] = "unknown"
            return result
        if game.initial not in fixed:
            return counterexample(model, checker_identity, "game", {**checks, "game_properties": "counterexample"},
                                  property=name, english=english, player=player, objective=objective,
                                  initial=config_data(game.initial),
                                  winning_configurations=[config_data(node) for node in winning_configs[:16]],
                                  winning_configuration_count=len(winning_configs))
        results[name] = {"status": "pass-with-scope", "kind": "game", "player": player,
                         "objective": objective, "english": english,
                         "configurations": len(game.configuration_nodes),
                         "winning_configurations": len(winning_configs), "initial_wins": True,
                         "solvers": ["fixed-point", "parity"]}
    if model.games:
        checks["game_properties"] = "pass-with-scope"
    scope = ("Finite atomic steps over declared control states and integer variables; context ranges over "
             "fresh Boolean and integer inputs on every step. Ranges, invariant preservation and "
             "requirements cover every admissible configuration (declared and satisfying every invariant); "
             "properties range over every infinite execution from the initial configuration, liveness "
             "only over executions satisfying every fairness declaration")
    if model.games:
        scope += ("; game properties range over every play from the initial configuration in which each player "
                  "chooses the event in its own states and the other player chooses the context.")
    report = {"status": "unsupported" if model.obligations else "pass-with-scope",
              "revision": model.revision, "profile": PROFILE, "checker": checker_identity,
              "scope": scope,
              "checks": checks, "input_tuples": graph.tuples,
              "configurations": len(graph.configs), "admissible_configurations": len(admissible),
              "reachable_configurations": len(graph.order),
              "reachable_states": sorted({graph.configs[ci][0] for ci in graph.order}),
              "unreachable_rules": sorted({r.id for r in model.rules} - {s.rule for s in graph.reachable_steps()}),
              "invariant_count": len(model.invariants), "requirement_count": len(model.requirements),
              "property_count": len(model.properties) + len(model.games), "fairness_count": len(model.fairness),
              "properties": results, "advisories": _advisories(model, graph, [s for row in steps for s in row], admitted_steps),
              "default_count": len(defaults), "default_witness": step_data(defaults[0]) if defaults else None,
              "unresolved": list(model.obligations), "factory": "not-run", "authority": "none"}
    if model.games:
        report["game_property_count"] = len(model.games)
    return report


def _advisories(model, graph, raw_steps, admitted_steps):
    advisories = []
    if not (model.invariants or model.requirements or model.properties or model.games):
        advisories.append({"kind": "no-application-claims",
                           "message": "No invariants, requirements or properties declared; no unstated application "
                                      "safety, authorization or progress policy has been checked."})
    possible = {s.rule for s in raw_steps}
    reachable_states = {graph.configs[ci][0] for ci in graph.order}
    for rule in model.rules:
        if rule.id not in possible:
            advisories.append({"kind": "guard-never-true-at-source", "rule": rule.id,
                               "message": f"Rule {rule.id}: guard is never true at its declared source across all "
                                          "variable values and context valuations."})
        if rule.source not in reachable_states:
            advisories.append({"kind": "unreachable-source", "rule": rule.id,
                               "message": f"Rule {rule.id}: its source state is unreachable from the initial "
                                          "configuration under the declared finite model."})
    for name, formula in model.invariants:
        if all(holds(formula, _Step(state, values, None, {})) for state, values in model.configurations()):
            advisories.append({"kind": "invariant-domain-tautology", "invariant": name,
                               "message": f"Invariant {name} is true on every declared configuration and excludes none of them."})
    for name, formula in model.requirements:
        if formula.op == "implies" and not any(holds(formula.args[0], s) for s in admitted_steps):
            advisories.append({"kind": "requirement-premise-never-true", "requirement": name,
                               "message": f"Requirement {name}: its premise is false on every admissible step, so it holds vacuously."})
    for name, formula in model.properties:
        shape = property_shape(formula)
        if shape["trigger"] is not None and shape["trigger"] != TRUE and not any(
                holds(shape["trigger"], s) for s in graph.reachable_steps()):
            advisories.append({"kind": "property-trigger-never-reached", "property": name,
                               "message": f"Property {name}: its premise is false on every reachable step, so it holds vacuously."})
    if model.fairness and any(property_shape(f)["kind"] == "liveness" for _, f in model.properties):
        if not _fair_cycles(graph, lambda s: True, model.fairness):
            advisories.append({"kind": "no-fair-execution",
                               "message": "No reachable execution satisfies every fairness declaration, so every "
                                          "liveness property holds vacuously."})
    return advisories


def semantic_diff(old: Machine, new: Machine) -> dict:
    def domain(m):
        return (m.states, m.events, m.facts, m.inputs, tuple((v.name, v.low, v.high) for v in m.variables))
    domain_same = domain(old) == domain(new)
    report = {"domain_same": domain_same, "changed_objects": [], "witnesses": [],
              "observations": ["business class", "control state", "variable values", "reason", "range trap",
                               "initial configuration"],
              "excluded_observations": ["Step usage", "budget refusals", "publication identity", "delivery"],
              "old_revision": old.revision, "new_revision": new.revision}
    a = {r.id: r.data() for r in old.rules}
    b = {r.id: r.data() for r in new.rules}
    report["changed_objects"] = sorted(key for key in a.keys() | b.keys() if a.get(key) != b.get(key))
    for key in ["initial", "default", "variables", "invariants", "requirements", "fairness", "properties", "obligations"]:
        if old.data()[key] != new.data()[key]:
            report["changed_objects"].append(key)
    for key in ["players", "games"]:
        if old.data().get(key, []) != new.data().get(key, []):
            report["changed_objects"].append(key)
    if not domain_same:
        report["comparison"] = "domain-changed"
        return report
    if (old.initial, old.initial_values()) != (new.initial, new.initial_values()):
        report["witnesses"].append({"kind": "initial-configuration", "trace": [],
                                    "before": {"state": old.initial, "variables": old.initial_values()},
                                    "after": {"state": new.initial, "variables": new.initial_values()}})
    def observe(model, state, values, event, context):
        result = model.outcome(state, values, event, context)
        if result[0] == "overlap":
            return None
        if result[0] == "trap":
            return {"class": "Trap", "rule": result[1].id, "variable": result[2], "value": result[3]}
        return _outcome_data(result[1])
    for state, values, event, context in old.inputs_domain():
        before, after = observe(old, state, values, event, context), observe(new, state, values, event, context)
        if before is None or after is None:
            report["comparison"] = "invalid-overlap"
            break
        keys = ["class", "state", "variables", "reason", "variable", "value"]
        if any(before.get(k) != after.get(k) for k in keys):
            report["witnesses"].append({"state": state, "variables": values, "event": event, "context": context,
                                        "before": before, "after": after})
    report.setdefault("comparison", "different" if report["witnesses"] else "same-functional-observations-on-declared-domain")
    return report


def assumption_groups(context):
    return {"supplied_assumptions": {"observations": dict(context)}, "authenticated_facts": []}


def _replay_steps(model, inputs):
    if not isinstance(inputs, list):
        raise Refusal("Trace inputs must be an owned array")
    if len(inputs) > MAX_TRACE:
        raise Refusal(f"Trace exceeds {MAX_TRACE} steps")
    state, values, trace = model.initial, model.initial_values(), []
    for item in inputs:
        if not isinstance(item, dict) or set(item) != {"event", "context"}:
            raise Refusal("A trace step needs exactly event and context")
        model.decide(state, values, item["event"], item["context"])  # Validates and refuses traps.
        step = model.outcome(state, dict(values), item["event"], dict(item["context"]))[1]
        trace.append(step)
        state, values = step.post_state, dict(step.post_values)
    return trace


def replay(model: Machine, inputs: list[dict]) -> list[dict]:
    return [{"pre": {"state": s.state, "variables": dict(s.values)}, "event": s.event, "context": dict(s.context),
             "outcome": _outcome_data(s), **assumption_groups(s.context)} for s in _replay_steps(model, inputs)]


def evaluate_finite(node, word, position=0):
    """ZAL-T finite-word meaning: strong next is false at the endpoint; within k is inclusive."""
    size, op = len(word), node.op
    if op == "always":
        return all(evaluate_finite(node.args[0], word, j) for j in range(position, size))
    if op == "never":
        return not any(evaluate_finite(node.args[0], word, j) for j in range(position, size))
    if op == "eventually":
        return any(evaluate_finite(node.args[0], word, j) for j in range(position, size))
    if op == "next":
        return position + 1 < size and evaluate_finite(node.args[0], word, position + 1)
    if op == "within":
        last = min(position + node.args[0], size - 1)
        return any(evaluate_finite(node.args[1], word, j) for j in range(position, last + 1))
    if op == "until":
        return any(evaluate_finite(node.args[1], word, j)
                   and all(evaluate_finite(node.args[0], word, m) for m in range(position, j))
                   for j in range(position, size))
    if op == "not":
        return not evaluate_finite(node.args[0], word, position)
    if op in {"and", "or"}:
        values = [evaluate_finite(a, word, position) for a in node.args]
        return all(values) if op == "and" else any(values)
    if op == "implies":
        return not evaluate_finite(node.args[0], word, position) or evaluate_finite(node.args[1], word, position)
    if op == "iff":
        return evaluate_finite(node.args[0], word, position) == evaluate_finite(node.args[1], word, position)
    return holds(node, word[position])


def _within_bounds(node):
    return ([node.args[0]] if node.op == "within" else []) + [b for a in node.args if isinstance(a, Expr)
                                                              for b in _within_bounds(a)]


def trace_properties(model: Machine, inputs: list[dict]) -> dict:
    """Judge every property on one supplied finite trace with the finite-word meaning."""
    word = _replay_steps(model, inputs)
    if not word:
        raise Refusal("A finite trace judgment needs at least one event")
    results = {}
    for name, formula in model.properties:
        if any(bound >= len(word) for bound in _within_bounds(formula)):
            results[name] = {"result": "refused",
                             "diagnostic": "within k needs k less than the trace length in finite mode"}
            continue
        shape = property_shape(formula)
        if shape["initial"]:
            satisfied, position = evaluate_finite(formula, word), 0
        else:
            body = formula.args[0]
            position = next((j for j in range(len(word)) if not evaluate_finite(body, word, j)), None)
            if formula.op == "never":
                position = next((j for j in range(len(word)) if evaluate_finite(body, word, j)), None)
            satisfied = position is None
        results[name] = {"result": "satisfied" if satisfied else "counterexample",
                         "position": None if satisfied else position}
    return {"kind": "finite-trace-judgment", "profile": PROFILE, "revision": model.revision,
            "trace": replay(model, inputs), "properties": results,
            "semantics": ("finite word of the supplied events: always and never cover the remaining events, "
                          "eventually and until need a witness before the trace ends, next is false at the last "
                          "event and within k covers offsets 0 through k; fairness does not apply to a finite word"),
            "authority": "none"}


def explain(model: Machine, state: str, variables: dict, event: str, context: dict, checker_identity="unbound") -> dict:
    """Explain one exact decision using only the admitted typed AST."""
    model._validate_input(state, variables, event, context)
    variables, context = dict(variables), dict(context)
    probe = _Step(state, variables, event, context)
    result = model.outcome(state, dict(variables), event, dict(context))
    if result[0] == "overlap":
        raise Refusal("Overlapping transitions; no precedence is implied")
    rules = []
    for rule in model.rules:
        guard = truth_tree(rule.guard, probe)
        state_matches, event_matches = rule.source == state, rule.event == event
        updates = []
        for update in rule.updates:
            operand = value_of(update.term, probe)
            before = variables[update.variable]
            after = operand if update.op == "=" else before + operand if update.op == "+=" else before - operand
            low, high = model._ranges[update.variable]
            updates.append({"variable": update.variable, "operation": update.op, "term": update.term.render(),
                            "english": update.render(True), "before": before, "after": after,
                            "in_range": low <= after <= high, "range": [low, high]})
        rules.append({"id": rule.id, "source": rule.source, "event": rule.event,
                      "state_matches": state_matches, "event_matches": event_matches,
                      "guard_english": rule.guard.render(True), "guard": guard, "updates": updates,
                      "enabled": state_matches and event_matches and guard["value"]})
    explanation = {"kind": "decision-explanation", "profile": PROFILE, "revision": model.revision,
                   "checker": checker_identity,
                   "input": {"state": state, "variables": variables, "event": event, "context": context},
                   "admissible": all(holds(f, _Step(state, variables, None, {})) for _, f in model.invariants),
                   "rules": rules, "default_reason": model.default, "effects": "none in this profile",
                   **assumption_groups(context),
                   "scope": "Deterministic explanation of one supplied input; context values are assumptions, "
                            "not authenticated external truth. Temporal properties are judged over executions; "
                            "see the check report or a finite trace judgment. Game readings describe the declared "
                            "objective and scope; they do not disclose a strategy.",
                   "readings": game_readings(model),
                   "method": "typed AST evaluation; no language model", "authority": "none"}
    if result[0] == "trap":
        _, rule, name, value = result
        explanation.update(outcome={"class": "Trap", "rule": rule.id, "variable": name, "value": value,
                                    "range": list(model._ranges[name])},
                           used_default=False, frame="the step is refused; nothing changes",
                           requirements=[], invariants=[])
    else:
        step = result[1]
        explanation.update(outcome=_outcome_data(step), used_default=step.rule == "default",
                           frame="state unchanged" if not step.accepted else
                                 "only the control state and the rule's updated variables may change",
                           requirements=[{"id": n, "english": f.render(True), "value": holds(f, step),
                                          "tree": truth_tree(f, step)} for n, f in model.requirements],
                           invariants=[{"id": n, "english": f.render(True),
                                        "before": holds(f, _Step(state, variables, None, {})),
                                        "after": holds(f, _Step(step.post_state, step.post_values, None, {}))}
                                       for n, f in model.invariants])
    if len(canonical(explanation).encode()) > 512 * 1024:
        raise Refusal("Explanation exceeds the 512 KiB output bound")
    return explanation


def game_readings(model: Machine):
    """Build display-only ATL formula and plain-language rungs for declared game properties."""
    modal = {"eventually": "◇", "always": "□", "infinitely often": "□◇"}
    readings = []
    for name, player, objective, formula in model.games:
        other = next(owner for owner, _ in model.players if owner != player)
        formula_text = formula.render().replace(" == ", " = ").replace(" != ", " ≠ ")
        formula_rung = f"⟨⟨{player}⟩⟩ {modal[objective]} ({formula_text})"
        if formula.op == "state":
            condition = f"the state is {formula.args[0]}"
            lead = "make sure "
        else:
            condition = f"the condition ({formula.render(True)}) holds"
            lead = "make sure that "
        if objective == "infinitely often":
            outcome = f"{lead}{condition} again and again"
        elif objective == "eventually":
            outcome = f"{lead}{condition} at least once"
        else:
            outcome = f"{lead}{condition} at every step"
        fairness = (", provided every declared fairness condition holds infinitely often"
                    if objective != "always" and model.fairness else "")
        readings.append({"property": name, "player": player, "objective": objective,
                         "formula_rung": formula_rung,
                         "plain_rung": f"The {player} can {outcome}, whatever the {other} does{fairness}."})
    return readings


# The construct registry: one entry per statement concept, shared by the legend and help.
CONSTRUCTS = {
    "header": ("zal 2;", "ZAL version 2.", "The language version selects the finite-FSM profile 2 grammar and semantics."),
    "name": ("machine NAME;", "The machine is NAME.", "The machine name is part of the canonical model identity."),
    "states": ("states NAME, NAME;", "The states are NAME, NAME.", "Exactly one declared control-state label is current at a time."),
    "events": ("events NAME, NAME;", "The events are NAME, NAME.", "Events are input labels; their ordering is supplied by the caller."),
    "facts": ("context NAME, NAME;", "The context observations are NAME, NAME.",
              "Context facts are fresh Boolean observations on every step. Naming a fact does not authenticate it."),
    "input": ("input NAME in LOW..=HIGH;", "The input NAME is an integer from LOW to HIGH inclusive.",
              "An input is a fresh integer observation on every step, from its declared inclusive range. Naming it does not authenticate it."),
    "var": ("var NAME in LOW..=HIGH initially VALUE;", "The variable NAME is an integer from LOW to HIGH inclusive, initially VALUE.",
            "A variable is a stored integer with a declared inclusive range and initial value. Only an Accept's updates change it; "
            "an update that would leave the range refuses the step and is a range counterexample, never a wraparound."),
    "initial": ("initial STATE;", "Initially the state is STATE.",
                "The initial state and every variable's initial value form the single start configuration."),
    "frame": ("step atomic; frame control_and_variables;",
              "Each step is atomic. Only the control state and the declared variables may change.",
              "An Accept may change the control state and the variables its rule updates. Reject preserves both. This profile has no effects."),
    "default": ("default reject REASON unchanged;", "Otherwise reject because REASON and keep the state unchanged.",
                "If no rule is enabled, the explicit default rejects and preserves the control state and every variable."),
    "rule": ("rule ID: STATE + EVENT [GUARD] -> accept TARGET, VARIABLE += TERM;",
             "Rule ID: when the state is STATE and the event is EVENT, if GUARD, move to TARGET with VARIABLE increased by TERM.",
             "A rule requires its source state, event and guard together. Source order gives no precedence; overlap is refused. "
             "Updates read the pre-state and apply together."),
    "invariant": ("invariant ID: STATE_FORMULA;", "Invariant ID: STATE_FORMULA.",
                  "An invariant constrains control states and variable values. Reachable safety and all raw Accept successors are checked separately."),
    "require": ("require ID: STEP_FORMULA;", "Requirement ID: STEP_FORMULA.",
                "A requirement relates one step's pre-state, event, observations, outcome and post-state. "
                "It must hold on every step from every declared pre-state, reachable or not."),
    "fairness": ("fairness ID: infinitely often (STEP_FORMULA);", "Fairness ID: assume (STEP_FORMULA) happens infinitely often.",
                 "A fairness assumption restricts liveness checking to executions in which its step formula holds at infinitely many steps. "
                 "It is an assumption about the environment, not a checked fact."),
    "player": ("player NAME moves in STATE;", "Player NAME moves in STATE.",
               "Players choose events in their disjoint declared state lists. A game property needs two players whose lists cover every state."),
    "property": ("property ID: TEMPORAL_FORMULA;", "Property ID: TEMPORAL_FORMULA.",
                 "A property is a temporal formula over the steps of every infinite execution from the initial configuration."),
    "game": ("property ID: NAME can force eventually (STATE_FORMULA);",
             "Property ID: NAME can force (FORMULA) to happen eventually.",
             "A game property checks whether its player can force its state formula over every play, under the declared fairness assumptions."),
    "unresolved": ("unresolved KIND: TEXT;", "Unresolved KIND: TEXT.",
                   "This records an unsupported requirement. It remains visible and blocks factory lowering."),
}
OPERATORS = {
    "!": ("not", "Boolean NOT", "True exactly when its one Boolean operand is false."),
    "&": ("and", "Boolean AND", "True exactly when every Boolean operand is true."),
    "|": ("or", "Boolean OR", "True exactly when at least one Boolean operand is true."),
    "=>": ("if A, then B", "Implication", "False only when A is true and B is false; a false premise makes it true."),
    "<=>": ("A if and only if B", "Equivalence", "True exactly when both operands have the same truth value."),
    "==": ("equals / is", "Equality", "Compares two integer terms, or the control state or event with one declared label."),
    "!=": ("does not equal", "Inequality", "True exactly when two integer terms differ."),
    "<": ("is less than", "Less than", "Compares two exact integer terms."),
    "<=": ("is at most", "At most", "Compares two exact integer terms."),
    ">": ("is greater than", "Greater than", "Compares two exact integer terms."),
    ">=": ("is at least", "At least", "Compares two exact integer terms."),
    "+": ("plus", "Addition", "Exact integer addition inside a formula; only stored updates can leave a range."),
    "-": ("minus", "Subtraction", "Exact integer subtraction inside a formula; only stored updates can leave a range."),
    "'": ("after this event", "Post-state", "Reads the control state or a variable after this step; it is neither negation nor a derivative."),
    "accepted": ("accepted", "Accepted step", "True exactly when this step's outcome is Accept."),
    "always": ("always (P)", "Always", "P holds at this step and every later step."),
    "never": ("never (P)", "Never", "P holds at no step from this one on."),
    "eventually": ("eventually (P)", "Eventually", "P holds now or at some later step; checked only under fairness."),
    "next": ("at the next event, which must exist, (P)", "Strong next",
             "The next step exists and satisfies P. Every machine execution is infinite; a finite trace has no next step at its end."),
    "within": ("now or within the next K events, (P)", "Bounded eventually",
               "P holds at an offset from 0 through K inclusive; within 0 means now. Events are not seconds."),
    "until": ("(P) holds until (Q) happens, and that must happen", "Strong until",
              "Q must occur, possibly now; P holds at every step before it. Checked only under fairness."),
}


def legend(model: Machine):
    singletons = grammar_patterns()
    repeated = statement_patterns()
    entries = []
    for symbolic, english in zip(model.render().splitlines(), model.render(True).splitlines()):
        kind = next((k for k, p in singletons.items() if re.fullmatch(p, symbolic)), None)
        kind = kind or next((k for k, p in repeated.items() if re.fullmatch(p, symbolic)), symbolic.split()[0])
        entries.append({"concept": kind, "symbolic": symbolic, "english": english})
    return {"profile": PROFILE, "entries": entries, "readings": game_readings(model),
            "operators": [{"symbol": s, "english": e} for s, (e, _, _) in OPERATORS.items()],
            "unsupported": sorted(FUTURE), "reserved": sorted(RESERVED),
            "identifiers": "ASCII lowercase letters, digits and underscores; max 32 characters"}


def _objects(model):
    return {**{"state:" + n: "states" for n in model.states}, **{"event:" + n: "events" for n in model.events},
            **{"context:" + n: "facts" for n in model.facts}, **{"input:" + i.name: "input" for i in model.inputs},
            **{"variable:" + v.name: "var" for v in model.variables}, **{"rule:" + r.id: "rule" for r in model.rules},
            **{"invariant:" + n: "invariant" for n, _ in model.invariants},
            **{"requirement:" + n: "require" for n, _ in model.requirements},
            **{"fairness:" + n: "fairness" for n, _ in model.fairness},
            **{"property:" + n: "property" for n, _ in model.properties},
            **{"player:" + name: "player" for name, _ in model.players},
            **{"property:" + name: "game" for name, _, _, _ in model.games},
            "initial": "initial", "default": "default", "frame": "frame"}


def help_topic(model: Machine, topic="", renderer_identity="unbound"):
    """Resolve canonical language/object help from the construct registry, without interpreting prose."""
    if not isinstance(topic, str) or len(topic) > 128:
        raise Refusal("Help topic must be text of at most 128 characters")
    topic = topic.strip()
    pairs = legend(model)["entries"]
    objects = _objects(model)
    aliases = {"machine": "name", "state": "states", "event": "events", "context": "facts", "variable": "var",
               "variables": "var", "inputs": "input", "rules": "rule", "invariants": "invariant",
               "requirement": "require", "requirements": "require", "properties": "property",
               "not": "!", "and": "&", "or": "|", "implies": "=>", "is": "==", "equals": "==", "prime": "'"}
    syntax = {"+=": "Increases a variable by a term on Accept; leaving the range refuses the step.",
              "-=": "Decreases a variable by a term on Accept; leaving the range refuses the step.",
              "->": "Separates a guard from its complete outcome; implication inside formulas is =>.",
              "accept": "Sets the control state to the declared target and applies the rule's updates; it grants no publication authority.",
              "reject": "Returns a declared reason and keeps the control state and every variable unchanged."}
    notes = ["Help describes the current canonical model, not arbitrary editor text or source spans.",
             "Context values are assumptions, not authenticated external truth."]
    if not (model.invariants or model.requirements or model.properties or model.games):
        notes.append("No invariants, requirements or properties declared. Consistency checks do not establish an unstated policy.")
    entries, title, meaning, kind = [], "ZAL profile 2 help", "One finite typed model has exact symbolic and controlled-English views.", "grammar"
    resolved, explicit = topic, topic.startswith("grammar:")
    if explicit:
        topic = topic.removeprefix("grammar:")
    if topic and topic not in objects and not explicit:
        matches = [key for key in objects if key.partition(":")[2] == topic]
        if len(matches) > 1 or matches and (topic in CONSTRUCTS or topic in OPERATORS or topic in aliases or topic in syntax):
            raise Refusal("Ambiguous help topic; use a typed object ID or grammar:TOPIC")
        if matches:
            topic = matches[0]
    if topic in objects and not explicit:
        kind, resolved, category = "object", topic, objects[topic]
        title, meaning = "Meaning of " + topic, CONSTRUCTS[category][2]
        prefix = {"rule": "rule", "invariant": "invariant", "require": "require", "fairness": "fairness",
                  "property": "property", "game": "property", "input": "input", "var": "var",
                  "player": "player"}.get(category)
        name = topic.partition(":")[2]
        marker = (f"{prefix} {name} " if category in {"input", "var", "player"}
                  else f"{prefix} {name}:")
        entries = ([e for e in pairs if e["symbolic"].startswith(marker)]
                   if prefix else [e for e in pairs if e["concept"] == category])
    elif topic:
        resolved = aliases.get(topic, topic)
        if resolved in OPERATORS:
            english, title, meaning = OPERATORS[resolved]
            entries = [{"symbolic": resolved, "english": english}]
            if resolved in {"eventually", "until"}:
                notes.append("Liveness properties need a fairness declaration and are judged only over fair executions.")
        elif resolved in syntax:
            title, meaning = "Rule syntax: " + resolved, syntax[resolved]
            entries = [e for e in pairs if e["concept"] in {"rule", "default"}][:2]
            notes.append("The terminal's accept command is a separate human review action, not a rule outcome.")
        elif resolved in CONSTRUCTS:
            title, meaning = "Language concept: " + resolved, CONSTRUCTS[resolved][2]
            entries = [e for e in pairs if e["concept"] == resolved] or [
                {"symbolic": CONSTRUCTS[resolved][0], "english": CONSTRUCTS[resolved][1]}]
        else:
            raise Refusal("Unknown help topic; use ? for topics or an exact typed object ID")
    else:
        resolved = "overview"
    return {"kind": "language-help", "schema": "zal/help/1", "profile": PROFILE, "revision": model.revision,
            "renderer": renderer_identity, "topic": resolved, "entry_kind": kind, "title": title,
            "meaning": meaning, "entries": entries, "notes": notes,
            "available_topics": sorted(set(CONSTRUCTS) | set(OPERATORS) | set(objects) | set(syntax)),
            "method": "built-in grammar/model registry; no language model", "authority": "none"}
