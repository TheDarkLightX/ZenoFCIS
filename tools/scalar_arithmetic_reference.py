"""Independent arbitrary-precision scalar oracle; no production code is executed.

Fraction supplies rational semantics independently of machine division, limb
arithmetic and emitted-code implementations. This reference grants no authority.
"""
from __future__ import annotations
from dataclasses import dataclass
from fractions import Fraction
from math import ceil, floor
import random

MIN = -(1 << 63)
MAX = (1 << 63) - 1

@dataclass(frozen=True)
class Case:
    operation: str
    operands: tuple[int, ...]


def result(case: Case) -> int | None:
    a = case.operands
    if any(type(x) is not int or not MIN <= x <= MAX for x in a):
        return None
    if case.operation == "mul" and len(a) == 2:
        answer = a[0] * a[1]
    elif case.operation in ("div_floor", "div_ceil") and len(a) == 2:
        if a[1] == 0:
            return None
        answer = (floor if case.operation == "div_floor" else ceil)(Fraction(*a))
    elif case.operation in ("mul_div_floor", "mul_div_ceil") and len(a) == 3:
        if a[0] < 0 or a[1] < 0 or a[2] <= 0:
            return None
        answer = (floor if case.operation == "mul_div_floor" else ceil)(Fraction(a[0] * a[1], a[2]))
    elif case.operation == "products_ge" and len(a) in (4, 6):
        if min(a) < 0:
            return None
        count = len(a) // 2
        left, right = 1, 1
        for x in a[:count]:
            left *= x
        for x in a[count:]:
            right *= x
        answer = int(left >= right)
    else:
        raise ValueError("unsupported oracle operation or arity")
    return answer if MIN <= answer <= MAX else None


def boundary_cases() -> tuple[Case, ...]:
    cases = []
    edges = (MIN, MIN + 1, -3, -2, -1, 0, 1, 2, 3, MAX - 1, MAX)
    for operation in ("mul", "div_floor", "div_ceil"):
        cases.extend(Case(operation, (a, b)) for a in edges for b in edges)
    for operation in ("mul_div_floor", "mul_div_ceil"):
        cases.extend(Case(operation, (a, b, d)) for a, b, d in (
            (0, MAX, 1), (1, 1, 2), (5, 1, 2), (MAX, MAX, MAX),
            (MAX, MAX, 1), (MAX - 1, MAX - 1, MAX - 2),
            (-1, 0, 1), (0, -1, 1), (0, 0, 0), (1, 1, -1),
            (MAX, MAX - 1, MAX), (MAX - 1, MAX - 1, MAX),
        ))
    cases.extend(Case("products_ge", a) for a in (
        (0, MAX, 1, 0), (MAX, MAX, MAX - 1, MAX),
        (MAX, MAX, MAX, MAX - 1, MAX, MAX),
        (MAX - 1, MAX, MAX, MAX, MAX, MAX),
        (1 << 43, 1 << 43, 1 << 43, 1, 1, 1),
        (1, 1, 1, 1 << 43, 1 << 43, 1 << 43),
        (0, MAX, MAX, 1, 1, 1), (-1, 0, 0, 0),
        (3, 3, 10, 1), (4, 4, 10, 1),
    ))
    rng = random.Random(20261013)
    for _ in range(128):
        a, b = rng.randint(MIN, MAX), rng.randint(MIN, MAX)
        cases.extend(Case(op, (a, b)) for op in ("mul", "div_floor", "div_ceil"))
        a, b, d = rng.randint(0, MAX), rng.randint(0, MAX), rng.randint(1, MAX)
        cases.extend(Case(op, (a, b, d)) for op in ("mul_div_floor", "mul_div_ceil"))
        cases.append(Case("products_ge", tuple(rng.randint(0, MAX) for _ in range(6))))
    return tuple(cases)


def vectors() -> str:
    return "".join("\t".join((case.operation, *(str(x) for x in case.operands),
        "arithmetic" if result(case) is None else str(result(case)))) + "\n" for case in boundary_cases())
