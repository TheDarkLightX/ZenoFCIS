#!/usr/bin/env python3
"""Independent reference: the complete table of a canonical finite program.

Standard library only, and sharing no code with the program-search crate or
the ZenoFCIS checker. It reads the canonical `program.zcve` encoding of the
current `zeno-fcis/finite-i128/1` profile, or of the historical
`zeno-fcis/finite-i64/1` profile, and evaluates every declared input tuple:

- values use ZCVE/1 tags (0x01 false, 0x02 true, 0x03 u128, 0x04 i128,
  0x06 text, 0x08 tuple), big-endian lengths and integers, and must be
  consumed completely;
- a program is the tuple (profile text, (input domains, output domains),
  nodes, roots); a domain is (is-Boolean, min, max); a node is its opcode
  (0 input, 1 int, 2 bool, 3 add, 4 sub, 5 eq, 6 lt, 7 and, 8 not, 9 select)
  followed by its arguments; roots are u128 node indices;
- the profile fixes the integer width: domain bounds and integer literals
  must fit it (signed i128 for finite-i128/1, signed i64 for finite-i64/1);
- evaluation is eager: every node runs in order, add and subtract trap on
  overflow of the profile's width (the tuple's result is then an `arithmetic`
  failure), and an output outside its declared domain is an `output-domain`
  failure;
- tuples are enumerated with the last input fastest, Booleans 0 then 1.

Two programs agree when they have the same input and output domains and, on
every tuple, identical outputs or identical failure kinds. Used by tests and
measurement controls only; it grants no authority.
"""
from __future__ import annotations

import argparse
import itertools
import json
import sys
from dataclasses import dataclass
from pathlib import Path

PROFILE = b"zeno-fcis/finite-i128/1"
I64_PROFILE = b"zeno-fcis/finite-i64/1"
I64_MIN, I64_MAX = -(1 << 63), (1 << 63) - 1
I128_MIN, I128_MAX = -(1 << 127), (1 << 127) - 1
BOUNDS = {PROFILE: (I128_MIN, I128_MAX), I64_PROFILE: (I64_MIN, I64_MAX)}
MAX_BYTES = 64 * 1024
MAX_TUPLES = 1 << 22
ARITY = {0: 1, 1: 1, 2: 1, 3: 2, 4: 2, 5: 2, 6: 2, 7: 2, 8: 1, 9: 3}


class ProgramRefused(ValueError):
    """The bytes are not a program this reference admits."""


@dataclass(frozen=True)
class Program:
    inputs: tuple[tuple[bool, int, int], ...]
    outputs: tuple[tuple[bool, int, int], ...]
    nodes: tuple[tuple[int, ...], ...]
    roots: tuple[int, ...]
    profile: bytes = PROFILE


class _Reader:
    def __init__(self, body: bytes):
        self.body, self.at = body, 0

    def take(self, count: int) -> bytes:
        if self.at + count > len(self.body):
            raise ProgramRefused("truncated")
        chunk = self.body[self.at:self.at + count]
        self.at += count
        return chunk

    def tag(self) -> int:
        return self.take(1)[0]

    def tuple(self) -> int:
        if self.tag() != 0x08:
            raise ProgramRefused("expected a tuple")
        return int.from_bytes(self.take(4), "big")

    def signed(self) -> int:
        if self.tag() != 0x04:
            raise ProgramRefused("expected i128")
        return int.from_bytes(self.take(16), "big", signed=True)

    def unsigned(self) -> int:
        if self.tag() != 0x03:
            raise ProgramRefused("expected u128")
        return int.from_bytes(self.take(16), "big")

    def domain(self, bounds: tuple[int, int]) -> tuple[bool, int, int]:
        if self.tuple() != 3:
            raise ProgramRefused("domain shape")
        tag = self.tag()
        if tag not in (0x01, 0x02):
            raise ProgramRefused("domain flag")
        low, high = self.signed(), self.signed()
        boolean = tag == 0x02
        if not bounds[0] <= low <= high <= bounds[1] or (boolean and (low, high) != (0, 1)):
            raise ProgramRefused("domain bounds")
        return boolean, low, high


def decode(body: bytes) -> Program:
    if len(body) > MAX_BYTES:
        raise ProgramRefused("too large")
    reader = _Reader(body)
    if reader.tuple() != 4:
        raise ProgramRefused("program shape")
    if reader.tag() != 0x06:
        raise ProgramRefused("expected profile text")
    profile = reader.take(int.from_bytes(reader.take(4), "big"))
    if profile not in BOUNDS:
        raise ProgramRefused("profile")
    if reader.tuple() != 2:
        raise ProgramRefused("schema shape")
    inputs = tuple(reader.domain(BOUNDS[profile]) for _ in range(reader.tuple()))
    outputs = tuple(reader.domain(BOUNDS[profile]) for _ in range(reader.tuple()))
    nodes = []
    for _ in range(reader.tuple()):
        width = reader.tuple()
        opcode = reader.signed()
        if ARITY.get(opcode) != width - 1:
            raise ProgramRefused("node arity")
        nodes.append((opcode, *(reader.signed() for _ in range(width - 1))))
    roots = tuple(reader.unsigned() for _ in range(reader.tuple()))
    if reader.at != len(body):
        raise ProgramRefused("trailing bytes")
    program = Program(inputs, outputs, tuple(nodes), roots, profile)
    _type_check(program)
    return program


