"""Fold the compiled Spot program under fixed small inputs, as proof hints.

Given values for a few small-domain inputs (action, direction, ...), each
node's value becomes a residual term over the remaining inputs. The terms,
and the nodes visited to build them, are only hints for the generated Verus
lemmas: Verus re-proves every stated equation from the node definitions, so
a wrong fold makes the proof fail rather than pass.
"""
from __future__ import annotations

from dataclasses import dataclass

import native_program

Term = tuple  # ('in', i) | ('k', v) | (op, *args)


def is_const(t: Term) -> bool:
    return t[0] == "k"


@dataclass
class Folder:
    native: native_program.Native
    fixed: dict[int, int]

    def __post_init__(self):
        self.memo: dict[int, Term] = {}
        self.visited: set[int] = set()

    def value(self, k: int) -> Term:
        if k in self.memo:
            self.visited.add(k)
            return self.memo[k]
        self.visited.add(k)
        op = self.native.nodes[k]
        name = op[0]
        if name == "Input":
            t = ("k", self.fixed[op[1]]) if op[1] in self.fixed else ("in", op[1])
        elif name == "Int":
            t = ("k", op[1])
        elif name == "Bool":
            t = ("k", 1 if op[1] else 0)
        elif name == "Select":
            c = self.value(op[1])
            if is_const(c):
                t = self.value(op[2]) if c[1] == 1 else self.value(op[3])
            else:
                a, b = self.value(op[2]), self.value(op[3])
                t = a if a == b else ("ite", c, a, b)
        elif name == "Not":
            a = self.value(op[1])
            t = ("k", 1 if a[1] == 0 else 0) if is_const(a) else ("not", a)
        elif name == "And":
            a = self.value(op[1])
            if is_const(a) and a[1] != 1:
                t = ("k", 0)
            else:
                b = self.value(op[2])
                if is_const(b) and b[1] != 1:
                    t = ("k", 0)
                elif is_const(a):
                    t = b
                elif is_const(b):
                    t = a
                else:
                    t = ("and", a, b)
        elif name in ("Eq", "Lt"):
            a, b = self.value(op[1]), self.value(op[2])
            if is_const(a) and is_const(b):
                t = ("k", int(a[1] == b[1] if name == "Eq" else a[1] < b[1]))
            elif name == "Eq" and a == b:
                t = ("k", 1)
            else:
                t = (name.lower(), a, b)
        elif name in ("Add", "Sub"):
            a, b = self.value(op[1]), self.value(op[2])
            if is_const(a) and is_const(b):
                t = ("k", a[1] + b[1] if name == "Add" else a[1] - b[1])
            elif is_const(b) and b[1] == 0:
                t = a
            elif name == "Add" and is_const(a) and a[1] == 0:
                t = b
            else:
                t = (name.lower(), a, b)
        elif name in ("Mul", "DivFloor"):
            a, c = self.value(op[1]), self.value(op[2])
            if is_const(a):
                t = ("k", a[1] * c[1] if name == "Mul" else a[1] // c[1])
            elif name == "Mul" and c[1] == 0:
                t = ("k", 0)
            else:
                t = ("mul" if name == "Mul" else "div", a, c)
        elif name in ("MulDivFloor", "MulDivCeil"):
            a, b, d = (self.value(i) for i in op[1:])
            if all(map(is_const, (a, b, d))):
                q, r = divmod(a[1] * b[1], d[1])
                t = ("k", q + (1 if name == "MulDivCeil" and r else 0))
            else:
                t = ("mdf" if name == "MulDivFloor" else "mdc", a, b, d)
        elif name == "ProductsGe":
            args = [self.value(i) for i in op[1] + op[2]]
            if all(map(is_const, args)):
                t = ("k", int(args[0][1] * args[1][1] * args[2][1] >= args[3][1] * args[4][1] * args[5][1]))
            else:
                t = ("pge", *args)
        else:
            raise ValueError(f"node {k}: operator {name} is not in the Spot profile")
        self.memo[k] = t
        return t


BOOLEAN = ("not", "and", "eq", "lt", "pge")


def boolean(t: Term) -> bool:
    """Whether a residual is 0/1-valued by construction."""
    if t[0] == "k":
        return t[1] in (0, 1)
    if t[0] == "ite":
        return boolean(t[2]) and boolean(t[3])
    return t[0] in BOOLEAN


def int_text(t: Term) -> str:
    """The residual as a Verus `int` expression."""
    name = t[0]
    if name == "in":
        return f"(input[{t[1]}] as int)"
    if name == "k":
        return f"{t[1]}int" if t[1] >= 0 else f"({t[1]}int)"
    if name in BOOLEAN:
        return f"(if {bool_text(t)} {{ 1int }} else {{ 0int }})"
    if name == "ite":
        return f"(if {bool_text(t[1])} {{ {int_text(t[2])} }} else {{ {int_text(t[3])} }})"
    if name in ("add", "sub"):
        return f"({int_text(t[1])} {'+' if name == 'add' else '-'} {int_text(t[2])})"
    if name in ("mul", "div"):
        return f"({int_text(t[1])} {'*' if name == 'mul' else '/'} {int_text(t[2])})"
    if name in ("mdf", "mdc"):
        return f"{name}({int_text(t[1])}, {int_text(t[2])}, {int_text(t[3])})"
    raise ValueError(name)


def bool_text(t: Term) -> str:
    """A 0/1 residual as a Verus `bool` expression (value == 1)."""
    name = t[0]
    if name == "k":
        return "true" if t[1] == 1 else "false"
    if name == "not":
        if t[1][0] == "not" and boolean(t[1][1]):
            return bool_text(t[1][1])
        return f"!{bool_text(t[1])}" if boolean(t[1]) else f"({int_text(t[1])} == 0)"
    if name == "and":
        return f"({bool_text(t[1])} && {bool_text(t[2])})"
    if name == "eq":
        if boolean(t[1]) and boolean(t[2]):
            return f"({bool_text(t[1])} == {bool_text(t[2])})"
        return f"({int_text(t[1])} == {int_text(t[2])})"
    if name == "lt":
        return f"({int_text(t[1])} < {int_text(t[2])})"
    if name == "pge":
        return f"pge({', '.join(int_text(a) for a in t[1:])})"
    if name == "ite" and boolean(t):
        return f"(if {bool_text(t[1])} {{ {bool_text(t[2])} }} else {{ {bool_text(t[3])} }})"
    return f"({int_text(t)} == 1)"
