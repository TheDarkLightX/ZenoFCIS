"""Profile zal/typed-fsm/1: zal/finite-fsm/2 machines whose variables and inputs have finite types.

No eval, model calls or I/O. A typed source elaborates exactly to a profile 2 machine,
which the unchanged profile 2 checker decides: a variable or input of an integer type
keeps its range, and one of any other finite type becomes the index of its value in the
type's canonical enumeration. A typed name is compared only with == and != against its
own type's values or a same-typed name, and an update only assigns such a value; the
type's enumeration must fit profile 2's per-variable range, or the source is refused.
"""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import re

import adt
import behavior2
from adt import TYPE_NAME, Tokens, Ty, Types, parse_ty
from behavior import NAME, Refusal, canonical


PROFILE = "zal/typed-fsm/1"
HEADER = {False: "zal typed 1;", True: "ZAL typed version 1."}
PROFILE_2_HEADER = {False: "zal 2;", True: "ZAL version 2."}
MAX_VALUES = behavior2.MAX_RANGE


def typed_patterns(english=False):
    """Typed declarations, beside the profile 2 statement registry."""
    if english:
        return {"type": adt.type_patterns(True),
                "typed_var": rf"The variable ({NAME}) holds (.+), initially (.+)\.",
                "typed_input": rf"The input ({NAME}) holds (.+)\."}
    return {"type": adt.type_patterns(),
            "typed_var": rf"var ({NAME}): (.+) initially (.+);",
            "typed_input": rf"input ({NAME}): (.+);"}


@dataclass(frozen=True)
class Typed:
    """One typed variable or input; `encoded` names a type other than an integer range."""
    name: str
    role: str
    ty: Ty
    encoded: bool
    initial: object = None

    def data(self):
        return [self.name, self.role, self.ty.data(), self.encoded]


@dataclass(frozen=True)
class TypedMachine:
    types: Types
    typed: tuple
    machine: behavior2.Machine

    def __post_init__(self):
        validate(self)

    def data(self):
        return {"profile": PROFILE, "types": self.types.data(), "typed": [t.data() for t in self.typed],
                "machine": self.machine.data()}

    @property
    def revision(self):
        return hashlib.sha256(canonical(self.data()).encode()).hexdigest()

    def entry(self, name):
        return next((t for t in self.typed if t.name == name), None)

    def values(self, name):
        """The canonical enumeration of an encoded name's type: index i is the i-th value."""
        item = self.entry(name)
        return self.types.values(item.ty, MAX_VALUES)

    def value_text(self, name, index):
        item = self.entry(name)
        return self.types.render_value(item.ty, self.values(name)[index])

    def render(self, english=False) -> str:
        lines = self.machine.render(english).splitlines()
        context = 4  # header, machine, states, events, context: types follow the context line
        out = [HEADER[english], *lines[1:context + 1], *[d.render(english) for d in self.types.decls]]
        for line in lines[context + 1:]:
            out.append(self._decode_line(line, english))
        return "\n".join(out) + "\n"

    def _decode_line(self, line, english):
        for item in self.typed:
            ty = item.ty.render(english)
            if item.role == "var":
                pattern = (rf"The variable {item.name} is an integer from (-?[0-9]+) to (-?[0-9]+) inclusive, initially (-?[0-9]+)\."
                           if english else rf"var {item.name} in (-?[0-9]+)\.\.=(-?[0-9]+) initially (-?[0-9]+);")
                match = re.fullmatch(pattern, line)
                if match:
                    initial = self.value_text(item.name, int(match[3])) if item.encoded else match[3]
                    return (f"The variable {item.name} holds {ty}, initially {initial}." if english
                            else f"var {item.name}: {ty} initially {initial};")
            else:
                pattern = (rf"The input {item.name} is an integer from (-?[0-9]+) to (-?[0-9]+) inclusive\." if english
                           else rf"input {item.name} in (-?[0-9]+)\.\.=(-?[0-9]+);")
                if re.fullmatch(pattern, line):
                    return f"The input {item.name} holds {ty}." if english else f"input {item.name}: {ty};"
        for item in self.typed:
            if not item.encoded:
                continue
            name = re.escape(item.name)
            decode = lambda match, item=item: match[1] + self.value_text(item.name, int(match[2]))
            if english:
                line = re.sub(rf"(?<![a-z0-9_])({name}(?: after this event)? (?:equals|does not equal) )([0-9]+)(?![0-9])", decode, line)
                line = re.sub(rf"(?<![a-z0-9_])({name} set to )([0-9]+)(?![0-9])", decode, line)
            else:
                line = re.sub(rf"(?<![a-z0-9_])({name}'? (?:==|!=) )([0-9]+)(?![0-9])", decode, line)
                line = re.sub(rf"(?<![a-z0-9_])({name} = )([0-9]+)(?![0-9])", decode, line)
        return line


