"""Exactness control for search pruning: every removal needs a Z3-proved twin or a checker refutation.

For a small grammar, length and specification on one target, every instruction sequence is enumerated with pruning
off and on. The cases cover profiles 1 and 2 on both targets and every registered profile-3 memory kernel over its
own grammar. A twin of a removed sequence is a kept, admitted sequence of no greater cost, in bytes and in
instructions, and of no greater search rank (the problem's objective cost, the other cost, then the code's hex, the
search's final tie-break), whose observed results are identical on every entry state: every register, every x86
entry flag on profiles 2 and 3, and every profile-3 region word is a free symbol, and Z3 must prove that no state
makes the observed result registers (and, on profile 3, the read-write region words) differ. So an exhausted search
reports the same least-ranked kernel with or without the pruning. Each case runs two passes:

- twins: the lost-input rule is off (no required inputs). Every sequence removed as dead, out of order or a
  duplicate needs a twin. This pass does not depend on the specification.
- spec: the full pruning. A sequence removed because a required input is lost has no twin; the independent checker
  must find it not equivalent to the specification (a counterexample or a refusal). Any other removal needs a twin
  or, when its only twins lose an input, the same checker verdict for the sequence itself.

A removed sequence the checker refuses statically (it reads a flag left undefined) can never be accepted, so it
needs no witness. Any other removal that fails its pass is an exactness failure.

Witnesses are tried in this order: the sequence with one instruction deleted, the sequence with two neighbouring
instructions swapped, then every kept sequence of no greater cost whose results agree on fixed sample states. Only
the Z3 proof accepts a witness; the samples only order the attempts. The proof covers the registered ISA models in
`verify.py` and `verify2.py`, not hardware.

The pruning under test is a predicate `pruned(target, forms, results) -> str | None` over the grammar forms of one
sequence, in enumeration order; it returns the pruning reason or None. `search_pruner` adapts the search's own
`footprint.Pruner`, and checks that it removes exactly what `footprint.sequences` omits from the enumeration.

    python3 -B -m tools.machine_qualification.pruning_exactness run [--case NAME]... [--out REPORT.json]

Exit 0 when every case is exact, 1 when any removal has no witness.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
import hashlib
from itertools import product
import json
from pathlib import Path
import random
import sys
from typing import Callable

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
from machine_code import verify as v1, verify2 as v2, verify3 as v3  # noqa: E402
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
    spec: tuple = ()  # profile-2 outputs, one per result, or the profile-1 expression; x when empty
    base: dict | None = field(default=None, compare=False)  # a whole profile-3 problem

    def problem(self) -> dict:
        """A valid problem over the case's grammar: the lost-input rule reads its specification."""
        if self.base is not None:
            return {**self.base, "grammar": [list(form) for form in self.forms], "max_instructions": self.length}
        two = self.target in (v2.X86, v2.RV64)
        x = ["input", "x"]
        if two:
            body = {"inputs": ["x", "y"], "outputs": list(self.spec) or [x] * self.results}
        else:
            body = {"inputs": ["x", "y"], "expression": self.spec[0] if self.spec else x}
        return {"schema": "zeno-fcis/machine-problem/2" if two else "zeno-fcis/machine-problem/1",
                "target": self.target, "grammar": [list(form) for form in self.forms],
                "max_instructions": self.length, "objective": "bytes", **body}


X, Y = ["input", "x"], ["input", "y"]
SUM, CARRY = ["add", X, Y], ["ult", ["add", X, Y], X]
X86_PROFILE2 = (("mov", "rax", "rdi"), ("add", "rax", "rsi"), ("cmp", "rdi", "rsi"), ("cmovb", "rax", "rsi"),
                ("setb", "rdx"), ("adc", "rax", "rsi"), ("mov", "rdx", "rsi"), ("xor", "rdi", "rdi"))
RV64_PROFILE2 = (("add", "a0", "a0", "a1"), ("sltu", "t0", "a0", "a1"), ("add", "a1", "a1", "t0"),
                 ("addi", "t0", "zero", 1), ("sub", "a0", "a0", "a1"))
