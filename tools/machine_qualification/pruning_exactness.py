"""Exactness control for search pruning: every removed sequence needs a kept witness proved equal by Z3.

For a small grammar and length on one target, every instruction sequence is enumerated twice, with pruning off and
on. For each sequence the pruning removed, the control looks for a kept sequence of no greater cost, in bytes and
in instructions, whose observed results are identical on every entry state: every register, and for x86 profile 2
every entry flag, is a free symbol, and Z3 must prove that no state makes the observed result registers differ. A
removed sequence the checker refuses statically (it reads a flag left undefined) can never be accepted, so it
needs no witness. Any other removed sequence without such a witness is an exactness failure.

Witnesses are tried in this order: the sequence with one instruction deleted, the sequence with two neighbouring
instructions swapped, then every kept sequence of no greater cost whose results agree on fixed sample states. Only
the Z3 proof accepts a witness; the samples only order the attempts. The proof covers the registered ISA models in
`verify.py` and `verify2.py`, not hardware.

The pruning under test is a predicate `pruned(target, forms, results) -> str | None` over the grammar forms of one
sequence, in enumeration order; it returns the pruning reason or None.
"""

from __future__ import annotations

from dataclasses import dataclass
from itertools import product
from pathlib import Path
import random
import sys
from typing import Callable

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
from machine_code import verify as v1, verify2 as v2  # noqa: E402
from machine_code.targets import encode_instruction, encode_program  # noqa: E402

MASK = (1 << 64) - 1
SAMPLES = 12
Pruner = Callable[[str, tuple, int], "str | None"]


@dataclass(frozen=True)
class Grammar:
    """One small exactness case: a target, its forms, the longest sequence and the observed result count."""

    name: str
    target: str
    forms: tuple
    length: int
    results: int = 1


# Small grammars for both profiles and both targets, with argument, scratch and flag interactions.
CASES = (
    Grammar("x86-profile1", v1.X86, (("mov", "rax", "rdi"), ("mov", "rax", "rsi"), ("add", "rax", "rsi"),
                                     ("mov", "rcx", "rax"), ("xor", "rax", "rcx"), ("shl", "rax", 1)), 3),
    Grammar("rv64i-profile1", v1.RV64, (("add", "a0", "a0", "a1"), ("sub", "t0", "a0", "a1"), ("addi", "a0", "t0", 0),
                                        ("xor", "a0", "a0", "a0"), ("slli", "t0", "a0", 1)), 3),
    Grammar("x86-profile2-one-result", v2.X86,
            (("mov", "rax", "rdi"), ("add", "rax", "rsi"), ("cmp", "rdi", "rsi"), ("cmovb", "rax", "rsi"),
             ("setb", "rdx"), ("adc", "rax", "rsi"), ("mov", "rdx", "rsi")), 3, 1),
    Grammar("x86-profile2-two-results", v2.X86,
            (("mov", "rax", "rdi"), ("add", "rax", "rsi"), ("cmp", "rdi", "rsi"), ("cmovb", "rax", "rsi"),
             ("setb", "rdx"), ("adc", "rax", "rsi"), ("mov", "rdx", "rsi")), 3, 2),
    Grammar("rv64i-profile2-one-result", v2.RV64,
            (("add", "a0", "a0", "a1"), ("sltu", "t0", "a0", "a1"), ("add", "a1", "a1", "t0"),
             ("addi", "t0", "zero", 1), ("sub", "a0", "a0", "a1")), 3, 1),
    Grammar("rv64i-profile2-two-results", v2.RV64,
            (("add", "a0", "a0", "a1"), ("sltu", "t0", "a0", "a1"), ("add", "a1", "a1", "t0"),
             ("addi", "t0", "zero", 1), ("sub", "a0", "a0", "a1")), 3, 2),
)


def profile2(target: str) -> bool:
    return target in (v2.X86, v2.RV64)


def code(target: str, forms: tuple) -> bytes:
    return encode_program(target, [list(form) for form in forms])


def cost(target: str, forms: tuple) -> tuple[int, int]:
    return sum(len(encode_instruction(target, list(form))) for form in forms), len(forms)


def refused(target: str, forms: tuple) -> bool:
    """Whether the checker refuses the sequence before any solver call: it reads a flag left undefined."""
    if not profile2(target):
        return False
    try:
        v2.require_defined_flag_reads(target, v2.decode(target, code(target, forms)))
    except ValueError:
        return True
    return False


def registers(target: str) -> tuple:
    if profile2(target):
        return v2.REGISTERS[target]
    return tuple(v1._SCRATCH[target]) + (("rdi", "rsi") if target == v1.X86 else ("a0", "a1"))


