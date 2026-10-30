"""Closed profile-2 problem data and an integer reference for flag-based word kernels.

Profile 2 keeps the modular-64 operations of profile 1 and adds up to six named
unsigned 64-bit inputs (none for a constant problem), up to two results, comparisons returning 0 or 1, select
and arithmetic right shift. Profile-1 problems keep their own validator.
"""

from __future__ import annotations

import json
import re

from .model import MASK, MAX_JSON_BYTES, canonical, integer, word

SCHEMA = "zeno-fcis/machine-problem/2"
TARGETS = ("x86_64-sysv-register/2", "rv64i-register/2")
MAX_INPUTS = 6
MAX_OUTPUTS = 2
SIGN = 1 << 63
UNARY = ("not",)
SHIFTS = ("shl", "lshr", "ashr")
BINARY = ("add", "sub", "mul", "and", "or", "xor", "ult", "ule", "slt", "sle", "eq")


def validate_problem(problem: dict) -> dict:
    keys = {"schema", "target", "inputs", "outputs", "grammar", "max_instructions", "objective"}
    if not isinstance(problem, dict) or set(problem) != keys:
        raise ValueError("machine problem fields differ from the closed version2 schema")
    if problem["schema"] != SCHEMA or problem["target"] not in TARGETS:
        raise ValueError("unsupported machine problem schema or target")
    inputs = problem["inputs"]
    # Zero inputs is a constant problem, for example one whose every input was fixed by specialization.
    if (not isinstance(inputs, list) or not 0 <= len(inputs) <= MAX_INPUTS or len(set(inputs)) != len(inputs)
            or any(type(name) is not str or not re.fullmatch(r"[a-z][a-z0-9_]{0,15}", name) for name in inputs)):
        raise ValueError("profile2 declares 0..6 distinct lowercase input names")
    integer(problem["max_instructions"], 0, 16, "max_instructions")
    if problem["objective"] not in ("bytes", "instructions"):
        raise ValueError("objective must be bytes or instructions")
    grammar = problem["grammar"]
    if not isinstance(grammar, list) or len(grammar) > 32:
        raise ValueError("grammar must contain at most32 instruction arrays")
    for instruction in grammar:
        if not isinstance(instruction, list) or not 1 <= len(instruction) <= 5:
            raise ValueError("grammar instruction must be an array of at most5 atoms")
        if any(type(value) not in (str, int) for value in instruction):
            raise ValueError("instruction atoms must be strings or integers")
        if any(isinstance(value, str) and len(value) > 32 for value in instruction):
            raise ValueError("instruction atom exceeds32 characters")
    outputs = problem["outputs"]
    if not isinstance(outputs, list) or not 1 <= len(outputs) <= MAX_OUTPUTS:
        raise ValueError("profile2 declares one or two output expressions")
    for output in outputs:
        if _nodes(output, inputs) > 64:
            raise ValueError("expression exceeds its depth/node/shape profile")
    if len(canonical(problem)) > MAX_JSON_BYTES:
        raise ValueError("problem exceeds its encoded byte profile")
    return json.loads(canonical(problem))


def _nodes(expr, inputs, depth=0) -> int:
    """Validate one output expression and return its node count."""
    if depth > 16 or not isinstance(expr, list) or not expr:
        raise ValueError("expression exceeds its depth/node/shape profile")
    op = expr[0]
    if op == "input" and len(expr) == 2 and type(expr[1]) is str and expr[1] in inputs:
        return 1
    if op == "const" and len(expr) == 2:
        word(expr[1])
        return 1
    if op in SHIFTS and len(expr) == 3:
        integer(expr[2], 0, 63, "shift")
        return 1 + _nodes(expr[1], inputs, depth + 1)
    if (op in UNARY and len(expr) == 2) or (op in BINARY and len(expr) == 3) or (op == "ite" and len(expr) == 4):
        return 1 + sum(_nodes(part, inputs, depth + 1) for part in expr[1:])
    raise ValueError("unsupported expression constructor")


def _signed(value: int) -> int:
    return value - (1 << 64) if value & SIGN else value


def evaluate(expr: list, env: dict) -> int:
    """Mathematical integer oracle over named u64 inputs, separate from machine-byte evaluation."""
    op = expr[0]
    if op == "input":
        return env[expr[1]] & MASK
    if op == "const":
        return word(expr[1])
    if op == "ite":
        return evaluate(expr[2], env) if evaluate(expr[1], env) else evaluate(expr[3], env)
    left = evaluate(expr[1], env)
    if op == "not":
        return (~left) & MASK
    if op == "shl":
        return (left << expr[2]) & MASK
    if op == "lshr":
        return left >> expr[2]
    if op == "ashr":
        return (_signed(left) >> expr[2]) & MASK
    right = evaluate(expr[2], env)
    return {"add": lambda: left + right, "sub": lambda: left - right,
            "mul": lambda: left * right, "xor": lambda: left ^ right,
            "and": lambda: left & right, "or": lambda: left | right,
            "ult": lambda: int(left < right), "ule": lambda: int(left <= right),
            "slt": lambda: int(_signed(left) < _signed(right)),
            "sle": lambda: int(_signed(left) <= _signed(right)),
            "eq": lambda: int(left == right)}[op]() & MASK
