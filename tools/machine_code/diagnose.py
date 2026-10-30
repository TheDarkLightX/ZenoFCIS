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

Profile-3 problems add a memory view from the profile-3 concrete model: every region word at entry and after each
instruction, the words each instruction wrote, every read-write word's expected and actual final value, and
`first_differing_region_word`, the first read-write word (in region and index order) whose final value differs,
with the instruction that last wrote it. Subterms are matched against registers and region words.
"""

from __future__ import annotations

import random

from . import profile2, profile3, verify2
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
    if problem.get("schema") == profile3.SCHEMA:
        return _diagnose_profile3(problem, code, verification)
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


def _diagnose_profile3(problem: dict, code: bytes, verification: dict) -> dict:
    from . import verify3
    target, regions = problem["target"], problem["regions"]
    x86 = target == verify3.X86
    inputs = {name: int(verification["inputs"][name]) for name in problem["inputs"]}
    bases = {name: int(value) for name, value in verification["bases"].items()}
    initial = {k: int(v) for k, v in verification["initial_registers"].items()}
    memory = {name: [int(v) for v in words] for name, words in verification["initial_regions"].items()}
    solver = _one_profile3(problem, code, inputs, bases, initial, verification.get("initial_flags"), memory)
    rng = random.Random(0x5EED)
    generic_report = None
    for _ in range(64):
        values = {name: rng.getrandbits(64) for name in problem["inputs"]}
        pointers = {region["name"]: rng.getrandbits(64) for region in regions}
        scratch = {name: rng.getrandbits(64) for name in initial}
        entry = {name: rng.getrandbits(1) for name in verify3.FLAGS} if x86 else None
        words = {region["name"]: [rng.getrandbits(64) for _ in range(region["words"])] for region in regions}
        actual, final = verify3.run_concrete(problem, code, values, pointers, words, scratch, entry)
        results, writable = profile3.expected(problem, values, words)
        if actual != results or any(final[name] != expected for name, expected in writable.items()):
            generic_report = _one_profile3(problem, code, values, pointers, scratch, entry, words)
            break
    preferred = generic_report or solver
    return {"available": True, "authority": "none", "note": NOTE, "solver_counterexample": solver,
            "generic_failure": generic_report, "first_missing_subterm": preferred["first_missing_subterm"],
            "first_differing_region_word": preferred["first_differing_region_word"]}


def _one_profile3(problem: dict, code: bytes, inputs: dict, bases: dict, initial: dict, flags, memory: dict) -> dict:
    from . import verify3
    target = problem["target"]
    x86 = target == verify3.X86
    input_registers, base_registers, scratch = verify3._layout(problem)
    registers = {name: initial.get(name, 0) for name in scratch}
    registers.update({input_registers[n]: v for n, v in inputs.items()})
    registers.update({base_registers[n]: v for n, v in bases.items()})
    entry_flags = {name: (flags or {}).get(name, 0) for name in verify3.FLAGS} if x86 else None
    instructions = verify3.decode(target, code)
    terminal = code[instructions[-1].offset:]
    states = [(registers, entry_flags, {name: list(words) for name, words in memory.items()})]
    for ins in instructions[:-1]:
        states.append(verify3.execute(problem, code[:ins.offset + ins.size] + terminal, registers, entry_flags, memory))
    text = lambda words: [str(v) for v in words]
    shown = lambda state: {"registers": {r: str(state[0][r]) for r in verify3.REGISTERS[target]},
                           **({"flags": dict(state[1])} if x86 else {}),
                           "regions": {name: text(words) for name, words in state[2].items()}}
    trace = []
    for k in range(1, len(states)):
        before, after = states[k - 1][2], states[k][2]
        wrote = [{"region": name, "index": i, "before": str(before[name][i]), "after": str(after[name][i])}
                 for name in after for i in range(len(after[name])) if before[name][i] != after[name][i]]
        trace.append({"after": k, "instruction": instructions[k - 1].text(), **shown(states[k]),
                      "region_writes": wrote})
    final_registers, final_memory = states[-1][0], states[-1][2]
    results, writable = profile3.expected(problem, inputs, memory)
    result_rows = [{"register": register, "expected": str(value), "actual": str(final_registers[register])}
                   for register, value in zip(verify3.RESULTS[target], results)]
    words, differing = [], None
    for region in problem["regions"]:
        name = region["name"]
        for i, value in enumerate(writable.get(name, [])):
            row = {"region": name, "index": i, "initial": str(memory[name][i]), "expected": str(value),
                   "actual": str(final_memory[name][i])}
            words.append(row)
            if differing is None and final_memory[name][i] != value:
                written = [k for k in range(1, len(states)) if states[k][2][name][i] != states[k - 1][2][name][i]
                           or (instructions[k - 1].op == "store" and verify3._access(instructions[k - 1].form())
                               == (region["base"], 8 * i))]
                differing = {**row, "last_written_after": written[-1] if written else None}
    expressions = [(f"result {k + 1}", output) for k, output in enumerate(problem["outputs"])]
    expressions += [(f"{name}[{i}]", e) for name, exprs in problem["memory"].items() for i, e in enumerate(exprs)]
    lifted_inputs = {**inputs, **{profile3.slot(n, i): v for n, values in memory.items() for i, v in enumerate(values)}}
    words_named = lambda state: {f"{n}[{i}]": v for n, values in state[2].items() for i, v in enumerate(values)}
    subterms, missing = [], None
    for label, expression in expressions:
        for node in _subterms(profile3.lifted({**problem, "outputs": [expression], "memory": {}})[0][0], []):
            value = profile2.evaluate(node, lifted_inputs)
            seen = next(({"after": k, "registers": [r for r, v in state[0].items() if v == value],
                          "region_words": [w for w, v in words_named(state).items() if v == value]}
                         for k, state in enumerate(states)
                         if value in state[0].values() or value in words_named(state).values()), None)
            subterms.append({"of": label, "expression": _text(node), "value": str(value), "first_seen": seen})
            if seen is None and missing is None and node[0] not in ("input", "const"):
                missing = {"of": label, "expression": _text(node), "value": str(value)}
    return {"inputs": {n: str(v) for n, v in inputs.items()}, "bases": {n: str(v) for n, v in bases.items()},
            "initial_registers": {k: str(v) for k, v in initial.items()},
            **({"initial_flags": dict(entry_flags)} if x86 else {}), "entry": shown(states[0]), "trace": trace,
            "results": result_rows, "region_words": words, "first_differing_region_word": differing,
            "spec_subterms": subterms, "first_missing_subterm": missing}
