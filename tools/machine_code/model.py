"""Closed problem data and an integer reference for modular word kernels."""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

MASK = (1 << 64) - 1
TARGETS = ("x86_64-sysv-register/1", "rv64i-register/1")
MAX_JSON_BYTES = 65_536


def canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True,
                      allow_nan=False).encode("ascii")


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def integer(value: object, low: int, high: int, name: str) -> int:
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"{name} must be an integer in [{low},{high}]")
    return value


def word(value: object) -> int:
    if not isinstance(value, str) or not re.fullmatch(r"0|[1-9][0-9]{0,19}", value):
        raise ValueError("word literals must be canonical unsigned decimal strings")
    result = int(value)
    if result > MASK:
        raise ValueError("word literal exceeds unsigned64")
    return result


def _pairs(items):
    result = {}
    for key, value in items:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path: Path) -> dict:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_JSON_BYTES:
        raise ValueError("JSON input must be a bounded regular non-symlink file")
    with path.open("rb") as stream:
        data = stream.read(MAX_JSON_BYTES + 1)
    if len(data) > MAX_JSON_BYTES:
        raise ValueError("JSON input grew beyond its byte limit")
    def bad_constant(value):
        raise ValueError(f"non-JSON constant: {value}")
    value = json.loads(data.decode("utf-8"), object_pairs_hook=_pairs,
                       parse_constant=bad_constant)
    if not isinstance(value, dict):
        raise ValueError("JSON root must be an object")
    return value


def validate_problem(problem: dict) -> dict:
    keys = {"schema", "target", "inputs", "expression", "grammar", "max_instructions", "objective"}
    if not isinstance(problem, dict) or set(problem) != keys:
        raise ValueError("machine problem fields differ from the closed version1 schema")
    if problem["schema"] != "zeno-fcis/machine-problem/1" or problem["target"] not in TARGETS:
        raise ValueError("unsupported machine problem schema or target")
    if problem["inputs"] != ["x", "y"]:
        raise ValueError("profile1 declares exactly the complete unsigned64 x,y input pair")
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
    nodes = 0
    def visit(expr, depth=0):
        nonlocal nodes
        nodes += 1
        if nodes > 64 or depth > 16 or not isinstance(expr, list) or not expr:
            raise ValueError("expression exceeds its depth/node/shape profile")
        op = expr[0]
        if op == "input" and len(expr) == 2 and expr[1] in ("x", "y"):
            return
        if op == "const" and len(expr) == 2:
            word(expr[1]); return
        if op == "not" and len(expr) == 2:
            visit(expr[1], depth + 1); return
        if op in ("shl", "lshr") and len(expr) == 3:
            visit(expr[1], depth + 1)
            integer(expr[2], 0, 63, "shift")
            return
        if op in ("add", "sub", "mul", "xor", "and", "or") and len(expr) == 3:
            visit(expr[1], depth + 1); visit(expr[2], depth + 1); return
        raise ValueError("unsupported expression constructor")
    visit(problem["expression"])
    if len(canonical(problem)) > MAX_JSON_BYTES:
        raise ValueError("problem exceeds its encoded byte profile")
    return json.loads(canonical(problem))


def evaluate(expr: list, x: int, y: int) -> int:
    """Mathematical integer oracle, separate from machine-byte evaluation."""
    op = expr[0]
    if op == "input":
        return {"x": x, "y": y}[expr[1]] & MASK
    if op == "const":
        return word(expr[1])
    left = evaluate(expr[1], x, y)
    if op == "not":
        return (~left) & MASK
    if op == "shl":
        return (left << expr[2]) & MASK
    if op == "lshr":
        return left >> expr[2]
    right = evaluate(expr[2], x, y)
    return {"add": lambda: left + right, "sub": lambda: left - right,
            "mul": lambda: left * right, "xor": lambda: left ^ right,
            "and": lambda: left & right, "or": lambda: left | right}[op]() & MASK
