"""Routing loop acts to the contract judges: the translations, not the judgements.

A law, invariant, scenario or probe written in ``zal/fcis-loop/1`` names fields as the loop language does
(``value``, ``pre.value``, ``post.value``, ``command.action``). The contract judges read ``project.zeno``
relations over numbered paths (``pre.100.110``, ``post.100.110``, ``command.101.120``) and decision-example
lines of numbers. This module writes the one in the other's form, exactly:

* a Boolean field reads as ``PATH == 1``, a comparison of truth values as their equivalence, ``c ? a : b``
  as ``(c && a) || (!c && b)``, a variant name as its number, a negative literal as ``(0 - k)``;
* the loop language's meaning is kept in truth positions: a value is true when it is 1, and ``!x`` on a
  number is ``x == 0`` (never ``!(x == 1)``). A Boolean field in a sum is its value 0 or 1, as in the
  contract's own laws. A form with no exact law text (a truth value compared with a number, ``||`` or ``->``
  on a number, a conditional whose branches mix a truth value and a number) raises ``Unrouted``;
* an invariant's state names become ``post.`` paths (a state formula is a law on ``commit, genesis``);
* a scenario becomes ``# scenario NAME`` and one line per step: the pre-state (genesis, the stated start or
  the previous step's expected successor), the request, the expected class, reason and successor.

Nothing here decides anything: ``contract review --gaps``, ``contract lemma``, ``contract scenario``,
``contract probe`` and the ``loop`` question, teach-back and decompose doors do. A form the contract judges
cannot read (a law scoped to one reject reason, a probe over program outputs) raises ``Unrouted`` and the
workspace says so.

This module is LLM-generated.
"""
from __future__ import annotations

from behavior import Refusal


class Unrouted(Refusal):
    """An act no contract judge can read in this form."""


def law_paths(subject: dict) -> dict:
    """``pre.value`` / ``post.value`` / ``command.action`` -> the numbered path."""
    out = {}
    for numbered, name in subject["paths"].items():
        prefix = numbered.split(".")[0]
        out[f"{prefix}.{name}"] = numbered
    return out


def plain_paths(subject: dict) -> dict:
    """Plain input names -> their numbered pre-state or request path."""
    out = {}
    for numbered, name in subject["paths"].items():
        if not numbered.startswith("post."):
            out[name] = numbered
    return out


def _kind_of(name: str, subject: dict) -> str | None:
    base = name.split(".", 1)[1] if "." in name else name
    kinds = dict(zip(subject["names"], subject["kinds"]))
    return kinds.get(base)


class _Writer:
    def __init__(self, subject: dict, paths: dict):
        self.subject = subject
        self.paths = paths
        self.variants = {}
        for table in subject["variants"].values():
            self.variants.update(table)

    def is_bool(self, node) -> bool:
        tag = node[0]
        if tag == "var":
            return node[1] in self.paths and _kind_of(node[1], self.subject) == "Bool"
        if tag == "not":
            return True
        if tag == "bin":
            return node[1] in ("&&", "||", "->", "==", "!=", "<", "<=", ">", ">=")
        if tag == "if":
            return self.is_bool(node[2]) and self.is_bool(node[3])
        return False

    def is_number(self, node) -> bool:
        """A number, not a truth value: a non-Boolean field or variant, a sum, a literal other than 0 and 1."""
        tag = node[0]
        if tag == "int":
            return node[1] not in (0, 1)
        if tag == "var":
            if node[1] in self.variants and node[1] not in self.paths:
                return True
            return _kind_of(node[1], self.subject) not in (None, "Bool")
        if tag == "bin":
            return node[1] in ("+", "-", "*")
        if tag == "if":
            return self.is_number(node[2]) or self.is_number(node[3])
        return False

    def check_conditionals(self, node) -> None:
        """Refuses a conditional anywhere in ``node`` whose branches mix a truth value and a number."""
        if node[0] == "if":
            a, b = node[2], node[3]
            if (self.is_number(a) and self.is_bool(b)) or (self.is_bool(a) and self.is_number(b)):
                raise Unrouted("law-conditional-mixes-a-truth-value-and-a-number")
        for child in node[1:]:
            if isinstance(child, tuple):
                self.check_conditionals(child)

    def formula(self, node) -> str:
        self.check_conditionals(node)
        return self.truth(node)

    def truth(self, node) -> str:
        tag = node[0]
        if tag == "int":
            if node[1] in (0, 1):
                return "true" if node[1] == 1 else "false"
            raise Unrouted("law-not-boolean")
        if tag == "var":
            if node[1] in self.variants:
                raise Unrouted("law-not-boolean")
            return f"{self.path(node[1])} == 1"
        if tag == "not":
            if self.is_number(node[1]):
                return f"{self.value(node[1])} == 0"
            return f"!({self.truth(node[1])})"
        if tag == "if":
            c, a, b = self.truth(node[1]), self.truth(node[2]), self.truth(node[3])
            return f"(({c}) && ({a})) || (!({c}) && ({b}))"
        if tag == "bin":
            op, a, b = node[1], node[2], node[3]
            if op in ("||", "->") and (self.is_number(a) or self.is_number(b)):
                raise Unrouted("law-truth-operator-on-a-number")
            if op in ("&&", "||", "->"):
                return f"({self.truth(a)}) {op} ({self.truth(b)})"
            if op in ("==", "!=") and (self.is_bool(a) or self.is_bool(b)):
                if self.is_number(a) or self.is_number(b):
                    raise Unrouted("law-compares-a-truth-value-with-a-number")
                x, y = self.truth(a), self.truth(b)
                both = f"(({x}) && ({y})) || (!({x}) && !({y}))"
                return both if op == "==" else f"!({both})"
            if op in ("==", "!=", "<", "<=", ">", ">="):
                return f"{self.value(a)} {op} {self.value(b)}"
        raise Unrouted("law-not-boolean")

    def value(self, node) -> str:
        tag = node[0]
        if tag == "int":
            return f"(0 - {-node[1]})" if node[1] < 0 else str(node[1])
        if tag == "var":
            if node[1] in self.variants and node[1] not in self.paths:
                return str(self.variants[node[1]])
            return self.path(node[1])
        if tag == "bin" and node[1] in ("+", "-"):
            right = self.value(node[3])
            if node[3][0] == "bin":
                right = f"({right})"
            return f"{self.value(node[2])} {node[1]} {right}"
        raise Unrouted("law-value-must-be-a-sum")

    def path(self, name: str) -> str:
        if name not in self.paths:
            raise Unrouted(f"no contract path for {name}")
        return self.paths[name]


