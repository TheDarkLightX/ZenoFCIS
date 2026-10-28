"""Bounded, declarative CFG parsing for ZAL notation grammar profiles.

The parser is a scannerless Earley recognizer with a packed parse forest.  It
accepts left recursion and empty productions, consumes the complete input, and
rejects syntactic ambiguity even when alternative parses would yield the same
AST.  A productive recursive forest denotes infinitely many parse trees and is
also ambiguous.  No grammar content is evaluated as host code or a regex.

``syntax.skip_whitespace`` defaults to true.  Built-in tokens are signed decimal
``int``, ASCII ``identifier``, and JSON double-quoted ``string``.  Literal and
token matches are deterministic; identifiers and integers use maximal munch.
Token limits and work limits are deliberate resource bounds, not assertions
that an exhausted grammar has no parse.  Constructor/sort typing belongs to the
core validator, which should also validate the returned ``{tag,fields,children}``.
"""

from __future__ import annotations

import json
from collections import defaultdict, deque
from dataclasses import dataclass
from typing import Any


class ParseError(ValueError):
    """Invalid syntax profile, malformed action, or no complete parse."""


class ParseLimit(ParseError):
    """Parsing or syntax validation exhausted an explicit resource bound."""


class Ambiguous(ParseError):
    """The input has more than one syntactic parse tree."""


_TOKEN_KINDS = frozenset({"int", "identifier", "string"})
_COERCIONS = frozenset({"int", "str", "bool"})
_TOKEN_CHARS = 16_384
_INTEGER_DIGITS = 1_024


@dataclass(frozen=True)
class _Terminal:
    value: str


@dataclass(frozen=True)
class _Rule:
    lhs: str
    rhs: tuple[tuple[str, str], ...]
    tag: str | None
    children: tuple[int, ...]
    fields: tuple[tuple[str, int, str], ...]
    forward: int | None


class _Budget:
    def __init__(self, max_items: int, max_steps: int):
        for name, value in (("max_items", max_items), ("max_steps", max_steps)):
            if type(value) is not int or value < 1:
                raise ParseError(f"{name} must be a positive integer")
        self.max_items = max_items
        self.steps = max_steps

    def spend(self, count: int = 1) -> None:
        self.steps -= count
        if self.steps < 0:
            raise ParseLimit("syntax work budget exhausted (max_steps)")


def _keys(value: dict, allowed: set[str], where: str) -> None:
    unknown = set(value) - allowed
    if unknown:
        raise ParseError(f"unknown {where} fields: {sorted(map(str, unknown))}")


def _name(value: Any, where: str) -> str:
    if not isinstance(value, str) or not value:
        raise ParseError(f"{where} must be a nonempty string")
    return value


def _index(value: Any, rhs: list, where: str) -> int:
    if type(value) is not int or not 0 <= value < len(rhs):
        raise ParseError(f"{where} must be an in-range RHS index")
    return value


