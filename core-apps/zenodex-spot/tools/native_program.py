"""Read the compiled Spot decision program and decision table as data.

The deployed Spot decision is the library evaluator running the generated
descriptor in src/v2_contract.rs. This module reads that generated source
text, never the authored policy, so the refinement checks examine the exact
nodes, roots, input bindings and branch assignments the library executes.
It also evaluates them with the evaluator specification's semantics
(evaluation/spec.rs `node`, value/scalar.rs), for differential replay.
It grants no decision or publication authority.
"""
from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

APP = Path(__file__).resolve().parents[1]
SOURCE = APP / "src/v2_contract.rs"
I128_MIN, I128_MAX = -(1 << 127), (1 << 127) - 1


class Failure(Exception):
    """An evaluator refusal: Reference, Arithmetic, InputDomain or OutputDomain."""


@dataclass(frozen=True)
class Assignment:
    field: int
    output: int | None
    constant: int | None
    low: int
    high: int


@dataclass(frozen=True)
class Branch:
    code: int
    accept: bool
    reason: int | None
    assignments: tuple[Assignment, ...]
    effects: int
    outbox: int


@dataclass(frozen=True)
class Native:
    inputs: tuple[tuple[int, int], ...]
    outputs: tuple[tuple[int, int], ...]
    nodes: tuple[tuple, ...]
    roots: tuple[int, ...]
    bindings: tuple[tuple[str, int], ...]
    decision_output: int
    branches: tuple[Branch, ...]

    def position(self, source: str, field: int) -> int:
        return self.bindings.index((source, field))


def _block(text: str, start: str) -> str:
    """The bracketed slice that opens at the unique occurrence of `start`."""
    if text.count(start) != 1:
        raise ValueError(f"expected one occurrence of {start!r}")
    begin = text.index(start) + len(start)
    depth, index = 1, begin
    while depth:
        char = text[index]
        depth += char == "["
        depth -= char == "]"
        index += 1
    return text[begin:index - 1]


def _domains(block: str) -> tuple[tuple[int, int], ...]:
    result = []
    for match in re.finditer(r"ScalarDomain::(Int\s*\{\s*min:\s*(-?\d+),\s*max:\s*(-?\d+),?\s*\}|Bool)", block):
        result.append((0, 1) if match.group(1) == "Bool" else (int(match.group(2)), int(match.group(3))))
    return tuple(result)


_OP = re.compile(r"Op::(\w+)\(([^()]*)\)")


def _op(name: str, args: str) -> tuple:
    if name == "ProductsGe":
        groups = re.fullmatch(r"\s*\[([^\]]*)\]\s*,\s*\[([^\]]*)\]\s*", args)
        if groups is None:
            raise ValueError(f"malformed ProductsGe({args})")
        left, right = (tuple(int(v) for v in g.split(",") if v.strip()) for g in groups.groups())
        if len(left) != 3 or len(right) != 3:
            raise ValueError("ProductsGe needs three factors per side")
        return (name, left, right)
    if name == "Bool":
        if args.strip() not in ("true", "false"):
            raise ValueError(f"malformed Bool({args})")
        return (name, args.strip() == "true")
    values = tuple(int(v) for v in args.split(","))
    arity = {"Input": 1, "Int": 1, "Not": 1, "Add": 2, "Sub": 2, "Eq": 2, "Lt": 2, "And": 2,
             "Mul": 2, "DivFloor": 2, "DivCeil": 2, "Select": 3, "MulDivFloor": 3,
             "MulDivCeil": 3, "SignedMulDivFloor": 3}
    if arity.get(name) != len(values):
        raise ValueError(f"unknown or malformed Op::{name}({args})")
    return (name, *values)


def _nodes(block: str) -> tuple[tuple, ...]:
    ops = [_op(m.group(1), m.group(2)) for m in _OP.finditer(block)]
    # Every non-blank token must belong to one parsed instruction.
    if _OP.sub("", block).replace(",", "").strip():
        raise ValueError("unparsed text in the node list")
    return tuple(ops)


def _branches(text: str) -> tuple[Branch, ...]:
    block = _block(text, "pub const BRANCHES: &[c::Branch<'static>] = &[")
    result = []
    for chunk in re.split(r"\n    c::Branch \{", block)[1:]:
        code = int(re.search(r"code: (\d+),", chunk).group(1))
        cls = re.search(r"class: c::Class::(\w+),", chunk).group(1)
        reason = re.search(r"reason: (None|Some\((\d+)\)),", chunk)
        assignments = []
        for m in re.finditer(r"c::Assignment \{\s*field: (\d+),\s*value: c::Expr::(Output\((\d+)\)|"
                             r"Constant\(c::Atom::I128\((-?\d+)\)\)),\s*domain: c::Domain::I128 \{\s*"
                             r"min: (-?\d+),\s*max: (-?\d+),?\s*\},?\s*\}", chunk):
            assignments.append(Assignment(int(m.group(1)), None if m.group(3) is None else int(m.group(3)),
                                          None if m.group(4) is None else int(m.group(4)),
                                          int(m.group(5)), int(m.group(6))))
        if chunk.count("c::Assignment {") != len(assignments):
            raise ValueError(f"branch {code}: unparsed assignment")
        if cls not in ("Accept", "Reject"):
            raise ValueError(f"branch {code}: unexpected class {cls}")
        effects, outbox = chunk.split("\n        effects: &[")[1].split("\n        outbox: &[")
        result.append(Branch(code, cls == "Accept", None if reason.group(2) is None else int(reason.group(2)),
                             tuple(assignments), effects.count("c::DeliveryPlan {"),
                             outbox.count("c::DeliveryPlan {")))
    return tuple(result)