def results(target: str, count: int) -> tuple:
    if profile2(target):
        return v2.RESULTS[target][:count]
    return ("rax",) if target == v1.X86 else ("a0",)


def concrete(target: str, forms: tuple, state: dict, flags: dict | None, count: int) -> tuple:
    """Observed results of one sequence from one complete entry state."""
    raw = code(target, forms)
    if profile2(target):
        final, _ = v2.execute(target, raw, dict(state), dict(flags) if flags is not None else None)
        return tuple(final[name] for name in results(target, count))
    arguments = ("rdi", "rsi") if target == v1.X86 else ("a0", "a1")
    initial = {name: value for name, value in state.items() if name not in arguments}
    return (v1.run_concrete(target, raw, {"x": state[arguments[0]], "y": state[arguments[1]]}, initial),)


def samples(target: str) -> list:
    rng = random.Random(20261030)
    edge = [0, 1, MASK, 1 << 63, (1 << 63) - 1, 0x5555555555555555]
    states = []
    for index in range(SAMPLES):
        state = {name: (rng.choice(edge) if index % 3 == 0 else rng.getrandbits(64)) for name in registers(target)}
        flags = {name: rng.getrandbits(1) for name in v2.FLAGS} if target == v2.X86 else None
        states.append((state, flags))
    return states


def proved_equal(target: str, left: tuple, right: tuple, count: int, timeout_ms: int = 10_000) -> bool:
    """Z3 proof that both sequences give the same observed results from every entry state."""
    import z3
    context = z3.Context()
    state = {name: z3.BitVec("entry_" + name, 64, ctx=context) for name in registers(target)}
    observed = []
    for forms in (left, right):
        raw = code(target, forms)
        if profile2(target):
            flags = {name: z3.Bool("entry_" + name, ctx=context) for name in v2.FLAGS} if target == v2.X86 else {}
            final, _ = v2.machine_smt(z3, target, v2.decode(target, raw), state, flags, context)
            observed.append([final[name] for name in results(target, count)])
        else:
            arguments = ("rdi", "rsi") if target == v1.X86 else ("a0", "a1")
            inputs = {"x": state[arguments[0]], "y": state[arguments[1]]}
            scratch = {name: state[name] for name in v1._SCRATCH[target]}
            observed.append([v1._machine_smt(z3, target, v1.decode(target, raw), inputs, scratch)])
    solver = z3.Solver(ctx=context)
    solver.set(timeout=timeout_ms)
    solver.add(z3.Or([a != b for a, b in zip(*observed)]))
    return solver.check() == z3.unsat


def neighbours(forms: tuple):
    for index in range(len(forms)):
        yield forms[:index] + forms[index + 1:]
    for index in range(len(forms) - 1):
        yield forms[:index] + (forms[index + 1], forms[index]) + forms[index + 2:]


def check(case: Grammar, pruned: Pruner) -> dict:
    """Enumerate the case with pruning off and on and witness every removed sequence."""
    every = [tuple(case.forms[i] for i in choice) for length in range(case.length + 1)
             for choice in product(range(len(case.forms)), repeat=length)]
    reasons = {forms: pruned(case.target, forms, case.results) for forms in every}
    kept = [forms for forms in every if reasons[forms] is None]
    removed = [forms for forms in every if reasons[forms] is not None]
    states = samples(case.target)

    def fingerprint(forms):
        try:
            return tuple(concrete(case.target, forms, state, flags, case.results) for state, flags in states)
        except ValueError:
            return None

    prints = {forms: fingerprint(forms) for forms in kept}
    kept_set = set(kept)
    report = {"case": case.name, "target": case.target, "forms": len(case.forms), "length": case.length,
              "results": case.results, "enumerated": len(every), "kept": len(kept), "removed": len(removed),
              "removed_refused": 0, "witnessed": 0, "failures": [], "reasons": {}}
    for forms in removed:
        report["reasons"][reasons[forms]] = report["reasons"].get(reasons[forms], 0) + 1
        if refused(case.target, forms):
            report["removed_refused"] += 1
            continue
        target_print = fingerprint(forms)
        bound = cost(case.target, forms)
        ordered = [n for n in neighbours(forms) if n in kept_set]
        ordered += [k for k in kept if k not in ordered]
        witness = None
        for candidate in ordered:
            size = cost(case.target, candidate)
            if size[0] > bound[0] or size[1] > bound[1] or prints[candidate] != target_print:
                continue
            if proved_equal(case.target, forms, candidate, case.results):
                witness = candidate
                break
        if witness is None:
            report["failures"].append({"removed": [list(f) for f in forms], "reason": reasons[forms]})
        else:
            report["witnessed"] += 1
    report["exact"] = not report["failures"]
    return report
