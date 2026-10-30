"""Per-result backward slices of a checked profile-2 kernel, each checked as a one-result kernel.

In the spirit of McSlice, the slice of one result keeps the instructions whose writes reach that result register
at the return through later reads, following the exact footprints of `footprint.py` backwards. Those footprints come
from the checker's own symbolic semantics: writes and reads over-approximate and overwrites under-approximate, so a
partial or conditional write keeps the earlier value in the slice. Each slice becomes a one-result
kernel: its instructions in their original order, then, for the second result, a copy into the first result
register. The independent profile-2 checker must find every slice equivalent to its own output over every input,
scratch register and entry flag; the slicer itself has no authority.

The claims hold under the registered ISA model; they do not establish native conformance or timing.
"""

from __future__ import annotations

from pathlib import Path

from . import footprint, profile2, verify2
from .model import canonical, digest
from .targets import encode_program

SCHEMA = "zeno-fcis/machine-slices/1"


def _require(condition, reason):
    if not condition:
        raise ValueError(reason)


def backward(prints, register):
    """Indices of the instructions whose writes reach `register` at the return through later reads."""
    needed, kept = {register}, []
    for index in range(len(prints) - 1, -1, -1):
        if not prints[index].writes & needed:
            continue
        kept.append(index)
        needed = (needed - prints[index].overwrites) | prints[index].reads
    return kept[::-1]


def instruction_footprints(problem: dict, forms: list) -> list:
    """The footprint of each instruction; a checked kernel's instructions are all grammar entries."""
    derived = {canonical(form): print_ for form, print_ in zip(problem["grammar"], footprint.footprints(problem))}
    prints = [derived.get(canonical(form)) for form in forms]
    _require(all(print_ is not None for print_ in prints), "FootprintUnavailable")
    return prints


def slices(problem: dict, code: bytes, timeout_ms: int = 1000) -> dict:
    """Slice every result of a kernel that checks equivalent to its problem, and check each slice."""
    _require(isinstance(problem, dict) and problem.get("schema") == profile2.SCHEMA, "SlicingSupportsProfile2Only")
    problem = profile2.validate_problem(problem)
    before = verify2.check(problem, code, timeout_ms)
    if before["status"] != "equivalent":
        return {"status": "refused", "reason": "KernelDidNotCheck", "verification": before}
    target = problem["target"]
    forms = [ins.form() for ins in verify2.decode(target, code)[:-1]]
    prints = instruction_footprints(problem, forms)
    first = verify2.RESULTS[target][0]
    rows = []
    for position, output in enumerate(problem["outputs"]):
        register = verify2.RESULTS[target][position]
        kept = backward(prints, register)
        body = [forms[index] for index in kept]
        if register != first:
            body.append(["mov", first, register] if target == verify2.X86 else ["addi", first, register, 0])
        sliced = encode_program(target, body)
        grammar = [list(form) for form in problem["grammar"]]
        grammar += [form for form in body if form not in grammar]
        one = profile2.validate_problem({**problem, "outputs": [output], "grammar": grammar,
                                         "max_instructions": max(problem["max_instructions"], len(body))})
        verdict = verify2.check(one, sliced, timeout_ms)
        rows.append({"result": position, "register": register, "instructions": kept,
                     "problem": one, "code_hex": sliced.hex(), "verification": verdict})
    accepted = all(row["verification"]["status"] == "equivalent" for row in rows)
    return {"status": "sliced" if accepted else "counterexample" if any(
                row["verification"]["status"] == "counterexample" for row in rows) else "inconclusive",
            "original_problem_sha256": digest(canonical(problem)), "original_code_sha256": digest(code),
            "slices": rows}


def write(result: dict, out: Path, problem: dict, code: bytes):
    """Write a checked slicing; refuse unless every slice is equivalent."""
    from .session import _json, _write
    _require(result["status"] == "sliced", "OnlyCheckedSlicesAreWritten")
    out.mkdir(mode=0o700)
    _json(out / "original.json", problem)
    _write(out / "original.bin", code)
    record = {"schema": SCHEMA, "authority": "none",
              "original_problem_sha256": result["original_problem_sha256"],
              "original_code_sha256": result["original_code_sha256"], "slices": []}
    for row in result["slices"]:
        name = f"result{row['result']}"
        _json(out / (name + ".json"), row["problem"])
        _write(out / (name + ".bin"), bytes.fromhex(row["code_hex"]))
        record["slices"].append({"result": row["result"], "register": row["register"],
                                 "instructions": row["instructions"],
                                 "problem_sha256": digest(canonical(row["problem"])),
                                 "code_sha256": digest(bytes.fromhex(row["code_hex"])),
                                 "verification": "equivalent", "cost": row["verification"]["cost"]})
    _json(out / "slices.json", record)
    return record