# Small grammars for both profiles and both targets, with argument, scratch and flag interactions. Forms that
# overwrite an argument register exercise the lost-input rule; equal-effect pairs exercise the duplicate rule.
CASES = (
    Grammar("x86-profile1", v1.X86, (("mov", "rax", "rdi"), ("mov", "rax", "rsi"), ("add", "rax", "rsi"),
                                     ("mov", "rcx", "rax"), ("xor", "rax", "rcx"), ("shl", "rax", 1),
                                     ("mov", "rdx", "rsi"), ("xor", "rdi", "rdi")), 3, spec=(SUM,)),
    # The specification ignores y, so overwriting a1 never loses a required input.
    Grammar("rv64i-profile1", v1.RV64, (("add", "a0", "a0", "a1"), ("sub", "t0", "a0", "a1"), ("addi", "a0", "t0", 0),
                                        ("xor", "a0", "a0", "a0"), ("slli", "t0", "a0", 1), ("slli", "t1", "a1", 1),
                                        ("addi", "a1", "zero", 0)), 3, spec=(["shl", X, 1],)),
    # x is irrelevant to (x ^ x) ^ y although it occurs in it; overwriting x's register loses nothing.
    Grammar("x86-profile1-irrelevant-input", v1.X86, (("mov", "rax", "rsi"), ("xor", "rdi", "rdi"), ("mov", "rax", "rdi"),
                                                      ("xor", "rax", "rdi"), ("xor", "rax", "rsi")),
            3, spec=(["xor", ["xor", X, X], Y],)),
    Grammar("rv64i-profile1-duplicates", v1.RV64, (("add", "t0", "a0", "zero"), ("addi", "t0", "a0", 0),
                                                   ("add", "a0", "t0", "a1"), ("sub", "a0", "a0", "a1")),
            3, spec=(SUM,)),
    Grammar("x86-profile2-one-result", v2.X86, X86_PROFILE2, 3, 1, spec=(SUM,)),
    Grammar("x86-profile2-two-results", v2.X86, X86_PROFILE2, 3, 2, spec=(SUM, CARRY)),
    # Two live flag writers: only their order decides the carry that SETB and CMOVB read.
    Grammar("x86-profile2-flag-order", v2.X86,
            (("add", "rdx", "rsi"), ("add", "rax", "rdi"), ("setb", "rax"), ("cmovb", "rdx", "rdi")), 3, 2,
            spec=(SUM, SUM)),
    # SUB and XOR of a register with itself leave the same register and flags; the byte-smaller one is kept.
    Grammar("x86-profile2-duplicates", v2.X86, (("mov", "rax", "rdi"), ("xor", "rdx", "rdx"), ("sub", "rdx", "rdx"),
                                                ("add", "rax", "rsi"), ("adc", "rax", "rdx")), 3, 1, spec=(SUM,)),
    Grammar("rv64i-profile2-one-result", v2.RV64, RV64_PROFILE2, 3, 1, spec=(SUM,)),
    Grammar("rv64i-profile2-duplicates", v2.RV64, (("add", "t0", "a0", "zero"), ("addi", "t0", "a0", 0),
                                                   ("add", "a0", "t0", "a1"), ("addi", "a1", "zero", 1)),
            3, 1, spec=(SUM,)),
    Grammar("rv64i-profile2-two-results", v2.RV64, RV64_PROFILE2, 3, 2, spec=(SUM, CARRY)),
    Grammar("rv64i-profile2-irrelevant-input", v2.RV64, (("add", "a0", "a1", "zero"), ("addi", "a0", "zero", 0),
                                                         ("xor", "a0", "a0", "a1"), ("addi", "a0", "a1", 0)),
            3, 1, spec=(["xor", ["xor", X, X], Y],)),
)


def memory_cases() -> tuple:
    """The registered profile-3 memory kernels, each over its own grammar up to its instruction bound."""
    cases = []
    for path in sorted((ROOT / "test-data/machine-code/memory-kernels").glob("*.json")):
        kernel = json.loads(path.read_text(encoding="utf-8"))
        problem = kernel["problem"]
        cases.append(Grammar("memory-" + kernel["name"], problem["target"],
                             tuple(tuple(form) for form in problem["grammar"]), problem["max_instructions"],
                             len(problem["outputs"]), base=problem))
    return tuple(cases)


CASES += memory_cases()


def profile2(target: str) -> bool:
    return target in (v2.X86, v2.RV64)


def profile3(target: str) -> bool:
    return target in (v3.X86, v3.RV64)


def writable(problem: dict | None) -> list:
    """(region, word index) of every read-write region word, which profile 3 observes at the end."""
    return [(r["name"], i) for r in (problem or {}).get("regions", []) if r["access"] == "read-write"
            for i in range(r["words"])]


