"""ZAL finite algebraic data types, profile zal/finite-types/1: aliases, records and variants.

No eval, model calls or I/O. The symbolic and controlled-English declarations parse
to the same frozen value. Recursion is refused, so every admitted type has finitely
many values, and each type reports how many: a record multiplies the counts of its
fields and a variant adds the counts of its cases.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import cache
from types import MappingProxyType
import itertools
import re

from behavior import NAME, Refusal, canonical


PROFILE = "zal/finite-types/1"
TYPE_NAME = r"[A-Z][A-Za-z0-9]{0,31}"
INTEGER = r"-?[0-9]{1,20}"
MAX_TYPES = 64
MAX_MEMBERS = 32
MAX_DEPTH = 8
MAX_CAP = 1 << 20
MAX_MAGNITUDE = (1 << 64) - 1
# Counts above 2^EXACT_BITS are reported as "more than 2^EXACT_BITS", never rounded.
EXACT_BITS = 4096
LIMIT = 1 << EXACT_BITS
# Primitive keywords never name a case, so `type A = unit;` is always an alias.
KEYWORDS = frozenset({"unit", "bool", "int", "bytes", "list", "of", "true", "false", "nothing"})
PRIMITIVES = ("unit", "bool", "int", "bytes_max", "bytes", "list")
SYMBOLIC_TOKENS = r"\.\.=|->|0x[0-9a-f]*|-?[0-9]+|[A-Za-z][A-Za-z0-9_]*|\S"
ENGLISH_TOKENS = r"0x[0-9a-f]*|-?[0-9]+|[A-Za-z][A-Za-z0-9_]*|\S"


@dataclass(frozen=True)
class Ty:
    """One type expression: a primitive, a bounded list or a reference to a declared type."""
    kind: str
    args: tuple = ()

    def __post_init__(self):
        kind, args = self.kind, self.args
        if not isinstance(args, tuple):
            raise Refusal("Unknown or mutable type node")
        if kind in {"unit", "bool"}:
            ok = args == ()
        elif kind == "int":
            ok = (len(args) == 2 and all(type(a) is int and abs(a) <= MAX_MAGNITUDE for a in args)
                  and args[0] <= args[1])
            if len(args) == 2 and all(type(a) is int for a in args) and not ok:
                raise Refusal(f"An integer type needs LOW <= HIGH within -{MAX_MAGNITUDE}..={MAX_MAGNITUDE}")
        elif kind in {"bytes_max", "bytes"}:
            ok = len(args) == 1 and type(args[0]) is int and 1 <= args[0] <= MAX_CAP
            if len(args) == 1 and type(args[0]) is int and not ok:
                raise Refusal(f"A byte cap or length lies within 1..={MAX_CAP}")
        elif kind == "list":
            ok = len(args) == 2 and isinstance(args[0], Ty) and type(args[1]) is int and 1 <= args[1] <= MAX_CAP
            if len(args) == 2 and type(args[1]) is int and not 1 <= args[1] <= MAX_CAP:
                raise Refusal(f"A list item cap lies within 1..={MAX_CAP}")
        elif kind == "ref":
            ok = len(args) == 1 and isinstance(args[0], str) and re.fullmatch(TYPE_NAME, args[0]) is not None
        else:
            ok = False
        if not ok:
            raise Refusal("Unknown or malformed type node")

    def data(self):
        return [self.kind, *[a.data() if isinstance(a, Ty) else a for a in self.args]]

    def render(self, english=False) -> str:
        kind, args = self.kind, self.args
        if kind == "ref":
            return args[0]
        if english:
            return {"unit": lambda: "nothing", "bool": lambda: "a Boolean",
                    "int": lambda: f"an integer from {args[0]} to {args[1]} inclusive",
                    "bytes_max": lambda: f"at most {args[0]} bytes", "bytes": lambda: f"exactly {args[0]} bytes",
                    "list": lambda: f"a list of at most {args[1]} items of {args[0].render(True)}"}[kind]()
        return {"unit": lambda: "unit", "bool": lambda: "bool", "int": lambda: f"int {args[0]}..={args[1]}",
                "bytes_max": lambda: f"bytes ..={args[0]}", "bytes": lambda: f"bytes {args[0]}",
                "list": lambda: f"list ..={args[1]} of {args[0].render()}"}[kind]()


@dataclass(frozen=True)
class Decl:
    """One named type: an alias, a record of fields, a variant of cases, or a profile's opaque built-in."""
    name: str
    form: str
    body: object

    def data(self):
        if self.form == "alias":
            return [self.name, "alias", self.body.data()]
        if self.form == "opaque":
            return [self.name, "opaque", str(self.body[0])]
        return [self.name, self.form, [[n, t.data() if t is not None else None] for n, t in self.body]]

    def render(self, english=False) -> str:
        if self.form == "opaque":
            raise Refusal(f"{self.name} is built into the profile and has no source form")
        if english:
            if self.form == "alias":
                text = self.body.render(True)
            elif self.form == "record":
                text = "a record with " + (("fields " + render_members(self.body, True)) if self.body else "no fields")
            else:
                text = "one of " + render_members(self.body, True, True)
            return f"The type {self.name} is {text}."
        if self.form == "alias":
            text = self.body.render()
        elif self.form == "record":
            text = "{" + render_members(self.body) + "}"
        else:
            text = render_members(self.body, cases=True)
        return f"type {self.name} = {text};"