def _encode_values(text, encoded, types, english):
    """Replace each typed value after an encoded name's comparison or assignment by its index."""
    operators = (r" after this event (?:equals|does not equal) | (?:equals|does not equal) | set to " if english
                 else r"'?\s*(?:==|!=)\s*|\s*=\s*(?!=)")
    names = "|".join(map(re.escape, sorted(encoded, key=len, reverse=True)))
    ordering = (r"(?: after this event)? (?:is less than|is at most|is greater than|is at least|plus|minus|increased by|decreased by) "
                if english else r"'?\s*(?:<=|>=|<|>|\+=|-=|\+|-)")
    misuse = re.search(rf"(?<![a-z0-9_])({names})({ordering})", text)
    if misuse:
        raise Refusal(f"Typed name {misuse[1]} supports only == and != against its type's values, and assignment")
    pattern = re.compile(rf"(?<![a-z0-9_])({names})({operators})")
    out, position = [], 0
    while True:
        match = pattern.search(text, position)
        if match is None:
            out.append(text[position:])
            return "".join(out)
        item, rest = encoded[match[1]], text[match.end():]
        other = re.match(NAME, rest)
        if other and other[0] in encoded:
            # A typed name on the right stays a comparison of indices; validation requires the same type.
            out.append(text[position:match.end()])
            position = match.end()
            continue
        value = _prefix_value(types, item.ty, Tokens(rest))
        rendered = types.render_value(item.ty, value)
        if not rest.startswith(rendered):
            raise Refusal(f"Write the value of {item.name} canonically, as {rendered}")
        index = types.values(item.ty, MAX_VALUES).index(value)
        out.append(text[position:match.end()] + str(index))
        position = match.end() + len(rendered)


def _prefix_value(types, ty, tokens):
    try:
        return types._value(ty, tokens)
    except Refusal as error:
        raise Refusal(f"Not a value of {ty.render()}: {error}") from None