def _type_check(program: Program) -> None:
    """Backward references and exact operand kinds; True means Boolean."""
    if program.profile not in BOUNDS:
        raise ProgramRefused("profile")
    least, most = BOUNDS[program.profile]
    kinds: list[bool] = []
    for index, (opcode, *args) in enumerate(program.nodes):
        if opcode == 0:
            if not 0 <= args[0] < len(program.inputs):
                raise ProgramRefused("input reference")
            kinds.append(program.inputs[args[0]][0])
            continue
        if opcode in (1, 2):
            if opcode == 1 and not least <= args[0] <= most:
                raise ProgramRefused("integer literal")
            if opcode == 2 and args[0] not in (0, 1):
                raise ProgramRefused("Boolean literal")
            kinds.append(opcode == 2)
            continue
        if any(not 0 <= ref < index for ref in args):
            raise ProgramRefused("forward reference")
        operand = [kinds[ref] for ref in args]
        if opcode in (3, 4, 6):
            ok, kind = operand == [False, False], opcode == 6
        elif opcode == 5:
            ok, kind = operand[0] == operand[1], True
        elif opcode in (7, 8):
            ok, kind = all(operand), True
        else:
            ok, kind = operand[0] and operand[1] == operand[2], operand[1]
        if not ok:
            raise ProgramRefused("operand kinds")
        kinds.append(kind)
    if len(program.roots) != len(program.outputs) or any(
            not 0 <= root < len(program.nodes) or kinds[root] != domain[0]
            for root, domain in zip(program.roots, program.outputs)):
        raise ProgramRefused("roots")


def run(program: Program, point: tuple[int, ...]) -> tuple:
    """`("ok", outputs)` or `("error", kind)` for one tuple."""
    least, most = BOUNDS[program.profile]
    values: list[int] = []
    for opcode, *args in program.nodes:
        if opcode == 0:
            value = point[args[0]]
        elif opcode in (1, 2):
            value = args[0]
        elif opcode in (3, 4):
            a, b = values[args[0]], values[args[1]]
            value = a + b if opcode == 3 else a - b
            if not least <= value <= most:
                return ("error", "arithmetic")
        elif opcode == 5:
            value = int(values[args[0]] == values[args[1]])
        elif opcode == 6:
            value = int(values[args[0]] < values[args[1]])
        elif opcode == 7:
            value = int(values[args[0]] == 1 and values[args[1]] == 1)
        elif opcode == 8:
            value = int(values[args[0]] == 0)
        else:
            value = values[args[1]] if values[args[0]] == 1 else values[args[2]]
        values.append(value)
    out = tuple(values[root] for root in program.roots)
    if any(not low <= value <= high for (_, low, high), value in zip(program.outputs, out)):
        return ("error", "output-domain")
    return ("ok", out)


def tuples(program: Program):
    return itertools.product(*(range(low, high + 1) for _, low, high in program.inputs))


def size(program: Program) -> int:
    count = 1
    for _, low, high in program.inputs:
        count *= high - low + 1
    return count


def table(program: Program, max_tuples: int = MAX_TUPLES) -> list[tuple]:
    if size(program) > max_tuples:
        raise ProgramRefused("domain over the tuple limit")
    return [run(program, point) for point in tuples(program)]


def compare(original: Program, candidate: Program, max_tuples: int = MAX_TUPLES) -> dict:
    """`{"status": "equal"}`, `"abi-differs"`, or the first differing tuple."""
    if (original.inputs, original.outputs) != (candidate.inputs, candidate.outputs):
        return {"status": "abi-differs"}
    if size(original) > max_tuples:
        raise ProgramRefused("domain over the tuple limit")
    for ordinal, point in enumerate(tuples(original)):
        left, right = run(original, point), run(candidate, point)
        if left != right:
            return {"status": "different", "ordinal": ordinal, "input": [str(v) for v in point],
                    "original": _observation(left), "candidate": _observation(right)}
    return {"status": "equal", "tuples": size(original)}


def _observation(observed: tuple) -> dict:
    kind, value = observed
    return {"ok": [str(v) for v in value]} if kind == "ok" else {"error": value}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    one = sub.add_parser("compare")
    one.add_argument("original", type=Path)
    one.add_argument("candidate", type=Path)
    args = parser.parse_args()
    try:
        result = compare(decode(args.original.read_bytes()), decode(args.candidate.read_bytes()))
    except (ProgramRefused, OSError) as error:
        print(json.dumps({"schema": "zeno-fcis/program-search-reference/1", "authority": "none",
                          "status": "refused", "message": str(error)}))
        return 2
    print(json.dumps({"schema": "zeno-fcis/program-search-reference/1", "authority": "none", **result},
                     sort_keys=True))
    return 0 if result["status"] == "equal" else 1


if __name__ == "__main__":
    sys.exit(main())