def law_formula(node, subject: dict) -> str:
    """A law formula over ``pre.``/``post.``/``command.``/``context.`` names as a ``project.zeno`` relation."""
    return _Writer(subject, law_paths(subject)).formula(node)


def plain_formula(node, subject: dict) -> str:
    """A formula over plain input names as a relation over pre-state and request paths."""
    return _Writer(subject, plain_paths(subject)).formula(node)


def state_formula(node, subject: dict) -> str:
    """A formula over state names as a state law over ``post.`` paths, which ``contract lemma`` reads."""
    paths = {name: numbered for numbered, name in subject["paths"].items() if numbered.startswith("post.")}
    return _Writer(subject, paths).formula(node)


SCOPES = {"accept": "accept", "reject": "reject", "failure": "failure", "commit": "commit", "any": "any"}


def law_scope(scope: str) -> str:
    if scope.startswith("reject:"):
        raise Unrouted("the contract law language has no scope for one reject reason")
    return SCOPES[scope]


# --- values and scenarios ------------------------------------------------------------------------------

def number(subject: dict, name: str, value) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, int):
        return value
    table = subject["variants"].get(name, {})
    if value in table:
        return table[value]
    return int(value)


def evaluate(node, env: dict, variants: dict) -> int:
    """A loop expression's value on one input (scenario successors are simple: constants and sums)."""
    tag = node[0]
    if tag == "int":
        return node[1]
    if tag == "var":
        if node[1] in env:
            return env[node[1]]
        if node[1] in variants:
            return variants[node[1]]
        raise Refusal(f"unknown-name:{node[1]}")
    if tag == "not":
        return int(evaluate(node[1], env, variants) == 0)
    if tag == "if":
        return evaluate(node[2] if evaluate(node[1], env, variants) == 1 else node[3], env, variants)
    op, a, b = node[1], evaluate(node[2], env, variants), evaluate(node[3], env, variants)
    return {"+": a + b, "-": a - b, "==": int(a == b), "!=": int(a != b), "<": int(a < b), "<=": int(a <= b),
            ">": int(a > b), ">=": int(a >= b), "&&": int(a == 1 and b == 1), "||": a if a == 1 else b,
            "->": b if a == 1 else 1}[op]


def genesis_state(subject: dict) -> dict:
    """The genesis values of the state fields."""
    out = {}
    for numbered, name in subject["paths"].items():
        if numbered.startswith("pre."):
            field = numbered.split(".")[2] if numbered.count(".") >= 2 else None
            value = subject["genesis"].get(field)
            if value is None:
                raise Refusal(f"no genesis value for {name}")
            out[name] = int(value)
    return out


def scenario_text(name: str, start, steps: list, subject: dict) -> str:
    """``# scenario NAME`` and one decision-example line per step, in the contract's input order."""
    variants = {}
    for table in subject["variants"].values():
        variants.update(table)
    state = genesis_state(subject) if not start else {f: number(subject, f, v) for f, v in start}
    missing = [f for f in subject["state"] if f not in state]
    if missing:
        raise Refusal(f"scenario start misses {missing[0]}")
    request_names = [n for n in subject["names"] if n not in subject["state"]]
    reason_ids = subject["reason_ids"]
    lines = [f"# scenario {name}"]
    for step in steps:
        given = {f: number(subject, f, v) for f, v in step.get("inputs")}
        if set(given) != set(request_names):
            raise Refusal(f"step {step.get('n')} must give every request field")
        outcome = step.get("outcome")
        env = {**state, **given}
        if outcome.cls == "Reject":
            word, post = "reject", dict(state)
        elif outcome.cls in ("Accept", "CommittedFailure"):
            word = "accept" if outcome.cls == "Accept" else "failure"
            post = dict(state)
            for field, expr in outcome.sets:
                post[field] = evaluate(expr, env, variants)
        else:
            raise Unrouted("a scenario step expects an accept, a reject or a committed failure")
        reason = "-" if outcome.cls == "Accept" else str(reason_ids[outcome.reason])
        numbers = [str(state[f]) for f in subject["state"]] + [str(given[f]) for f in request_names]
        lines.append(" ".join(numbers) + f" | {word} {reason} " + " ".join(str(post[f]) for f in subject["state"])
                     + " | -")
        state = post
    return "\n".join(lines) + "\n"
