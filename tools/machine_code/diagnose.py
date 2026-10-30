"""Advisory counterexample diagnosis for machine-code proposers.

Nothing here decides anything. The independent checker's verdict is final, and this module never changes it.
For one counterexample (actual inputs and scratch registers), it uses the concrete model to report three things:
- `trace`: the value of every admitted register after each body instruction;
- `spec_subterms`: every subterm of the declared expression with its value, and the earliest point at which it
  appears, with every register that holds it then;
- `first_missing_subterm`: the earliest compound subterm whose value never appears in any register.

That tells a proposer where its computation departs from the requirement. A value can appear by coincidence, so an
appearance is a hint, not evidence of a correct partial computation.

Profile-2 problems get the same report over every declared result, using the profile-2 concrete model. Their trace
also shows CF, ZF, SF and OF on x86, with null for a flag the model leaves undefined, and each view lists the
expected and actual value of every result. Subterms are matched against registers only. A comparison usually lives
in a flag, so a comparison reported as never computed may still be held in a flag the trace shows.
"""

from __future__ import annotations

import random

from . import profile2, verify2
from .model import evaluate
from .targets import encode_instruction
from .verify import X86, decode, run_concrete

REGISTERS = {X86: ("rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"),
             "rv64i-register/1": ("a0", "a1", "t0", "t1")}
NOTE = ("Advisory and untrusted: computed with the concrete model on this counterexample only. "
        "It never changes a verdict; a matching value can be a coincidence.")


def _children(node) -> list:
    op = node[0]
    if op in ("input", "const"):
        return []
    if op in ("not", "shl", "lshr", "ashr"):
        return [node[1]]
    return list(node[1:])


def _text(node) -> str:
    op = node[0]
    if op in ("input", "const"):
        return node[1]
    if op in ("shl", "lshr", "ashr"):
        return f"({op} {_text(node[1])} {node[2]})"
    return "(" + " ".join([op] + [_text(child) for child in node[1:]]) + ")"


def _subterms(node, out):
    for child in _children(node):
        _subterms(child, out)
    out.append(node)
    return out


def _read(target, prefix: bytes, register: str, inputs, initial) -> int:
    if target == X86:
        move = b"" if register == "rax" else encode_instruction(target, ["mov", "rax", register])
        return run_concrete(target, prefix + move + b"\xc3", inputs, initial)
    move = b"" if register == "a0" else encode_instruction(target, ["addi", "a0", register, 0])
    return run_concrete(target, prefix + move + bytes.fromhex("67800000"), inputs, initial)


def _generic_failure(problem, code, scratch_names):
    """A deterministic pseudo-random input on which the candidate also disagrees with the expression, if any."""
    rng = random.Random(0x5EED)
    for _ in range(64):
        inputs = {"x": rng.getrandbits(64), "y": rng.getrandbits(64)}
        initial = {name: rng.getrandbits(64) for name in scratch_names}
        if run_concrete(problem["target"], code, inputs, initial) != evaluate(problem["expression"], inputs["x"], inputs["y"]):
            return inputs, initial
    return None


def diagnose(problem: dict, code: bytes, verification: dict) -> dict:
    """Return the advisory diagnosis for a counterexample verdict, or a reason it is unavailable.

    Solver counterexamples often use degenerate values such as zero, which make value matching ambiguous. The
    diagnosis therefore also covers the first of 64 deterministic pseudo-random inputs on which the candidate
    disagrees, and `first_missing_subterm` prefers that generic failure.
    """
    if verification.get("status") != "counterexample":
        return {"available": False, "reason": "only counterexamples are diagnosed"}
    if problem.get("schema") == profile2.SCHEMA:
        return _diagnose_profile2(problem, code, verification)
    inputs = {k: int(v) for k, v in verification["inputs"].items()}
    initial = {k: int(v) for k, v in verification["initial_registers"].items()}
    solver = _one(problem, code, inputs, initial)
    generic = _generic_failure(problem, code, tuple(initial))
    generic_report = _one(problem, code, *generic) if generic else None
    preferred = generic_report or solver
    return {"available": True, "authority": "none", "note": NOTE, "solver_counterexample": solver,
            "generic_failure": generic_report, "first_missing_subterm": preferred["first_missing_subterm"]}