def parse(text: str, syntax="symbolic") -> TypedMachine:
    if not isinstance(text, str) or len(text.encode()) > behavior2.MAX_SOURCE or not text.isascii():
        raise Refusal("Source must be ASCII text at most 64 KiB")
    if syntax not in {"symbolic", "english"}:
        raise Refusal("Choose symbolic or english syntax")
    english = syntax == "english"
    lines = [line.strip() for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")]
    if not lines or lines[0] != HEADER[english]:
        raise Refusal(f"A typed machine starts with {HEADER[english]}")
    patterns = typed_patterns(english)
    decls, declared, rest = [], [], []
    for line in lines[1:]:
        match = re.fullmatch(patterns["type"], line)
        if match:
            decls.append(adt.parse_decl(match[1], match[2], english))
            continue
        for role in ("var", "input"):
            match = re.fullmatch(patterns["typed_" + role], line)
            if match:
                declared.append((role, match))
                rest.append(None)
                break
        else:
            rest.append(line)
    types = Types(tuple(sorted(decls, key=lambda d: d.name)))
    typed, elaborated = {}, {}
    for role, match in declared:
        name = match[1]
        tokens = Tokens(match[2], english)
        ty = parse_ty(tokens)
        tokens.done()
        target = types.resolve(ty) if all(n in types.by_name for n in adt.references(ty)) else None
        if target is None:
            raise Refusal(f"{role.capitalize()} {name} refers to an undeclared type")
        encoded = not (isinstance(target, Ty) and target.kind == "int")
        if encoded:
            count = types.cardinality(ty)
            if count is None or count > MAX_VALUES:
                raise Refusal(f"Type {ty.render()} of {name} has {adt.card_text(count)} values; a profile 2 "
                              f"variable or input admits at most {MAX_VALUES}")
        if name in typed:
            raise Refusal(f"{name} is declared twice")
        typed[name] = Typed(name, role, ty, encoded)
        low, high = (0, types.cardinality(ty) - 1) if encoded else target.args
        if role == "var":
            initial = types.parse_value(ty, match[3])
            if types.render_value(ty, initial) != match[3]:
                raise Refusal(f"Write the initial value of {name} canonically, as {types.render_value(ty, initial)}")
            start = types.values(ty, MAX_VALUES).index(initial) if encoded else initial
            elaborated[name] = (f"The variable {name} is an integer from {low} to {high} inclusive, initially {start}."
                                if english else f"var {name} in {low}..={high} initially {start};")
        else:
            elaborated[name] = (f"The input {name} is an integer from {low} to {high} inclusive." if english
                                else f"input {name} in {low}..={high};")
    encoded = {name: item for name, item in typed.items() if item.encoded}
    cases = {case for d in types.decls if d.form == "variant" for case, _ in d.body}
    order, body = iter(m[1] for _, m in declared), []
    for line in rest:
        if line is None:
            body.append(elaborated[next(order)])
        else:
            body.append(_encode_values(line, encoded, types, english) if encoded else line)
    machine = behavior2.parse("\n".join([PROFILE_2_HEADER[english], *body]) + "\n", syntax)
    clashes = cases & ({*machine.facts} | {i.name for i in machine.inputs} | {v.name for v in machine.variables})
    if clashes:
        raise Refusal("A case name may not also name a context fact, input or variable: " + ", ".join(sorted(clashes)))
    return TypedMachine(types, tuple(sorted(typed.values(), key=lambda t: t.name)), machine)


def _atom(node, encoded):
    return node.args[0] if node.op in {"var", "var_next", "input"} and node.args[0] in encoded else None


def _mentions(node, encoded):
    if not isinstance(node, behavior2.Expr):
        return False
    return _atom(node, encoded) is not None or any(_mentions(a, encoded) for a in node.args)


def _walk(node, encoded, owner):
    if node.op == "cmp":
        op, left, right = node.args
        if _mentions(left, encoded) or _mentions(right, encoded):
            name, other = _atom(left, encoded), _atom(right, encoded)
            same = other is not None and encoded[other].ty == encoded[name].ty if name else False
            if op not in {"==", "!="} or name is None or not (right.op == "int" or same):
                raise Refusal(f"{owner}: a typed name is compared only with == or != against one of its type's "
                              "values or a name of the same type, written on the left")
        return
    if node.op in {"add", "sub"} and _mentions(node, encoded):
        raise Refusal(f"{owner}: typed values have no arithmetic")
    for arg in node.args:
        if isinstance(arg, behavior2.Expr):
            _walk(arg, encoded, owner)


def validate(model: TypedMachine):
    if (not isinstance(model.types, Types) or model.types.prelude or not isinstance(model.machine, behavior2.Machine)
            or model.machine.profile != behavior2.PROFILE
            or not isinstance(model.typed, tuple) or any(not isinstance(t, Typed) for t in model.typed)
            or [t.name for t in model.typed] != sorted({t.name for t in model.typed})):
        raise Refusal("Invalid IR typed machine")
    machine = model.machine
    integers = {v.name: ("var", v.low, v.high) for v in machine.variables} | {i.name: ("input", i.low, i.high) for i in machine.inputs}
    encoded = {}
    for item in model.typed:
        declared = integers.get(item.name)
        if declared is None or declared[0] != item.role or not isinstance(item.ty, Ty) or type(item.encoded) is not bool:
            raise Refusal(f"Typed {item.name} has no matching profile 2 declaration")
        count = model.types.cardinality(item.ty)
        target = model.types.resolve(item.ty)
        expected = (0, count - 1) if item.encoded else target.args if isinstance(target, Ty) and target.kind == "int" else None
        if item.encoded == (isinstance(target, Ty) and target.kind == "int") or tuple(integers[item.name][1:]) != tuple(expected):
            raise Refusal(f"Typed {item.name} does not elaborate to its declared profile 2 range")
        if item.encoded:
            encoded[item.name] = item
    for rule in machine.rules:
        _walk(rule.guard, encoded, "Rule " + rule.id)
        for update in rule.updates:
            target, term = update.variable in encoded, update.term
            source = term.op in {"var", "input"} and term.args[0] in encoded
            if target or source:
                same = source and target and encoded[term.args[0]].ty == encoded[update.variable].ty
                if update.op != "=" or not target or not (term.op == "int" or same):
                    raise Refusal(f"Rule {rule.id}: a typed variable is only assigned one of its type's values "
                                  "or a name of the same type")
    for collection in (machine.invariants, machine.requirements, machine.fairness, machine.properties):
        for name, formula in collection:
            _walk(formula, encoded, name)


def parse_any(text: str, syntax="symbolic"):
    first = next((line.strip() for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")), "")
    if first in HEADER.values():
        return parse(text, syntax)
    return behavior2.parse_any(text, syntax)


def _decode(model, value, key=None):
    """Write every encoded index in a profile 2 result as its typed value."""
    def text(name, index):
        item = model.entry(name) if isinstance(name, str) else None
        return model.value_text(name, index) if item and item.encoded and type(index) is int else index
    if isinstance(value, list):
        return [_decode(model, v) for v in value]
    if not isinstance(value, dict):
        return value
    if key in {"variables", "context"}:
        return {name: text(name, v) for name, v in value.items()}
    result = {k: _decode(model, v, k) for k, v in value.items()}
    if "comparison" in result and isinstance(result.get("left"), dict) and result["left"].get("name"):
        name = result["left"]["name"]
        result["left"] = {**result["left"], "value": text(name, result["left"]["value"])}
        result["right"] = {**result["right"], "value": text(name, result["right"]["value"])}
    elif result.get("operator") in {"var", "var_next", "input"} and "name" in result:
        result["value"] = text(result["name"], result["value"])
    if "variable" in result and {"before", "after"} <= set(result):
        name = result["variable"]
        item = model.entry(name)
        if item and item.encoded:
            after = text(name, result["after"])
            result.update(before=text(name, result["before"]), after=after, term=after,
                          english=f"{name} set to {after}", range=None)
    if result.get("variable") in {t.name for t in model.typed if t.encoded} and type(result.get("value")) is int:
        result["value"] = text(result["variable"], result["value"])
    return result


def encoding(model):
    return [{"name": t.name, "role": t.role, "type": t.ty.render(), "english": t.ty.render(True),
             "values": [model.types.render_value(t.ty, v) for v in model.values(t.name)] if t.encoded else None}
            for t in model.typed]


def check(model: TypedMachine, checker_identity="unbound") -> dict:
    """The profile 2 verdict on the elaborated machine, with typed values decoded in every witness."""
    report = _decode(model, behavior2.check(model.machine, checker_identity))
    return {**report, "profile": PROFILE, "revision": model.revision, "machine_revision": model.machine.revision,
            "encoding": encoding(model),
            "elaboration": "every typed name is the index of its value in its type's canonical enumeration"}


def explain(model: TypedMachine, state, variables, event, context, checker_identity="unbound") -> dict:
    """Explain one decision; typed values are written as value text and decoded in the result."""
    def encode(values):
        if not isinstance(values, dict):
            raise Refusal("Input outside the declared state/variable/event/context domain")
        out = {}
        for name, value in values.items():
            item = model.entry(name)
            if item is not None and item.encoded:
                if not isinstance(value, str):
                    raise Refusal(f"Write the value of {name} as text")
                parsed = model.types.parse_value(item.ty, value)
                out[name] = model.values(name).index(parsed)
            else:
                out[name] = value
        return out
    result = behavior2.explain(model.machine, state, encode(variables), event, encode(context), checker_identity)
    return {**_decode(model, result), "profile": PROFILE, "revision": model.revision,
            "machine_revision": model.machine.revision, "encoding": encoding(model)}