def load(path: Path = SOURCE) -> Native:
    return load_text(path.read_text())


def load_text(text: str) -> Native:
    program = text[text.index("program: V2ScalarProgram {"):]
    inputs = _domains(_block(program, "inputs: &["))
    outputs = _domains(_block(program, "outputs: &["))
    nodes = _nodes(_block(program, "nodes: &["))
    roots = tuple(int(v) for v in _block(program, "roots: &[").split(",") if v.strip())
    bindings = tuple((m.group(1), int(m.group(2))) for m in re.finditer(
        r"c::Binding \{\s*source: c::Source::(\w+),\s*selector: c::Selector::Field\((\d+)\),\s*\}",
        _block(program, "bindings: &[")))
    decision_output = int(re.search(r"decision_output: (\d+),", program).group(1))
    native = Native(inputs, outputs, nodes, roots, bindings, decision_output, _branches(text))
    if len(inputs) != len(bindings) or len(outputs) != len(roots):
        raise ValueError("input bindings or output roots do not match their domains")
    return native


def narrow(value: int) -> int:
    if not I128_MIN <= value <= I128_MAX:
        raise Failure("Arithmetic")
    return value


def node(op: tuple, inputs: list[int], values: list[int]) -> int:
    """One instruction, exactly as evaluation/spec.rs `node` defines it."""
    name = op[0]
    def at(index: int) -> int:
        if index >= len(values):
            raise Failure("Reference")
        return values[index]
    if name == "Input":
        if op[1] >= len(inputs):
            raise Failure("Reference")
        return inputs[op[1]]
    if name == "Int":
        return op[1]
    if name == "Bool":
        return 1 if op[1] else 0
    if name == "Not":
        return 1 if at(op[1]) == 0 else 0
    if name == "Select":
        c, a, b = at(op[1]), at(op[2]), at(op[3])
        return a if c == 1 else b
    if name == "ProductsGe":
        a = [at(i) for i in op[1]]
        b = [at(i) for i in op[2]]
        if min(a + b) < 0:
            raise Failure("Arithmetic")
        return 1 if a[0] * a[1] * a[2] >= b[0] * b[1] * b[2] else 0
    args = [at(i) for i in op[1:]]
    if name == "Add":
        return narrow(args[0] + args[1])
    if name == "Sub":
        return narrow(args[0] - args[1])
    if name == "Eq":
        return 1 if args[0] == args[1] else 0
    if name == "Lt":
        return 1 if args[0] < args[1] else 0
    if name == "And":
        return 1 if args[0] == 1 and args[1] == 1 else 0
    if name == "Mul":
        return narrow(args[0] * args[1])
    if name in ("DivFloor", "DivCeil"):
        a, b = args
        if b == 0:
            raise Failure("Arithmetic")
        negative = (a < 0) != (b < 0)
        q, r = divmod(abs(a), abs(b))
        rounded = q + (1 if r != 0 and ((name == "DivCeil") != negative) else 0)
        return narrow(-rounded if negative else rounded)
    if name in ("MulDivFloor", "MulDivCeil"):
        a, b, d = args
        if a < 0 or b < 0 or d <= 0:
            raise Failure("Arithmetic")
        q, r = divmod(a * b, d)
        return narrow(q + (1 if name == "MulDivCeil" and r != 0 else 0))
    if name == "SignedMulDivFloor":
        a, b, d = args
        if d <= 0:
            raise Failure("Arithmetic")
        return narrow((a * b) // d)
    raise Failure("Reference")


def execute(native: Native, inputs: list[int]) -> list[int]:
    """The evaluator specification's `execution`: domains, prefix, projection."""
    if len(inputs) != len(native.inputs) or any(not lo <= v <= hi for v, (lo, hi) in zip(inputs, native.inputs)):
        raise Failure("InputDomain")
    values: list[int] = []
    for op in native.nodes:
        values.append(node(op, inputs, values))
    outputs = []
    for root in native.roots:
        if root >= len(values):
            raise Failure("Reference")
        outputs.append(values[root])
    if any(not lo <= v <= hi for v, (lo, hi) in zip(outputs, native.outputs)):
        raise Failure("OutputDomain")
    return outputs


def decide(native: Native, inputs: list[int]) -> tuple[bool, int | None, dict[int, int]]:
    """Selected branch and successor state; unlisted fields keep the pre-state."""
    outputs = execute(native, inputs)
    branch = next(b for b in native.branches if b.code == outputs[native.decision_output])
    post = {field: inputs[i] for i, (source, field) in enumerate(native.bindings) if source == "State"}
    for a in branch.assignments:
        value = a.constant if a.output is None else outputs[a.output]
        if not a.low <= value <= a.high:
            raise Failure("OutputDomain")
        post[a.field] = value
    return branch.accept, branch.reason, post