def render_members(members, english=False, cases=False):
    """Fields `name: T` / `name (T)`, or cases `name` / `name(T)` / `name (T)`, joined in order."""
    parts = []
    for name, ty in members:
        if ty is None:
            parts.append(name)
        elif english:
            parts.append(f"{name} ({ty.render(True)})")
        else:
            parts.append(f"{name}({ty.render()})" if cases else f"{name}: {ty.render()}")
    return (" | " if cases and not english else ", ").join(parts)


class Tokens:
    """A bounded token stream over one statement fragment."""

    def __init__(self, text, english=False):
        if not isinstance(text, str) or len(text) > 4096 or not text.isascii():
            raise Refusal("A type or value text is ASCII of at most 4096 characters")
        self.items = re.findall(ENGLISH_TOKENS if english else SYMBOLIC_TOKENS, text)
        if len(self.items) > 1024:
            raise Refusal("A type or value text exceeds 1024 lexical tokens")
        self.index, self.english = 0, english

    def peek(self, offset=0):
        position = self.index + offset
        return self.items[position] if position < len(self.items) else None

    def at(self, *words):
        return tuple(self.items[self.index:self.index + len(words)]) == words

    def take(self, *words):
        if words and not self.at(*words):
            raise Refusal(f"Expected {' '.join(words)} near {self.peek() or 'the end'}")
        if self.index >= len(self.items):
            raise Refusal("Unexpected end of a type or value")
        self.index += max(len(words), 1)
        return self.items[self.index - 1]

    def integer(self):
        token = self.take()
        if not re.fullmatch(INTEGER, token):
            raise Refusal(f"Expected an integer near {token}")
        return int(token)

    def done(self):
        if self.index != len(self.items):
            raise Refusal(f"Unexpected token {self.items[self.index]}")


def parse_ty(tokens: Tokens) -> Ty:
    """One type expression in the stream's surface."""
    token = tokens.peek()
    if token is None:
        raise Refusal("Expected a type")
    if re.fullmatch(TYPE_NAME, token):
        tokens.take()
        return Ty("ref", (token,))
    if tokens.english:
        if tokens.at("nothing"):
            tokens.take()
            return Ty("unit")
        if tokens.at("a", "Boolean"):
            tokens.take("a", "Boolean")
            return Ty("bool")
        if tokens.at("an", "integer", "from"):
            tokens.take("an", "integer", "from")
            low = tokens.integer()
            tokens.take("to")
            high = tokens.integer()
            tokens.take("inclusive")
            return Ty("int", (low, high))
        if tokens.at("at", "most"):
            tokens.take("at", "most")
            cap = tokens.integer()
            tokens.take("bytes")
            return Ty("bytes_max", (cap,))
        if tokens.at("exactly"):
            tokens.take("exactly")
            length = tokens.integer()
            tokens.take("bytes")
            return Ty("bytes", (length,))
        if tokens.at("a", "list", "of", "at", "most"):
            tokens.take("a", "list", "of", "at", "most")
            cap = tokens.integer()
            tokens.take("items", "of")
            return Ty("list", (parse_ty(tokens), cap))
        raise Refusal(f"Expected a type near {token}")
    if token in {"unit", "bool"}:
        tokens.take()
        return Ty(token)
    if token == "int":
        tokens.take()
        low = tokens.integer()
        tokens.take("..=")
        return Ty("int", (low, tokens.integer()))
    if token == "bytes":
        tokens.take()
        if tokens.at("..="):
            tokens.take("..=")
            return Ty("bytes_max", (tokens.integer(),))
        return Ty("bytes", (tokens.integer(),))
    if token == "list":
        tokens.take("list", "..=")
        cap = tokens.integer()
        tokens.take("of")
        return Ty("list", (parse_ty(tokens), cap))
    raise Refusal(f"Expected a type near {token}")


