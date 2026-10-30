"""Closed profile-3 problem data and an integer reference for straight-line memory kernels.

Profile 3 keeps the profile-2 word operations and adds declared memory regions. A region names the argument
register that holds its base, a count of 1 to 16 64-bit words, and an access, read-only or read-write. Word
inputs take the argument registers that no region names, in order. Expressions may read the initial word INDEX of
a region as ["initial", REGION, INDEX]. A problem states up to two results and the expected final contents of every
read-write region, over the inputs and the initial region words. Regions are assumed pairwise disjoint and disjoint
from the stack; that assumption is part of the scope, not checked.
"""

from __future__ import annotations

import json
import re

from . import profile2
from .model import MAX_JSON_BYTES, canonical, integer

SCHEMA = "zeno-fcis/machine-problem/3"
TARGETS = ("x86_64-sysv-memory/3", "rv64i-memory/3")
ARGUMENTS = {TARGETS[0]: ("rdi", "rsi", "rdx", "rcx", "r8", "r9"), TARGETS[1]: ("a0", "a1", "a2", "a3", "a4", "a5")}
RESULTS = {TARGETS[0]: ("rax", "rdx"), TARGETS[1]: ("a0", "a1")}
ACCESS = ("read-only", "read-write")
MAX_WORDS = 16
NAME = re.compile(r"[a-z][a-z0-9_]{0,15}")
FIELDS = {"schema", "target", "inputs", "regions", "outputs", "memory", "grammar", "max_instructions", "objective"}


def slot(region: str, index: int) -> str:
    """The reference name of one initial region word; '#' cannot occur in a declared name."""
    return f"{region}#{index}"


def registers(problem: dict) -> tuple[dict, dict]:
    """(word input -> argument register, region -> base register) for a validated problem."""
    bases = {region["name"]: region["base"] for region in problem["regions"]}
    free = [name for name in ARGUMENTS[problem["target"]] if name not in bases.values()]
    return dict(zip(problem["inputs"], free)), bases


def _lift(expr, regions: dict, depth=0):
    """Replace each ["initial", REGION, INDEX] by an input reference, checking the region word exists."""
    if depth > 16 or not isinstance(expr, list) or not expr:
        raise ValueError("expression exceeds its depth/node/shape profile")
    if expr[0] == "initial":
        if (len(expr) != 3 or type(expr[1]) is not str or expr[1] not in regions or type(expr[2]) is not int
                or not 0 <= expr[2] < regions[expr[1]]):
            raise ValueError("initial names a declared region and a word index inside it")
        return ["input", slot(expr[1], expr[2])]
    if expr[0] in ("input", "const"):
        return list(expr)
    if expr[0] in profile2.SHIFTS:
        return [expr[0], _lift(expr[1], regions, depth + 1), *expr[2:]]
    return [expr[0], *(_lift(part, regions, depth + 1) for part in expr[1:])]


def _regions(problem: dict) -> dict:
    regions = problem["regions"]
    if not isinstance(regions, list) or not 1 <= len(regions) <= 4:
        raise ValueError("profile3 declares one to four regions")
    seen, bases = set(), set()
    for region in regions:
        if not isinstance(region, dict) or set(region) != {"name", "base", "words", "access"}:
            raise ValueError("region fields are name, base, words and access")
        if type(region["name"]) is not str or not NAME.fullmatch(region["name"]) or region["name"] in seen:
            raise ValueError("region names are distinct lowercase names")
        if region["base"] not in ARGUMENTS[problem["target"]] or region["base"] in bases:
            raise ValueError("each region names a distinct argument register as its base")
        integer(region["words"], 1, MAX_WORDS, "region words")
        if region["access"] not in ACCESS:
            raise ValueError("region access must be read-only or read-write")
        seen.add(region["name"]); bases.add(region["base"])
    return {region["name"]: region["words"] for region in regions}


def validate_problem(problem: dict) -> dict:
    if not isinstance(problem, dict) or set(problem) != FIELDS:
        raise ValueError("machine problem fields differ from the closed version3 schema")
    if problem["schema"] != SCHEMA or problem["target"] not in TARGETS:
        raise ValueError("unsupported machine problem schema or target")
    words = _regions(problem)
    inputs = problem["inputs"]
    if (not isinstance(inputs, list) or len(set(inputs)) != len(inputs)
            or any(type(name) is not str or not NAME.fullmatch(name) or name in words for name in inputs)):
        raise ValueError("inputs are distinct lowercase names that no region uses")
    if len(inputs) + len(words) > len(ARGUMENTS[problem["target"]]):
        raise ValueError("word inputs and region bases together take at most six argument registers")
    integer(problem["max_instructions"], 0, 16, "max_instructions")
    if problem["objective"] not in ("bytes", "instructions"):
        raise ValueError("objective must be bytes or instructions")
    grammar = problem["grammar"]
    if not isinstance(grammar, list) or len(grammar) > 32 or any(
            not isinstance(item, list) or not 1 <= len(item) <= 5 or any(type(v) not in (str, int) for v in item)
            or any(isinstance(v, str) and len(v) > 32 for v in item) for item in grammar):
        raise ValueError("grammar holds at most32 arrays of at most5 string or integer atoms")
    outputs, memory = problem["outputs"], problem["memory"]
    writable = {r["name"] for r in problem["regions"] if r["access"] == "read-write"}
    if not isinstance(outputs, list) or len(outputs) > 2:
        raise ValueError("profile3 declares zero to two result expressions")
    if not isinstance(memory, dict) or set(memory) != writable:
        raise ValueError("memory gives the final contents of exactly the read-write regions")
    if not outputs and not writable:
        raise ValueError("a problem needs a result or a read-write region to check")
    names = list(inputs) + [slot(name, i) for name, count in words.items() for i in range(count)]
    expressions = list(outputs)
    for name in sorted(writable):
        if not isinstance(memory[name], list) or len(memory[name]) != words[name]:
            raise ValueError("memory gives one expression for every word of its region")
        expressions += memory[name]
    for expr in expressions:
        if profile2._nodes(_lift(expr, words), names) > 64:
            raise ValueError("expression exceeds its depth/node/shape profile")
    if len(canonical(problem)) > MAX_JSON_BYTES:
        raise ValueError("problem exceeds its encoded byte profile")
    return json.loads(canonical(problem))


def evaluate(problem: dict, expr: list, inputs: dict, initial: dict) -> int:
    """Integer reference for one expression over named inputs and initial region words."""
    words = {r["name"]: r["words"] for r in problem["regions"]}
    env = dict(inputs)
    env.update({slot(name, i): word_value for name, values in initial.items() for i, word_value in enumerate(values)})
    return profile2.evaluate(_lift(expr, words), env)


def expected(problem: dict, inputs: dict, initial: dict) -> tuple[list, dict]:
    """(results, final read-write region words) the problem specifies for one state."""
    results = [evaluate(problem, output, inputs, initial) for output in problem["outputs"]]
    regions = {name: [evaluate(problem, e, inputs, initial) for e in exprs] for name, exprs in problem["memory"].items()}
    return results, regions


def lifted(problem: dict) -> tuple[list, dict]:
    """Outputs and memory expressions with initial words as input references, for the checker's semantics."""
    words = {r["name"]: r["words"] for r in problem["regions"]}
    return ([_lift(e, words) for e in problem["outputs"]],
            {name: [_lift(e, words) for e in exprs] for name, exprs in problem["memory"].items()})