def code(target: str, forms: tuple) -> bytes:
    return encode_program(target, [list(form) for form in forms])


def cost(target: str, forms: tuple) -> tuple[int, int]:
    return sum(len(encode_instruction(target, list(form))) for form in forms), len(forms)


def refused(target: str, forms: tuple, problem: dict | None = None) -> bool:
    """Whether the checker refuses the sequence before any solver call: it reads a flag left undefined or, on
    profile 3, breaks a memory admission rule such as addressing through a redefined base."""
    if not (profile2(target) or profile3(target)):
        return False
    try:
        if profile3(target):
            v3.admit(problem, v3.decode(target, code(target, forms)))
        else:
            v2.require_defined_flag_reads(target, v2.decode(target, code(target, forms)))
    except ValueError:
        return True
    return False


def registers(target: str) -> tuple:
    if profile3(target):
        return v3.REGISTERS[target]
    if profile2(target):
        return v2.REGISTERS[target]
    return tuple(v1._SCRATCH[target]) + (("rdi", "rsi") if target == v1.X86 else ("a0", "a1"))


def results(target: str, count: int) -> tuple:
    if profile3(target):
        return v3.RESULTS[target][:count]
    if profile2(target):
        return v2.RESULTS[target][:count]
    return ("rax",) if target == v1.X86 else ("a0",)


def concrete(target: str, forms: tuple, state: dict, flags: dict | None, count: int,
             problem: dict | None = None) -> tuple:
    """Observed results (and profile-3 read-write region words) of one sequence from one complete entry state."""
    raw = code(target, forms)
    if profile3(target):
        memory = {r["name"]: [state[f"{r['name']}[{i}]"] for i in range(r["words"])] for r in problem["regions"]}
        final, _, words = v3.execute(problem, raw, {name: state[name] for name in registers(target)},
                                     dict(flags) if flags is not None else None, memory)
        return tuple(final[name] for name in results(target, count)) + tuple(words[n][i] for n, i in
                                                                             writable(problem))
    if profile2(target):
        final, _ = v2.execute(target, raw, dict(state), dict(flags) if flags is not None else None)
        return tuple(final[name] for name in results(target, count))
    arguments = ("rdi", "rsi") if target == v1.X86 else ("a0", "a1")
    initial = {name: value for name, value in state.items() if name not in arguments}
    return (v1.run_concrete(target, raw, {"x": state[arguments[0]], "y": state[arguments[1]]}, initial),)


def samples(target: str, problem: dict | None = None) -> list:
    rng = random.Random(20261030)
    edge = [0, 1, MASK, 1 << 63, (1 << 63) - 1, 0x5555555555555555]
    words = [f"{r['name']}[{i}]" for r in (problem or {}).get("regions", []) for i in range(r["words"])]
    states = []
    for index in range(SAMPLES):
        state = {name: (rng.choice(edge) if index % 3 == 0 else rng.getrandbits(64))
                 for name in (*registers(target), *words)}
        flags = {name: rng.getrandbits(1) for name in v2.FLAGS} if target in (v2.X86, v3.X86) else None
        states.append((state, flags))
    return states


def proved_equal(target: str, left: tuple, right: tuple, count: int, timeout_ms: int = 10_000,
                 problem: dict | None = None, context=None) -> bool:
    """Z3 proof that both sequences give the same observed results from every entry state.

    One `context` may serve many proofs: the entry variables are the same in each, and each proof has its own
    solver.
    """
    import z3
    context = context or z3.Context()
    state = {name: z3.BitVec("entry_" + name, 64, ctx=context) for name in registers(target)}
    observed = []
    for forms in (left, right):
        raw = code(target, forms)
        if profile3(target):
            # Every register, flag and region word is free; addresses are region-relative, as in the checker.
            flags = {name: z3.Bool("entry_" + name, ctx=context) for name in v3.FLAGS} if target == v3.X86 else {}
            r, f = dict(state), dict(flags)
            if target == v3.RV64:
                r["zero"] = z3.BitVecVal(0, 64, ctx=context)
            m = {region["name"]: [z3.BitVec(f"entry_{region['name']}_{i}", 64, ctx=context)
                                  for i in range(region["words"])] for region in problem["regions"]}
            bases = {region["base"]: region["name"] for region in problem["regions"]}
            for instruction in v3.decode(target, raw)[:-1]:
                v3._step_smt(z3, target, instruction, r, f, m, bases, context)
            observed.append([r[name] for name in results(target, count)] + [m[n][i] for n, i in writable(problem)])
        elif profile2(target):
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