def parse_members(tokens: Tokens, cases: bool, close=None):
    """Fields `name: T` (symbolic) / `name (T)` (English), or cases with an optional `(T)` payload."""
    members = []
    separator = "|" if cases and not tokens.english else ","
    while True:
        name = tokens.take()
        if not re.fullmatch(NAME, name) or cases and name in KEYWORDS:
            raise Refusal(f"Expected a {'case' if cases else 'field'} name near {name}")
        if cases and not tokens.at("("):
            members.append((name, None))
        elif cases or tokens.english:
            tokens.take("(")
            ty = parse_ty(tokens)
            tokens.take(")")
            if cases and ty == Ty("unit"):
                raise Refusal(f"Write case {name} without a payload instead of a unit payload")
            members.append((name, ty))
        else:
            tokens.take(":")
            members.append((name, parse_ty(tokens)))
        if len(members) > MAX_MEMBERS:
            raise Refusal(f"A record or variant has at most {MAX_MEMBERS} members")
        if not tokens.at(separator):
            if close is not None:
                tokens.take(close)
            return tuple(members)
        tokens.take()


def parse_decl(name: str, body: str, english=False) -> Decl:
    """The body of `type NAME = BODY;` or `The type NAME is BODY.`"""
    tokens = Tokens(body, english)
    if english and tokens.at("a", "record", "with", "no", "fields"):
        tokens.take("a", "record", "with", "no", "fields")
        decl = Decl(name, "record", ())
    elif english and tokens.at("a", "record", "with", "fields"):
        tokens.take("a", "record", "with", "fields")
        decl = Decl(name, "record", parse_members(tokens, False))
    elif english and tokens.at("one", "of"):
        tokens.take("one", "of")
        decl = Decl(name, "variant", parse_members(tokens, True))
    elif not english and tokens.at("{", "}"):
        tokens.take("{", "}")
        decl = Decl(name, "record", ())
    elif not english and tokens.at("{"):
        tokens.take("{")
        decl = Decl(name, "record", parse_members(tokens, False, "}"))
    elif (not english and tokens.peek() is not None and re.fullmatch(NAME, tokens.peek())
          and tokens.peek() not in KEYWORDS):
        decl = Decl(name, "variant", parse_members(tokens, True))
    else:
        decl = Decl(name, "alias", parse_ty(tokens))
    tokens.done()
    return decl


def type_patterns(english=False):
    """The type statement's pattern, shared by every profile that declares types."""
    return rf"The type ({TYPE_NAME}) is (.+)\." if english else rf"type ({TYPE_NAME}) = (.+);"


def card_sum(values):
    total = 0
    for value in values:
        if value is None:
            return None
        total += value
    return total if total <= LIMIT else None


def card_product(values):
    total = 1
    for value in values:
        if value is None:
            return None
        total *= value
        if total > LIMIT:
            return None
    return total


def card_sequences(count, cap):
    """Sequences of 0..=cap items drawn from `count` values: 1 + count + ... + count^cap."""
    if count is None:
        return None
    if count == 1:
        return cap + 1 if cap + 1 <= LIMIT else None
    if cap * (count.bit_length() - 1) > EXACT_BITS:
        return None
    total = (count ** (cap + 1) - 1) // (count - 1)
    return total if total <= LIMIT else None


def card_text(count) -> str:
    """Lossless count text: decimal or a short sum of powers of two; a bound only above the limit."""
    if count is None:
        return f"more than 2^{EXACT_BITS}"
    if count < 1 << 64:
        return str(count)
    powers = [bit for bit in range(count.bit_length()) if count >> bit & 1]
    if len(powers) <= 4:
        return " + ".join("1" if bit == 0 else f"2^{bit}" for bit in reversed(powers))
    return str(count)


