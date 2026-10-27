"""Planted defects in the compiled Spot program, for the refinement gate.

Each defect edits the generated descriptor text (src/v2_contract.rs) the way
a faulty implementation would compute, and names the checks that must
reject it. Nodes are located by what they compute (their folded terms), not
by fixed indices, so a regenerated program keeps the controls meaningful.
"""
from __future__ import annotations

import re

import native_program
import native_terms

NODES = "nodes: &["
ROOTS = "roots: &["


def _render(op: tuple) -> str:
    name = op[0]
    if name == "ProductsGe":
        return f"Op::ProductsGe([{', '.join(map(str, op[1]))}], [{', '.join(map(str, op[2]))}])"
    if name == "Bool":
        return f"Op::Bool({'true' if op[1] else 'false'})"
    return f"Op::{name}({', '.join(str(v) for v in op[1:])})"


def _block_span(text: str, start: str) -> tuple[int, int]:
    program = text.index("program: V2ScalarProgram {")
    begin = text.index(start, program) + len(start)
    depth, index = 1, begin
    while depth:
        depth += text[index] == "["
        depth -= text[index] == "]"
        index += 1
    return begin, index - 1


def _replace_program(text: str, nodes: list[tuple], roots: list[int]) -> str:
    """Rewrite the node and root lists in place, keeping the descriptor layout."""
    indent = " " * 20
    begin, end = _block_span(text, NODES)
    body = "".join(f"\n{indent}{_render(op)}," for op in nodes) + "\n" + " " * 16
    text = text[:begin] + body + text[end:]
    begin, end = _block_span(text, ROOTS)
    body = "".join(f"\n{indent}{r}," for r in roots) + "\n" + " " * 16
    return text[:begin] + body + text[end:]


def _remap(op: tuple, f) -> tuple:
    if op[0] in ("Input", "Int", "Bool"):
        return op
    if op[0] == "ProductsGe":
        return (op[0], tuple(map(f, op[1])), tuple(map(f, op[2])))
    return (op[0], *map(f, op[1:]))


def _find(native: native_program.Native, fixed: dict[int, int], wanted: native_terms.Term, kind: str) -> int:
    folder = native_terms.Folder(native, fixed)
    found = [k for k, op in enumerate(native.nodes) if op[0] == kind and folder.value(k) == wanted]
    if not found:
        raise ValueError(f"no {kind} node computes {wanted}")
    return found[0]


def _const(native: native_program.Native, value: int) -> int:
    return next(k for k, op in enumerate(native.nodes) if op == ("Int", value))


def _den(t):
    return ("ite", ("lt", ("k", 1), t), t, ("k", 1))


def wrong_rounding(text: str) -> str:
    """Remove rounds the asset-0 payout up instead of down."""
    native = native_program.load_text(text)
    a, r0, s = ("in", native.position("Command", 122)), ("in", native.position("State", 110)), \
        ("in", native.position("State", 112))
    k = _find(native, {}, ("mdf", a, r0, _den(s)), "MulDivFloor")
    nodes = list(native.nodes)
    nodes[k] = ("MulDivCeil", *nodes[k][1:])
    return _replace_program(text, nodes, list(native.roots))


def fee_wrong_side(text: str) -> str:
    """Swaps price the gross input: the fee is not taken on the input side."""
    native = native_program.load_text(text)
    fixed = {native.position("Command", 120): 1}
    a = ("in", native.position("Command", 122))
    fee = ("mdc", a, ("k", 30), ("k", 10000))
    k = _find(native, fixed, ("sub", a, fee), "Sub")
    nodes = list(native.nodes)
    nodes[k] = ("Sub", nodes[k][1], _const(native, 0))
    return _replace_program(text, nodes, list(native.roots))


def skipped_reserve_update(text: str) -> str:
    """Every accepted command writes back the old reserve 1."""
    native = native_program.load_text(text)
    accept = next(b for b in native.branches if b.accept)
    output = next(a.output for a in accept.assignments if a.field == 111)
    reserve = next(k for k, op in enumerate(native.nodes) if op == ("Input", native.position("State", 111)))
    roots = list(native.roots)
    roots[output] = reserve
    return _replace_program(text, list(native.nodes), roots)


def mint_off_by_one(text: str) -> str:
    """AddLiquidity mints one share more than the floor, to supply and actor alike."""
    native = native_program.load_text(text)
    fixed = {native.position("Command", 120): 3}
    folder = native_terms.Folder(native, fixed)
    s = ("in", native.position("State", 112))
    k = next(k for k, op in enumerate(native.nodes)
             if op[0] == "Select" and native.nodes[op[2]][0] == "MulDivFloor" == native.nodes[op[3]][0]
             and folder.value(op[2])[2] == s and folder.value(op[3])[2] == s)
    one = _const(native, 1)
    shift = lambda i: i + 1 if i > k else i
    nodes = [_remap(op, shift) for op in native.nodes]
    users = lambda i: k + 1 if i == k else i
    nodes = nodes[:k + 1] + [("Add", k, one)] + [_remap(op, users) for op in nodes[k + 1:]]
    roots = [k + 1 if r == k else shift(r) for r in native.roots]
    return _replace_program(text, nodes, roots)


def wrapping_balances(text: str) -> str:
    """Balance limbs wrap at 2^128: overflow guards and the complete
    conservation recheck are gone, as in a wrapping-u128 implementation."""
    native = native_program.load_text(text)
    reasons = {b.code: b.reason for b in native.branches}
    zero = _const(native, 0)
    nodes = list(native.nodes)
    k = native.roots[native.decision_output]
    while nodes[k][0] == "Select" and nodes[nodes[k][2]][0] == "Int":
        code = nodes[nodes[k][2]][1]
        if reasons.get(code) in (223, 225, 226):  # INGRESS_OVERFLOW, BALANCE_OVERFLOW, UNBALANCED_EFFECTS
            nodes[k] = ("Select", zero, nodes[k][2], nodes[k][3])
        k = nodes[k][3]
    return _replace_program(text, nodes, list(native.roots))


DEFECTS = {
    "wrong_rounding_direction": (wrong_rounding, "payout0 = ceil(z*r0/S) instead of floor"),
    "fee_on_wrong_side": (fee_wrong_side, "swap prices gross input; fee not deducted on the input side"),
    "skipped_reserve_update": (skipped_reserve_update, "reserve1 successor assigned the pre-state reserve"),
    "mint_off_by_one": (mint_off_by_one, "AddLiquidity mints floor + 1 shares"),
    "wrapping_overflow": (wrapping_balances, "u128 balances wrap; overflow and conservation rechecks removed"),
}


def applied(text: str) -> dict[str, str]:
    """Every defect's mutated source; each must differ and still parse."""
    result = {}
    for name, (mutate, _) in DEFECTS.items():
        changed = mutate(text)
        if changed == text:
            raise ValueError(f"{name}: mutation changed nothing")
        native_program.load_text(changed)
        if not re.search(r"Op::", changed):
            raise ValueError(f"{name}: program lost")
        result[name] = changed
    return result