def check(case: Grammar, pruned: Pruner, against_spec: bool = False, memo: dict | None = None) -> dict:
    """Enumerate the case with pruning off and on and witness every removed sequence.

    Without `against_spec` every removal except a lost input needs a kept twin. With it, the pruning may also
    remove a sequence whose only twins lose an input, so a removal without a kept twin passes when the checker
    finds the sequence itself not equivalent to the specification. Passes over the same case may share a `memo`
    of admission, sample results, proofs and checker verdicts, which depend only on the sequences.
    """
    memo = {} if memo is None else memo
    every = [tuple(case.forms[i] for i in choice) for length in range(case.length + 1)
             for choice in product(range(len(case.forms)), repeat=length)]
    reasons = {forms: pruned(case.target, forms, case.results) for forms in every}
    kept = [forms for forms in every if reasons[forms] is None]
    removed = [forms for forms in every if reasons[forms] is not None]
    problem = case.problem()
    states = samples(case.target, problem)

    def fingerprint(forms):
        cache = memo.setdefault("prints", {})
        if forms not in cache:
            try:
                cache[forms] = tuple(concrete(case.target, forms, state, flags, case.results, problem)
                                     for state, flags in states)
            except ValueError:
                cache[forms] = None
        return cache[forms]

    def admitted(forms):
        cache = memo.setdefault("refused", {})
        if forms not in cache:
            cache[forms] = refused(case.target, forms, problem)
        return not cache[forms]

    def verdict(forms):
        cache = memo.setdefault("verdicts", {})
        if forms not in cache:
            cache[forms] = checker.check(problem, code(case.target, forms), timeout_ms=5000)["status"]
        return cache[forms]

    def rank(forms):
        """The search's rank: objective cost, the other cost, then the code's hex (its final tie-break)."""
        cache = memo.setdefault("ranks", {})
        if forms not in cache:
            size = {"bytes": cost(case.target, forms)[0], "instructions": len(forms)}
            other = "instructions" if problem["objective"] == "bytes" else "bytes"
            cache[forms] = (size[problem["objective"]], size[other], code(case.target, forms).hex())
        return cache[forms]

    def equal(forms, candidate):
        cache = memo.setdefault("proofs", {})
        if (forms, candidate) not in cache:
            cache[forms, candidate] = proved_equal(case.target, forms, candidate, case.results, problem=problem,
                                                   context=context)
        return cache[forms, candidate]

    # A twin must itself be admitted, or the search could never accept it in place of the removed sequence.
    prints = {forms: fingerprint(forms) if admitted(forms) else None for forms in kept}
    kept_set = set(kept)
    report = {"case": case.name, "target": case.target, "forms": len(case.forms), "length": case.length,
              "results": case.results, "enumerated": len(every), "kept": len(kept), "removed": len(removed),
              "removed_refused": 0, "witnessed": 0, "lost_not_equivalent": 0, "not_equivalent": 0,
              "failures": [], "reasons": {}}
    checker = v3 if profile3(case.target) else v2 if profile2(case.target) else v1
    if "context" not in memo:
        import z3
        memo["context"] = z3.Context()
    context = memo["context"]
    for forms in removed:
        report["reasons"][reasons[forms]] = report["reasons"].get(reasons[forms], 0) + 1
        if not admitted(forms):
            report["removed_refused"] += 1
            continue
        if reasons[forms] == "lost":
            # A lost input has no twin: the removed sequence itself must never be equivalent to the spec.
            status = verdict(forms)
            if status in ("counterexample", "refused"):
                report["lost_not_equivalent"] += 1
            else:
                report["failures"].append({"removed": [list(f) for f in forms], "reason": "lost",
                                           "checker": status})
            continue
        target_print = fingerprint(forms)
        bound, place = cost(case.target, forms), rank(forms)
        ordered = [n for n in neighbours(forms) if n in kept_set]
        first = set(ordered)
        ordered += [k for k in kept if k not in first]
        witness = None
        for candidate in ordered:
            size = cost(case.target, candidate)
            if (size[0] > bound[0] or size[1] > bound[1] or rank(candidate) > place or prints[candidate] is None
                    or prints[candidate] != target_print):
                continue
            if equal(forms, candidate):
                witness = candidate
                break
        if witness is not None:
            report["witnessed"] += 1
            continue
        status = verdict(forms) if against_spec else None
        if status in ("counterexample", "refused"):
            report["not_equivalent"] += 1
        else:
            report["failures"].append({"removed": [list(f) for f in forms], "reason": reasons[forms],
                                       **({"checker": status} if status else {})})
    report["exact"] = not report["failures"]
    # Identifies the removal decisions, so a planted edit can be shown to change none of them.
    decisions = sorted([[list(f) for f in forms], reasons[forms]] for forms in removed)
    report["removals_sha256"] = hashlib.sha256(json.dumps(decisions).encode()).hexdigest()
    return report