def card_data(count):
    return {"text": card_text(count), "exact": str(count) if count is not None else None,
            "within_exact_limit": count is not None}


@dataclass(frozen=True)
class Types:
    """A checked set of declared types plus a profile's opaque built-ins, sorted by name."""

    decls: tuple
    prelude: tuple = ()

    @property
    def by_name(self):
        """A read-only lookup derived solely from the immutable declarations."""
        return MappingProxyType({d.name: d for d in (*self.prelude, *self.decls)})

    def __post_init__(self):
        decls, prelude = self.decls, self.prelude
        if (not isinstance(decls, tuple) or not isinstance(prelude, tuple) or len(decls) > MAX_TYPES
                or any(not isinstance(d, Decl) for d in (*decls, *prelude))):
            raise Refusal(f"At most {MAX_TYPES} declared types")
        names = [d.name for d in decls]
        builtin = {d.name for d in prelude}
        for name in names:
            if names.count(name) > 1 or name in builtin:
                raise Refusal(f"Type {name} is declared twice")
        if names != sorted(names):
            raise Refusal("IR types must be sorted by name")
        for decl in prelude:
            if (decl.form != "opaque" or not re.fullmatch(TYPE_NAME, decl.name)
                    or not isinstance(decl.body, tuple) or len(decl.body) != 2
                    or type(decl.body[0]) is not int or not 1 <= decl.body[0] <= LIMIT
                    or not isinstance(decl.body[1], str)):
                raise Refusal(f"Invalid opaque built-in {decl.name}")
        for decl in decls:
            self._validate(decl)
        verified = set()
        for decl in decls:
            self._acyclic(decl.name, [], verified)
        for decl in decls:
            if self.depth(Ty("ref", (decl.name,))) > MAX_DEPTH:
                raise Refusal(f"Type {decl.name} nests deeper than {MAX_DEPTH} levels")

    def _validate(self, decl):
        if not re.fullmatch(TYPE_NAME, decl.name) or decl.form not in {"alias", "record", "variant"}:
            raise Refusal(f"Invalid IR type declaration {decl.name}")
        if decl.form == "alias":
            if not isinstance(decl.body, Ty):
                raise Refusal(f"Invalid IR alias {decl.name}")
            self._known(decl.name, decl.body)
            return
        members = decl.body
        what = "field" if decl.form == "record" else "case"
        if (not isinstance(members, tuple) or len(members) > MAX_MEMBERS
                or decl.form == "variant" and not members):
            raise Refusal(f"Type {decl.name} needs {'1' if decl.form == 'variant' else '0'}..{MAX_MEMBERS} {what}s")
        seen = set()
        for member in members:
            if not isinstance(member, tuple) or len(member) != 2 or not isinstance(member[0], str):
                raise Refusal(f"Invalid IR {what} in {decl.name}")
            name, ty = member
            if not re.fullmatch(NAME, name) or decl.form == "variant" and name in KEYWORDS:
                raise Refusal(f"Type {decl.name} has an invalid {what} name {name}")
            if name in seen:
                raise Refusal(f"{'Record' if decl.form == 'record' else 'Variant'} {decl.name} repeats {what} {name}")
            seen.add(name)
            if ty is None and decl.form == "record" or ty is not None and not isinstance(ty, Ty):
                raise Refusal(f"Invalid IR {what} type in {decl.name}")
            if decl.form == "variant" and ty == Ty("unit"):
                raise Refusal(f"Case {name} of {decl.name} has a unit payload; write it without one")
            if ty is not None:
                self._known(decl.name, ty)

    def _known(self, owner, ty):
        for name in references(ty):
            if name not in self.by_name:
                raise Refusal(f"Type {owner} refers to undeclared type {name}")

    def _acyclic(self, name, path, done=None):
        done = set() if done is None else done
        if name in path:
            cycle = " -> ".join([*path[path.index(name):], name])
            raise Refusal(f"Type {path[0]} is recursive ({cycle}); only finite types are admitted")
        if name in done:
            return
        for child in decl_references(self.by_name[name]):
            self._acyclic(child, [*path, name], done)
        done.add(name)

    def data(self):
        return [d.data() for d in self.decls]

    def __eq__(self, other):
        return isinstance(other, Types) and (self.decls, self.prelude) == (other.decls, other.prelude)

    def __hash__(self):
        return hash((self.decls, self.prelude))

    def resolve(self, ty: Ty):
        """Follow aliases to a primitive node or a record, variant or opaque declaration."""
        while ty.kind == "ref":
            decl = self.by_name[ty.args[0]]
            if decl.form != "alias":
                return decl
            ty = decl.body
        return ty

    def cardinality(self, ty: Ty):
        """The exact number of values, or None when it exceeds 2^EXACT_BITS."""
        @cache
        def count(node):
            target = self.resolve(node)
            if isinstance(target, Decl):
                if target.form == "opaque":
                    return target.body[0]
                counts = [1 if t is None else count(t) for _, t in target.body]
                return card_product(counts) if target.form == "record" else card_sum(counts)
            kind, args = target.kind, target.args
            if kind == "unit":
                return 1
            if kind == "bool":
                return 2
            if kind == "int":
                return args[1] - args[0] + 1
            if kind == "bytes":
                return 256 ** args[0] if 8 * args[0] <= EXACT_BITS else None
            if kind == "bytes_max":
                return card_sequences(256, args[0])
            return card_sequences(count(args[0]), args[1])
        return count(ty)

    def depth(self, ty: Ty) -> int:
        """Composite nesting: a record, variant or list adds one level; aliases add none."""
        @cache
        def levels(node):
            target = self.resolve(node)
            if isinstance(target, Decl):
                if target.form == "opaque":
                    return 0
                return 1 + max([levels(t) for _, t in target.body if t is not None], default=0)
            return 1 + levels(target.args[0]) if target.kind == "list" else 0
        return levels(ty)

    def values(self, ty: Ty, cap: int) -> list:
        """Every value in canonical order; refuses a type with more than `cap` values by name."""
        count = self.cardinality(ty)
        if count is None or count > cap:
            raise Refusal(f"Type {ty.render()} has {card_text(count)} values; at most {cap} may be enumerated here")
        return list(self._enumerate(ty))

    def _enumerate(self, ty):
        target = self.resolve(ty)
        if isinstance(target, Decl):
            if target.form == "opaque":
                raise Refusal(f"Values of {target.name} cannot be enumerated")
            if target.form == "record":
                yield from itertools.product(*[list(self._enumerate(t)) for _, t in target.body])
                return
            for case, payload in target.body:
                if payload is None:
                    yield (case, ())
                else:
                    for value in self._enumerate(payload):
                        yield (case, value)
            return
        kind, args = target.kind, target.args
        if kind == "unit":
            yield ()
        elif kind == "bool":
            yield from (False, True)
        elif kind == "int":
            yield from range(args[0], args[1] + 1)
        elif kind in {"bytes", "bytes_max"}:
            for length in ([args[0]] if kind == "bytes" else range(args[0] + 1)):
                for items in itertools.product(range(256), repeat=length):
                    yield bytes(items)
        else:
            items = list(self._enumerate(args[0]))
            for length in range(args[1] + 1):
                yield from itertools.product(items, repeat=length)

    def parse_value(self, ty: Ty, text: str):
        tokens = Tokens(text)
        value = self._value(ty, tokens)
        tokens.done()
        return value

    def _value(self, ty, tokens):
        target = self.resolve(ty)
        if isinstance(target, Decl):
            if target.form == "opaque":
                raise Refusal(f"Values of {target.name} cannot be written in a source")
            if target.form == "record":
                tokens.take("{")
                values = []
                for index, (field, field_ty) in enumerate(target.body):
                    if index:
                        tokens.take(",")
                    tokens.take(field, ":")
                    values.append(self._value(field_ty, tokens))
                tokens.take("}")
                return tuple(values)
            case = tokens.take()
            payload = dict(target.body)
            if case not in payload:
                raise Refusal(f"{case} is not a case of {target.name}")
            if payload[case] is None:
                return (case, ())
            tokens.take("(")
            value = self._value(payload[case], tokens)
            tokens.take(")")
            return (case, value)
        kind, args = target.kind, target.args
        if kind == "unit":
            tokens.take("(", ")")
            return ()
        token = tokens.peek()
        if kind == "bool":
            if token not in {"true", "false"}:
                raise Refusal(f"Expected true or false near {token or 'the end'}")
            tokens.take()
            return token == "true"
        if kind == "int":
            value = tokens.integer()
            if not args[0] <= value <= args[1]:
                raise Refusal(f"Value {value} is outside {args[0]}..={args[1]}")
            return value
        if kind in {"bytes", "bytes_max"}:
            token = tokens.take()
            if not re.fullmatch(r"0x(?:[0-9a-f]{2})*", token):
                raise Refusal(f"Expected bytes as 0x and lowercase hex pairs near {token}")
            value = bytes.fromhex(token[2:])
            if kind == "bytes" and len(value) != args[0] or kind == "bytes_max" and len(value) > args[0]:
                raise Refusal(f"Bytes {token} do not fit {target.render()}")
            return value
        tokens.take("[")
        items = []
        while not tokens.at("]"):
            if items:
                tokens.take(",")
            items.append(self._value(args[0], tokens))
            if len(items) > args[1]:
                raise Refusal(f"A list value exceeds its cap of {args[1]} items")
        tokens.take("]")
        return tuple(items)

    def render_value(self, ty: Ty, value) -> str:
        target = self.resolve(ty)
        if isinstance(target, Decl):
            if target.form == "record":
                return "{" + ", ".join(f"{f}: {self.render_value(t, v)}" for (f, t), v in zip(target.body, value)) + "}"
            case, payload = value
            ty = dict(target.body)[case]
            return case if ty is None else f"{case}({self.render_value(ty, payload)})"
        kind = target.kind
        if kind == "unit":
            return "()"
        if kind == "bool":
            return "true" if value else "false"
        if kind == "int":
            return str(value)
        if kind in {"bytes", "bytes_max"}:
            return "0x" + value.hex()
        return "[" + ", ".join(self.render_value(target.args[0], v) for v in value) + "]"

    def explain(self, name: str) -> dict:
        """How many values one declared type admits, and how the count is formed."""
        if name not in self.by_name:
            raise Refusal(f"Unknown type {name}")
        decl = self.by_name[name]
        ty = Ty("ref", (name,))
        count = self.cardinality(ty)
        if decl.form == "opaque":
            operator, members, rule = "opaque", [], "fixed by the profile: " + decl.body[1]
        elif decl.form == "alias":
            target = self.resolve(decl.body)
            operator, members = "alias", [{"name": None, "type": decl.body.render(), "english": decl.body.render(True),
                                           "cardinality": card_data(count)}]
            rule = (_primitive_rule(target) if isinstance(target, Ty) else f"the same values as {target.name}")
        else:
            operator = "product" if decl.form == "record" else "sum"
            members = [{"name": n, "type": "unit" if t is None else t.render(), "english": "nothing" if t is None else t.render(True),
                        "cardinality": card_data(1 if t is None else self.cardinality(t))} for n, t in decl.body]
            sign = " x " if operator == "product" else " + "
            rule = ((("product of its fields: " if operator == "product" else "sum of its cases: ")
                     + sign.join(f"{m['name']} ({m['cardinality']['text']})" for m in members))
                    if members else "an empty record has exactly one value")
        result = {"kind": "type-explanation", "profile": PROFILE, "type": name, "form": decl.form,
                  "cardinality": card_data(count), "operator": operator, "members": members, "rule": rule,
                  "depth": self.depth(ty), "method": "exact integer arithmetic over the declared types; no language model",
                  "authority": "none"}
        if decl.form != "opaque":
            result.update(symbolic=decl.render(), english=decl.render(True))
        if len(canonical(result).encode()) > 512 * 1024:
            raise Refusal("Explanation exceeds the 512 KiB output bound")
        return result


def _primitive_rule(ty):
    return {"unit": "exactly one value", "bool": "false and true",
            "int": "every integer from the low bound to the high bound",
            "bytes_max": "every byte string of length 0 through the cap: 1 + 256 + ... + 256^cap",
            "bytes": "every byte string of exactly that length: 256^length",
            "list": "every list of 0 through the cap items: 1 + n + ... + n^cap for n item values"}[ty.kind]


def references(ty: Ty):
    if ty.kind == "ref":
        return [ty.args[0]]
    return references(ty.args[0]) if ty.kind == "list" else []


def decl_references(decl: Decl):
    if decl.form == "alias":
        return references(decl.body)
    if decl.form == "opaque":
        return []
    return [name for _, ty in decl.body if ty is not None for name in references(ty)]