def _one(problem: dict, code: bytes, inputs: dict, initial: dict) -> dict:
    target = problem["target"]
    instructions = decode(target, code)
    states = []
    for index in range(len(instructions)):  # index 0 is the entry state; index k is after body instruction k
        prefix = code[:instructions[index - 1].offset + instructions[index - 1].size] if index else b""
        states.append({r: _read(target, prefix, r, inputs, initial) for r in REGISTERS[target]})
    trace = [{"after": k, "instruction": instructions[k - 1].text(),
              "registers": {r: str(v) for r, v in states[k].items()}} for k in range(1, len(states))]
    subterms, missing = [], None
    for node in _subterms(problem["expression"], []):
        value = evaluate(node, inputs["x"], inputs["y"])
        seen = next(({"after": k, "registers": [r for r, v in state.items() if v == value]}
                     for k, state in enumerate(states) if value in state.values()), None)
        subterms.append({"expression": _text(node), "value": str(value), "first_seen": seen})
        if seen is None and missing is None and node[0] not in ("input", "const"):
            missing = {"expression": _text(node), "value": str(value)}
    return {"inputs": {k: str(v) for k, v in inputs.items()}, "initial_registers": {k: str(v) for k, v in initial.items()},
            "entry": {r: str(v) for r, v in states[0].items()}, "trace": trace,
            "spec_subterms": subterms, "first_missing_subterm": missing}


def _diagnose_profile2(problem: dict, code: bytes, verification: dict) -> dict:
    names, target = problem["inputs"], problem["target"]
    arguments = [int(verification["inputs"][name]) for name in names]
    initial = {k: int(v) for k, v in verification["initial_registers"].items()}
    flags = verification.get("initial_flags")
    solver = _one_profile2(problem, code, arguments, initial, flags)
    rng = random.Random(0x5EED)
    generic_report = None
    for _ in range(64):
        values = [rng.getrandbits(64) for _ in names]
        scratch = {name: rng.getrandbits(64) for name in initial}
        entry = {name: rng.getrandbits(1) for name in verify2.FLAGS} if target == verify2.X86 else None
        env = dict(zip(names, values))
        if (verify2.run_concrete(target, code, values, scratch, entry, len(problem["outputs"]))
                != [profile2.evaluate(output, env) for output in problem["outputs"]]):
            generic_report = _one_profile2(problem, code, values, scratch, entry)
            break
    preferred = generic_report or solver
    return {"available": True, "authority": "none", "note": NOTE, "solver_counterexample": solver,
            "generic_failure": generic_report, "first_missing_subterm": preferred["first_missing_subterm"]}


def _one_profile2(problem: dict, code: bytes, arguments: list, initial: dict, flags) -> dict:
    target, names = problem["target"], problem["inputs"]
    x86 = target == verify2.X86
    instructions = verify2.decode(target, code)
    terminal = code[instructions[-1].offset:]
    registers, entry_flags = verify2.entry_state(target, arguments, initial, flags)
    states = [(registers, entry_flags)]
    for ins in instructions[:-1]:
        states.append(verify2.execute(target, code[:ins.offset + ins.size] + terminal, registers, entry_flags))
    shown = lambda state: {"registers": {r: str(state[0][r]) for r in verify2.REGISTERS[target]},
                           **({"flags": dict(state[1])} if x86 else {})}
    trace = [{"after": k, "instruction": instructions[k - 1].text(), **shown(states[k])}
             for k in range(1, len(states))]
    env = dict(zip(names, arguments))
    final = states[-1][0]
    results = [{"register": register, "expected": str(profile2.evaluate(output, env)), "actual": str(final[register])}
               for register, output in zip(verify2.RESULTS[target], problem["outputs"])]
    subterms, missing = [], None
    for index, output in enumerate(problem["outputs"]):
        for node in _subterms(output, []):
            value = profile2.evaluate(node, env)
            seen = next(({"after": k, "registers": [r for r, v in state[0].items() if v == value]}
                         for k, state in enumerate(states) if value in state[0].values()), None)
            subterms.append({"output": index, "expression": _text(node), "value": str(value), "first_seen": seen})
            if seen is None and missing is None and node[0] not in ("input", "const"):
                missing = {"output": index, "expression": _text(node), "value": str(value)}
    return {"inputs": {n: str(v) for n, v in env.items()}, "initial_registers": {k: str(v) for k, v in initial.items()},
            **({"initial_flags": dict(states[0][1])} if x86 else {}), "entry": shown(states[0])["registers"],
            "trace": trace, "results": results, "spec_subterms": subterms, "first_missing_subterm": missing}