def search_pruner(case: Grammar, lost: bool = True) -> Pruner:
    """The search's footprint pruning as a predicate, checked against the enumeration the search actually runs.

    `lost=False` empties the required inputs, which turns the lost-input rule off and leaves only the rules
    whose every removal has a twin.
    """
    from machine_code import footprint
    encoded = [encode_instruction(case.target, list(form)) for form in case.forms]
    pruner = footprint.Pruner(case.problem(), encoded)
    if not lost:
        pruner.required = frozenset()
    index = {form: position for position, form in enumerate(case.forms)}

    def pruned(target, forms, results):
        chosen = [index[form] for form in forms]
        for end in range(1, len(chosen) + 1):
            reason = pruner.judge(chosen[:end], len(chosen))
            if reason:
                return reason
        return None

    for length in range(case.length + 1):
        enumerated = set(footprint.sequences(len(case.forms), length, pruner.judge, {}))
        mine = {choice for choice in product(range(len(case.forms)), repeat=length)
                if pruned(case.target, tuple(case.forms[i] for i in choice), case.results) is None}
        if enumerated != mine:
            raise ValueError(f"{case.name}: the predicate and footprint.sequences disagree at length {length}")
    return pruned


def run(cases=CASES) -> dict:
    """Two passes per case: twins for every removal with the lost-input rule off, then the full pruning."""
    rows = []
    for case in cases:
        memo = {}
        passes = {"twins": check(case, search_pruner(case, lost=False), memo=memo),
                  "spec": check(case, search_pruner(case), against_spec=True, memo=memo)}
        for report in passes.values():
            report["failure_count"] = len(report["failures"])
            report["failures"] = report["failures"][:20]
        rows.append({"case": case.name, "target": case.target, "length": case.length, "results": case.results,
                     **passes, "failure_count": sum(r["failure_count"] for r in passes.values()),
                     "exact": all(r["exact"] for r in passes.values())})
    return {"schema": "zeno-fcis/search-pruning-exactness/2", "authority": "none", "cases": rows,
            "exact": all(row["exact"] for row in rows),
            "scope": "Witnesses are proved by Z3 and verdicts given by the independent checker under the "
                     "registered ISA models, for these grammars, lengths and specifications."}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    measured = commands.add_parser("run")
    measured.add_argument("--out", type=Path)
    measured.add_argument("--case", action="append", choices=[case.name for case in CASES],
                          help="run only this case (repeatable); every case by default")
    args = parser.parse_args(argv)
    report = run(tuple(case for case in CASES if not args.case or case.name in args.case))
    text = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    for row in report["cases"]:
        twins, spec = row["twins"], row["spec"]
        print(f"{row['case']}: enumerated {spec['enumerated']}; without lost: removed {twins['removed']} "
              f"{twins['reasons']}, witnessed {twins['witnessed']}, refused {twins['removed_refused']}; full: "
              f"removed {spec['removed']} {spec['reasons']}, witnessed {spec['witnessed']}, lost not equivalent "
              f"{spec['lost_not_equivalent']}, other not equivalent {spec['not_equivalent']}, refused "
              f"{spec['removed_refused']}; failures {row['failure_count']}")
    print("ZENO_EXACTNESS " + json.dumps({
        "exact": report["exact"], "failures": {r["case"]: r["failure_count"] for r in report["cases"]},
        "removals": {r["case"]: [r["twins"]["removals_sha256"], r["spec"]["removals_sha256"]]
                     for r in report["cases"]}}))
    return 0 if report["exact"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