def _prepare(grammar: dict, budget: _Budget) -> tuple[str, tuple[_Rule, ...], bool]:
    if not isinstance(grammar, dict):
        raise ParseError("grammar must be an object")
    syntax = grammar.get("syntax")
    if not isinstance(syntax, dict):
        raise ParseError("grammar.syntax must be an object")
    _keys(syntax, {"start", "rules", "skip_whitespace"}, "syntax")
    start = _name(syntax.get("start"), "syntax.start")
    skip = syntax.get("skip_whitespace", True)
    if type(skip) is not bool:
        raise ParseError("syntax.skip_whitespace must be Boolean")
    raw_rules = syntax.get("rules")
    if not isinstance(raw_rules, list) or not raw_rules:
        raise ParseError("syntax.rules must be a nonempty array")
    if len(raw_rules) > budget.max_items:
        raise ParseLimit("syntax rule count exceeds max_items")
    constructors = grammar.get("constructors")
    if not isinstance(constructors, dict):
        raise ParseError("grammar.constructors must be an object")
    rules: list[_Rule] = []
    for number, raw in enumerate(raw_rules):
        budget.spend()
        where = f"syntax rule {number}"
        if not isinstance(raw, dict):
            raise ParseError(f"{where} must be an object")
        _keys(raw, {"lhs", "rhs", "tag", "children", "fields", "forward"}, where)
        lhs = _name(raw.get("lhs"), f"{where}.lhs")
        raw_rhs = raw.get("rhs")
        if not isinstance(raw_rhs, list):
            raise ParseError(f"{where}.rhs must be an array")
        budget.spend(len(raw_rhs))
        rhs = []
        for symbol in raw_rhs:
            if not isinstance(symbol, dict) or len(symbol) != 1:
                raise ParseError(f"{where}: each RHS symbol must have exactly one kind")
            kind, value = next(iter(symbol.items()))
            if kind not in {"ref", "lit", "token"}:
                raise ParseError(f"{where}: unknown RHS symbol kind {kind!r}")
            value = _name(value, f"{where}.{kind}")
            budget.spend(len(value))
            if kind == "token" and value not in _TOKEN_KINDS:
                raise ParseError(f"{where}: unsupported token {value!r}")
            rhs.append((kind, value))
        if ("tag" in raw) == ("forward" in raw):
            raise ParseError(f"{where}: specify exactly one of tag or forward")
        if "forward" in raw:
            if "children" in raw or "fields" in raw:
                raise ParseError(f"{where}: forward cannot have fields or children")
            forward = _index(raw["forward"], rhs, f"{where}.forward")
            if rhs[forward][0] != "ref":
                raise ParseError(f"{where}: forward must select a nonterminal")
            rules.append(_Rule(lhs, tuple(rhs), None, (), (), forward))
            continue
        tag = _name(raw["tag"], f"{where}.tag")
        if tag not in constructors:
            raise ParseError(f"{where}: undeclared constructor {tag!r}")
        raw_children = raw.get("children", [])
        if not isinstance(raw_children, list):
            raise ParseError(f"{where}.children must be an array")
        children = []
        for child in raw_children:
            budget.spend()
            index = _index(child, rhs, f"{where}.children")
            if rhs[index][0] != "ref":
                raise ParseError(f"{where}: children must select nonterminals")
            children.append(index)
        raw_fields = raw.get("fields", {})
        if not isinstance(raw_fields, dict):
            raise ParseError(f"{where}.fields must be an object")
        fields = []
        for field, binding in raw_fields.items():
            budget.spend()
            _name(field, f"{where}: field name")
            if not isinstance(binding, dict) or set(binding) != {"token", "as"}:
                raise ParseError(f"{where}: field binding requires only token and as")
            index = _index(binding["token"], rhs, f"{where}: field token")
            if rhs[index][0] == "ref":
                raise ParseError(f"{where}: field binding must select a terminal")
            coercion = binding["as"]
            if not isinstance(coercion, str) or coercion not in _COERCIONS:
                raise ParseError(f"{where}: unsupported field coercion")
            fields.append((field, index, coercion))
        rules.append(_Rule(lhs, tuple(rhs), tag, tuple(children), tuple(fields), None))
    declared = {rule.lhs for rule in rules}
    if start not in declared:
        raise ParseError("syntax.start has no productions")
    for rule in rules:
        for kind, value in rule.rhs:
            budget.spend()
            if kind == "ref" and value not in declared:
                raise ParseError(f"undeclared nonterminal {value!r}")
    return start, tuple(rules), skip


def validate_syntax(grammar: dict) -> None:
    """Validate an optional syntax descriptor without importing the core.

    Absence of ``syntax`` is valid: the grammar may use direct AST input or an
    explicitly installed external parser.  Envelope and constructor typing are
    the core's responsibility.  Structural validation uses the default bounds.
    """
    if not isinstance(grammar, dict):
        raise ParseError("grammar must be an object")
    if "syntax" in grammar:
        _prepare(grammar, _Budget(20_000, 100_000))


def _identifier_first(char: str) -> bool:
    return "a" <= char <= "z" or "A" <= char <= "Z" or char == "_"


def _digit(char: str) -> bool:
    return "0" <= char <= "9"


def _integer(value: str, budget: _Budget) -> int:
    budget.spend(len(value))
    digits = value[1:] if value[:1] in {"+", "-"} else value
    if not digits or not all(_digit(char) for char in digits):
        raise ParseError("int coercion requires a signed decimal integer")
    if len(digits) > _INTEGER_DIGITS:
        raise ParseLimit("integer token exceeds the 1024-digit bound")
    return int(value, 10)


def _coerce(value: str, as_type: str, budget: _Budget) -> str | int | bool:
    if as_type == "str":
        return value
    if as_type == "int":
        return _integer(value, budget)
    if value not in {"true", "false"}:
        raise ParseError("bool coercion accepts exactly 'true' or 'false'")
    return value == "true"


def parse_text(
    text: str, grammar: dict, *, max_items: int = 20_000, max_steps: int = 100_000
) -> dict:
    """Parse a unique complete CFG derivation into an exact term dictionary.

    Raises ``Ambiguous`` for multiple parse trees, ``ParseLimit`` for bounded
    work exhaustion, and ``ParseError`` for malformed grammar/actions or input
    outside the grammar.  The returned term still needs core constructor and
    sort validation.  ``max_items`` also bounds the number of emitted AST nodes.
    """
    if not isinstance(text, str):
        raise ParseError("text must be a string")
    budget = _Budget(max_items, max_steps)
    start, rules, skip_whitespace = _prepare(grammar, budget)
    by_lhs: dict[str, list[int]] = defaultdict(list)
    for number, rule in enumerate(rules):
        by_lhs[rule.lhs].append(number)
    skip_cache: dict[int, int] = {}
    scan_cache: dict[tuple, tuple[int, _Terminal] | None] = {}

    def skip(position: int) -> int:
        if not skip_whitespace:
            return position
        if position not in skip_cache:
            end = position
            while end < len(text) and text[end].isspace():
                budget.spend()
                end += 1
            skip_cache[position] = end
        return skip_cache[position]

    def scan(kind: str, name: str, position: int) -> tuple[int, _Terminal] | None:
        key = (kind, name, position)
        if key in scan_cache:
            return scan_cache[key]
        budget.spend()
        end = position
        value: str | None = None
        if kind == "lit":
            budget.spend(len(name))
            if text.startswith(name, position):
                end += len(name)
                value = name
        elif name == "identifier":
            if end < len(text) and _identifier_first(text[end]):
                end += 1
                while end < len(text) and (_identifier_first(text[end]) or _digit(text[end])):
                    budget.spend()
                    end += 1
                    if end - position > _TOKEN_CHARS:
                        raise ParseLimit("identifier token exceeds the character bound")
                value = text[position:end]
        elif name == "int":
            if end < len(text) and text[end] in "+-":
                end += 1
            first_digit = end
            while end < len(text) and _digit(text[end]):
                budget.spend()
                end += 1
                if end - first_digit > _INTEGER_DIGITS:
                    raise ParseLimit("integer token exceeds the 1024-digit bound")
            if end > first_digit:
                value = text[position:end]
        elif name == "string":
            if end < len(text) and text[end] == '"':
                end += 1
                escaped = False
                while end < len(text):
                    budget.spend()
                    char = text[end]
                    end += 1
                    if end - position > _TOKEN_CHARS:
                        raise ParseLimit("string token exceeds the character bound")
                    if escaped:
                        escaped = False
                    elif char == "\\":
                        escaped = True
                    elif char == '"':
                        try:
                            decoded = json.loads(text[position:end])
                        except (ValueError, RecursionError):
                            break
                        if any(0xD800 <= ord(c) <= 0xDFFF for c in decoded):
                            break
                        value = decoded
                        break
        result = None if value is None else (skip(end), _Terminal(value))
        scan_cache[key] = result
        return result

    # Item nodes are ("item", rule, dot, origin, end).  Symbol nodes are
    # ("symbol", nonterminal, origin, end).  Forest alternatives reference
    # these nodes; terminal leaves are inert strings, never executable actions.
    forest: dict[tuple, set[tuple]] = defaultdict(set)
    items: set[tuple] = set()
    agenda: deque[tuple] = deque()
    waiting: dict[tuple[str, int], set[tuple]] = defaultdict(set)
    completed: dict[tuple[str, int], set[tuple]] = defaultdict(set)

    def add(item: tuple, alternative: tuple) -> None:
        budget.spend()
        forest[item].add(alternative)
        if item not in items:
            if len(items) >= max_items:
                raise ParseLimit("Earley item bound exhausted (max_items)")
            items.add(item)
            agenda.append(item)

    def advance(item: tuple, child: tuple | _Terminal, end: int) -> None:
        _, number, dot, origin, _ = item
        add(("item", number, dot + 1, origin, end), (item, child))

    initial = skip(0)
    for number in by_lhs[start]:
        add(("item", number, 0, initial, initial), ())
    while agenda:
        budget.spend()
        item = agenda.popleft()
        _, number, dot, origin, position = item
        rule = rules[number]
        if dot == len(rule.rhs):
            symbol = ("symbol", rule.lhs, origin, position)
            forest[symbol].add((item,))
            key = (rule.lhs, origin)
            if symbol not in completed[key]:
                completed[key].add(symbol)
                for parent in tuple(waiting[key]):
                    budget.spend()
                    advance(parent, symbol, position)
        else:
            kind, value = rule.rhs[dot]
            if kind == "ref":
                key = (value, position)
                waiting[key].add(item)
                for predicted in by_lhs[value]:
                    budget.spend()
                    add(("item", predicted, 0, position, position), ())
                for symbol in tuple(completed[key]):
                    budget.spend()
                    advance(item, symbol, symbol[3])
            else:
                match = scan(kind, value, position)
                if match is not None:
                    end, token = match
                    advance(item, token, end)

    root = ("symbol", start, initial, len(text))
    if root not in forest:
        farthest = max((item[4] for item in items), default=initial)
        raise ParseError(f"no complete parse; farthest offset {farthest} of {len(text)}")

    # Topological traversal of the reachable forest.  A cycle here is a
    # productive recursive derivation (recognition already found finite base
    # derivations), hence infinitely many parse trees rather than a unique one.
    colors: dict[tuple, int] = {root: 1}
    postorder = []

    def dependencies(node: tuple):
        for alternative in forest[node]:
            for part in alternative:
                if isinstance(part, tuple):
                    yield part

    stack = [(root, iter(dependencies(root)))]
    while stack:
        budget.spend()
        node, iterator = stack[-1]
        child = next(iterator, None)
        if child is None:
            colors[node] = 2
            postorder.append(node)
            stack.pop()
        elif colors.get(child) == 1:
            raise Ambiguous("input has a productive recursive parse forest")
        elif child not in colors:
            colors[child] = 1
            stack.append((child, iter(dependencies(child))))

    counts: dict[tuple, int] = {}
    for node in postorder:
        total = 0
        for alternative in forest[node]:
            budget.spend()
            product = 1
            for part in alternative:
                budget.spend()
                if isinstance(part, tuple):
                    product = min(2, product * counts[part])
            total = min(2, total + product)
        counts[node] = total
    if counts[root] != 1:
        raise Ambiguous("input has multiple syntactic parse trees")

    def rhs_values(symbol: tuple) -> tuple[_Rule, list]:
        item = next(iter(forest[symbol]))[0]
        rule = rules[item[1]]
        values = []
        while item[2]:
            budget.spend()
            previous, child = next(iter(forest[item]))
            values.append(child)
            item = previous
        values.reverse()
        return rule, values

    # Construct the result iteratively, with no aliasing between repeated child
    # occurrences and with a bound on expansion of a compact epsilon forest.
    tasks: list[tuple] = [("visit", root)]
    results: list[dict] = []
    emitted = 0
    while tasks:
        budget.spend()
        task = tasks.pop()
        if task[0] == "assemble":
            _, tag, fields, count = task
            children = results[-count:] if count else []
            if count:
                del results[-count:]
            emitted += 1
            if emitted > max_items:
                raise ParseLimit("AST expansion exceeds max_items")
            results.append({"tag": tag, "fields": fields, "children": children})
            continue
        rule, values = rhs_values(task[1])
        if rule.forward is not None:
            tasks.append(("visit", values[rule.forward]))
            continue
        fields = {}
        for name, index, coercion in rule.fields:
            budget.spend()
            fields[name] = _coerce(values[index].value, coercion, budget)
        tasks.append(("assemble", rule.tag, fields, len(rule.children)))
        for index in reversed(rule.children):
            tasks.append(("visit", values[index]))
    return results[0]
